/**
 * WSL Composable - WSL 发行版信息
 *
 * @deprecated 请使用 useWslStore() 代替，WSL 信息已在应用启动时预加载并缓存
 * 此 composable 保留仅为向后兼容，不再自动在 onMounted 中加载
 */

import { ref } from 'vue'
import { listWslDistributions, isWslAvailable, type WslDistro } from '@/composables/useDesktopCommands'

export function useWsl() {
  const distros = ref<WslDistro[]>([])
  const isAvailable = ref(false)

  async function loadDistros() {
    isAvailable.value = await isWslAvailable()
    if (isAvailable.value) {
      distros.value = await listWslDistributions()
    }
  }

  return {
    distros,
    loadDistros,
    isAvailable,
  }
}
