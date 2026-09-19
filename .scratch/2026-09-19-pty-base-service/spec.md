# 桌面端 PTY 基础能力服务：host-pty 插件原语

> Status: implemented（票 01-07 全部 done，2026-09-19；桌面 lib 999 绿 + 8 集成目标绿，唯一红项 `broadcast_shutdown` 属认证中心线在途；D4 / D9 已补实施定形，与票面的偏离逐条登记在各票 Comments）
> Date: 2026-09-19
> 分支：dev（桌面端；移动端不动，双端偏离见 D12 与 ADR 0022「双端偏离」节）
> 同类前例：`2026-09-10-mdns-service-plugin/spec-basic-capability-service.md`（同构模板：单守护 / 事件定向投递 / 双表回收 / 零业务红线）、`2026-09-18-ws-base-service/spec.md`（owner 作用域 topic / 权限五同步点 / ABI bump / fixture 闭环）

---

## Problem Statement

插件（auto-task、agent-hub、ai-chatbox、未来的功能插件）需要在桌面上执行**交互式进程**：跑 TUI 程序（vim/top）、与 shell 保持长会话逐帧交互、按终端尺寸渲染输出、感知进程退出。当前宿主只给了它们：

- `host-process.run/kill`——**非交互**进程执行，无 PTY、无 TTY 行为（TUI 程序直接没法跑）；
- `host-terminal.send(session-id, data)` + `terminal-hooks`——只能向**宿主业务会话**（主机终端会话，有产品语义：会话配置、SessionManager 生命周期、前端 UI）注入输入/做字符串变换钩子，插件**无法创建属于自己的终端会话**，也无法读取输出流；
- `host-session.create(config-id)`——按**业务配置**建宿主会话，结构化产品语义，插件创建的东西会污染宿主会话列表、进宿主生命周期/事件链。

三者都不满足「插件要一个干净的、只属于自己的交互式 PTY」：不挂业务会话，输出可读，随插件生命周期回收。这是「无业务内核」路线的既有空地——进程（PTY）在 §5 内核清单里，但宿主侧 `pty/` 模块的能力被业务会话线（`session/`）独占，未对插件开放。

## Solution

按「基础能力服务」模式（mDNS v2 / host-websocket / db-http 同构），把宿主 PTY 引擎封装为 **host-pty** 插件原语：

- **spawn 一个裸伪终端**（纯引擎参数：command + args + env + workingDir + cols/rows），返回句柄 `pty-<uuid>`，不挂钩 SessionManager / 会话配置 / 前端 UI——插件私有资源，业务语义零携带（ADR 0022）；
- **输出面走「单生产者环形缓冲 + 插件拉取游标」**（`ring-fetch(pty-id, from-offset, max-bytes)`）：复用 2026-09-17 pty-pull-subscribers 的背压教训——生产端零暂停，慢插件只损失自己的 ring 历史（满淘汰最旧 + truncated 标记），绝不把背压踢回 PTY 读取端；**不做 push 回调**（WASM 同步调用模型无法被异步唤醒，push 通知语义上无意义）；
- **属主隔离**：全部函数仅属主可调（`not owner of pty handle`）；插件停用宿主自动 `purge_for_plugin`（kill 全部本人 PTY + 摘注册表 + 补发退出事件）；
- **生命周期事件**：进程退出 → 宿主发布 owner 作用域 topic `pty:exit.<owner>`（payload 含 ptyId/reason，exitCode 视 PtySession 能力扩展而定，见 D4）；
- **权限两域**：`pty:spawn`（spawn/kill，创建型高风险面）+ `pty:io`（write/resize/ring-fetch/is-running，数据面），可独立授予/审计。

## User Stories

