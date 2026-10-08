# 架构图 / 流程图（docs/diagrams）

仓库内所有**架构图、数据流图、流程图**的自包含 HTML 交付物统一存放于此目录。

每个 HTML 都是单文件可交付产物（内联 SVG + Viewer Runtime），双击即可在浏览器打开，支持深色 / 浅色主题切换、节点聚焦、路由追踪、导出分享卡。可离线分发、可直接贴进 README。

## 目录约定

| 约定 | 说明 |
| --- | --- |
| 命名 | `<主题>-<范围>.<ext>`，kebab-case，例：`plugin-ai-chatbox-desktop.html` |
| 类型 | `architecture`（架构拓扑）/ `dataflow`（数据流链路） |
| IR 源 | 有 `.json` 同伴文件的，JSON 是 Archify IR（HTML 由它渲染）；**改 IR 后必须重新渲染 HTML** |
| 预览图 | `*.png` 仅用于 README 静态嵌入，HTML 才是权威版本 |
| 新增 | 新图直接放本目录；同时更新本索引与相关 README 链接 |
| 不入库内容 | `.scratch/<task>/` 下的 UI 原型（splash、prototype、icon preview、bug repro）属开发过程产物，**不放这里** |

## 图清单

### 全局架构

| 图 | 类型 | 主题 | IR 源 |
| --- | --- | --- | --- |
| [bedcode-overall-architecture.html](./bedcode-overall-architecture.html) | architecture | 桌面主机 · 移动端远程 · WASM 沙箱 · wasm-core 微内核与能力域 crate 家族（ADR 0037 整核抽出 + ADR 0035/0039 能力域 crate 化 + ADR 0031/0033 认证中心 fail-closed · ABI desktop v34 / mobile v16） | [bedcode-overall-architecture.json](./bedcode-overall-architecture.json) |

静态预览：[bedcode-overall-architecture.png](./bedcode-overall-architecture.png)

### wasm-core 模块架构与扩展点（ADR 0022 · ADR 0037）

`bedcode-wasm-core` 引擎内核的模块拓扑与 5 个扩展点。内核只做机制（应用无关的通用引擎：进程 / 网络 / 存储 / 安全 / 通信 + wasmtime 48），业务事实真源全部下沉插件（ADR 0022 无业务内核红线）。4 闸门（能力 / 身份 / 隔离 / 生命周期）4 通道（WIT host-* / bus / events / 互调）构成宿主↔插件边界；5 个扩展点（引擎钩子 / 能力 crate / 权限词汇 / 插件贡献 / SystemComponent）是插件与内核的契约面：

| 图 | 类型 | 覆盖范围 | IR 源 |
| --- | --- | --- | --- |
| [wasm-core-modules-extension.html](./wasm-core-modules-extension.html) | architecture | 引擎内核 9 模块（PluginHost / WasmRuntime / API 桥接 / LoaderRegistry / host_api 实现 / CapabilityRegistry / 引擎真源 / 安全框架 / 能力安装）+ 依赖底座 4 crate（runtime SDK / plugin-api-wasm / bedcode-host-kit / server-base）+ 扩展点 5 面（引擎钩子 / 能力 crate / 权限词汇 / 插件贡献 / SystemComponent）· 13 条关系线覆盖装配 / 实例化 / 加载 / impl Host / check_permission / 授权 / wasmtime 48 / Ports 下发 / on_load / check_permission / submit_module! 全链路 | [wasm-core-modules-extension.json](./wasm-core-modules-extension.json) |

关键代码锚点：`packages/bedcode-wasm-core/src/`（`manager.rs` PluginManager + `manager/loader.rs` 加载器 + `manager/runtime.rs` WasmRuntime + `manager/host.rs` Host 装配 + `manager/registry.rs` 注册表 + `manager/capability.rs` 能力注册/安装 + `host_api.rs` 命令路由 + `host_api/http.rs` + `host_api/pty.rs` + `host_api/storage.rs` + `host_api/auth.rs` 授权 + `permission.rs` 权限门禁 + `security.rs` 安全框架 + `db.rs` 主库 + `storage.rs` 存储 + `system.rs` SystemComponent）

### 会话真源下沉（ADR 0022）

会话登记 / 状态机 / 生命周期分发 / 输入输出编排从宿主迁入 `com.bedcode.terminal-session` 私有登记域，宿主侧只剩 PTY 引擎、`host-pty` 原语与 `utils/session_gateway.rs` 窄转发层：

| 图 | 类型 | 覆盖范围 | IR 源 |
| --- | --- | --- | --- |
| [session-source-flow-desktop.html](./session-source-flow-desktop.html) | sequence | 桌面端：创建 / 输入 / 关闭 / 生命周期四条调用链的插件背书与 fail-visible 判据 | [session-source-flow-desktop.json](./session-source-flow-desktop.json) |

### 插件架构（桌面 / 移动双端）

| 插件 | 桌面端 | 移动端 |
| --- | --- | --- |
| ai-chatbox | [plugin-ai-chatbox-desktop.html](./plugin-ai-chatbox-desktop.html) | [plugin-ai-chatbox-mobile.html](./plugin-ai-chatbox-mobile.html) |
| file-transfer | [plugin-file-transfer-desktop.html](./plugin-file-transfer-desktop.html) | [plugin-file-transfer-mobile.html](./plugin-file-transfer-mobile.html) |

