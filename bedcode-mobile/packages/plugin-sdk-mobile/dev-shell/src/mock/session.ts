/**
 * 模拟会话/终端环境
 *
 * 浏览器中不存在真实 WS 连接，这里提供可交互的假会话：
 * - MockTerminalView 的输入/输出、会话生命周期按钮都驱动本模块
 * - context.terminal / context.session / context.lifecycle / mobileApi 全部接本模块
 * 事件经全局 emitter 分发（key 与宿主 plugin/events.ts 一致，插件无感知）。
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
  { id: 'mock-session-2', name: 'mock-session-2', agent: 'claude-code', status: 'running' },
])
const activeSessionId = ref<string>(sessions.value[0].id)
const connected = ref(true)
const outputs = reactive<Record<string, string[]>>({
  [sessions.value[0].id]: ['$ 欢迎使用 BedCode Dev Shell（模拟会话）', '$ 在右侧输入命令，或点击「模拟输出」触发插件输出解析'],
  [sessions.value[1].id]: ['$ mock session 2 ready'],
})
const inputs = reactive<Record<string, string[]>>({})

// ==================== 全局事件 emitter ====================

const listeners = new Map<string, Set<(...args: any[]) => void>>()

/** 订阅 dev-shell 事件（返回 disposable） */
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

/** 发射 dev-shell 事件 */
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

/** 发送终端输入（触发插件 lifecycle onTerminalInput） */
export function sendInputToSession(sessionId: string, text: string): void {
  if (!text.trim()) return
  ;(inputs[sessionId] ||= []).push(text)
  emitDevEvent('plugin:lifecycle:terminalInput', { sessionId, data: text })
}

/** 模拟终端输出（触发插件 terminal.onOutput + lifecycle onTerminalOutput） */
export function sendOutput(sessionId: string, data: string): void {
  ;(outputs[sessionId] ||= []).push(data)
  emitDevEvent('terminal:output', { sessionId, data })
  emitDevEvent('plugin:lifecycle:terminalOutput', { sessionId, data })
}

/** 新建会话（触发 session:statusChange + lifecycle onSessionCreated） */
export function createSession(): void {
  const id = `mock-session-${sessions.value.length + 1}`
  sessions.value.unshift({ id, name: id, agent: 'bedcode', status: 'running' })
  outputs[id] = [`$ session ${id} created`]
  activeSessionId.value = id
  emitStatusChange(id, 'created')
  emitDevEvent('plugin:lifecycle:sessionCreated', { sessionId: id })
}

/** 停止会话（触发 session:statusChange + lifecycle onSessionStopped） */
export function stopSession(sessionId: string): void {
  const s = sessions.value.find((x) => x.id === sessionId)
  if (!s) return
  s.status = 'stopped'
  emitStatusChange(sessionId, 'stopped')
  emitDevEvent('plugin:lifecycle:sessionStopped', { sessionId })
}

/** 切换连接状态（触发 lifecycle onDisconnect / onAuthSuccess） */
export function setConnected(next: boolean): void {
  if (connected.value === next) return
  connected.value = next
  if (!next) {
    emitDevEvent('plugin:lifecycle:disconnect', { reason: 'dev-shell 手动断开' })
  }
}

/** 模拟认证成功（触发 lifecycle onAuthSuccess） */
export function authSuccess(): void {
  connected.value = true
  emitDevEvent('plugin:lifecycle:authSuccess', {})
}

function emitStatusChange(sessionId: string, status: string): void {
  emitDevEvent('session:statusChange', {
    sessionId,
    status,
    sessions: sessions.value.map((s) => ({ ...s })),
  })
}

export { sessions, activeSessionId, connected, outputs, inputs }
