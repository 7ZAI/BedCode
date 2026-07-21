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

  context.ui.registerTerminalToolbarItem({
    id: 'auto-task-toolbar',
    label: context.i18n.t('mobile.autoTask.title'),
    icon: '📋',
    onClick: () => {
      // 面板切换由宿主 PluginTerminalBar.handleClick 路由
    },
  })

  context.logger.info('Auto Task plugin activated')
}

export async function deactivate(): Promise<void> {
  _ctx?.logger.info('Auto Task plugin deactivated')
}
