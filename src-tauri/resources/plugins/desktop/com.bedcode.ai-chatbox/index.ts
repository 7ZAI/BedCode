/**
 * AI Chatbox 插件入口
 *
 * 侧边栏 AI 对话面板 + 终端提示词优化
 */
import ChatView from './components/ChatView.vue'
import { usePromptOptimizer } from './composables/usePromptOptimizer'
import type { ApiProvider } from './types'

/** 插件上下文类型（与宿主 PluginContext 一致） */
interface PluginContext {
  readonly id: string
  readonly extensionPath: string
  readonly commands: {
    register(id: string, handler: (...args: any[]) => any): { dispose(): void }
    execute(id: string, ...args: any[]): Promise<any>
  }
  readonly terminal: {
    sendInput(sessionId: string, text: string): Promise<void>
    onOutput(handler: (sessionId: string, data: string) => void): { dispose(): void }
    onInput(handler: (sessionId: string, text: string) => string | null): { dispose(): void }
  }
  readonly session: {
    list(): Promise<any[]>
    get(sessionId: string): Promise<any>
    onStatusChange(handler: (event: any) => void): { dispose(): void }
  }
  readonly ui: {
    registerSidebarPanel(panel: { id: string; title: string; component: any }): { dispose(): void }
    registerToolboxPage(page: { id: string; title: string; component: any }): { dispose(): void }
    registerStatusBarItem(item: { id: string; label: string; icon?: string; onClick?: () => void }): { dispose(): void }
    registerInputExtension(ext: { id: string; label: string; icon?: string; onActivate?: () => void }): { dispose(): void }
    registerTerminalToolbarItem(item: { id: string; label: string; icon?: string; onClick?: () => void }): { dispose(): void }
    registerTitleBarItem(item: { id: string; label: string; icon?: string; onClick?: () => void }): { dispose(): void }
    registerFileHandler(handler: { id: string; extensions: string[]; component: any }): { dispose(): void }
  }
  readonly events: {
    on(event: string, handler: (...args: any[]) => void): { dispose(): void }
    emit(event: string, ...args: any[]): void
  }
  readonly storage: {
    get<T = any>(key: string): Promise<T | undefined>
    set(key: string, value: any): Promise<void>
    delete(key: string): Promise<void>
    flush(): Promise<void>
  }
  readonly http: {
    registerEndpoint(path: string, handler: any): { dispose(): void }
  }
  readonly _disposables: { dispose(): void }[]
}

export async function activate(context: PluginContext): Promise<void> {
  // 将 PluginContext 暴露给 Vue 组件（通过全局变量）
  ;(window as any).__ai_chatbox_context__ = context

  // 注册侧边栏面板
  context.ui.registerSidebarPanel({
    id: 'ai-chatbox.sidebar',
    title: 'AI 对话',
    component: ChatView,
  })

  // 终端提示词优化
  const optimizer = usePromptOptimizer(
    // getActiveProvider：从 storage 读取当前活跃 provider
    async () => {
      const providers = await context.storage.get<string>('apiProviders')
      const activeName = await context.storage.get<string>('activeProvider')
      if (!providers || !activeName) return undefined
      try {
        const parsed = typeof providers === 'string' ? JSON.parse(providers) : providers
        const list = Array.isArray(parsed) ? parsed : []
        return list.find((p: ApiProvider) => p.name === activeName)
      } catch {
        return undefined
      }
    },
    // sendInput：代理 context.terminal.sendInput
    (sessionId, text) => context.terminal.sendInput(sessionId, text),
    // eventEmit：使用插件事件系统
    (event, ...args) => context.events.emit(event, ...args),
    // eventOn：使用插件事件系统
    (pluginId, event, handler) => context.events.on(event, handler),
  )

  // 将 optimizer 状态暴露给 ChatView（PromptOptimizeDialog 需要读取）
  ;(window as any).__ai_chatbox_optimizer__ = optimizer

  // 注册终端工具栏按钮
  context.ui.registerTerminalToolbarItem({
    id: 'ai-optimize-prompt',
    label: 'AI 优化',
    icon: '✨',
    onClick: () => optimizer.optimizePrompt(),
  })

  console.log('[AI Chatbox] Plugin activated')
}

export async function deactivate(): Promise<void> {
  delete (window as any).__ai_chatbox_context__
  delete (window as any).__ai_chatbox_optimizer__
  console.log('[AI Chatbox] Plugin deactivated')
}