1. 作为 auto-task 插件的开发者，我想让插件创建一个独立的交互式 PTY 运行命令，以便插件能在自己的沙箱里执行任意脚本并逐帧读取输出，而不污染宿主终端会话列表。
2. 作为 agent-hub 插件的开发者，我想在插件里 spawn 一个 shell 并保持长会话（多次 write / 多次拉取输出），以便实现"给模型一个可对话的 shell"这一产品能力。
3. 作为 ai-chatbox 插件的开发者，我想在插件里运行 TUI 程序（vim、top、htop），以便让助手能操作交互式工具；这些程序在非 PTY 环境下因缺 TTY 行为而无法运行。
4. 作为插件开发者，我想在创建 PTY 时指定 cols/rows，并在需要时 resize，以便以正确的终端尺寸渲染 TUI 程序的输出（全屏刷新对齐）。
5. 作为插件开发者，我想以字节流（list<u8>）读写 PTY（而不是字符串），以便处理非 UTF-8 输出、二进制协议与任意转义序列。
6. 作为插件开发者，我想用 offset 游标增量拉取输出历史（ring-fetch），以便以自己可控的节奏消费输出，不会因输出风暴被宿主流控或阻塞。
7. 作为插件开发者，我想在拉取时知道我的 offset 是否落后于环形缓冲起点（truncated 标记），以便检测到输出缺口并按 resync 语义重建上下文。
8. 作为插件开发者，我想收到我的 PTY 进程退出的通知（任意原因：正常退出 / 被 kill / 错误），以便清理插件侧资源并推进任务状态机。
9. 作为插件开发者，我想查询 PTY 是否仍在运行（is-running），以便在丢失事件（插件停用重激活、bus 订阅窗口外）后自愈。
10. 作为插件开发者，我想确认**他人创建的 PTY 句柄对我不可用**（not owner 错误），以便明确"我的 PTY 只能我操作"这一隔离契约。
11. 作为插件开发者，我想插件被停用时宿主自动 kill 我的全部 PTY，以便不泄漏进程（孤儿进程托管在宿主，不随插件消失而悬挂）。
12. 作为插件开发者，我想按需申请 `pty:spawn` 与 `pty:io` 两种权限，以便最小授权（例如只做数据观测的插件可只申请 pty:io 前缀合集之外的最小面）。
13. 作为宿主用户，我想插件的 PTY 操作经过权限门禁（manifest 声明 + 宿主最终仲裁），以便插件不能未经授权就在我的机器上 spawn 任意命令。
14. 作为宿主开发者，我想 host-pty 的 spawn 只接受**裸命令 + 参数数组**（不做 shell 包装 / WSL 转换 / 危险字符校验），以便参数数组 exec 天然免注入，且业务性 shell 包装留在插件层（ADR 0022 裁剪线）。
15. 作为插件开发者，我想在具备 PTY 但共享同一宿主执行引擎的前提下，让插件 PTY 与宿主业务会话**互不可见**（不同注册表、不同生命周期、不互调），以便业务线（远程终端）零感知本次改造。
16. 作为插件开发者，我想在 SDK 里拿到类型化的 host-pty 包装函数与 topic 构造助手，以便不手写 JSON 协议、不猜事件名。
17. 作为 SDK 维护者，我想一次定稿 host-pty 全部函数签名（含未来读接口），以便 ABI 只在 v16 bump 一次，旧插件不受影响。
18. 作为测试工程师，我想有一条 fixture 演示插件把 WIT → 宿主实现 → 组件接线 → 权限 → SDK → 真 PTY 行为全链路跑通（spawn→拉取→写→resize→kill→退出事件→权限拒绝→属主隔离→停用回收），以便用最高 seam 证明能力闭环。

## Implementation Decisions

### D1 裁剪线判定（ADR 0022）：PTY 是标准引擎原语

「离宿主无法实现」：WASI 0.2（wasip2）无 PTY 接口，wasmtime 48 默认 deny `wasi:sockets`/设备访问，`portable-pty` 是宿主独占依赖（桌面依赖树已有），插件（WASM 沙箱）物理上不可能自建伪终端——必须宿主提供。「零业务语义」：spawn 一个裸伪终端 = command/env/cwd/尺寸，不表达会话、配置、UI、设备连接任何产品概念。对照 `host-process`（非交互 run/kill）——host-pty 是它的「交互式流」补集；进程执行既有设施（`process.rs::create_command` 等）尽量复用。

### D2 业务隔离（需求①，形式化）

