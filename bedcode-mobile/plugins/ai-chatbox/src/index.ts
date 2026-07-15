/**
 * AI Chatbox 插件入口 (Mobile)
 *
 * 工具箱 AI 对话面板 + 底部导航 Tab + 终端提示词优化
 */
import ChatView from './components/ChatView.vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'

export async function activate(context: PluginContext): Promise<void> {
  context.ui.registerToolboxPage({
    id: 'ai-chatbox.toolbox',
    title: 'AI 对话',
    component: ChatView,
  })

  context.ui.registerNavTab({
    id: 'ai-chatbox.navtab',
    title: 'AI',
    icon: '💬',
    component: ChatView,
    order: 10,
  })

  context.ui.registerTerminalToolbarItem({
    id: 'ai-optimize-prompt',
    label: 'AI 优化',
    icon: '✨',
    onClick: () => context.events.emit('ai-chatbox:triggerOptimize'),
  })

  console.log('[AI Chatbox] Plugin activated (wasm mode, mobile)')
}

export async function deactivate(): Promise<void> {
  console.log('[AI Chatbox] Plugin deactivated')
}
