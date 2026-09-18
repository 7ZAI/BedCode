/**
 * 任务卡进度条配色契约（真机报障：移动端下载/暂停时进度条是灰色）
 *
 * 契约来源（用户报障）：
 * 1) 活跃态（传输中 / 排队 / 已暂停）进度条一律取主题色（`ft-progress-active`
 *    → var(--mobile-accent)）——暂停/排队意味着「进度仍在」，不是「禁用」；
 * 2) 终态保留各自语义色，不得被改成主题色；
 * 3) 配色表必须覆盖全部状态（漏键会在渲染侧回退成无底色 = 只剩灰轨道）。
 */

import { describe, it, expect } from 'vitest'
import {
  TASK_STATE_PROGRESS_CLASS,
  type TaskStateName,
} from '../../../../plugins/file-transfer/src/types'

/** 全状态清单（新增状态时必须同步配色表，本清单是漏键守卫） */
const ALL_STATES: TaskStateName[] = [
  'transferring',
  'pending',
  'paused',
  'completed',
  'failed',
  'rejected',
  'cancelled',
  'interrupted',
]

/** 灰底 class（--mobile-text-disabled）：活跃态不得出现 */
const GREY_CLASS = 'ft-progress-cancelled'

const ACTIVE_STATES: TaskStateName[] = ['transferring', 'pending', 'paused']

describe('TASK_STATE_PROGRESS_CLASS', () => {
  it.each(ACTIVE_STATES)(
    'should paint active state %s in theme accent, never disabled grey',
    (state) => {
      expect(TASK_STATE_PROGRESS_CLASS[state]).toBe('ft-progress-active')
      expect(TASK_STATE_PROGRESS_CLASS[state]).not.toBe(GREY_CLASS)
    },
  )

  it('should keep terminal state semantic colors', () => {
    expect(TASK_STATE_PROGRESS_CLASS.completed).toBe('ft-progress-completed')
    expect(TASK_STATE_PROGRESS_CLASS.failed).toBe('ft-progress-failed')
    expect(TASK_STATE_PROGRESS_CLASS.rejected).toBe('ft-progress-failed')
    expect(TASK_STATE_PROGRESS_CLASS.cancelled).toBe(GREY_CLASS)
    expect(TASK_STATE_PROGRESS_CLASS.interrupted).toBe(GREY_CLASS)
  })

  it('should cover every task state exactly once', () => {
    expect(Object.keys(TASK_STATE_PROGRESS_CLASS).sort()).toEqual([...ALL_STATES].sort())
  })
})
