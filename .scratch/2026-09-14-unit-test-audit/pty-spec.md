# 桌面端 PTY 模块单元测试审查报告

> 状态: **审计完成，7 张修复票据待处理**（2026-09-14，20:00）
> 范围: `bedcode-desktop/src-tauri/src/pty/`（5 文件，1242 行）+ `tests/pty_session_chain.rs`（665 行）
> 测试规模: **17 个**（16 单测 + 1 集成测试）
> 分支: `dev`（工作区审计前后均干净，无代码改动入库）

---

## 1. 摘要（Verdict）

**集成测试是真的，单元测试是装饰。**

- 17 个测试里**只有 4 个真正测行为**：`pty_session_chain_flow`（1 个，含 5 子场景）、`backpressure_pause_blocks_reads_and_resume_drains_without_loss`、`with_id_creates_session_with_properties_and_kill_stops_it`、`linux_escapes_single_quotes_in_working_dir`。
- 变异测试实锤：把 PTY 输出投递整段删除（`pty_reader.rs:66` 的 `on_output(event).await` 换成 `drop(_event)`），**16 个单测全部 PASS**，只有集成测试在场景 2 失败。**单测对「终端输出彻底丢失」这一最致命回归的覆盖率为 0。**
- 探针实测抓到 **2 个真实 bug**（1 功能 / 1 安全），现有测试全部漏过。
- 存在 **1 个恒真测试**（`test_list_wsl_distributions`，Linux CI 上零断言通过）与 **1 处死代码测试**（`pty_handler.rs` 2 个测试测的 `AtomicBool` 生产代码从无人调用）。

---

## 2. 审查基线（全部实跑）

```bash
cd bedcode-desktop/src-tauri
cargo test --lib pty::            # → 16 passed; 0 failed; finished in 0.03s
cargo test --test pty_session_chain   # → 1 passed; finished in 0.34s（连跑 3 次: 0.34 / 0.33 / 0.34，稳定）
```

lib 全量 604 个测试，其中 pty 16 个（`598 filtered out`）。

---

## 3. 审查方法

三种手段叠加，避免「读了测试就说没问题」的自证循环：

1. **实跑基线**：确认测试当前状态与耗时特征（见 §2）。
2. **变异测试（Mutation Test）**：改坏生产代码 → 观察哪些测试变红 → **判断测试是否真有守卫力**。
   - 变异体：`src/pty/pty_reader.rs:66` `global_manager.on_output(event).await;` → `drop(_event);`（模拟「PTY 输出静默丢弃」回归，即 commit 19c0c3d30 那类问题的极端形态）
   - 结果：16 单测全绿，`pty_session_chain_flow` 场景 2 失败（`panicked at tests/pty_session_chain.rs:495:5: PTY echo output not observed within 20s; collected so far: ""`）
   - 变异体已在审查结束前 `git checkout -- src/pty/pty_reader.rs` 完全回滚，并重新跑通 16 + 1 确认无残留。
3. **探针（Probe）**：写临时 `tests/__pty_probe.rs`，直接调用生产公共函数打印真实输出（不依赖断言，避免"断言写了什么就看什么"），验证未覆盖输入的真实行为。探针文件已删除。

> 方法学备注：审计过程中 `read` 工具一度返回 `pty_reader.rs` 的过期快照（与磁盘不符，如 `start_offset` 实际硬编码为 0）。本文件所有引用行号与代码片段均以 `bash` 直接从磁盘读取的真值为准，已逐项重新核对。

---

## 4. 总判定表

| 文件 | 测试数 | 判定 | 关键问题 |
|---|---|---|---|
| `tests/pty_session_chain.rs` | 1（5 子场景） | 🟢 **有效防线** | 变异测试证明能抓回归；但 1 个 `#[tokio::test]` 串 5 场景、零隔离 |
| `src/pty/pty_reader.rs` | 3 | 🟡 部分有效 | 1 个真 + 2 个只查生命周期；`Err` / `running=false` / 队列关闭 / `Ok(0)` 分支全空 |
| `src/pty/command.rs` | 6 | 🟡 部分有效 | 全字符串 `contains` 断言；`env_vars` 循环 0% 覆盖；漏掉 PowerShell/CMD 注入 |
| `src/pty/pty_process.rs` | 2 | 🔴 **形同虚设** | 8 个 pub 方法无测试触达（7 完全 + `kill` 仅退化路径）；杀进程阶梯 0% 覆盖 |
| `src/pty/wsl.rs` | 3 | 🔴 **1 恒真 + 漏真 bug** | `test_list_wsl_distributions` Linux 上零断言通过；漏掉正斜杠路径解析错误 |
| `src/pty/pty_handler.rs` | 2 | 🔴 **测死代码** | `is_running()`/`set_running()` 生产代码从无人调用；trait 两个真方法（生产各有调用点）零测试 |

