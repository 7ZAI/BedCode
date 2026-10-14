/**
 * 终端域类型（票 15：宿主 @/composables/model 的用点子集随迁）
 */

// ==================== 预设任务类型 ====================

/** 预设任务执行状态（真源 = 宿主 presetTaskState；此处仅类型消费） */
export type PresetTaskStatus = 'unused' | 'executing' | 'completed' | 'interrupted'

/** 预设任务 */
export interface PresetTask {
  id: string
  content: string
  createdAt: string
  updatedAt: string
  /** 可重复：true 不受执行历史限制可反复入队；false 一旦执行过即锁定 */
  repeatable: boolean
  /** 执行状态（本地记录，入队即视为已执行） */
  status: PresetTaskStatus
  /** 入队时记录的队列项 id，用于完成广播/移除事件匹配 */
  pendingTaskId: string | null
  /** 入队时所在会话 id，对账仅对同会话的预设做判定（防多会话误中断） */
  pendingSessionId: string | null
}
