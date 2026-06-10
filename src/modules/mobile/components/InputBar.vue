<template>
  <div
    class="input-bar bg-[var(--mobile-bg-secondary)] border-t border-[var(--mobile-border)] px-2 pt-2"
    :class="{ 'landscape-mode': isLandscapeMode }"
  >
    <!-- Special keys panel - 默认显示 -->
    <div v-if="showSpecialKeys && !isLandscapeMode" class="mb-2 grid grid-cols-8 gap-1.5">
      <button
        v-for="key in specialKeys"
        :key="key.code"
        class="bg-[var(--mobile-bg-primary)] text-[var(--mobile-text-secondary)] text-xs py-2 rounded-lg active:bg-[var(--mobile-accent-muted)]"
        @click="sendSpecialKey(key.code)"
      >
        {{ key.label }}
      </button>
    </div>

    <!-- 横屏时显示的快捷键行 -->
    <div v-if="isLandscapeMode" class="mb-1.5 flex flex-wrap gap-1">
      <button
        v-for="key in specialKeys"
        :key="key.code"
        class="bg-[var(--mobile-bg-primary)] text-[var(--mobile-text-secondary)] text-xs py-1 px-1.5 rounded active:bg-[var(--mobile-accent-muted)]"
        @click="sendSpecialKey(key.code)"
      >
        {{ key.label }}
      </button>
    </div>

    <!-- Main input row -->
    <div class="flex items-center gap-2 pb-1">
      <!-- 输入按钮 -->
      <button
        class="flex-1 bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border)] rounded-lg px-3 py-2 text-sm text-[var(--mobile-text-muted)] text-left"
        @click="showInputModal = true"
      >
        点击输入命令...
      </button>

      <!-- Special keys toggle - 非横屏时显示 -->
      <button
        v-if="!isLandscapeMode"
        class="p-2 rounded-xl"
        :class="showSpecialKeys ? 'bg-[var(--mobile-accent)] text-[var(--mobile-text-primary)]' : 'bg-[var(--mobile-bg-primary)] text-[var(--mobile-text-muted)]'"
        @click="toggleSpecialKeys"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4" />
        </svg>
      </button>
    </div>

    <!-- Connection status -->
    <div v-if="showStatus" class="flex items-center justify-center gap-2 mt-1">
      <div
        :class="[
          'w-2 h-2 rounded-full',
          isConnected ? 'bg-[var(--mobile-success)]' : 'bg-[var(--mobile-error)]'
        ]"
      ></div>
      <span class="text-xs text-[var(--mobile-text-muted)]">
        {{ isConnected ? '已连接' : '未连接' }}
      </span>
    </div>

    <!-- 输入弹窗 -->
    <Teleport to="body">
      <div
        v-if="showInputModal"
        class="fixed inset-0 z-[100] flex items-center justify-center p-4"
        @click.self="showInputModal = false"
      >
        <!-- 弹窗背景 -->
        <div class="absolute inset-0 bg-black/50" @click="showInputModal = false"></div>

        <!-- 弹窗内容 -->
        <div class="relative bg-[var(--mobile-bg-card)] rounded-xl w-full max-w-md p-4 shadow-xl">
          <div class="text-sm font-medium text-[var(--mobile-text-secondary)] mb-3">
            输入命令
          </div>

          <textarea
            ref="modalInputRef"
            v-model="inputText"
            class="w-full bg-[var(--mobile-bg-primary)] border border-[var(--mobile-border)] rounded-lg px-3 py-2 text-sm text-[var(--mobile-text-primary)] placeholder-[var(--mobile-text-disabled)] focus:outline-none focus:border-[var(--mobile-accent)] resize-none"
            placeholder="输入命令..."
            rows="4"
            @keydown.enter.ctrl="submitText"
          ></textarea>

          <div class="flex justify-between gap-2 mt-4">
            <button
              class="px-4 py-2 text-sm text-[var(--mobile-text-muted)]"
              @click="showInputModal = false"
            >
              取消
            </button>
            <div class="flex gap-2">
              <button
                class="px-4 py-2 text-sm bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-secondary)] rounded-lg"
                :disabled="!inputText.trim()"
                @click="submitText"
              >
                发送
              </button>
              <button
                class="px-4 py-2 text-sm bg-[var(--mobile-accent)] text-[var(--mobile-text-primary)] rounded-lg"
                :disabled="!inputText.trim()"
                @click="executeText"
              >
                执行
              </button>
            </div>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'

const props = defineProps<{
  disabled?: boolean
  placeholder?: string
  isConnected?: boolean
  showStatus?: boolean
  isLandscape?: boolean
}>()

const emit = defineEmits<{
  submit: [text: string]
  execute: [text: string]
  specialKey: [key: string]
  focus: []
  blur: []
}>()

const inputText = ref('')
const inputRef = ref<HTMLInputElement | null>(null)
const modalInputRef = ref<HTMLInputElement | null>(null)
const showSpecialKeys = ref(true) // 默认显示
const showInputModal = ref(false)
const isLandscapeMode = computed(() => props.isLandscape || false)

// 弹窗打开时聚焦输入框
watch(showInputModal, async (show) => {
  if (show) {
    inputText.value = ''
    await nextTick()
    modalInputRef.value?.focus()
  }
})

function handleFocus() {
  emit('focus')
}

function handleBlur() {
  emit('blur')
}

const specialKeys = [
  { label: 'Tab', code: 'tab' },
  { label: 'Enter', code: 'enter' },
  { label: 'Esc', code: 'escape' },
  { label: 'Del', code: 'delete' },
  { label: 'Ctrl+C', code: 'ctrl_c' },
  { label: 'Ctrl+Z', code: 'ctrl_z' },
  { label: 'Ctrl+L', code: 'ctrl_l' },
  { label: '↑', code: 'arrow_up' },
  { label: '↓', code: 'arrow_down' },
  { label: '←', code: 'arrow_left' },
  { label: '→', code: 'arrow_right' },
]

function submitText() {
  if (inputText.value.trim()) {
    emit('submit', inputText.value)
    inputText.value = ''
    showInputModal.value = false
  }
}

function executeText() {
  if (inputText.value.trim()) {
    emit('execute', inputText.value)
    inputText.value = ''
    showInputModal.value = false
  }
}

function sendSpecialKey(code: string) {
  emit('specialKey', code)
}

function toggleSpecialKeys() {
  showSpecialKeys.value = !showSpecialKeys.value
}

function focus() {
  showInputModal.value = true
}

defineExpose({ focus })
</script>