- **句柄属主隔离**：全局注册表 `LazyLock<Mutex<HashMap<pty_id, PtyEntry>>>`，`PtyEntry { session: PtySession, owner: String, ring: PtyRing, ... }`；全部函数先查属主（`entry.owner != plugin_id` → `not owner of pty handle`，同 mdns `NOT_OWNER` / ws 先例）。
- **停用回收**：`deactivate_plugin_inner` 追加 `pty::purge_for_plugin(plugin_id)`——kill 该插件全部 PTY（先 Ctrl-C 优雅再强杀，复用 PtySession::kill 语义）、摘注册表、补发 `pty:exit.<owner>`（reason=killed）；只碰本人。
- **零接入业务线**：插件 PTY 不进入 `SessionComponents`（业务会话注册表）、不注册 `GlobalOutputManager`、不参与业务会话生命周期事件链（session_event / event_bus）；与 `terminal-hooks` / `host-terminal` / `host-session` 的边界在文档中写死：后三者服务「宿主业务会话线」，host-pty 只服务「插件私有 PTY」，互不交叉。
- **spawn 参数不复用 `SessionLaunchConfig`**（其 `name` 字段是业务语义、`environment` 枚举含业务 shell 包装）。`spawn` 收纯引擎 config-json。

### D3 输出模型（关键定案）：纯拉取，不做 push 回调

否决「可选导出 events-pty（push 回调）」方案，两条理由：

1. **背压先例**（2026-09-17 pty-pull-subscribers 重构的动因）：推送会把背压踢回生产端；慢消费者应只影响自己。宿主已为业务会话把「单生产者环 + 每订阅者游标」建成正确形态，host-pty 沿用同一形态。
2. **WASM 执行模型**：插件是同步调用栈（wasmtime Store 不可重入），宿主无法异步唤醒插件；push 通知只能在插件下一次被调用时送达，语义上等于"轮询但多一层回调"，徒增宿主面复杂度与高频 guest 调用。

定案：宿主为每个插件 PTY 维护**有界环形缓冲 `PtyRing`**（单生产者 = PTY 读取端，offset 单调递增，满淘汰最旧），插件以自己节奏调用 `ring-fetch(pty-id, from-offset, max-bytes)` 拉取。`from-offset` 落后于环起点（数据被淘汰）→ 返回当前可读最早段 + `truncated: true`（resync 语义，对齐 pty-pull-subscribers §1.5：检测到缺口即重建上下文）。生产端零暂停、零感知订阅者（同 `PtyReader`「源零等待」原则）。

PtyRing **自持实现（~100 行，VecDeque + 全局 offset），不抽取/复用 `SessionOutputManager`**：2026-09-17 刚重构的业务链路不背回归风险，且二者生命周期不同（业务 ring 随业务会话、插件 ring 随 pty 句柄）；「抽象提取候选」记录进 ADR。

### D4 生命周期与退出事件

- 进程退出（任意原因）→ host-pty 守护订阅 `PtySession::subscribe_lifecycle()`（票 01 后载荷为 `broadcast<PtyTerminated>`：`{ status: Stopped|Error, exit_code: Option<i32>, killed: bool }`）→ 发布 **owner 作用域 topic `pty:exit.<owner>`**（同 `ws:close.<owner>` 模式；payload `{ ptyId, reason: "stopped"|"killed"|"error", exitCode? }`，camelCase）→ 摘除注册表、释放 ring。**exit 事件即摘除**：插件应在看到输出结束（`ring-fetch` nextOffset 不再前进）后消费完再等退出事件，竞争窗口在文档写清（不退让，不做延迟保留）。
- **exitCode（实施确认点）**：`PtySessionStatus` 当前不带退出码，`PtySession` 无 exit status 暴露。实施时优先在 `PtySession` 层补等待/收集 `portable-pty` `ExitStatus` 的能力并把 exitCode 随事件带出；若 portable-pty 绑定限制导致不可行，降级为不带 exitCode 的事件并在票 01（能力侧）/ 票 04（事件侧）Comments 记录原因。不得为凑 exitCode 引入轮询 waitpid。
  - **票 01 实施修正（2026-09-19，用户裁决 B）**：① exitCode **可取**（`Child::wait()` 阻塞回收，`ExitStatus::exit_code()`），不降级；但 `ExitStatus` 无 signal 访问器，信号终止也报 `code=1`，故「被杀 vs `exit 1`」由宿主侧 `kill()` 请求位承担（`PtyTerminated.killed`），票 04 据此映射 `reason`。② **本 D4 的「进程退出 → 读线程 EOF」前提实测不成立**：父进程持有 `pair.slave` 时内核不让 master 读返回 EOF，自然退出不可观测（业务线今天同样如此，只有 kill/销毁才翻 Stopped）。落地为 `PtySlaveFdPolicy`：业务 = `Hold`（现役语义，票 01「零变化」要求），**插件私有 PTY = `ReleaseOnSpawn`**（`PtySession::with_private_sink`，spawn 后释放 slave fd，退出即 EOF）。票 02/04/05 一律走 `with_private_sink`，不得改用业务构造器。③ 终态事件由 `PtyTerminationGate` 保证「EOF + 回收」齐备后恰好一条，因此「exit 事件即摘除、ring 不再增长」对插件 PTY 成立。
  - **票 04 实施定形（2026-09-19）——单一发布者不变量**：本 D4 与 D6 分别写了「kill → 摘注册表 + 发布事件」和「退出监听 → 摘除 + 发布」，两条路径交汇会双发。落地为：**`kill()` 只发起终止**，句柄摘除与事件发布统一由 spawn 时起动的退出监听在终态（EOF + 回收）齐备时完成，且**只有从注册表 `remove` 成功的那一方**才发布；停用回收先摘除、后 kill、再补发（其监听醒来看不到句柄即静默结束）。故自然退出 / 主动 kill / deactivate 三条路径下每条 PTY 恰好一条 `pty:exit`。附带硬约束：监听任务必须派生到 **ambient runtime**——WASI 预打开模式下宿主函数跑在无 runtime handle 的阻塞线程上，直接 `tokio::spawn` 会 panic 并穿透污染 wasmtime Store。
