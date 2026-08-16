/**
 * File Transfer 插件入口 (Mobile)
 *
 * activate：
 *   1. 注册 i18n 消息（key 自动加插件 id 前缀）
 *   2. 注入插件全局样式（宿主不加载插件 dist/style.css，运行时注入一次）
 *   3. 注册工具箱视图（component=浏览主页面 + entry=入口卡片带状态角标）
 *   4. 注册设置区（宿主 registerSettingsSection；同时插件内设置页复用）
 *
 * 事件监听由组件内的 useTasks.start() 注册（入口卡与浏览页各自启动），
 * 组件卸载/插件停用时经 context._disposables 与组件 onUnmounted 清理。
 */
import FileTransferView from './components/FileTransferView.vue'
import ToolboxEntry from './components/ToolboxEntry.vue'
import SettingsSection from './components/SettingsSection.vue'
import SettingsPage from './components/SettingsPage.vue'
import { messages } from './i18n'
import styles from './styles.css?inline'
import type { PluginContext, PluginDevMock } from '@binblink/plugin-sdk-mobile'

const STYLE_ID = 'file-transfer-plugin-style'

/**
 * dev-shell 领域数据：SAF 目录树 + 免授权目录浏览条目（浏览器 mock 宿主用）
 * 仅 dev-shell 消费（见 PluginDevMock 协议），真实宿主忽略此导出
 */
export const devMock: PluginDevMock = {
  safTree: {
    'mock:root': [
      { name: '文档资料', isDir: true, size: 0, mime: 'application/vnd.google-apps.folder', docId: 'mock:docs' },
      { name: '安装包.zip', isDir: false, size: 190_000_000, mime: 'application/zip', docId: 'mock:zip' },
      { name: '旅行记录.mp4', isDir: false, size: 1_450_000_000, mime: 'video/mp4', docId: 'mock:mp4' },
      { name: '季度报告.pptx', isDir: false, size: 4_400_000, mime: 'application/vnd.openxmlformats-officedocument.presentationml.presentation', docId: 'mock:pptx' },
    ],
    'mock:docs': [
      { name: '合同扫描件.pdf', isDir: false, size: 8_600_000, mime: 'application/pdf', docId: 'mock:pdf' },
      { name: '会议纪要.docx', isDir: false, size: 350_000, mime: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', docId: 'mock:docx' },
    ],
  },
  listDirEntries: [
    { name: 'report.pdf', isDir: false, size: 2_400_000, mime: 'application/pdf' },
    { name: 'photo.jpg', isDir: false, size: 4_800_000, mime: 'image/jpeg' },
    { name: 'archive.zip', isDir: false, size: 190_000_000, mime: 'application/zip' },
  ],
}

export async function activate(context: PluginContext): Promise<void> {
  // 0. 清扫中转复制残留（spec「复制桥语义」：插件激活时扫描清理 cache 残留）
  //    非 Android / 宿主未就绪时静默降级（dev-shell 无真实 cache）
  void context.fileService.saf.cleanupStaleCopies().catch(() => {})

  // 1. 注册 i18n 消息（必须在组件 setup 前完成，保证模板取文案可用）
  for (const [locale, msgs] of Object.entries(messages)) {
    context.i18n.registerMessages(locale, msgs)
  }

  // 2. 注入插件样式（幂等，热重载不重复插入）
  if (!document.getElementById(STYLE_ID)) {
    const styleEl = document.createElement('style')
    styleEl.id = STYLE_ID
    styleEl.textContent = styles
    document.head.appendChild(styleEl)
  }

  // 3. 工具箱视图：component 为浏览主页面，entry 为带状态角标的入口卡片
  context.ui.registerToolboxPage({
    id: 'file-transfer.toolbox',
    title: context.i18n.t('transfer.toolbox.title'),
    icon: 'M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4',
    component: FileTransferView,
    entry: ToolboxEntry,
  })

  // 4. 设置二级页动态路由：宿主 addRoute 至 /mobile/plugins/{pluginId}/settings，
  //    齿轮入口经 ui.openPage('settings') 整体跳转。
  //    header: false — 插件自行渲染页头，避免与 ToolboxView 已有的 < 文件传输 页头重复
  context.ui.registerRoute({
    id: 'settings',
    title: context.i18n.t('transfer.settings.title'),
    component: SettingsPage,
    header: false,
  })

  // 5. 设置区贡献（宿主 registry 支持；当前 SettingsView 尚未渲染插件设置区，
  //    插件内 FileTransferView 顶部齿轮亦入口 SettingsSection）
  context.ui.registerSettingsSection({
    id: 'file-transfer.settings',
    pluginId: context.id,
    section: 'file-transfer',
    component: SettingsSection,
  })

  context.logger.info('File Transfer plugin activated (wasm mode, mobile)')
}

export async function deactivate(): Promise<void> {
  // 样式保留（幂等），组件级监听已在卸载时清理；
  // 注册表/事件由宿主 loader 依据 context._disposables 统一摘除
  console.log('[File Transfer] Plugin deactivated')
}
