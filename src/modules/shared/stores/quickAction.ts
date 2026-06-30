import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useQuickActionStore = defineStore('quickAction', () => {
  const pendingInput = ref<string | null>(null)

  function setPendingInput(text: string) {
    pendingInput.value = text
  }

  function consumePendingInput(): string | null {
    const text = pendingInput.value
    pendingInput.value = null
    return text
  }

  return {
    pendingInput,
    setPendingInput,
    consumePendingInput,
  }
})
