/**
 * {{NAME}} 插件入口 (Desktop)
 *
 * 最小可编译模板：注册一个侧边栏面板（内联渲染函数组件，
 * 面板 UI 较大时建议拆成独立 .vue 组件引入）。
 */
import { defineComponent, h } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'

let _ctx: PluginContext

export async function activate(context: PluginContext): Promise<void> {
  _ctx = context

  // 注册侧边栏面板（宿主经 PluginViewHost provide pluginContext，组件内可 inject）
  context.ui.registerSidebarPanel({
    id: '{{ID}}.sidebar',
    title: '{{NAME}}',
    icon: 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z',
    order: 600,
    component: defineComponent({
      name: '{{STRUCT}}Panel',
      render: () => h('div', { style: 'padding: 16px; color: var(--text-secondary); font-size: 13px' }, '{{NAME}} 面板内容 — 在这里编写你的插件 UI'),
    }),
  })

  console.log('[{{ID}}] Plugin activated')
}

export async function deactivate(): Promise<void> {
  console.log('[{{ID}}] Plugin deactivated')
}
