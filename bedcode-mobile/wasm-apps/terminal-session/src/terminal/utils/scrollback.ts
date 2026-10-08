/**
 * 终端可滚动历史行数上限（移动端专用，低于桌面端 30000）
 *
 * 桌面主机服务端历史队列保留 25000 个输出事件（channels.global_queue_capacity），回放时超出
 * scrollback 的早期行会被 xterm buffer 直接丢弃。移动端受限于手机内存与滚动
 * 性能，不追求全量历史：10000 行已覆盖数小时连续输出，足够手动回翻；
 * 30k 行常驻约 15-25MB，是会话越久页面越卡的直接来源。
 */
export const TERMINAL_SCROLLBACK = 10000