---

## 5. 逐文件结论

### 5.1 `tests/pty_session_chain.rs` — 🟢 有效，但有结构性弱点

**做得好的**：
- 真实链路端到端：进程内 Actix HTTP+WS（OS 分配端口）→ 真实 `tokio-tungstenite` 客户端 → HTTP 配对拿 JWT → `/ws/event` 首消息 JWT 认证 → `StartSession` → 真实 `openpty` + `bash` spawn → `PtyReader` 线程 → `GlobalOutputManager` → **TB v2 二进制帧解码**（16B 帧头，`len` 在第 12..16 字节 u32 LE）→ 断言 echo 输出。
- 失败信息分因：空输出 = 环境问题（powershell 未启动 / 未读到输出）；有启动输出无 echo = 输入链路缺陷。
- 场景 5 是**真实回归锁**：`restart_session` 后重新订阅拿 `subscribe_ok`（而非 `SESSION_NOT_FOUND`）+ echo 往返，守着 commit 19c0c3d30（重启丢输出管理器注册 → 桌面打开终端窗口空白）。
- 变异测试证明它有守卫力（见 §3）。

**弱点**：
- **1 个 `#[tokio::test]` 串 5 个子场景、零隔离**：场景 2 超时/失败 → 场景 3/4/5 永不执行。若将来场景 2 因环境抖动常红，场景 5 的回归锁会长期处于"没人知道它在不在"的状态。
- 超时预算与实跑耗时严重不对称：实跑 0.34s，但场景 2/5 各挂 20s 预算。平时完全不逼近边界，CI 变慢时第一个失败场景就吃掉 20s。

### 5.2 `src/pty/pty_reader.rs` — 🟡 1 个真 + 2 个只查生命周期

**真行为测试**（`backpressure_pause_blocks_reads_and_resume_drains_without_loss`）：
通过 `PtyReader::start_with_pause(..., Some(pause_check))` 注入可控闭包，`RecordingReader` 记录每次 read 的字节数，断言「暂停中 30ms 内 `reads.len() == 0` + 恢复后 `total == payload.len()` 零字节丢失」。**变异能被抓**，是全模块除集成测试外唯一真正测行为的单测。

**问题**：
- `reads_output_and_reports_stopped_on_eof` 灌 9000 字节负载（注释明写「强制分多次 read → 多轮输出」），**实际只断言 `lifecycle_rx == Stopped`**，从不检查任何字节到达 `GlobalOutputManager::on_output`。消费者任务是 `tauri::async_runtime::spawn` 发射后不管，测试也不等它完成 —— 这正是变异测试下它仍绿的直接原因。
- 未覆盖分支：
  - `Err(e)` → `exit_status = PtySessionStatus::Error`：两个假 Reader（`MemoryReader` / `RecordingReader`）都从不返回 `Err`，**Error 生命周期路径 0% 覆盖**。
  - `running=false` 中途退出读循环。
  - `output_tx.blocking_send(...).is_err()` 队列关闭路径。
  - `Ok(0)` 非 EOF 路径：代码里没有 `if n > 0` 守卫，0 字节读会构造空 `OutputEvent` 发送（`BufReader::read` 下 `Ok(0)` 即 EOF，风险低，但无守卫）。
  - 默认 pause 粘合层 `GlobalOutputManager::global().should_pause(&sid)`（`start_with_pause` 的 `None` 分支）不在测试范围，只有注入闭包变体被测。低险（2 行胶水）。

### 5.3 `src/pty/command.rs` — 🟡 字符串断言，漏掉注入

6 个测试全部是 `assert!(full.contains("..."))` 正向断言，覆盖 3 种环境 + cwd + Linux 单引号转义。

问题：
- `env_vars` 在 6 个测试构造的配置里**全是空 `HashMap`** → `build_command` 末尾的 `cmd.env(key, value)` 循环 0% 覆盖。
- **可测性结构盲区**：`portable_pty::CommandBuilder::get_env()` 需要传 key、无法枚举 → **结构上无法断言 env 是否真的被设置**（探针实测 `error[E0061]: this method takes 1 argument`）。这一路径只能靠端到端集成测试间接验证。
- `wsl_path_passthrough_for_unix_style_paths` 近似无效：`let _ = argv(&cmd);` 调用后丢弃结果，唯一断言 `assert!(cmd.get_cwd().is_none())` 已在 `wsl2_uses_distro_and_converts_windows_path` 里重复断言过。测试名承诺"类 Unix 路径原样透传"，实际不验证透传。
- 漏掉 PowerShell / CMD 的 working_dir 注入（见 §6 Bug B）。

