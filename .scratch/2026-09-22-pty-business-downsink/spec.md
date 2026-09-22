# PTY 业务语义下沉专项（Business PTY Downsink）

Status: in-progress（2026-09-22 立项；阶段 1 实施中）
Date: 2026-09-22
范围: **仅桌面端**（`bedcode-desktop/src-tauri/src/pty/`、`src-tauri/src/session/` 的 Business 消费线、
`plugins/terminal-session/` 的 launch 域、`packages/plugin-sdk-desktop/`）；`bedcode-mobile/` 零改动、
零验证责任（host-session 是桌面独有接口，ADR 0022 双端偏离节）
决策依据: AGENTS.md §5（无业务内核、ADR 0022 裁剪线：「宿主只暴露离宿主无法实现且无业务语义的原语」）、
`.scratch/2026-09-10-plugin-kernel-roadmap/spec.md`（阶段 3/4）、`.scratch/2026-09-19-terminal-session-plugin/`（票 08-10
会话语义已下沉）、`.scratch/2026-09-20-host-business-decarriage/`（已 done，本专项是其后继的 pty 面收尾）
承接: 会话创建编排已下沉 `com.bedcode.terminal-session`（`launch.rs`：命名唯一化 + config→launch spec 映射 +
两阶段启动决策，2006-09 票 09）；配置真源已进插件私有库（票 08）。**唯独 PTY 命令构造（shell 包装）仍留在宿主**
——本专项把最后一块业务语义挪出 pty 引擎。

---

## Problem Statement

宿主 `pty/` 引擎接两条消费线（`PtyCommandSource`），其中 **Business 线**携带四类业务语义，违反裁剪线：

| 业务语义 | 宿主位置 | 产品概念证据 |
| --- | --- | --- |
| ① shell 包装（`bash -lic` / PowerShell `-NoExit -Command` / CMD `/K` + chcp/Set-Location/cd+pwd 命令体） | `pty/command.rs::build_command`（模块注释自认「构建不同执行环境的命令」） | `.bashrc` 交互守卫、nvm/pnpm PATH、登录 shell 语义——都是**产品决策**（插件 launch.rs 已有 `resolve_environment` 做发行版/shell 分支，缺最后一跳命令体拼装） |
| ② WSL 路径转换（`windows_to_wsl_path` + `wsl.exe -d <distro> -- bash -lic`） | `pty/wsl.rs`（275 行）+ `command.rs` Wsl2 分支 | WSL2 是**产品环境名**（存在 `host-platform.wsl-distros` v19 原语）；启动线改成同段落：`environment={"type":"Wsl2"}` 只会在插件编排里产生 |
| ③ `BEDCODE_SESSION_ID` 环境变量注入 | `pty_process.rs` start() 的 Business 分支 | Claude Code hook（`plugins/terminal-session/scripts/`）识别会话的**业务链路身份** |
| ④ `PtySlaveFdPolicy::Hold`（业务线持有 slave fd → 自然退出不可观测） | `pty_process.rs`（注释已登记「统一 ReleaseOnSpawn 属票 07 抽取候选」） | 「自然退出不翻 Stopped」是**业务会话现役语义**；插件私有 PTY 的 `ReleaseOnSpawn` 才是引擎干净形态 |

现状调用链（v21 后唯一编排入口）：

```
命令面/HTTP/WS ── session_create_bridge (插件必需, 无降级)
  → 插件 session-create api → launch.rs 编排（命名/config→launch/start 决策）
    → host-session create-with-spec {name, command, args?, cwd, env?, environment, ...}
      → resolve_launch_spec（校验 + args 空格拼接）→ SessionLaunchConfig
        → PtySession (Business 线) → build_command（shell 包装 ①②）+ ENV 注入 ③ + Hold ④
```

**矛盾点**：编排决策（选什么 shell、什么发行版、什么 cwd/命令）已在插件侧（launch.rs），
但「把这些决策变成可 exec 的命令」仍在宿主。且 **WIT 注释声称 `args` 是「参数数组 exec
天然免注入」，实现却是空格拼接进 shell 命令体**——文档与实现脱节，恰好暴露「Raw exec
路径应落地而未落地」。

