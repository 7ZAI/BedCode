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
| [bedcode-overall-architecture.html](./bedcode-overall-architecture.html) | architecture | 桌面主机 · 移动端远程 · WASM 插件沙箱 · 内核边界（P1-b 后 `wasm_core` 五模块 + `host-*` 能力域 + 会话真源下沉） | [bedcode-overall-architecture.json](./bedcode-overall-architecture.json) |

静态预览：[bedcode-overall-architecture.png](./bedcode-overall-architecture.png)

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
| auto-task | [plugin-auto-task-desktop.html](./plugin-auto-task-desktop.html) | [plugin-auto-task-mobile.html](./plugin-auto-task-mobile.html) |

各插件 README 的「架构图」链接均指向本目录对应文件。

### PTY 输出数据流

终端 PTY 输出的端到端链路。桌面本地路径自 2026-09-17 起为**拉取模型**（PtyRing + 游标续拉），旧推模型三件套（`SessionOutputManager` / `UnifiedOutputQueue` / `SubscriberState` + `forward_loop`）与前端 Tauri Channel 桥、ack 反馈环均已整体下线：

| 图 | 类型 | 覆盖范围 | IR 源 |
| --- | --- | --- | --- |
| [pty-output-flow-desktop.html](./pty-output-flow-desktop.html) | dataflow | 桌面端：shell → `PtySession` master fd → `PtyReader`（独立读线程）→ `PtyRing`（有界环形 · `min_offset`）→ `host-pty` 原语（16 KiB 钳位 · 属主仲裁）→ `terminal-session` 输出域 → 前端拉取循环（100 ms / 500 ms 双档）→ `xterm.js` | [pty-output-flow-desktop.json](./pty-output-flow-desktop.json) |
| [pty-output-flow-mobile.html](./pty-output-flow-mobile.html) | dataflow | 移动端：`TerminalLink`（TB v3 解析 + ack 节流）→ `SessionCache`（16MB LRU）→ `terminalBuffer` → `writeCoalescer` → `xterm.js`，含 ack 反馈环 | [pty-output-flow-mobile.json](./pty-output-flow-mobile.json) |

关键代码锚点：`bedcode-desktop/src-tauri/src/pty/pty_reader.rs` + `pty/pty_ring.rs`（输出环）、`bedcode-desktop/src-tauri/src/plugin/` 下 `host-pty` 原语实现、`bedcode-desktop/wasm-apps/terminal-session/rust/src/output.rs`（拉取接口 `session.output.pull`）、`bedcode-desktop/wasm-apps/terminal-session/src/components/terminal/TerminalPreview.vue`（轮询档位）、`bedcode-desktop/src-tauri/src/server/websocket/subscription.rs` + `terminal_ws/`（远程订阅，同为拉取模型执行体 + 流代数门控）、`bedcode-desktop/src-tauri/src/utils/session_gateway.rs`（宿主↔插件窄转发层）、`bedcode-mobile/src-tauri/src/terminal_link.rs`、`bedcode-mobile/src/composables/useTerminalBuffer.ts`、`bedcode-mobile/src/composables/writeCoalescer.ts`、`bedcode-mobile/src/stores/terminalBuffer.ts`

## 相关

- 领域模型 / 术语：仓库根 `CONTEXT.md`
- 架构决策：[`docs/adr/`](../adr/)
- 代码地图：[`bedcode-desktop/docs/code-map.md`](../../bedcode-desktop/docs/code-map.md) / [`bedcode-mobile/docs/code-map.md`](../../bedcode-mobile/docs/code-map.md)
- 日志与排障：[`docs/knowledge/logging.md`](../knowledge/logging.md)
