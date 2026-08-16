/**
 * {{NAME}} 插件入口 (Mobile)
 *
 * 最小可编译模板：激活时注册一个终端工具栏项
 */
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'

let _ctx: PluginContext

export async function activate(context: PluginContext): Promise<void> {
  _ctx = context
  context.logger.info('{{NAME}} plugin activating...')

  context.ui.registerTerminalToolbarItem({
    id: '{{ID}}.toolbar',
    label: '{{NAME}}',
    icon: '🧩',
    onClick: () => {
      // 点击终端工具栏按钮时的行为
    },
  })

  context.logger.info('{{NAME}} plugin activated')
}

export async function deactivate(): Promise<void> {
  _ctx?.logger.info('{{NAME}} plugin deactivated')
}
