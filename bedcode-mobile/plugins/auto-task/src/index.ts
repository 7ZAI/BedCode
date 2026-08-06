/**
 * Auto Task 插件入口 (Mobile)
 *
 * 终端工具栏按钮 — 面板由宿主 PluginTerminalBar 管理
 */
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'

let _ctx: PluginContext

export async function activate(context: PluginContext): Promise<void> {
  _ctx = context
  context.logger.info('Auto Task plugin activating...')

  // 图标使用 SVG path（Heroicons outline 风格，与终端工具栏按钮一致），不使用 emoji
  context.ui.registerTerminalToolbarItem({
    id: 'auto-task-toolbar',
    label: context.i18n.t('mobile.autoTask.title'),
    icon: 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 7l2 2 4-4',
    onClick: () => {
      // 面板切换由宿主 PluginTerminalBar.handleClick 路由
    },
  })

  context.logger.info('Auto Task plugin activated')
}

export async function deactivate(): Promise<void> {
  _ctx?.logger.info('Auto Task plugin deactivated')
}
