/**
 * 终端会话域模型（插件侧自持副本）
 *
 * 宿主 `SessionInfo` 的按值复制（插件不引宿主模块的既有约定——同
 * `BUILTIN_MENU_ORDERS` 槽位常量先例，票 18 核对口径）。字段与线协议 DTO
 * 对齐，保持 camelCase / snake_case 双形态兼容（老端忽略未知字段）。
 */
export interface SessionInfo {
  id: string
  name: string
  config_id: string
  configId?: string
  status: string
  session_type?: string
  sessionType?: string
  created_at: string
  createdAt?: string
  startedAt?: string
  stoppedAt?: string
  /** 任务执行状态（Plugin 会话使用） */
  taskStatus?: string
  /** 任务状态原因 */
  taskReason?: string
}
