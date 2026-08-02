/**
 * 插件 UI 事件名（与 Rust SDK `constants.rs` 保持一致，单一事实来源）
 *
 * 宿主 `host.emit_event` / 消息总线 `bus_publish` 与前端 `context.events.on`
 * 共用同一事件名；新增事件必须同时更新 Rust 侧常量，避免两端漂移。
 */

/** 任务状态变更（task:status-changed） */
export const EVENT_TASK_STATUS_CHANGED = 'task:status-changed'

/** 会话自动授权模式变更（session:mode-changed） */
export const EVENT_SESSION_MODE_CHANGED = 'session:mode-changed'

/** 任务队列变更（task:queue-changed） */
export const EVENT_TASK_QUEUE_CHANGED = 'task:queue-changed'
