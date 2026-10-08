# 桌面端架构图重绘（2026-10-08）

触发：桌面端架构图相对 2026-10-06 之后的代码已过期，且 `auto-task-automation-flow.html` 描述的业务应用（`com.bedcode.auto-task`）已并入 `com.bedcode.terminal-session` 任务域、实体不再存在。

## 处理

| 图 | 动作 |
| --- | --- |
| `auto-task-automation-flow.html` | 删除（该图早先已随 D8-P4 合并从 HEAD 移除，本次未再产生删除动作） |
| `pty-output-flow-desktop.html` | 删除并重绘（拉取模型 + 双水位迟滞回环） |
| `session-source-flow-desktop.html` | 删除并重绘（ADR 0022 / 0039 宿主调用链，含 8 条互调 api 与 fail-visible） |
| `task-automation-flow-desktop.html` | **新增**，承接原 auto-task 图的语义：入口面 → 插件 HTTP → 双调度状态机 → `launch` 编排 → `host-pty` → Agent hooks → 私有库真源 → `host-events` 广播 |

三张图均通过 archify 四道门：`validate` / `deliver` / `check` / `browser-check` 全部 pass（`--quality showcase`）。

## 事实校准（以代码为准，不沿用旧图文案）

- 轮询档位是 `50 ms / 250 ms`（`bedcode-desktop/wasm-apps/terminal-session/src/utils/terminal/terminalPullPolicy.ts`：`OUTPUT_PULL_INTERVAL_MS = 50` / `OUTPUT_IDLE_INTERVAL_MS = 250`），不是旧 README 表格写的 100/500。README 表格已同步修正。
- 会话创建是 `host-pty.spawn` **同步可见**，不是 `create-with-spec` 异步（`rust/src/launch.rs`）。
- 插件未激活错误类型是 `AppError::Plugin("session plugin not active: ...")`，不是 `SessionPluginRequired`。
- 互调真源在 `bedcode-wasm-core::intercall`（ADR 0033 归属更正），默认超时 5 s。
- 环容量：terminal-session 声明 4 MiB（`SESSION_PTY_RING_BYTES`），宿主默认 256 KiB / 上限 4 MiB；配额 8 取内核默认 `PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN`（manifest 未声明 `pty_quota`）。
- 移动端 WS 已迁插件 `ws_terminal.rs`，路径 `/ws/plugin/com.bedcode.terminal-session/terminal`；旧 `/ws/terminal/session/{id}` 描述作废。
- 旧推模型三件套 + `host_session_broadcast` + `/ws/terminal/local` 环回已全部下线。

## 交付过程笔记

- 工作目录 `.archify/desktop-diagram-refresh-20261008-0930/`（candidates + receipts）。
- `browser-check` 门最初因无浏览器可执行文件而 skip：用 `ARCHIFY_CHROME=$HOME/.cache/ms-playwright/chromium_headless_shell-1234/chrome-headless-shell-linux64/chrome-headless-shell` 后四门全过。
- 两处门控约束值得记住：
  1. dataflow `meta.viewBox` 宽度 ≤ 1085（`composition/desktop-readability`：930px 桌面预算下 7px 源字号投影需 ≥ 6px）；`stages` 上限 5 段，行号 0..4（`rowYs = [128,242,356,470,584]`，`stageBottomPad 74`）。
  2. sequence 若手写 `meta.viewBox` 会触发 `composition/viewport-height`（宽比 < 1.55 的画布既不能收窄也不能纵向滚动）；**省略 `meta.viewBox`** 让渲染器自动定高并声明 `data-reader-fit="intrinsic-height"` 即可通过。另注意 7 个参与者时盒子宽 90px，`textUnits(label)*6.8` 超过 96 即报错（全角字符按 2 计）。
- 跨 4 段回流线（广播 → 移动端）与入口面所有前向边共走廊，`proper-crossing` 过不去，因此以节点 tag + 卡片文案承载该语义，不画回流线。

## 未纳入本次

- `docs/diagrams/` 下 archify 的侧车收据（`.delivery.json` / `.browser-check.json` / `.finalize*.json`）保持未跟踪，与该目录既有三张图的口径一致；权威交付物是 HTML + IR JSON。
