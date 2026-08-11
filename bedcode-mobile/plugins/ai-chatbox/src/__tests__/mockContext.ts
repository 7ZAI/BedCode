/**
 * 测试工具：mock PluginContext（commands/events/storage）
 *
 * dev-shell mock-context 思路的轻量版：命令按名 stub、事件可手动触发、
 * storage 内存 map，供 composables 单测（接缝 4）。
 */
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'

export interface MockContext {
  context: PluginContext
  /** 命令调用记录（断言参数用） */
  calls: { command: string; args: any }[]
  /** 事件监听器注册表（测试手动触发 payload） */
  listeners: Record<string, (payload: any) => void>
  storageMap: Map<string, any>
  /** 手动触发流事件 */
  emitStream(event: string, payload: any): void
}

/** 默认命令行为（可被 overrides 覆盖；chat-complete/fetch-models 返回宿主
 * 响应形状 { status, body }——回复/模型解析已前移前端 adapter） */
const DEFAULT_COMMANDS: Record<string, (args: any) => any> = {
  'ai-chatbox.list-conversations': () => ({ conversations: [] }),
  'ai-chatbox.get-messages': () => ({ messages: [] }),
  'ai-chatbox.chat-stream': (args) => ({ streamId: args.streamId }),
  'ai-chatbox.save-conversation': () => ({ success: true }),
  'ai-chatbox.save-message': () => ({ success: true }),
  'ai-chatbox.delete-conversation': () => ({ success: true }),
  'ai-chatbox.fetch-models': () => ({
    status: 200,
    body: JSON.stringify({ data: [{ id: 'model-a' }, { id: 'model-b' }] }),
  }),
  'ai-chatbox.chat-complete': () => ({
    status: 200,
    body: JSON.stringify({ choices: [{ message: { content: 'pong' } }] }),
  }),
}

export function createMockContext(
  overrides?: { commands?: Record<string, (args: any) => any> },
): MockContext {
  const storageMap = new Map<string, any>()
  const listeners: Record<string, (payload: any) => void> = {}
  const calls: { command: string; args: any }[] = []

  const commands = { ...DEFAULT_COMMANDS, ...overrides?.commands }

  const context = {
    id: 'com.bedcode.ai-chatbox',
    commands: {
      async execute(command: string, args: any = {}): Promise<any> {
        calls.push({ command, args })
        const handler = commands[command]
        if (!handler) throw new Error(`unknown command: ${command}`)
        return handler(args)
      },
    },
    events: {
      on(event: string, handler: (payload: any) => void): { dispose(): void } {
        listeners[event] = handler
        return { dispose: () => { delete listeners[event] } }
      },
      emit(event: string, ...args: any[]): void {
        listeners[event]?.(args[0])
      },
    },
    storage: {
      async get<T = any>(key: string): Promise<T | undefined> {
        return storageMap.get(key) as T | undefined
      },
      async set(key: string, value: any): Promise<void> {
        storageMap.set(key, value)
      },
      async delete(key: string): Promise<void> {
        storageMap.delete(key)
      },
    },
  } as unknown as PluginContext

  return {
    context,
    calls,
    listeners,
    storageMap,
    emitStream: (event: string, payload: any) => {
      listeners[event]?.(payload)
    },
  }
}

/** 构造一个已配置好的供应商（测试共用） */
export function makeProvider(overrides: Partial<any> = {}): any {
  return {
    id: 'p1',
    name: 'DeepSeek',
    apiKey: 'sk-test',
    baseUrl: 'https://api.deepseek.com/v1',
    apiStyle: 'openai',
    models: ['deepseek-chat', 'deepseek-reasoner'],
    activeModel: 'deepseek-chat',
    ...overrides,
  }
}
