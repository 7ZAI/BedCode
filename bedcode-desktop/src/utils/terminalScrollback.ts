/**
 * 终端可滚动历史行数上限
 *
 * 与 Rust 端 channels.global_queue_capacity（默认 25000 事件）对齐：
 * 后端历史队列保留 25000 个输出事件，若 xterm scrollback 小于事件展开后的
 * 行数，回放时最早的历史会被 xterm buffer 直接丢弃，无法向上滚动查看。
 * 单个事件可含多行输出，故留 1.2 倍余量（30000 行 ≈ 覆盖 25000 事件的常见展开）。
 * 若用户调大后端队列容量，此值需同步上调。
 */
export const TERMINAL_SCROLLBACK = 30000
