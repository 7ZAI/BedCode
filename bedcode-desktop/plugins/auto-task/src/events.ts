/**
 * Auto Task 插件 UI 事件名（插件业务常量，单一事实来源）
 *
 * 与 Rust SDK `constants.rs` 及宿主消息总线/事件广播的 topic 字符串保持一致；
 * 新增事件必须同步 Rust 侧常量，避免两端漂移。
 */

/** 任务状态变更 */
export const EVENT_TASK_STATUS_CHANGED = 'task:status-changed'

/** 会话自动授权模式变更 */
export const EVENT_SESSION_MODE_CHANGED = 'session:mode-changed'

/** 任务队列变更 */
export const EVENT_TASK_QUEUE_CHANGED = 'task:queue-changed'