- 自愈：插件在 `pty:exit.<owner>` 之外可随时 `is-running` 快照查询（对齐 ws `is-connected` 自愈哲学）；bus 不缓冲不重放，晚订阅期间的事件丢失靠快照自愈。

### D5 spawn 最小原语（config-json，camelCase）

```
spawn config-json: {
  command: string,          // 可执行文件（路径或 PATH 内名称），宿主 CommandBuilder::new(command)
  args?: string[],          // 参数数组；数组 exec 天然免注入，宿主不做 shell 解析
  env?: { [k: string]: string } | null,
  workingDir?: string | null,
  cols?: u16,               // 默认 80
  rows?: u16,               // 默认 24
} -> result<string, string> // "pty-<uuid>" 句柄；失败只回错误，不发任何事件（无句柄可寻址）
```

**明确不做**（业务语义，归插件层，符合 ADR 0022）：bash -lic / PowerShell `-Command` / CMD `/K` 包装、WSL 路径转换（`windows_to_wsl_path`）、CMD 危险字符校验、默认 shell 探测、`name` 标识（插件用 ptyId 自管）。`host-process` / `host-platform` / `host-fs` 的组合足够插件自拼命令。尺寸默认与业务线 `default_cols/rows` 常量同源即可。

### D6 读写与查询

- `write(pty-id, data: list<u8>) -> result`：复用 `PtySession::write` 的 4000 字节分块 + yield 逻辑；超上限（`PLUGIN_PTY_MAX_WRITE_BYTES`）拒绝。
- `resize(pty-id, cols, rows) -> result`：透传 `PtySize`。
- `kill(pty-id) -> result`：属主校验 → `PtySession::kill`（优雅 Ctrl-C + exit 兜底强杀，复用既有实现）→ 摘注册表 + 发布 `pty:exit.<owner>`（reason=killed）。
- `is-running(pty-id) -> result<bool>`：`PtySession::is_running`。
- **不做** `send-special-key`（键盘组合属终端业务语义）、不做 close/reopen（kill 即销毁）。

### D7 WIT 定稿（host-pty，一次写全，ABI v16）

```
interface host-pty {
    /// 创建裸 PTY（D5 config-json）；成功 → "pty-<uuid>" 句柄并登记属主；
    /// 失败 → 错误上抛且不发布任何事件
    spawn: func(config-json: string) -> result<string, string>;
    /// 写入输入（分块语义宿主内建，D6）
    write: func(pty-id: string, data: list<u8>) -> result<_, string>;
    resize: func(pty-id: string, cols: u16, rows: u16) -> result<_, string>;
    /// 终止并销毁（优雅→强杀，D6）；缺省 reason=killed 的 pty:exit 事件
    kill: func(pty-id: string) -> result<_, string>;
    /// 拉取输出（D3）：from-offset 单调递增；落后于环起点 → truncated=true
    ring-fetch: func(pty-id: string, from-offset: u64, max-bytes: u32) -> result<option<ring-fetch-result>, string>;
    is-running: func(pty-id: string) -> result<bool, string>;
}
record ring-fetch-result {
    data: list<u8>,
    next-offset: u64,
    truncated: bool,
}
```

