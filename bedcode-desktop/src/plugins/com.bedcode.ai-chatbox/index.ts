/**
 * AI Chatbox 插件入口
 *
 * 侧边栏 AI 对话面板 + 终端提示词优化
 * cdylib 插件架构：Rust 后端处理 AI 请求，前端通过 PluginContext 调用
 */
import { provide } from 'vue'
import ChatView from './components/ChatView.vue'
import { usePromptOptimizer } from './composables/usePromptOptimizer'
import type { ApiProvider } from './types'
import type { PluginContext } from '../../plugin/types'

export async function activate(context: PluginContext): Promise<void> {
  // 将 PluginContext 通过 provide/inject 传递给 Vue 组件
  // 在注册组件时包装一层，自动 provide context
  const contextProvidedChatView = {
    name: 'ChatViewWithContext',
    setup(_props: any, { slots }: any) {
      provide('pluginContext', context)
      return () => ChatView
    },
  }

  // 注册侧边栏面板
  context.ui.registerSidebarPanel({
    id: 'ai-chatbox.sidebar',
    title: 'AI 对话',
    component: ChatView,
  })

  // 终端提示词优化
  const optimizer = usePromptOptimizer(context)

  // 注册终端工具栏按钮
  context.ui.registerTerminalToolbarItem({
    id: 'ai-optimize-prompt',
    label: 'AI 优化',
    icon: '✨',
    onClick: () => optimizer.optimizePrompt(),
  })

  console.log('[AI Chatbox] Plugin activated (rust-ts mode)')
}

export async function deactivate(): Promise<void> {
  console.log('[AI Chatbox] Plugin deactivated')
}
