/**
 * File Transfer 插件入口 (Mobile) — 独立页面版（宿主壳运行面）
 *
 * activate：
 *   1. 注册 i18n 消息（key 自动加插件 id 前缀）
 *   2. 注入插件全局样式（宿主不加载插件 dist/style.css，运行时注入一次）
 *   3. 注册宿主壳运行面（新设计界面 /mobile/shell 的独立页面；不再以工具箱页
 *      嵌入旧宿主——票 2026-10-09-mobile-host-into-wasm-apps）
 *   4. 保留设置动态路由与设置区贡献（设置仍可达；退役随旧宿主同批）
 *
 * 主视图 FileTransferView 采用三段式布局：传输列表默认可见、浏览与设备
 * 折叠为 tab，底栏按多选/活跃/上传三态收敛唯一入口，齿轮进设置。领域逻辑
 * （useTasks / usePeerDevices / useRemoteFs / useSettings）由组件内 setup 启动，
 * 组件卸载/插件停用时经 context._disposables 与组件 onUnmounted 清理。
 */
import FileTransferView from './components/FileTransferView.vue'
import SettingsPage from './components/SettingsPage.vue'
import { messages } from './i18n'
import styles from './styles.css?inline'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import peerDevMock from './devMock'
import { useConsent, type ConsentController } from './composables/useConsent'
import { useBatchDialog, type BatchDialogController } from './composables/useBatchDialog'

// dev-shell 领域种子数据（SDK PluginDevMock 协议；真实宿主忽略）
export const devMock = peerDevMock

const STYLE_ID = 'file-transfer-plugin-style'

/** 首连确认编排（激活期常驻单例；deactivate 时对称停止） */
let consentController: ConsentController | null = null
/** 批量传输请求全局弹窗（激活期常驻，宿主全局弹窗，跨页面可达） */
let batchDialogController: BatchDialogController | null = null
/** 激活期捕获的统一日志入口（deactivate 无 context 入参；与旧宿主 logger 机制一致） */
let activeLogger: PluginContext['logger'] | null = null

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

  // 3. 宿主壳运行面：独立页面（不再是旧宿主工具箱页；appId = 插件 id 由宿主代填）
  context.ui.registerSurface({ component: FileTransferView })

  // 4. 设置二级页动态路由：header: false — 插件自行渲染页头（FileTransferView 齿轮入口）
  context.ui.registerRoute({
    id: 'settings',
    title: context.i18n.t('transfer.settings.title'),
    component: SettingsPage,
    header: false,
  })

  // 5. 壳内设置入口（旧宿主「设置区」随阶段 B 退役：壳设置屏按 settingsEntries 渲染，
  //    点击直达本应用已注册的设置路由）
  context.ui.registerSettingsEntry({
    id: 'file-transfer.settings',
    label: context.i18n.t('transfer.settings.title'),
    order: 10,
    onSelect: () => {
      void context.ui.openPage('settings')
    },
  })

  // 6. 首连确认编排：激活期常驻订阅（不依赖视图挂载），确认框经插件
  // 对话框 API 全局弹出，终端配对迁移规则静默互信 + toast（spec 决策 7）
  consentController = useConsent(context)
  consentController.start()

  // 7. 批量传输请求全局弹窗：激活期常驻订阅（不依赖视图挂载），经宿主
  // 全局弹窗（context.ui.showDialog 预设模式）渲染，任何页面可见
  batchDialogController = useBatchDialog(context)
  batchDialogController.start()

  activeLogger = context.logger
  activeLogger.info('[File Transfer] plugin activated (standalone-page, mobile)')
}

export async function deactivate(): Promise<void> {
  consentController?.stop()
  consentController = null
  batchDialogController?.stop()
  batchDialogController = null
  // 样式保留（幂等），组件级监听已在卸载时清理；
  // 注册表/事件由宿主 loader 依据 context._disposables 统一摘除
  activeLogger?.info('[File Transfer] plugin deactivated')
  activeLogger = null
}
