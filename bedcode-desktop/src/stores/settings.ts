import { defineStore } from 'pinia'
import { logger } from '@/utils/frontendLogger'
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'

/**
 * 宿主前端设置视图（AppConfig 的子集）
 *
 * 收敛记录（2026-09-21）：原先声明的 `session.*`（默认执行环境 / WSL 发行版 /
 * 工作目录 / 启动命令 / 会话超时）、`network.qr_host`、`ui.show_preview`、
 * `ui.max_cached_terminals`、`ui.notify_in_background` 实测**全无消费者**——
 * 会话默认值已迁入 `com.bedcode.terminal-session` 插件存储（`session.formDefaults`），
 * QR host 归插件（`pairing.qrHost`），其余为移动端旧字段残留 → 一并删除。
 * 保存仍为整表回传（Rust 侧 AppConfig 反序列化时缺段用默认值补齐）。
 */
export interface Settings {
  network: {
    port: number
    // 服务器运行时阻止系统休眠
    prevent_sleep?: boolean
  }
  ui: {
    theme: string
    // 色板（warm 暖调工作台，未来可扩展）
    theme_palette?: string
    // 全局动画效果总开关（关闭时禁用所有页面过渡/动画），默认开启
    animations_enabled?: boolean
    // 全局界面字体大小（终端字体大小由 terminal_font_size 独立控制）
    font_size: number
    terminal_font_size: number
    terminal_font_family: string
    terminal_theme: string
    // 语言偏好
    language?: string
    // 终端背景图片文件名（位于应用数据目录，空/未设置表示不启用）
    terminal_bg_image?: string | null
    // 终端背景图片不透明度（0-100，越小图片越淡）
    terminal_bg_opacity?: number
  }
}

const defaultSettings: Settings = {
  network: {
    port: 8765,
  },
  ui: {
    theme: 'system',
    theme_palette: 'warm',
    animations_enabled: true,
    font_size: 12,
    terminal_font_size: 12,
    terminal_font_family: 'Consolas',
    terminal_theme: 'dracula',
    language: 'zh-CN',
    terminal_bg_image: undefined,
    terminal_bg_opacity: 30,
  },
}

export const useSettingsStore = defineStore('settings', () => {
  // 深拷贝默认值（structuredClone：默认值是纯数据常量，结构克隆即可；
  // 原 JSON.parse(JSON.stringify) 等义，函数/符号字段不存在，不涉任何异常路径）
  const settings = ref<Settings>(structuredClone(defaultSettings))
  // 最近一次成功保存内容的 JSON 快照：deep watch 触发时对比内容判断是否已持久化。
  // 不能用对象引用比对——Pinia ref 赋值会包一层 reactive proxy，settings.value
  // 永远不等于原始对象；且用户变更发生在同一对象上，引用比对也无法区分新旧状态
  let lastSavedSnapshot: string | null = null
  // 保存序号：并发保存时仅最后一次的响应回写 store，先发慢回的旧响应不得覆盖新值
  let saveSeq = 0

  async function loadSettings() {
    try {
      const loaded = await invoke<Settings>('get_app_settings')
      settings.value = {
        network: { ...defaultSettings.network, ...loaded.network },
        ui: { ...defaultSettings.ui, ...loaded.ui },
      }
    } catch (e) {
      logger.error('[Settings] Failed to load settings:', e)
    }
  }

  async function saveSettings(newSettings: Partial<Settings>) {
    try {
      const merged = { ...settings.value, ...newSettings }
      const mySeq = ++saveSeq
      await invoke('save_app_settings', { settings: merged })
      // 期间已有更新的保存请求：回写会覆盖更新值，丢弃本次回写
      if (mySeq !== saveSeq) return
      settings.value = merged
      lastSavedSnapshot = JSON.stringify(merged)
    } catch (e) {
      logger.error('[Settings] Failed to save settings:', e)
    }
  }

  /** 当前 store 状态是否与最近一次持久化一致（避免保存回写触发重复保存） */
  function isPersisted(current: Settings): boolean {
    return lastSavedSnapshot !== null && JSON.stringify(current) === lastSavedSnapshot
  }

  // 全局动画总开关：关闭时给 <html> 添加 anim-disabled，
  // 由全局 CSS（style.css 的 html.anim-disabled 规则）瞬时禁用所有过渡/动画。
  // immediate 在 loadSettings 回填后与初始默认值都生效，保证刷新后状态一致。
  watch(
    () => settings.value.ui.animations_enabled,
    (enabled) => {
      const root = document.documentElement
      if (enabled === false) root.classList.add('anim-disabled')
      else root.classList.remove('anim-disabled')
    },
    { immediate: true },
  )

  return {
    settings,
    loadSettings,
    saveSettings,
    isPersisted,
  }
})