- **事件面**：仅 `pty:exit.<owner>` 一条，走 host-bus 订阅（topic 内嵌 owner，非属主物理订阅不到；SDK 提供 `pty_event_topic(event, plugin_id)` 助手 + `PTY_EXIT` 常量 + 订阅时序硬提示：activate 期订阅、不重放、自愈靠快照）。无状态事件（spawn 成败在返回值里）、无错误事件（错误上抛即 Err）。
- **ABI**：desktop v14 → **v16**，纯增量 `world plugin` 加一个 import，不比改既有函数；既有插件零迁移（宿主侧 `version > 当前 → 拒绝` 兼容旧插件）。`abi.interface` 版本演进注释补 v16，并注明 **v15 已预留给认证中心线的 `host-auth`**（两条线不撞号；host-auth 先落 v15，host-pty 落 v16）。
- 复用 `list<u8>` 直传（events-binary / publish-binary / ws send-binary 先例），不做 base64/JSON 包装。

### D8 权限与同步点（5 处，缺一不可）

`pty:spawn` = PERMISSION_PTY_SPAWN、`pty:io` = PERMISSION_PTY_IO（按域拆分，对齐 `ws:client/ws:server` 哲学；spawn = 任意命令执行高风险面，可独立授予/审计）。五同步点与 ws 完全同构：

1. SDK `permission.rs`：常量 + `VALID_PERMISSIONS` + `PERMISSION_API_MAP`（`pty.spawn/kill`、`pty.write/resize/ringFetch/isRunning`）；
2. 打包 CLI（`packages/plugin-sdk-desktop/bin/cli.js`）合法权限集合；
3. 前端 `src/plugin/permission.ts` 合法集合 + API 映射（WASM-only，映射为空）；
4. 宿主能力清单 `capability.rs`：`host-pty` 一组（现 19 组 → 20 组，以落地时实际为准）；
5. `host_impl/pty.rs` 权限门（`check_permission` 统一守卫 + 结构化日志）。

### D9 限制常量（`system/constants/plugin.rs`，命名对齐 `PLUGIN_WS_*`）

- `PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN`（每插件在册 PTY 上限，默认 8）；
- `PLUGIN_PTY_RING_BYTES`（每 PTY 环形缓冲容量，起步对齐业务会话环容量同量级，实施时取现役值）；
- `PLUGIN_PTY_RING_FETCH_MAX_BYTES`（单次 ring-fetch 最多返回字节，限制单次 wasm 边界拷贝，默认 16 KiB）；
- `PLUGIN_PTY_MAX_WRITE_BYTES`（单次 write 上限，默认与 4000 分块语义协调，如 64 KiB）。

超限行为：创建超限 → Err（fail-visible，不静默降级）；ring 满 → 淘汰最旧 + truncated（数据面，符合背压设计）。

- **票 05 实施定形（2026-09-19，用户裁决）——ring 容量改为声明式参数**：`PLUGIN_PTY_RING_BYTES` 的「起步对齐业务会话环容量同量级、取现役值」不成立：业务侧 `channels.global_queue_max_bytes` 是**每条会话队列 50 MB**，而插件环随 pty 句柄存活、每插件可到 8 条，字面对齐即单插件最坏 400 MB 常驻。落地为：`spawn` config 新增 `ringBytes?`（省略取宿主默认 256 KiB），宿主以 `PLUGIN_PTY_RING_MAX_BYTES`（4 MiB）仲裁上限，`0` / 越界一律 `Err`（**不静默夹取**）；单插件常驻上界由「条数 × 容量上限」= 32 MiB 表达。另补环的条目上限（`PtyRing::DEFAULT_MAX_CHUNKS`，碎块防御，与业务环 `global_queue_max_chunks` 同惯例）。
- **读侧截断 ≠ 写侧拒绝**：`PLUGIN_PTY_RING_FETCH_MAX_BYTES` 是「本次最多返回」——读侧超上限**截断 + `next-offset` 续拉**（数据面正常语义，`Err` 反而破坏流式消费）；`PLUGIN_PTY_MAX_WRITE_BYTES` 才是拒绝语义（把半条命令喂进交互进程比失败更糟）。票据措辞里「单次读/写超限 → Err」按此分裂执行。

