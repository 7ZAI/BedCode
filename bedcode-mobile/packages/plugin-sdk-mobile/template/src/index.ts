/**
 * {{NAME}} 应用入口 (Mobile)
 *
 * 最小可编译模板：激活时注册一个宿主壳运行面（registerSurface）。
 *
 * 票 2026-10-10 批次 C2：模板原用 `registerTerminalToolbarItem`（终端工具栏项），
 * 该扩展点已随宿主壳改纯 surface 形态整面退役。应用在壳内的运行面只有一种形态——
 * `registerSurface`：应用自持整个界面（含底部导航与页签），壳不向其内加壳 chrome。
 */
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import { defineComponent, h } from 'vue'

let _ctx: PluginContext

export async function activate(context: PluginContext): Promise<void> {
  _ctx = context
  context.logger.info('{{NAME}} app activating...')

  context.ui.registerSurface({
    component: defineComponent({
      name: '{{ID}}Surface',
      setup() {
        return () =>
          h('div', { class: 'p-4 text-sm' }, [
            h('p', '{{NAME}} 运行面'),
            h('p', { class: 'opacity-70 mt-2' }, '在这里放你的应用界面（底部导航 / 页签由应用自持）'),
          ])
      },
    }),
  })

  context.logger.info('{{NAME}} app activated')
}

export async function deactivate(): Promise<void> {
  _ctx?.logger.info('{{NAME}} app deactivated')
}
