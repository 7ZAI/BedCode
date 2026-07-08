/**
 * AI Chatbox 插件入口
 *
 * 侧边栏 AI 对话面板 + 终端提示词优化
 * cdylib 插件架构：Rust 后端处理 AI 请求，前端通过 PluginContext 调用
 */
import ChatView from './components/ChatView.vue'
import type { PluginContext } from '../../plugin/types'

export async function activate(context: PluginContext): Promise<void> {
  // 注册侧边栏面板
  // PluginViewHost 会自动 provide('pluginContext', context)，
  // ChatView 通过 inject('pluginContext') 获取
  context.ui.registerSidebarPanel({
    id: 'ai-chatbox.sidebar',
    title: 'AI 对话',
    component: ChatView,
  })

  // 注册终端工具栏按钮 — 点击后由 ChatView 内部的 PromptOptimizer 处理
  // 通过事件桥接：工具栏按钮 emit 事件，ChatView 内监听并触发优化流程
  context.ui.registerTerminalToolbarItem({
    id: 'ai-optimize-prompt',
    label: 'AI 优化',
    icon: '✨',
    onClick: () => context.events.emit('ai-chatbox:triggerOptimize'),
  })

  console.log('[AI Chatbox] Plugin activated (rust-ts mode)')
}

export async function deactivate(): Promise<void> {
  console.log('[AI Chatbox] Plugin deactivated')
}
