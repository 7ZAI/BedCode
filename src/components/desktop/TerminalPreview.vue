<template>
  <div class="h-full flex flex-col bg-dark-900">
    <!-- Header -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-3 flex items-center justify-between">
      <div class="flex items-center gap-3">
        <div
          :class="[
            'w-2 h-2 rounded-full',
            statusColor
          ]"
        ></div>
        <h3 class="font-medium">{{ session?.name || '终端预览' }}</h3>
      </div>

      <div class="flex items-center gap-2">
        <!-- Font Size -->
        <select
          v-model="fontSize"
          class="bg-dark-700 border border-dark-600 rounded px-2 py-1 text-sm text-white"
        >
          <option v-for="size in [12, 14, 16, 18, 20]" :key="size" :value="size">
            {{ size }}px
          </option>
        </select>

        <!-- Clear Button -->
        <Button variant="ghost" size="sm" @click="clearOutput">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </Button>
      </div>
    </header>

    <!-- Terminal Output -->
    <div
      ref="terminalRef"
      class="flex-1 overflow-auto p-4 font-mono text-sm"
      :style="{ fontSize: `${fontSize}px` }"
    >
      <pre v-if="output.length > 0" class="whitespace-pre-wrap break-words text-dark-100">{{ renderedOutput }}</pre>
      <div v-else class="h-full flex items-center justify-center text-dark-500">
        <p>等待输出...</p>
      </div>
    </div>

    <!-- Input Bar -->
    <div v-if="showInput && session" class="border-t border-dark-700 p-3 bg-dark-800">
      <div class="flex gap-2">
        <input
          v-model="inputText"
          type="text"
          placeholder="输入命令..."
          class="flex-1 bg-dark-700 border border-dark-600 rounded-lg px-4 py-2 text-white placeholder-dark-400 focus:border-primary-500 outline-none"
          @keydown.enter="sendInput"
          @keydown.tab.prevent="sendSpecialKey('tab')"
          @keydown.up.prevent="navigateHistory(-1)"
          @keydown.down.prevent="navigateHistory(1)"
        />
        <Button variant="primary" @click="sendInput">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
          </svg>
        </Button>
      </div>

      <!-- Quick Keys -->
      <div class="flex gap-2 mt-2">
        <button
          v-for="key in quickKeys"
          :key="key.value"
          class="px-3 py-1 bg-dark-700 hover:bg-dark-600 rounded text-xs text-dark-300 transition-colors"
          @click="sendSpecialKey(key.value)"
        >
          {{ key.label }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import type { SessionInfo } from '@/stores/session'
import { useSessionStore } from '@/stores/session'
import Button from '@/components/common/Button.vue'
import { usePtyOutput } from '@/composables/useTauri'

interface Props {
  session?: SessionInfo | null
  showInput?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  showInput: true,
})

const sessionStore = useSessionStore()
const terminalRef = ref<HTMLElement | null>(null)
const fontSize = ref(14)
const inputText = ref('')
const historyIndex = ref(-1)

const sessionId = computed(() => props.session?.id || '')

const { output, clearOutput } = usePtyOutput(sessionId)

const quickKeys = [
  { label: 'Tab', value: 'tab' },
  { label: 'Enter', value: 'enter' },
  { label: 'Esc', value: 'escape' },
  { label: 'Ctrl+C', value: 'ctrl_c' },
  { label: 'Ctrl+D', value: 'ctrl_d' },
  { label: '↑', value: 'arrow_up' },
  { label: '↓', value: 'arrow_down' },
]

const statusColor = computed(() => {
  if (!props.session) return 'bg-dark-500'

  switch (props.session.status) {
    case 'Running':
      return 'bg-green-500'
    case 'WaitingInput':
      return 'bg-yellow-500 animate-pulse'
    case 'Error':
      return 'bg-red-500'
    case 'Stopped':
      return 'bg-dark-500'
    default:
      return 'bg-blue-500'
  }
})

const renderedOutput = computed(() => {
  return output.value.join('')
})

const inputHistory = ref<string[]>([])

// Auto-scroll to bottom
watch(output, async () => {
  await nextTick()
  if (terminalRef.value) {
    terminalRef.value.scrollTop = terminalRef.value.scrollHeight
  }
})

async function sendInput() {
  if (!inputText.value.trim() || !props.session) return

  const text = inputText.value

  // Add to history
  inputHistory.value.push(text)
  historyIndex.value = -1

  // Send to session
  await sessionStore.writeToSession(props.session.id, text + '\n')

  inputText.value = ''
}

async function sendSpecialKey(key: string) {
  if (!props.session) return

  if (key === 'tab') {
    inputText.value += '\t'
  } else {
    await sessionStore.sendSpecialKey(props.session.id, key)
  }
}

function navigateHistory(direction: number) {
  if (inputHistory.value.length === 0) return

  const newIndex = historyIndex.value + direction
  if (newIndex < -1) return
  if (newIndex >= inputHistory.value.length) return

  historyIndex.value = newIndex

  if (newIndex === -1) {
    inputText.value = ''
  } else {
    inputText.value = inputHistory.value[inputHistory.value.length - 1 - newIndex]
  }
}
</script>

<style scoped>
pre {
  font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
}
</style>