### D10 宿主实现形态（同 `host_impl/mdns.rs` / `host_impl/ws.rs`）

- 新增 `host_impl/pty.rs`：`LazyLock<Mutex<HashMap<String, PtyEntry>>>` 注册表 + 上述函数域 + `purge_for_plugin`；spawn 后启动读取任务（复用 `PtyReader`——读线程 → PtyRing 写入，生产端零暂停）与退出监听任务（subscribe_lifecycle → 发布 `pty:exit.<owner>` → 摘除）。
- `component.rs`：`impl bedcode::plugin::host_pty::Host`（6 函数转发）+ `host_pty::add_to_linker` 接线 + verify_abi。
- `host.rs`：`deactivate_plugin_inner` 追加 `pty::purge_for_plugin(plugin_id)`（紧邻 ws 回收）。
- 事件发布走既有 host-bus 核心 publish 路径（与 mdns `publish_dir_event` 同路）。

### D11 SDK（Rust，桌面 `packages/plugin-sdk-desktop`）

- `host/pty.rs`：`HostPty` trait（6 函数）+ `spawn_pty_json` 之类帮助函数 + `PTY_EXIT` 常量 + `pty_event_topic(event, plugin_id)` 助手 + 文档注释（camelCase 字段、ring 语义、轮询时序、activate 期订阅、truncated/resync、exit 即摘除的消费窗口）。
- `wasm_pty.rs` 绑定 + `lib.rs` 模块声明（`generate!(world: "plugin")` 内已有 import，无新 world）。**无可选导出**（D3 否决 events-pty）。

### D12 双端（移动端）

移动端**全不动**：移动端是远程终端控制端，不承载 PTY 引擎；`host-pty` 为桌面独有，移动端 WIT / ABI / SDK 不跟着演进（AGENTS §7「改 WIT 双端同步」的**文档化偏离**，同 ws-base-service、wasmtime-48 分叉先例；ADR 0018/0019 各自演进）。SDK 双端独立包（`plugin-sdk-desktop` / `plugin-sdk-mobile`），互不影响。

## Testing Decisions

**好测试的定义**：只验证外部行为（权限门禁、属主隔离、ring 语义、退出事件、回收），不测实现细节（不测注册表内部字段布局、不 mock PtyReader 内部）。真 PTY 行为（spawn 后 shell 真的能跑、write 真的进进程、kill 真的退出）必须用真实 `portable-pty`（Linux 临时目录 + 短命命令，`pty_process.rs` 已有 `linux_config("sleep 1")` 先例），禁止用 mock 替代 PTY 本体。

**测试层级与 seam**（自高到低）：

| seam | 内容 | 先例 |
| --- | --- | --- |
| ① fixture WASM 插件 e2e（最高 seam，一次贯通） | `bedcode-desktop/packages/plugin-pty-test`（与 `plugin-ws-test` 同层级同惯例）：spawn → ring-fetch 断言输出 → write 输入回读 → resize（验证无错 + 可查）→ kill → 收 `pty:exit.<owner>` → 权限拒绝（未授权插件 spawn → permission denied）→ 属主隔离（他人句柄 → not owner）→ 停用回收（deactivate 后进程必须消失、注册表空） | ws 票 04/05 fixture 闭环（`test_ws_*` 回环 e2e） |
| ② 宿主实现单测（`host_impl/pty.rs`） | `build_host_ctx` 模式：权限门（grant/未 grant/错权限三态）、属主隔离、注册表操作、purge_for_plugin 只碰本人、ring 语义（写入→拉取→offset 推进→淘汰→truncated）、容量上限拒绝 | `host_impl/mod.rs::tests`、`pty_process.rs` linux_config 真 PTY 测试 |
| ③ SDK 校验 | `cargo test`（常量/助手/topic 生成）+ `cargo check --target wasm32-unknown-unknown --features wasm` | ws SDK 验证 |
| ④ 收尾门禁 | 桌面 `cargo test` 全绿；无前端改动则无 vitest/eslint 门禁；grep 确认无 `let _ =` 静默错误、错误带上下文（AppError 规范） | AGENTS §10 |

