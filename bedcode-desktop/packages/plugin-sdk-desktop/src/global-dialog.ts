/**
 * 全局弹窗控制器（桌面宿主 + dev-shell 共用）
 *
 * 发布-订阅实现：不直接依赖 Vue reactivity，宿主 / dev-shell 的
 * PluginGlobalDialog 组件经 subscribeGlobalDialog 订阅渲染，单元测试无需
 * 初始化共享运行时即可驱动。
 *
 * 语义：
 * - FIFO 队列：同一时刻至多一个弹窗，后到的排队；当前关闭后自动接替；
 * - update() 生成新条目对象再广播（订阅方拿到新引用触发重渲染）；
 * - close() 幂等；排队中的条目也可关闭（从队列移除）；
 * - 定时自动关闭为**可选能力**：仅当提供 timeoutSec / deadlineAt 时生效，
 *   计时从条目成为当前弹窗起计算（迟到打开/排队经过的时间不重复计入相对秒数）。
 */
import type { PluginDialogOptions } from './types'

/** 内部弹窗条目 = 公开选项 + 宿主注入字段 */
export interface GlobalDialogEntry extends PluginDialogOptions {
  /** 内部唯一 ID（句柄路由用） */
  _id: number
  /** 内容组件宿主上下文（provide('pluginContext') 用；宿主 showDialog 注入） */
  pluginContext?: unknown
  /** 条目创建时刻（ms） */
  createdAt: number
}

/** 订阅输入（宿主注入 pluginContext 后交 openGlobalDialog） */
export type OpenGlobalDialogInput = PluginDialogOptions & { pluginContext?: unknown }

export type GlobalDialogListener = (entry: GlobalDialogEntry | null) => void

/** 打开/更新/关闭返回的句柄（供插件调用方使用） */
export interface GlobalDialogHandleOutput {
  close(): void
  update(options: Partial<PluginDialogOptions>): void
}

let seq = 0
let current: GlobalDialogEntry | null = null
const queue: GlobalDialogEntry[] = []
const listeners = new Set<GlobalDialogListener>()
/** 条目 → 超时定时器（仅当条目指定 timeoutSec / deadlineAt） */
const timers = new Map<number, ReturnType<typeof setTimeout>>()

/** 当前弹窗（无则 null） */
export function getGlobalDialog(): GlobalDialogEntry | null {
  return current
}

/** 订阅弹窗变化；立即以当前值回调一次，返回退订函数 */
export function subscribeGlobalDialog(listener: GlobalDialogListener): () => void {
  listeners.add(listener)
  listener(current)
  return () => {
    listeners.delete(listener)
  }
}

function emit(): void {
  for (const fn of [...listeners]) fn(current)
}

function clearTimer(id: number): void {
  const timer = timers.get(id)
  if (timer) {
    clearTimeout(timer)
    timers.delete(id)
  }
}

/** 解析条目的绝对截止时刻（deadlineAt 优先；缺省无定时关闭） */
export function resolveDialogDeadline(entry: GlobalDialogEntry): number | null {
  if (entry.deadlineAt != null && entry.deadlineAt > 0) return entry.deadlineAt
  if (entry.timeoutSec != null && entry.timeoutSec > 0) return entry.createdAt + entry.timeoutSec * 1000
  return null
}

/** 条目成为当前弹窗时的侧效：启动（或立即触发）超时计时 */
function startTimeout(entry: GlobalDialogEntry): void {
  clearTimer(entry._id)
  const deadline = resolveDialogDeadline(entry)
  if (deadline == null) return
  const delay = Math.max(0, deadline - Date.now())
  timers.set(
    entry._id,
    setTimeout(() => {
      // 超时：先回调 onTimeout（缺省仅关闭），再关闭（触发 onClose + 队列接替）
      try {
        entry.onTimeout?.()
      } catch (e) {
        console.error('[PluginDialog] onTimeout failed:', e)
      }
      closeGlobalDialog(entry._id)
    }, delay),
  )
}

function fireOnClose(entry: GlobalDialogEntry): void {
  try {
    entry.onClose?.()
  } catch (e) {
    console.error('[PluginDialog] onClose failed:', e)
  }
}

/** 关闭指定弹窗：当前弹窗关闭后队列自动顶上；排队中直接移除（均幂等） */
export function closeGlobalDialog(id: number): void {
  if (current && current._id === id) {
    const closed = current
    clearTimer(id)
    current = null
    const next = queue.shift() ?? null
    current = next
    emit()
    fireOnClose(closed)
    if (next) startTimeout(next)
    return
  }
  const idx = queue.findIndex((e) => e._id === id)
  if (idx >= 0) {
    const [removed] = queue.splice(idx, 1)
    fireOnClose(removed)
  }
}

/** 热更新指定弹窗（当前或排队中；新对象广播触发重渲染） */
export function updateGlobalDialog(id: number, options: Partial<PluginDialogOptions>): void {
  if (current && current._id === id) {
    current = { ...current, ...options }
    emit()
    startTimeout(current)
    return
  }
  const idx = queue.findIndex((e) => e._id === id)
  if (idx >= 0) {
    queue[idx] = { ...queue[idx], ...options }
  }
}

/**
 * 打开全局弹窗（宿主 ui.showDialog 调用）。
 * 返回句柄：close() 幂等关闭（当前或排队项），update() 热更新选项。
 */
export function openGlobalDialog(input: OpenGlobalDialogInput): GlobalDialogHandleOutput {
  const entry: GlobalDialogEntry = {
    ...input,
    _id: ++seq,
    createdAt: Date.now(),
  }
  if (current) {
    queue.push(entry)
  } else {
    current = entry
    emit()
    startTimeout(entry)
  }
  return {
    close: () => closeGlobalDialog(entry._id),
    update: (options) => updateGlobalDialog(entry._id, options),
  }
}

/** 测试辅助：清空全部状态（队列/当前/监听器/定时器），用例间隔离 */
export function _resetGlobalDialogForTest(): void {
  for (const id of [...timers.keys()]) clearTimer(id)
  current = null
  queue.length = 0
  listeners.clear()
}