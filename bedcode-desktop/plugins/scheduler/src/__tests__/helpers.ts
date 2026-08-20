/**
 * 计划任务面板测试上下文（mock PluginContext）
 *
 * - commands.execute：可编程队列，按调用顺序返回预置响应
 * - events.on：记录订阅，emit() 驱动 handler（模拟 Rust 广播到达）
 * - i18n.t：直接解析插件 zh-CN 消息（与宿主合并前缀前的 key 形状一致）
 */
import { vi, type Mock } from 'vitest'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import zhCN from '../i18n/zh-CN'

/** 端点成功信封（http_response.ok_with_data 形状） */
export function ok(data: unknown) {
  return { status: 200, body: { code: 0, message: 'ok', data } }
}

/** 端点错误信封（http_response.error 形状） */
export function endpointError(status: number, message: string) {
  return { status, body: { code: status, message } }
}

/** 扁平解析 'panel.title' → zh-CN 文案（未知 key 返回 key 本身） */
function resolveKey(obj: unknown, key: string): string {
  // 扁平点号 key 优先（messages 顶层即 'panel.*' 字面 key）
  if (obj && typeof obj === 'object' && key in (obj as Record<string, unknown>)) {
    return String((obj as Record<string, unknown>)[key])
  }
  // 兜底：嵌套对象路径解析（兼容旧结构）
  const value = key.split('.').reduce<unknown>((o, k) => {
    if (o && typeof o === 'object' && k in (o as Record<string, unknown>)) {
      return (o as Record<string, unknown>)[k]
    }
    return undefined
  }, obj)
  return typeof value === 'string' ? value : key
}

export interface MockContext {
  context: PluginContext
  execute: Mock
  /** event → handler 列表（emit 驱动，模拟事件到达） */
  listeners: Record<string, Array<(payload: unknown) => void>>
}

export function makeContext(): MockContext {
  const execute = vi.fn()
  const listeners: Record<string, Array<(payload: unknown) => void>> = {}
  // 仅 mock 面板用到的面：commands / events / i18n，其余面不需要
  const context = {
    commands: { execute },
    events: {
      on: vi.fn((event: string, handler: (payload: unknown) => void) => {
        ;(listeners[event] ||= []).push(handler)
        return { dispose: () => undefined }
      }),
      emit: (event: string, payload?: unknown) => {
        for (const handler of listeners[event] || []) handler(payload)
      },
    },
    i18n: { t: (key: string) => resolveKey(zhCN, key) },
  } as unknown as PluginContext
  return { context, execute, listeners }
}

/** 端点调用断言辅助：execute 是否以指定 method/path 调用过 _http_endpoint */
export function expectEndpoint(execute: Mock, method: string, path: string): void {
  const call = execute.mock.calls.find(
    ([cmd, args]) => cmd === '_http_endpoint' && args?.method === method && args?.path === path,
  )
  expect(call).toBeTruthy()
}
