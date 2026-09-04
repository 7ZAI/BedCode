/**
 * File Transfer 插件入口 (Mobile) — 三段式主视图版
 *
 * activate：
 *   1. 注册 i18n 消息（key 自动加插件 id 前缀）
 *   2. 注入插件全局样式（宿主不加载插件 dist/style.css，运行时注入一次）
 *   3. 注册工具箱视图（component=三段式主视图 传输/浏览/设备 + entry=入口卡片）
 *   4. 注册设置区与设置二级页路由
 *
 * 主视图 FileTransferView 采用三段式布局：传输列表默认可见、浏览与设备
 * 折叠为 tab，底栏按多选/活跃/上传三态收敛唯一入口。领域逻辑（useTasks /
 * usePeerDevices / useRemoteFs / useSettings）由组件内 setup 启动，组件
 * 卸载/插件停用时经 context._disposables 与组件 onUnmounted 清理。
 *
 * 历史：此入口早期为「浏览为主 + 底部 sheet」布局（FileTransferView/TaskQueueSheet/
 * PeerDevicesSheet），重构为三段式后旧实现已并入并删除，src/v2/ 并行层随之清理。
 */
import FileTransferView from './components/FileTransferView.vue'
import ToolboxEntry from './components/ToolboxEntry.vue'
import SettingsSection from './components/SettingsSection.vue'
import SettingsPage from './components/SettingsPage.vue'
import { messages } from './i18n'
import styles from './styles.css?inline'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
import peerDevMock from './devMock'
import { useConsent, type ConsentController } from './composables/useConsent'

// dev-shell 领域种子数据（SDK PluginDevMock 协议；真实宿主忽略）
export const devMock = peerDevMock

const STYLE_ID = 'file-transfer-plugin-style'

/** 首连确认编排（激活期常驻单例；deactivate 时对称停止） */
let consentController: ConsentController | null = null

export async function activate(context: PluginContext): Promise<void> {
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

  // 3. 工具箱视图：component 为主页面（三段式），entry 为带状态角标的入口卡片
  context.ui.registerToolboxPage({
    id: 'file-transfer.toolbox',
    title: context.i18n.t('transfer.toolbox.title'),
    icon: 'M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4',
    component: FileTransferView,
    entry: ToolboxEntry,
  })

  // 4. 设置二级页动态路由：header: false — 插件自行渲染页头
  context.ui.registerRoute({
    id: 'settings',
    title: context.i18n.t('transfer.settings.title'),
    component: SettingsPage,
    header: false,
  })

  // 5. 设置区贡献（宿主 registry 支持；插件内齿轮亦入口 SettingsSection）
  context.ui.registerSettingsSection({
    id: 'file-transfer.settings',
    pluginId: context.id,
    section: 'file-transfer',
    component: SettingsSection,
  })

  // 6. 首连确认编排：激活期常驻订阅（不依赖视图挂载），确认框经插件
  // 对话框 API 全局弹出，终端配对迁移规则静默互信 + toast（spec 决策 7）
  consentController = useConsent(context)
  consentController.start()

  context.logger.info('File Transfer plugin activated (3-section view, mobile)')
}

export async function deactivate(): Promise<void> {
  consentController?.stop()
  consentController = null
  // 样式保留（幂等），组件级监听已在卸载时清理；
  // 注册表/事件由宿主 loader 依据 context._disposables 统一摘除
  console.log('[File Transfer] Plugin deactivated')
}