**不做**：无移动端测试（移动端零改动）；不做前端测试（无前端改动）。

## Out of Scope

- **移动端 host-pty / WIT / ABI / SDK**（远程终端端无 PTY 引擎；文档化偏离，§12）。
- **宿主业务会话线的 PTY 服务化**：`session/`（SessionManager / SessionComponents / GlobalOutputManager / session_event）保持宿主业务私有，仅新增插件私有 PTY 原语；「会话配置建 PTY」仍走既有 `host-session`。
- **shell 包装 / WSL 转换 / 危险字符校验 / 默认 shell 探测 / 特殊键 API**（D5/D6，业务语义归插件层）。
- **push 输出模型（events-pty 可选导出）**（D3 否决）。
- **退出码收集的复杂方案**（polling waitpid 等；D4 限定了收集边界，不可行则降级）。
- **PTY 权限申请 UI / 弹窗授权**（走既有权限门与前端集合，不新增 UI）。
- **移动端远程终端通过宿主 PTY 能力复用**（跨端业务编排，属未来插件/路线话题）。
- **`PtyRing` 与 `SessionOutputManager` 的代码级抽取合并**（自持实现；候选记录进 ADR，不在本期动业务链路）。

## Further Notes

- **阶段对照**：`plugin-kernel-roadmap` §5 内核清单含「进程（PTY）」——本 spec 是其服务化原语化落地的第一步（host 原语形态，业务编排仍留插件层）。
- **前例复用清单**：mdns（NOT_OWNER / 双表回收 / owner 作用域 topic）、ws（权限五同步点 / ABI bump 一次 / 常量风格 / 自愈哲学 / 排队语义 fail-visible）、pty-pull-subscribers（背压：单生产者环 + 游标 + truncated/resync）、db-http（v14 内函数级追加惯例——**注意**：host-pty 是新接口，走 **ABI bump v16**，不是函数级追加）。
- **风险**：`PtySession` 增能力（exitCode）与 PTY 读取线程输出汇解耦是前置地基（票 01），若降级按 D4 记录原因不阻塞主线；共享文件（host 接线 / 能力清单 / deactivate 回收）与认证中心线的 host-auth 票存在并发编辑可能——开工前确认对侧不在途。
- **票据拓扑（2026-09-19 纵向重切，7 张）**：01（引擎前置，frontier）→ 02（契约 + ABI v16 + spawn/ring-fetch 贯通）→ **03 / 04 / 05 并行**（数据面 / 生命周期 / 背压限额，均只依赖 02）→ 06（隔离与 e2e 契约矩阵，依赖 03+04+05）→ 07（文档与 ADR 收口）。原 01–05 横向分层票（契约/宿主/SDK/fixture/文档）作废替换。
- **收口状态（2026-09-19）**：票 01-07 全 done。票 03/04/05 未完全按「并行」执行——票 04 与票 05 都要动 `host_impl/pty.rs`，实际由同一条会话串行落地（票 04 前半程由并发会话写入后由本会话接手收尾，见票 04 Comments「交接事实」）；票 05 的环容量按用户裁决从「常量对齐业务现役值」改为「spawn 声明参数 + 宿主上下限仲裁」，票 06 的隔离矩阵据此补了 `ringBytes` 的 WIT 端到端分格。

## 参考

- `bedcode-desktop/src-tauri/src/pty/`（pty_process.rs / pty_reader.rs / pty_handler.rs / command.rs / wsl.rs）
- `bedcode-desktop/src-tauri/src/session/session_output.rs`（GlobalOutputManager / SessionOutputManager：环 + 游标 + snapshot_bytes）
- `bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit`（host-process / host-terminal / host-websocket / events-binary 形态）
- `docs/adr/0022-plugin-host-interface-primitive-boundary.md`（裁剪线）
- `.scratch/2026-09-17-pty-pull-subscribers/spec.md`（背压/游标/resync 教训）
- `.scratch/2026-09-18-ws-base-service/spec.md` 与 `.scratch/2026-09-10-mdns-service-plugin/spec-basic-capability-service.md`（同构模板）