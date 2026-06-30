/**
 * WSL Store - WSL 信息全局缓存
 *
 * 应用启动时一次性加载 WSL 可用性和发行版列表，
 * 避免每次打开会话配置弹窗时重复执行 wsl 命令导致 UI 卡顿
 */

import { defineStore } from 'pinia'
import { ref } from 'vue'
import {
  isWslAvailable,
  listWslDistributions,
  type WslDistro,
} from '@/modules/desktop/composables/useDesktopCommands'

export const useWslStore = defineStore('wsl', () => {
  const isAvailable = ref(false)
  const distros = ref<WslDistro[]>([])
  const isLoading = ref(true)
  const error = ref<string | null>(null)

  /** 加载 WSL 信息（应用启动时调用一次） */
  async function loadWslInfo() {
    isLoading.value = true
    error.value = null

    try {
      isAvailable.value = await isWslAvailable()
      if (isAvailable.value) {
        distros.value = await listWslDistributions()
      }
    } catch (e: any) {
      error.value = e?.message || String(e)
      // WSL 不可用不算致命错误，仅记录
      console.warn('[WslStore] Failed to load WSL info:', e)
    } finally {
      isLoading.value = false
    }
  }

  return {
    isAvailable,
    distros,
    isLoading,
    error,
    loadWslInfo,
  }
})
