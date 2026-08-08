/**
 * 模拟会话/终端环境（桌面端）
 *
 * 与移动端 dev-shell 同构：context.terminal / context.session 及骨架视图
 * 全部接本模块，事件 key 与宿主一致。
 */
import { reactive, ref } from 'vue'
import type { Disposable } from '../../src/types'

export interface MockSession {
  id: string
  name: string
  agent: string
  status: 'running' | 'stopped'
}

const sessions = ref<MockSession[]>([
  { id: 'mock-session-1', name: 'mock-session-1', agent: 'bedcode', status: 'running' },
])
const activeSessionId = ref<string>(sessions.value[0].id)
const connected = ref(true)
const outputs = reactive<Record<string, string[]>>({
  [sessions.value[0].id]: ['$ 欢迎使用 BedCode Dev Shell（模拟会话）'],
})
const inputs = reactive<Record<string, string[]>>({})

// ==================== 全局事件 emitter ====================

const listeners = new Map<string, Set<(...args: any[]) => void>>()

export function onDevEvent(event: string, handler: (...args: any[]) => void): Disposable {
  let set = listeners.get(event)
  if (!set) {
    set = new Set()
    listeners.set(event, set)
  }
  set.add(handler)
  return {
    dispose() {
      set!.delete(handler)
      if (set!.size === 0) listeners.delete(event)
    },
  }
}

export function emitDevEvent(event: string, ...args: any[]): void {
  const set = listeners.get(event)
  if (!set) return
  for (const handler of [...set]) {
    try {
      handler(...args)
    } catch (e) {
      console.error(`[dev-shell] event handler "${event}" failed:`, e)
    }
  }
}

// ==================== 会话模拟操作 ====================

/** 发送终端输入（触发插件 terminal.onInput / onTerminalInput 语义） */
export function sendInputToSession(sessionId: string, text: string): void {
  if (!text.trim()) return
  ;(inputs[sessionId] ||= []).push(text)
  emitDevEvent('terminal:input', { sessionId, text })
}

/** 模拟终端输出（触发插件 terminal.onOutput） */
export function sendOutput(sessionId: string, data: string): void {
  ;(outputs[sessionId] ||= []).push(data)
  emitDevEvent('terminal:output', { sessionId, data })
}

/** 新建会话 */
export function createSession(): void {
  const id = `mock-session-${sessions.value.length + 1}`
  sessions.value.unshift({ id, name: id, agent: 'bedcode', status: 'running' })
  outputs[id] = [`$ session ${id} created`]
  activeSessionId.value = id
  emitStatusChange(id, 'created')
}

/** 停止会话 */
export function stopSession(sessionId: string): void {
  const s = sessions.value.find((x) => x.id === sessionId)
  if (!s) return
  s.status = 'stopped'
  emitStatusChange(sessionId, 'stopped')
}

/** 切换连接状态 */
export function setConnected(next: boolean): void {
  connected.value = next
  emitStatusChange(activeSessionId.value, next ? 'connected' : 'disconnected')
}

function emitStatusChange(sessionId: string, status: string): void {
  emitDevEvent('session:statusChange', {
    sessionId,
    status,
    sessions: sessions.value.map((s) => ({ ...s })),
  })
}

export { sessions, activeSessionId, connected, outputs, inputs }
