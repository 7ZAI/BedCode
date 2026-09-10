# 定时自动任务 —— 扩展核心宿主 API（session_create + 定时器）

---
status: accepted
---

auto-task 插件的定时自动任务要求"指定时刻开启会话执行任务"，但插件侧没有创建会话的能力：WASM 宿主仅有 `session_get/list/config_list` 与 `terminal_send`，TS SDK 无宿主命令桥，HTTP `/api/sessions/start` 需 JWT。我们决定扩展核心宿主 API：新增 `session_create(config_id)` host function（包一层核心已有的 `SessionManager::create_session`），并新增宿主定时器 API（宿主 tokio interval 到期回调插件命令，插件以数据库中的到期时间做幂等判断）。会话创建完成后由 `Created` 生命周期事件（带 session_id + config_id）作为就绪信号，插件据此把任务组注入队列。

## Considered Options

- **TS 层 `setInterval` 驱动 + 只作用于已运行会话**：零核心改动，但定时器随 webview 生命周期失效（窗口关闭即停），且无法"开启会话"，与需求语义不符。否决。
- **插件借道 HTTP `/api/sessions/start`**：需要 JWT，插件网关中间件与移动端认证体系不同路，为插件伪造凭证是错误方向。否决。
- **宿主侧直接内置调度器**：把定时任务语义固化进核心，与"插件系统观察和扩展会话行为"的设计支柱冲突，且 auto-task 的队列/状态逻辑都在插件内，调度拆到核心会割裂状态机。否决。

## Consequences

- 核心 `session_manager.rs`、`wasm_host.rs`、插件 SDK 需同步新增两个 host function（WASM ABI 一并扩展）。
- 定时器回调的目标命令由插件注册（manifest/事件方式），宿主只负责"到点调用"，具体到点做什么、幂等与否归插件。
- 触发时序：`session_create` → `Created` 事件 → 入队 → 现有队列调度链（终态出队 + 自动授权联动）复用，定时自动任务不引入第二条执行路径。
- 应用完全退出时定时任务不触发，重启后由插件按 `trigger_at` 判断补标 `missed`（不补跑）。