### 5.4 `src/pty/pty_process.rs` — 🔴 形同虚设

只有 2 个测试：`with_id_creates_session_with_properties_and_kill_stops_it`、`new_generates_unique_session_ids`。

- **8 个 pub 方法无任何测试触达**：7 个完全零覆盖（`name()`、`start()`、`write()`、`write_str()`、`send_special_key()`、`resize()`、`subscribe_lifecycle()`）+ `kill()` 仅退化路径。另：私有 `start_output_reader()` 只在 `start()` 内部被调（`pty_process.rs:160`），因此同样靠 `start()` 间接覆盖。
- `kill()` 唯一被覆盖的是**未 `start` 的退化路径**（`process_id: None` → 跳过 `taskkill`）：

  ```rust
  session.kill().await.expect("kill should succeed");
  assert!(!session.is_running());
  ```

  即 graceful 四级阶梯（`Ctrl-C` → `\nexit\n` → Windows `taskkill /pid /t /f` / Unix `kill -9` → 4s 超时后 `force_kill`）**0% 覆盖**。
- 讽刺点：代码注释明确记录了「Windows bash 下 `ctrl_c` 无效，必须 `taskkill /t /f` 递归杀整树」的修复结论（`kill()` 注释 + `send_special_key` 注释），**这个修复没有任何测试守着**。若将来有人把 `taskkill` 分支改坏，全部 17 个测试都会保持绿色（Linux 上走 `kill -9` 分支，Windows 上无 CI）。
- `new_generates_unique_session_ids` 近乎恒真：ID 由 `process::id` + 单调 `AtomicU64` 计数器构造，构造方式即保证唯一，只有把计数器整个删掉才会失败。价值低。
- `resize()` 在注释里写着「`PtyProcess` dropped 后 PTY 关闭」的 Windows 竞态，无任何测试。

### 5.5 `src/pty/wsl.rs` — 🔴 1 个恒真测试 + 漏掉真实 bug

`test_windows_to_wsl_path`（4 断言）、`test_wsl_to_windows_path`（2 断言）是真断言。

`test_list_wsl_distributions` **恒真**，本机实测输出：

```
========== Testing WSL Distribution List ==========
WSL Available: false
WSL is not installed or not enabled. Skipping test.
test result: ok. 1 passed; 0 failed
```

**零断言通过**。任何 Linux CI 上它都是空转；同时它缺 `#[cfg(windows)]`，在 Windows 上又耦合真实环境（断言「存在默认发行版」，全新装了 WSL2 但没装发行版的机器会挂）—— 正好没做到 `.scratch/desktop-integration-tests/spec.md` 自己写的「失败时先确认是测试环境问题还是链路 bug」。它是全模块唯一涉及真实进程执行（`cmd.exe` / `wsl`）的测试。

漏掉的正斜杠路径 bug 见 §6 Bug A。

### 5.6 `src/pty/pty_handler.rs` — 🔴 测的是死代码

2 个测试（`running_flag_toggles_lifecycle`、`default_equals_new`）只测 `running: AtomicBool` 的 get/set。

- grep 证实 `is_running()` / `set_running()` **生产代码从无任何调用方**（`system/lifecycle.rs:277` 那个 hit 是 `supervisor.is_running()`，无关对象）。
- 而 trait 真正被 `SessionManager` 使用的两个方法**零测试覆盖**（生产调用点：`create_session` × 2 于 `session_manager.rs:331`/`430`；`create_session_with_id` × 2 于 `:330`/`:605`，后者还经插件宿主 `host_impl/session.rs:86` 间接触达）。
- 注释自称「将 PTY 操作抽象为 trait，便于测试和替换实现」—— 实际没带来任何可测性：没有任何测试使用 trait 边界替换实现。

---

## 6. 探针实测抓到的真实 bug（现有测试全部漏过）

### Bug A — WSL 路径转换：正斜杠形式解析错误（功能）

`windows_to_wsl_path` 的文档注释明确声明支持两种正斜杠写法（`//wsl.localhost/`、`//wsl$/`），实测两种都错：

