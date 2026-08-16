/**
 * OCR 插件入口 (Mobile)
 *
 * activate：
 *   1. 注册 i18n 消息（key 自动加插件 id 前缀）
 *   2. 注入插件全局样式（宿主不加载插件 dist/style.css，运行时注入一次）
 *   3. 注册工具箱视图（component=主页 + entry=入口卡片）
 *   4. 注册设置区（宿主 registerSettingsSection：模型管理）
 *   5. 注册结果页路由（context.ui.openPage('result') 跳转）
 *
 * 识别命令不经 WASM：前端经 context.ocr.* 直通宿主命令（spec §6），
 * Rust 壳仅为激活/停用日志的最小实现。
 */
import OcrView from './components/OcrView.vue'
import ToolboxEntry from './components/ToolboxEntry.vue'
import OcrSettings from './components/OcrSettings.vue'
import ResultPage from './components/ResultPage.vue'
import { messages } from './i18n'
import './styles.css'
import { devMock } from './mock'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'

export { devMock }

export async function activate(context: PluginContext): Promise<void> {
  // 1. 注册 i18n 消息（必须在组件 setup 前完成，保证模板取文案可用）
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // 2. 注册工具箱入口 + 主页
  const toolbox = context.ui.registerToolboxPage({
    id: 'ocr.toolbox',
    title: 'OCR',
    // SVG path d（宿主 isSvgIcon 识别 M 开头渲染为 stroke 图标，禁 emoji）
    icon: 'M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m5.231 13.481L15 17.25m-4.5-15H5.625c-.621 0-1.125.504-1.125 1.125v16.5c0 .621.504 1.125 1.125 1.125h10.5c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9zm3.75 11.625a2.25 2.25 0 100-4.5 2.25 2.25 0 000 4.5z',
    component: OcrView,
    entry: ToolboxEntry,
  })
  context._disposables.push(toolbox)

  // 3. 设置区（模型管理）
  const settings = context.ui.registerSettingsSection({
    id: 'ocr.settings',
    pluginId: context.id,
    section: 'ocr',
    component: OcrSettings,
  })
  context._disposables.push(settings)

  // 4. 结果页路由
  const route = context.ui.registerRoute({
    id: 'result',
    title: context.i18n.t('ocr.result.title'),
    component: ResultPage,
  })
  context._disposables.push(route)

  context.logger.info('OCR plugin activated (wasm shell; recognize via host commands)')
}

export async function deactivate(): Promise<void> {
  // 视图/设置/路由经 _disposables 由宿主统一清理
}
