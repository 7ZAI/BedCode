import { ref } from 'vue'
import { getLocalIpAddresses } from '@/composables/useDesktopCommands'

export function useNetwork() {
  const localAddresses = ref<string[]>([])

  async function loadLocalAddresses() {
    localAddresses.value = await getLocalIpAddresses()
  }

  return {
    localAddresses,
    loadLocalAddresses,
  }
}