| 输入 | 实际输出 | 期望 |
|---|---|---|
| `//wsl.localhost/Ubuntu/home/user` | `Ubuntu/home/user` | `/home/user` |
| `//wsl$/Ubuntu/home/user` | `wsl$/Ubuntu/home/user` | `/home/user` |
| `\\wsl$\Ubuntu\home\user` | `/home/user` | `/home/user` ✅ |
| `\\wsl.localhost\Ubuntu\home\user` | `/home/user` | `/home/user` ✅ |

**根因**：新格式分支连续 `trim_start_matches('\\').trim_start_matches('/')` 把开头两个 `/` 一次吃掉，之后用 `splitn(2, '\\')` 按**反斜杠**切分 distro 段——正斜杠输入切不出 distro，`parts.len() >= 2` 不成立，落到兜底 `path.replace('\\', "/")` 原样返回。反斜杠两种形式都正确，所以现有 4 条断言全绿。

**影响**：移动端选择 `//wsl.localhost/...` 形式的 WSL 路径作为工作目录时，WSL2 会话的 `cd` 目标错误（`cd 'Ubuntu/home/user'` 在 WSL 内是相对路径，通常 `No such file or directory`）。

### Bug B — 命令注入：PowerShell / CMD 分支不转义 working_dir（安全）

`working_dir` 来自 `SessionLaunchConfig`，由移动端经 wire 下发（会话配置），Rust 端未做路径白名单校验。三个环境的转义**不对称**：

| 环境 | 构造 | 注入 payload | 结果 |
|---|---|---|---|
| PowerShell | `Set-Location '{}'` | `D:\x'; $env:BEDCODE_PWN='1; cmd /c powershell.exe -NoProfile -Command '; #` | 单引号提前闭合 → **任意 PowerShell 可执行** |
| CMD | `cd /d "{}"` | `D:\x" & echo PWNED &` | 双引号闭合 + `&` 拼接 → **任意 CMD 可执行** |
| Linux | `cd '{}'` + `replace('\'', "'\\''")` | `/tmp/o'clock; touch /tmp/pwn` | 转义为 `'/tmp/o'\''clock; touch /tmp/pwn'`，**正确隔离** ✅ |

探针实测输出（原始 argv）：

```
powershell: chcp 65001 > $null; ... Set-Location 'D:\x'; $env:BEDCODE_PWN='1; cmd /c powershell.exe -NoProfile -Command '; #'; Write-Host 'Working directory:' $PWD.Path; echo ok
cmd:        @chcp 65001 > nul && cd /d "D:\x" & echo PWNED &" && echo Working directory: %cd% && echo ok
linux:      cd '/tmp/o'\''clock; touch /tmp/pwn' && pwd && echo ok
```

**现有测试漏过原因**：只有 `linux_escapes_single_quotes_in_working_dir` 一条转义测试，且只测 Linux 分支；PowerShell / CMD 只有 `contains("Set-Location 'D:\\work'")` / `contains("cd /d \"D:\\work\"")` 这种正向断言，注入 payload 零覆盖。

---

## 7. 顺带发现（非测试问题，但与「能否测出 bug」直接相关）

| # | 位置 | 问题 | 关联规范 |
|---|---|---|---|
| O1 | `pty_reader.rs:116` `let _ = lifecycle_tx.send(exit_status);` | EOF / Error 的**最终判定**发送失败静默忽略，前端可能永远收不到「会话已停止」 | §6「重要路径禁止 `let _ =` 静默忽略错误」 |
| O2 | `pty_reader.rs:63` `tauri::async_runtime::spawn(...)` | 未用 `spawn_with_error_boundary()` 包装，async 块 panic 会静默杀任务（全 `src/` 45 处裸 spawn，仅 14 个文件用了包装） | §6 同上 |
| O3 | `pty_reader.rs:125` `let _ = handle.join();` | 读线程 join 错误（线程 panic）被忽略 | §6 |
| O4 | `wsl.rs:48` `let mutgbk = encoding_rs::GBK;` | 命名不规范（应为 `gbk`）；且是死路径——`wsl --list --verbose` 输出恒为 UTF-16LE，`UTF_16LE.decode` 不会失败 | §6 命名 / 注释 |
| O5 | 全局 | 无覆盖率门禁：`cargo-llvm-cov` 未安装，CI 仅 eslint + cargo test + vitest，覆盖率倒退无法度量也无法阻断 | §10 / CI |

---

## 8. 修复优先级

