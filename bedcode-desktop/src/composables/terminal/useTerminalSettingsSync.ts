/**
 * 终端设置同步域（TerminalPreview 拆分产物）
 *
 * 职责：字号 / 主题 / 背景图片三个设置项的响应式状态、外部设置变化同步、
 * 变化后应用到 xterm（含防抖持久化）、背景图片 URL 解析与预加载校验。
 *
 * 依赖：共享内核 ctx（terminalRef / fitAddonRef / isLinux / bgImageUrl +
 * fitAndRefresh / syncTerminalSize / rebuildRenderer 回调）。
 */
import type { TerminalKernelContext } from './terminalKernel'
import { ref, computed, watch, nextTick } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import { invoke } from '@tauri-apps/api/core'
import { logger } from '@/utils/frontendLogger'
import { PLATFORM_UI_SCALE } from '@/composables/useFontSize'
import {
  TERMINAL_THEME_SELECT_OPTIONS,
  TERMINAL_FONT_SIZE_SELECT_OPTIONS,
  buildTerminalTheme,
  getTerminalContainerBg,
} from '@/utils/terminalThemes'

export function useTerminalSettingsSync(ctx: TerminalKernelContext) {
  const settingsStore = useSettingsStore()

  const fontSize = ref(settingsStore.settings.ui.terminal_font_size)
  const terminalTheme = ref<string>(settingsStore.settings.ui.terminal_theme || 'dracula')

  // 终端视觉字号：去 zoom 后（issue 06）Linux 按 PLATFORM_UI_SCALE 放大，视觉字号
  // = 设置值 × 1.15，等效原 zoom；用户设置值（terminal_font_size）与下拉显示保持原值不乘
  const effectiveFontSize = computed(() =>
    ctx.isLinux.value ? fontSize.value * PLATFORM_UI_SCALE : fontSize.value,
  )

  // 背景图片：设置中存原始文件名（仅用于判断是否启用与回显），
  // 实际图片由本地服务器 /static/terminal-bg 端点提供
  const bgImage = ref<string>(settingsStore.settings.ui.terminal_bg_image || '')
  const bgOpacity = ref<number>(settingsStore.settings.ui.terminal_bg_opacity ?? 30)
  // 已解析 URL 存于内核（渲染器 computed 也读取）
  const bgImageUrl = ctx.bgImageUrl

  // 主题/字号下拉选项：与原生 <option> 一一对应，供共享 Select 使用
  const themeSelectOptions = TERMINAL_THEME_SELECT_OPTIONS
  const fontSizeSelectOptions = TERMINAL_FONT_SIZE_SELECT_OPTIONS

  /** 构造当前主题：背景图片启用时终端背景设为全透明，让图片层透出 */
  function getTheme(): object {
    return buildTerminalTheme(terminalTheme.value, !!bgImageUrl.value)
  }

  /** 终端容器底色：背景图片启用时 xterm 背景透明，由容器补上主题背景色 */
  const containerBgColor = computed(() => getTerminalContainerBg(terminalTheme.value))

  /** 解析背景图片 URL：本地服务器静态端点提供图片（先查实际运行端口，?t= 时间戳防缓存） */
  async function resolveBgImageUrl() {
    if (!bgImage.value) {
      bgImageUrl.value = ''
      return
    }
    try {
      const status = await invoke<{ port: number }>('get_server_status')
      // 端口为 0 表示服务器尚未启动，回退到配置端口（服务器可能稍后启动）
      const port = status.port || settingsStore.settings.network.port
      const url = `http://127.0.0.1:${port}/static/terminal-bg?t=${Date.now()}`
      // 预加载校验：图片不可达（服务器未启动/404 等）时不启用透明主题，
      // 避免终端背景已切为全透明、图片却加载不出来，看起来像丢失了背景色
      await new Promise<void>((resolve, reject) => {
        const probe = new Image()
        probe.onload = () => resolve()
        probe.onerror = () => reject(new Error(`background image not loadable: ${url}`))
        probe.src = url
      })
      bgImageUrl.value = url
    } catch (e) {
      logger.error('[TerminalPreview] Failed to resolve background image URL:', e)
      bgImageUrl.value = ''
    }
  }

  // 字体大小变化（用户侧）：应用 + fit + 同步 PTY + 防抖持久化
  let fontSizeSaveTimeout: ReturnType<typeof setTimeout> | null = null
  watch(fontSize, (newSize) => {
    const terminal = ctx.terminalRef.value
    if (!terminal) return
    terminal.options.fontSize = effectiveFontSize.value
    if (ctx.fitAddonRef.value) {
      ctx.callbacks.fitAndRefresh()
    }
    nextTick(() => ctx.callbacks.syncTerminalSize())
    if (fontSizeSaveTimeout) clearTimeout(fontSizeSaveTimeout)
    fontSizeSaveTimeout = setTimeout(() => {
      settingsStore.saveSettings({
        ui: { ...settingsStore.settings.ui, terminal_font_size: newSize },
      })
    }, 300)
  })

  // 外部设置变化同步字号
  watch(
    () => settingsStore.settings.ui.terminal_font_size,
    (newSize) => {
      if (fontSize.value !== newSize) {
        fontSize.value = newSize
        const terminal = ctx.terminalRef.value
        if (terminal) {
          terminal.options.fontSize = effectiveFontSize.value
          if (ctx.fitAddonRef.value) ctx.callbacks.fitAndRefresh()
          nextTick(() => ctx.callbacks.syncTerminalSize())
        }
      }
    },
    { immediate: true },
  )

  // 主题变化（用户侧）：更新终端 + 防抖持久化
  let themeSaveTimeout: ReturnType<typeof setTimeout> | null = null
  watch(terminalTheme, (newTheme) => {
    const terminal = ctx.terminalRef.value
    if (terminal) {
      terminal.options.theme = getTheme()
    }
    if (themeSaveTimeout) clearTimeout(themeSaveTimeout)
    themeSaveTimeout = setTimeout(() => {
      settingsStore.saveSettings({
        ui: { ...settingsStore.settings.ui, terminal_theme: newTheme },
      })
    }, 300)
  })

  // 外部设置变化同步主题
  watch(
    () => settingsStore.settings.ui.terminal_theme,
    (newTheme) => {
      if (newTheme && terminalTheme.value !== newTheme) {
        terminalTheme.value = newTheme
      }
    },
  )

  // 外部设置变化同步背景图片配置
  watch(
    () => settingsStore.settings.ui.terminal_bg_image,
    (v) => {
      bgImage.value = v || ''
    },
  )
  watch(
    () => settingsStore.settings.ui.terminal_bg_opacity,
    (v) => {
      if (v != null) bgOpacity.value = v
    },
  )

  // 背景图片变化：重新解析 URL 并刷新终端主题（透明/不透明切换）
  watch(bgImage, () => {
    resolveBgImageUrl()
  })
  watch([bgImageUrl, bgOpacity], (newVals, oldVals) => {
    const terminal = ctx.terminalRef.value
    if (!terminal) return
    const [newUrl] = newVals as [string, number]
    const [oldUrl] = oldVals as [string, number]
    // 透明度有无切换以 bgImageUrl 有无为准（背景图开关 = 透明开关）。
    // 为什么必须重建而非只改 options：addon-webgl 0.19.0 不监听
    // allowTransparency 的运行时变化（_setTransparency 是零调用的死代码），
    // 渲染层 alpha 标志与 canvas 的 { alpha } 属性在 getContext 之后不可变——
    // 只改 options + refresh 会让"无图→开图"后透明背景被 premultiply 成黑色
    // （背景图不可见）、"开图→关图"后 alpha 残留留下透明洞（spec D-1）
    if (Boolean(newUrl) !== Boolean(oldUrl)) {
      ctx.callbacks.rebuildRenderer()
      return
    }
    // 仅不透明度变化：透明状态未切换，无需重建；theme 重设 + 重绘一次即可
    // （背景图透明度由图片层 CSS opacity 实时控制，xterm 侧无额外状态）
    terminal.options.theme = getTheme()
    terminal.refresh(0, terminal.rows - 1)
  })

  /** 组件卸载清理：取消字号/主题防抖持久化定时器 */
  function disposeSettingsSync() {
    if (fontSizeSaveTimeout) {
      clearTimeout(fontSizeSaveTimeout)
      fontSizeSaveTimeout = null
    }
    if (themeSaveTimeout) {
      clearTimeout(themeSaveTimeout)
      themeSaveTimeout = null
    }
  }

  // 注册跨域回调（调用时解析，创建顺序无关）
  ctx.callbacks.getTheme = getTheme

  return {
    fontSize,
    terminalTheme,
    effectiveFontSize,
    bgImage,
    bgOpacity,
    bgImageUrl,
    themeSelectOptions,
    fontSizeSelectOptions,
    getTheme,
    containerBgColor,
    resolveBgImageUrl,
    disposeSettingsSync,
  }
}