**安全面**：`build_command` 的转义是安全关键（working_dir 来自移动端 wire，不可信）：
PowerShell 单引号 `''` 转义、CMD 危险字符拒绝、Linux/WSL 单引号 `'\''` 转义。scratchpad 记有
两个既有缺陷随迁必须修：WSL 分支正斜杠路径解析错误（`//wsl.localhost/Ubuntu/...` 形态）、
WSL 单引号转义遗漏补齐（注释：「票据 02 仅补了 PowerShell/CMD/Linux 三路，WSL 遗漏」）——
下沉到插件重写时按正确语义实现，不允许把 bug 原样搬过去。

---

## Solution

**下沉形态：业务会话改走 Raw exec 语义，shell 包装逻辑迁入插件 launch.rs，宿主 pty 引擎只留裸 PTY 原语 + 会话寄存器 + 输出环。**

### 契约面（增量演进，不 bump ABI）

`host-session.create-with-spec` 的 spec-json **追加字段** `commandArgs`（可选，`Vec<String>`）：

- 缺省/空数组 → **旧路径**（command 字符串 + args 空格拼接 + 宿主 build_command 包装）——老产物不变，向后兼容
- 非空 → **新路径**（argv exec：`argv[0]` 为主程序，`argv[1..]` 为参数数组，宿主原样 exec，
  **不经过** build_command——无 shell 包装、无 WSL 转换、无 host 注入）；`environment` 语义收窄为
  「宿主仅用其分辨 WSL2 env 转发与尺寸策略」或不再需要（实施时定）
- 协议面 HTTP/WS 启动线（`/api/sessions/start`、WS 终端订阅）**零改动**：桥接 `create_session_via_plugin`
  与插件 `session-create` api 形状不变，只变插件→宿主那一跳的 spec 字段

与此对应，插件 `launch.rs` 新增 `build_argv`（复刻并修正 build_command 语义）：

- Linux：`bash -lic "cd '<escaped>' && pwd && <cmd>"`（argv = `[bash, -lic, <脚本串>]`，保留交互守卫语义）
- Wsl2：`wsl.exe -d <distro> -- bash -lic <脚本串>`（路径转换 + 单引号转义随迁，**修 scratchpad 两处缺陷**）
- Windows PowerShell：`powershell.exe -NoLogo -NoExit -Command <脚本串>`（`''` 转义 + chcp 65001 保留）
- Windows CMD：`cmd.exe /K <脚本串>`（危险字符拒绝上移插件侧输入仲裁；**宿主 create-with-spec 保留最小
  防御**：非空 commandArgs 且含 NUL/空 argv0 拒绝——引擎仍守住可 exec 边界）

### 引擎面（退役清单）

1. `pty/command.rs` → **删除**（shell 包装全部迁插件；其 200+ 行测试转插件侧契约测试）
2. `pty/wsl.rs` → **迁移**（纯函数字符串转换，无 OS 依赖，挪插件 launch 域；宿主 create-with-spec 新路径
   不解析 WSL 路径，`environment` 的 Wsl2 分支仅保留在旧路径过渡期）
3. `PtyCommandSource::Business(SessionLaunchConfig)` 变体 → **退役**（create-with-spec 新路径直接构造
   `CommandBuilder`（Raw 语义）进 PtySession；Business 只在旧产物兼容窗口期保留后删除）；
   终态 = `PtyCommandSource` 枚举收敛为裸命令（或直接参数化，实施时定）
4. ③ `BEDCODE_SESSION_ID` 注入归属 → 开放点 1（克制选项：宿主把「给子进程注入会话 id env」原语化，
   保留注入点但定义落到引擎语义；激进选项：插件经 spec `env` 传——但 session_id 宿主预生成、插件拿不到，
   需两阶段编排改动，不推荐）
5. ④ `PtySlaveFdPolicy::Hold` → 独立子票（行为修正：统一 ReleaseOnSpawn 让「自然退出 → Stopped 状态事件」
   可达；改动弹作用面：前端会话状态机 / 设备列表 / 插件事件消费——**单独评估，不夹带**）

### 过渡窗口（双轨）