| 优先级 | 票据 | 内容 | 理由 |
|---|---|---|---|
| P0 | `issues/02` | 修 Bug B（命令注入）+ 补 PowerShell/CMD 转义对称测试 | 安全漏洞，wire 可控输入 |
| P0 | `issues/01` | 修 Bug A（WSL 正斜杠）+ 补正斜杠用例 | 功能 bug，文档承诺的行为 |
| P1 | `issues/03` | 补 `PtySession` 行为测试（`start`/`write`/`resize`/`kill` 阶梯/`Drop`） | 最大空洞：8 个 pub 方法无测试触达 |
| P1 | `issues/04` | `pty_reader` 补数据投递断言 + `Err`/队列关闭/`running=false` 分支 | 变异测试暴露的直接缺口 |
| P2 | `issues/05` | `wsl::test_list_wsl_distributions` 加 `#[cfg(windows)]` 或改为不依赖真实发行版 | 恒真测试，CI 噪音 |
| P2 | `issues/06` | 集成测试拆成 5 个独立 `#[tokio::test]` | 前序失败遮蔽后序回归锁 |
| P3 | `issues/07` | `PtySessionHandler` 死代码决策（删除或接线）+ trait 方法测试 | 注释承诺的可测性未兑现 |

---

## 9. 观察项（暂不成票，需要设计决策）

- **`CommandBuilder::env_vars` 可测性盲区**：`get_env()` 需要传 key、无法枚举，`cmd.env()` 循环结构上无法被单测断言。可选方案：(a) 接受现状，靠端到端集成测试验证；(b) 在 `build_command` 旁加一个可测的 `env_vars` 规范化纯函数并单测它，`CommandBuilder` 只做应用。倾向 (a)，成本最低。
- **`PtyReader` 的 `start_offset` 恒为 0**：offset 分配挪到了 `GlobalOutputManager::on_output` 的串行临界区内（`session_output.rs:448`），设计合理，但意味着 `pty_reader` 单测无法验证字节偏移正确性——该不变量只能靠 `session_output` 的测试守。
- **`write()` 的 4000 字节分块**：注释写明「一次性写大负载可能失败」，属真实平台约束。修复归入 `issues/03`（用真实 `start()` 后验证 8KB 写入）。

---

## 10. 复现命令

```bash
cd bedcode-desktop/src-tauri

# 基线
cargo test --lib pty::
cargo test --test pty_session_chain

# 单测过滤
cargo test --lib pty::pty_reader
cargo test --lib pty::command

# 复现 Bug A（WSL 正斜杠）：临时探针或直接在测试里断言
#   assert_eq!(windows_to_wsl_path("//wsl.localhost/Ubuntu/home/user"), "/home/user");  # 当前失败

# 复现 Bug B（注入）：临时探针打印 build_command 的 argv（见报告 §6 原始输出）

# 复现恒真测试
cargo test --lib pty::wsl::tests::test_list_wsl_distributions -- --nocapture
# → "WSL is not installed or not enabled. Skipping test." + "test result: ok. 1 passed"

# 复现变异测试守卫力（审查时已回滚，勿留存）
#   src/pty/pty_reader.rs:66  global_manager.on_output(event).await;  →  drop(_event);
#   → 16 单测全绿，pty_session_chain 场景 2 失败（20s 超时）
```

---

## 11. 审计纪律记录

- 变异测试与探针全部回滚/删除：`git status` 干净，`grep -rn "MUTATION-PROBE"` 无残留，回滚后重新跑通 16 + 1。
- 未提交任何代码改动；本文档与票据均为新增文件。
- 审计结束已检查无测试残留进程/监听端口（`:8766` 无监听；gradle daemon 与 tsserver 为既有 IDE/Gradle 进程，非本次测试产生）。
- **自查发现并修正了两处 grep 作用域错误**（写文档后逐条复核才发现，记录以便后续审计复用）：
  1. 初版把 `PtySession` 方法调用点统计限定在 `tests/ src/pty/`，漏掉 `src/session/`、`src/plugin/` —— 导致误报「`create_session_with_id` 生产代码未在用」（实际 `session_manager.rs:330`/`:605` 各一处）与「6 个 pub 方法」的低估（实际 8 个）。已改为直接 grep `tests/` 目录取零引用结论。
  2. 初版把 `start_output_reader` 归为 pub 方法，实为私有（仅在 `start()` 内部调，`pty_process.rs:160`）。
- **工具陷阱**：`read` 工具一度返回 `pty_reader.rs` 的过期快照（与磁盘不符）。凡结论要写进文档的，一律用 `bash` 直接从磁盘取真值。
