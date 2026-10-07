/**
 * 终端宿主能力注入（票 03a；票 05 收口；2026-09-24 随窗口宿主通用化从视图壳拆出）
 *
 * 窗口宿主（`@/views/PluginWindowHostView.vue`）在 setup 期调用本函数，把「离宿主
 * 无法实现」的宿主原语 provide 给渲染出来的插件视图（ADR 0022 裁剪线判断）：
 * 终端渲染 / 写入 / IME 全在下沉的会话插件里，但**宿主存储面与命令面**仍在宿主——
 * settingsStore 持久化、`set_terminal_bg_image` 文件复制命令、宿主扩展点 registry
 * 的响应式数组，三者插件侧都拿不到，只能经注入桥接。
 *
 * 注入键 `terminalHostCapabilities`：未被渲染插件消费时无副作用（插件侧
 * `useTerminalHostCapabilities()` 取不到会回落内存版）。
 *
 * 注入契约类型真源：`wasm-apps/terminal-session/src/components/terminal/
 * terminalHostCapabilities.ts`（宿主不 import 插件，就地定义同构结构）。
 */
import { provide, reactive, computed, ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { invoke } from '@tauri-apps/api/core'
import { getPluginRegistry } from './registry'
import { useSettingsStore } from '@/stores/settings'
import { logger } from '@/utils/frontendLogger'
import { TERMINAL_HOST_CAPABILITIES_KEY } from './terminal-host-capabilities-contract'
import type { TerminalSettingsAccessor } from './terminal-host-capabilities-contract'

/** 背景图片选择允许的图片扩展名（与宿主旧 TerminalWindowView 一致） */
const BG_IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'ico']

/** 端口预取失败时的兜底值（服务器永久自启动；正常路径由 `get_server_status` 覆盖） */
const FALLBACK_SERVER_PORT = 8080

/**
 * 向当前组件树注入终端宿主能力（必须在 setup 期调用）
 */
export function provideTerminalHostCapabilities(): void {
  const settingsStore = useSettingsStore()
  const registry = getPluginRegistry()

  // 背景图 URL 端口：`/static/terminal-bg` 挂在宿主主服务器上（端口由 supervisor 持有，
  // 用户可改），插件侧的 `getServerPort()` 是**同步**接口 → 打开窗口时预取一次写入 ref。
  // 旧实现硬编码 8080：与默认端口不符时背景图预加载必然失败
  // （`Failed to resolve background image URL`，2026-09-26 修复）。
  const serverPort = ref(FALLBACK_SERVER_PORT)
  async function refreshServerPort(): Promise<void> {
    try {
      const info = await invoke<{ port?: number }>('get_server_status')
      if (info?.port) serverPort.value = info.port
    } catch (e) {
      logger.warn('[TerminalHostCapabilities] Failed to resolve server port, keep fallback:', e)
    }
  }
  void refreshServerPort()

  // ==================== 终端设置桥（TerminalSettingsAccessor） ====================
  const settings: TerminalSettingsAccessor = {
    getFontSize: () => settingsStore.settings.ui.terminal_font_size ?? 12,
    getTheme: () => settingsStore.settings.ui.terminal_theme || 'dracula',
    getBgImage: () => settingsStore.settings.ui.terminal_bg_image || '',
    getBgOpacity: () => settingsStore.settings.ui.terminal_bg_opacity ?? 30,
    getLetterSpacing: () => settingsStore.settings.ui.terminal_letter_spacing ?? 0,
    getServerPort: () => serverPort.value,
    save: (patch) => {
      // 背景图变化：先刷新端口再落盘——插件侧 watch(落盘后读到的新设置值) 会**同步**
      // 解析一次背景图 URL，端口必须先就位（否则用陈旧端口解析出不可达 URL）
      const pending = patch.bgImage != null ? refreshServerPort() : Promise.resolve()
      void pending.then(() =>
        settingsStore.saveSettings({
          ui: {
            ...settingsStore.settings.ui,
            ...(patch.fontSize != null ? { terminal_font_size: patch.fontSize } : {}),
            ...(patch.letterSpacing != null ? { terminal_letter_spacing: patch.letterSpacing } : {}),
            ...(patch.theme != null ? { terminal_theme: patch.theme } : {}),
            ...(patch.bgImage != null ? { terminal_bg_image: patch.bgImage } : {}),
            ...(patch.bgOpacity != null ? { terminal_bg_opacity: patch.bgOpacity } : {}),
          },
        }),
      )
    },
    onChange: () => () => {}, // 终端设置只由终端窗口设置面板读写（单写者），无外部变化源
  }

  // ==================== 背景图命令桥 ====================

  /** 选择系统图片文件并设为终端背景（复制到应用数据目录，避免原图移动/删除后失效） */
  async function pickAndSetBgImage(): Promise<boolean> {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Background Image', extensions: BG_IMAGE_EXTENSIONS }],
      })
      if (!selected || typeof selected !== 'string') return false
      const fileName = await invoke<string | null>('set_terminal_bg_image', { sourcePath: selected })
      if (fileName) {
        // 端口先于设置落盘就位（同 accessor.save 的时序理由），再写入文件名
        await refreshServerPort()
        // 设置中存原始文件名用于回显，实际复制文件由后端统一命名为 terminal_bg.<ext>
        const displayName = selected.split(/[\\/]/).pop() || fileName
        await settingsStore.saveSettings({
          ui: { ...settingsStore.settings.ui, terminal_bg_image: displayName },
        })
      }
      return true
    } catch (e) {
      logger.error('[TerminalHostCapabilities] Failed to set background image:', e)
      return false
    }
  }

  async function removeBgImage(): Promise<void> {
    try {
      await invoke('set_terminal_bg_image', { sourcePath: null })
      await settingsStore.saveSettings({
        ui: { ...settingsStore.settings.ui, terminal_bg_image: '' },
      })
    } catch (e) {
      logger.error('[TerminalHostCapabilities] Failed to remove background image:', e)
    }
  }

  const bgImageName = computed(() => {
    const v = settingsStore.settings.ui.terminal_bg_image
    if (!v) return ''
    return v.split(/[\\/]/).pop() || v
  })

  // bgImage 桥整体包 reactive：imageName/hasImage 是 computed 属性（reactive 自动
  // 解包），插件壳模板访问时响应式依赖跟随 settingsStore 变化，不依赖 provide 时点快照
  const bgImageBridge = reactive({
    pickAndSet: pickAndSetBgImage,
    remove: removeBgImage,
    imageName: computed(() => bgImageName.value),
    hasImage: computed(() => !!settingsStore.settings.ui.terminal_bg_image),
  })

  provide(TERMINAL_HOST_CAPABILITIES_KEY, {
    settings,
    bgImage: bgImageBridge,
    // 插件扩展点：宿主 registry 响应式数组直接注入（插件壳复刻渲染按钮；
    // TerminalExtensionItem 结构镜像——id/label/icon/pluginId/onClick 字段宿主侧超集）
    extensions: {
      terminalToolbarItems: registry.terminalToolbarItems,
      titleBarItems: registry.titleBarItems,
      pageToolbarItems: registry.pageToolbarItems,
    },
  })
}