- 新插件产物（含 commandArgs）与旧插件产物（无 commandArgs）**可共存**：宿主 create-with-spec 按字段
  有无分叉执行；`resources/plugins/desktop/com.bedcode.terminal-session/` 产物在票 1 验证闭环后替换
- 窗口结束条件：宿主 build_command/wsl.rs 全量删除、Business 变体移除、旧路径代码删除（票 2）

### 明确保留在宿主（引擎原语，非本专项范围）

- PTY 本体（spawn/读线程/终态门/回收）——`pty_process.rs` / `pty_reader.rs` / `pty.lifecycle.rs`
- 输出管线（`GlobalOutputManager` / `SessionOutputSink` / `pty_ring` 游标环）——多端共享输出订阅是引擎机制
- 会话寄存器/状态机/尺寸裁决登记/属主表（`session_manager.rs`）——上一轮已定调：引擎登记簿
- host-session 会话生命周期/注解槽/连接清单原语——插件闸门下的引擎面
- **安全边界**：create-with-spec 输入仲裁（空名/空命令/空 cwd 拒绝）+ commandArgs 可 exec 校验留宿主

---

## 分票建议

- **票 1（核心）**：create-with-spec 加 `commandArgs` 新路径（宿主 resolve_launch_spec + PtySession Raw 接线）+
  插件 launch.rs `build_argv`（复刻+修正转义）+ commandArgs 全链路 e2e（真实 wasm 闭环 + 双轨对照：
  新旧 spec 各起一次会话，输出/状态/尺寸行为逐项一致断言）
- **票 2（收尾退役）**：宿主 build_command/wsl.rs 删除、Business 变体移除、旧路径代码/测试清理；
  command.rs 测试迁移与插件侧契约测试对齐；`PtyCommandSource` 简化
- **票 3（独立，行为修正）**：`PtySlaveFdPolicy` 统一 ReleaseOnSpawn（自然退出 → Stopped 事件评估）
- **票 4（独立，注入归属裁定）**：BEDCODE_SESSION_ID 注入点原语化 vs 插件 env（开放点 1 定案）

## 开放点

1. ③ BEDCODE_SESSION_ID 注入归属（引擎原语「spawn 注入会话 id env」 vs 插件 env 占位——session_id 宿主
   预生成，插件无注入时机，倾向宿主原语化保留，见票 4）
2. `commandArgs` 下 `environment` 字段的语义收窄：仅剩「WSL2 发行版名」还能起什么作用（实施时按
   session_manager 尺寸/记录字段需要定；候选：收窄为只影响旧路径，新路径忽略）
3. WSL 路径转换挪插件后，`plugins/terminal-session` 是否要暴露一个「Window 宿主路径 → WSL 路径」的
   host 原语给第三方插件（不建，YAGNI——WSL 是 session 插件自己的编排概念，经互调 API 复用即可）
4. `args`（既有字段，空格拼接语义）与 `commandArgs`（argv 语义）的关系：新路径存在后 args 字段
   在 WIT 注释与实现双重修正（实现其实应按 argv 语义走新路径，args 仅服务旧路径）

## 验收标准

- [ ] 新路径（commandArgs）与旧路径（command 字符串）创建会话行为逐项一致：输出、自然退出、resize、
      Windows/WSL/Linux 三环境、尺寸缺省、两阶段启动
- [ ] 移动端 `vitest` 零改动全绿；`/api/sessions/start` 与 WS 终端订阅协议面逐字节不变（e2e 断言）
- [ ] scratchpad 两处 WSL/转义缺陷在新实现中修复并有反例测试（正斜杠路径、`'` 注入）
- [ ] 票 2 后 `pty/` 目录不含任何 shell 包装 / WSL 转换 / 产品环境分支代码；`PtyCommandSource::Business` 不存在
- [ ] 宿主 cargo test 全绿（含 session_e2e / pty_e2e / terminal_output_perf）、`pnpm run test:run` 全绿、
      eslint 0 error、权限词汇五同步无漂移（本专项不新增权限位，需确认无删减）
- [ ] AGENTS.md §7（如有能力清单计数变化）、code-map（pty 章节）、ADR 0022（如 wire 行为变更）同步更新