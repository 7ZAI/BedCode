import { ref, onMounted } from 'vue'
import { listWslDistributions, isWslAvailable, type WslDistro } from '@/modules/desktop/composables/useDesktopCommands'

export function useWsl() {
  const distros = ref<WslDistro[]>([])
  const isAvailable = ref(false)

  async function loadDistros() {
    isAvailable.value = await isWslAvailable()
    if (isAvailable.value) {
      distros.value = await listWslDistributions()
    }
  }

  onMounted(async () => {
    await loadDistros()
  })

  return {
    distros,
    loadDistros,
    isAvailable,
  }
}