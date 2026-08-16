/**
 * presetTaskState 单测（seam 1：预设任务执行状态状态机）
 *
 * 覆盖 spec 的 21 条测试清单：状态转换（入队/完成广播/对账/手动执行/移除回退/编辑重置）、
 * 锁定判定 2×4 矩阵、可入队筛选。
 */
import { describe, it, expect } from 'vitest'
import {
  enqueue,
  taskDone,
  reconcile,
  manualExecute,
  queueItemRemoved,
  edit,
  canEnqueue,
  filterEnqueueable,
  type PresetTaskExecState,
} from '@/composables/presetTaskState'

const UNUSED: PresetTaskExecState = { status: 'unused', pendingTaskId: null }

function executing(taskId = 't1'): PresetTaskExecState {
  return { status: 'executing', pendingTaskId: taskId }
}

describe('presetTaskState 状态转换', () => {
  it('入队：unused → executing，记录新 taskId', () => {
    expect(enqueue(UNUSED, 't1')).toEqual({ status: 'executing', pendingTaskId: 't1' })
  })

  it('入队：可重复 completed 预设 → executing，新 taskId 覆盖旧', () => {
    const prev: PresetTaskExecState = { status: 'completed', pendingTaskId: 'old' }
    expect(enqueue(prev, 't2')).toEqual({ status: 'executing', pendingTaskId: 't2' })
  })

  it('入队：可重复 interrupted 预设 → executing，新 taskId 覆盖旧', () => {
    const prev: PresetTaskExecState = { status: 'interrupted', pendingTaskId: 'old' }
    expect(enqueue(prev, 't3')).toEqual({ status: 'executing', pendingTaskId: 't3' })
  })

  it('完成广播：executing + taskId 匹配 → completed，清除 pendingTaskId', () => {
    expect(taskDone(executing('t1'), 't1')).toEqual({ status: 'completed', pendingTaskId: null })
  })

  it('完成广播：taskId 不匹配 → 状态与记录不变（忽略孤儿）', () => {
    const s = executing('t1')
    expect(taskDone(s, 'other')).toBe(s)
  })

  it('对账：executing → interrupted', () => {
    expect(reconcile(executing('t1'))).toEqual({ status: 'interrupted', pendingTaskId: 't1' })
  })

  it('对账：interrupted 再对账 → interrupted（幂等）', () => {
    const s: PresetTaskExecState = { status: 'interrupted', pendingTaskId: 't1' }
    expect(reconcile(s)).toBe(s)
  })

  it('对账：unused / completed → 不变', () => {
    expect(reconcile(UNUSED)).toBe(UNUSED)
    const c: PresetTaskExecState = { status: 'completed', pendingTaskId: null }
    expect(reconcile(c)).toBe(c)
  })

  it('手动执行：任意状态 → completed，清除 pendingTaskId', () => {
    expect(manualExecute(UNUSED)).toEqual({ status: 'completed', pendingTaskId: null })
    expect(manualExecute(executing('t1'))).toEqual({ status: 'completed', pendingTaskId: null })
  })

  it('队列项移除：taskId 匹配 → unused，清除记录', () => {
    expect(queueItemRemoved(executing('t1'), 't1')).toEqual(UNUSED)
  })

  it('队列项移除：不匹配 → 不变', () => {
    const s = executing('t1')
    expect(queueItemRemoved(s, 'other')).toBe(s)
  })

  it('编辑：任意状态 → unused，清除记录', () => {
    expect(edit(UNUSED)).toEqual(UNUSED)
    expect(edit(executing('t1'))).toEqual(UNUSED)
    expect(edit({ status: 'completed', pendingTaskId: null })).toEqual(UNUSED)
    expect(edit({ status: 'interrupted', pendingTaskId: 't1' })).toEqual(UNUSED)
  })
})

describe('presetTaskState 锁定判定（2×4 矩阵）', () => {
  const rep = (status: PresetTaskExecState['status']) => ({ repeatable: true, status })
  const one = (status: PresetTaskExecState['status']) => ({ repeatable: false, status })

  it('不可重复 + 未使用 → 可入队', () => {
    expect(canEnqueue(one('unused'))).toBe(true)
  })

  it('不可重复 + 执行中 → 锁定', () => {
    expect(canEnqueue(one('executing'))).toBe(false)
  })

  it('不可重复 + 已完成 → 锁定', () => {
    expect(canEnqueue(one('completed'))).toBe(false)
  })

  it('不可重复 + 已中断 → 锁定', () => {
    expect(canEnqueue(one('interrupted'))).toBe(false)
  })

  it('可重复 + 未使用 → 可入队', () => {
    expect(canEnqueue(rep('unused'))).toBe(true)
  })

  it('可重复 + 执行中 → 锁定（防重复，同刻一实例）', () => {
    expect(canEnqueue(rep('executing'))).toBe(false)
  })

  it('可重复 + 已完成 → 可入队', () => {
    expect(canEnqueue(rep('completed'))).toBe(true)
  })

  it('可重复 + 已中断 → 可入队', () => {
    expect(canEnqueue(rep('interrupted'))).toBe(true)
  })
})

describe('presetTaskState 筛选', () => {
  it('混合列表 → 仅返回可入队子集', () => {
    const tasks = [
      { id: 'a', repeatable: false, status: 'unused' },
      { id: 'b', repeatable: false, status: 'completed' },
      { id: 'c', repeatable: false, status: 'interrupted' },
      { id: 'd', repeatable: false, status: 'executing' },
      { id: 'e', repeatable: true, status: 'unused' },
      { id: 'f', repeatable: true, status: 'completed' },
      { id: 'g', repeatable: true, status: 'interrupted' },
      { id: 'h', repeatable: true, status: 'executing' },
    ] as any
    const result = filterEnqueueable(tasks)
    expect(result.map((t: any) => t.id)).toEqual(['a', 'e', 'f', 'g'])
  })
})