桌面端 `auto-task` 已并入 `terminal-session` 的任务域（spec D8-P4，`bedcode-desktop/wasm-apps/terminal-session/rust/src/task/`），不再有独立桌面端架构——`plugin-auto-task-desktop.html` 已随该合并删除；桌面端业务应用另含 `agent-hub`（Agent CLI 统一管理台，无独立架构图，随整体架构中的 `wasm_apps` 组块呈现）。移动端 `auto-task` 已于 2026-10-08 并入移动端 `terminal-session`（D6 选项 A，`bedcode-mobile/plugins/terminal-session/src/task/`），`plugin-auto-task-mobile.html` 保留为合并前快照（移动契约 ADR 0018）。

各插件 README 的「架构图」链接均指向本目录对应文件。

### 任务自动化链路（原 auto-task，已并入 terminal-session）

原 `com.bedcode.auto-task` 桌面端业务应用已并入 `com.bedcode.terminal-session` 的任务域（spec D8-P4，`bedcode-desktop/wasm-apps/terminal-session/rust/src/task/`），旧的 `auto-task-automation-flow.html` 已随该合并删除；下图为并入后的完整链路——入口面 → 插件 HTTP 端点 → 双调度状态机 → `launch` 会话编排 → `host-pty` 执行 → Agent hooks 回调 → 私有库真源 → `host-events` 广播（广播终点是移动端 500 ms 去抖全量重拉，不在图内画回流线，理由见图中卡）：

| 图 | 类型 | 覆盖范围 | IR 源 |
| --- | --- | --- | --- |
| [task-automation-flow-desktop.html](./task-automation-flow-desktop.html) | dataflow | 桌面端：入口面 → 插件 HTTP 端点（`task-queue/*` · `scheduled-jobs/*`）→ 定时任务域 / 队列调度域 → `launch` 编排 → `host-pty.spawn` → Agent CLI 会话 → hooks 回调 → 状态机真源（`task_history` 幂等去重）→ `host-events` 广播 | [task-automation-flow-desktop.json](./task-automation-flow-desktop.json) |

关键代码锚点：`bedcode-desktop/wasm-apps/terminal-session/rust/src/task/`（`queue.rs` 队列调度 · `scheduled.rs` 定时调度 · `state.rs` 状态机与 `handle_update_task_status` 幂等 · `hooks.rs` Agent 集成注入 · `agent.rs` agent 识别 · `preset.rs` 预设）、`rust/src/launch.rs`（会话编排与 `create_via_host`）、`bedcode-desktop/wasm-apps/terminal-session/src/components/TaskQueueModal.vue` + `TaskHistoryView.vue`（桌面端任务面板）、`bedcode-mobile/plugins/terminal-session/src/task/`（移动端只读视图 + 500 ms 去抖全量重拉）

### PTY 输出数据流

终端 PTY 输出的端到端链路。桌面本地路径自 2026-09-17 起为**拉取模型**（PtyRing + 游标续拉），旧推模型三件套（`SessionOutputManager` / `UnifiedOutputQueue` / `SubscriberState` + `forward_loop`）与前端 Tauri Channel 桥、ack 反馈环均已整体下线：

| 图 | 类型 | 覆盖范围 | IR 源 |
| --- | --- | --- | --- |
| [pty-output-flow-desktop.html](./pty-output-flow-desktop.html) | dataflow | 桌面端：shell → `PtySession` master fd → `PtyReader`（独立读线程）→ `PtyRing`（有界环形 · `min_offset`）→ `host-pty` 原语（16 KiB 钳位 · 属主仲裁）→ `terminal-session` 输出域 → 前端拉取循环（50 ms / 250 ms 双档，见 `terminalPullPolicy.ts`）→ `xterm.js`，含双水位迟滞回环 | [pty-output-flow-desktop.json](./pty-output-flow-desktop.json) |
| [pty-output-flow-mobile.html](./pty-output-flow-mobile.html) | dataflow | 移动端：`TerminalLink`（TB v3 解析 + ack 节流）→ `SessionCache`（16MB LRU）→ `terminalBuffer` → `writeCoalescer` → `xterm.js`，含 ack 反馈环 | [pty-output-flow-mobile.json](./pty-output-flow-mobile.json) |

关键代码锚点：`bedcode-desktop/src-tauri/src/pty/pty_reader.rs` + `pty/pty_ring.rs`（输出环）、`bedcode-desktop/src-tauri/src/plugin/` 下 `host-pty` 原语实现、`bedcode-desktop/wasm-apps/terminal-session/rust/src/output.rs`（拉取接口 `session.output.pull`）、`bedcode-desktop/wasm-apps/terminal-session/src/components/terminal/TerminalPreview.vue`（轮询档位）、`bedcode-desktop/src-tauri/src/server/websocket/subscription.rs` + `terminal_ws/`（远程订阅，同为拉取模型执行体 + 流代数门控）、`bedcode-desktop/src-tauri/src/utils/session_gateway.rs`（宿主↔插件窄转发层）、`bedcode-mobile/plugins/terminal-session/rust/src/link.rs`（订阅协议客户端，自退役的 `terminal_link.rs` 迁入）+ `bedcode-mobile/src-tauri/src/terminal_stream_gateway.rs`（输出窄转发）、`bedcode-mobile/src/composables/useTerminalBuffer.ts`、`bedcode-mobile/src/composables/writeCoalescer.ts`、`bedcode-mobile/src/stores/terminalBuffer.ts`

## 相关

- 领域模型 / 术语：仓库根 `CONTEXT.md`
- 架构决策：[`docs/adr/`](../adr/)
- 代码地图：[`bedcode-desktop/docs/code-map.md`](../../bedcode-desktop/docs/code-map.md) / [`bedcode-mobile/docs/code-map.md`](../../bedcode-mobile/docs/code-map.md)
- 日志与排障：[`docs/knowledge/logging.md`](../knowledge/logging.md)
