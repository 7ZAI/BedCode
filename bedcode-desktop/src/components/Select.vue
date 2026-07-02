<template>
  <div class="form-group">
    <label v-if="label" :for="id" class="block text-xs font-medium mb-1.5 text-[var(--text-secondary)]">
      {{ label }}
      <span v-if="required" class="text-red-500">*</span>
    </label>

    <div ref="triggerRef" class="relative">
      <!-- Trigger -->
      <button
        :id="id"
        type="button"
        :disabled="disabled"
        class="w-full h-[var(--input-height)] border rounded-input px-4 text-sm text-left transition-all duration-200 outline-none cursor-pointer bg-[var(--bg-input)] text-[var(--text-primary)] focus:border-brand focus:shadow-input-focus shadow-xs dark:shadow-none flex items-center justify-between gap-2"
        :class="[
          error ? 'border-red-500' : 'border-[var(--border-input)]',
          { 'opacity-50 cursor-not-allowed': disabled }
        ]"
        @click="toggle"
        @keydown.down.prevent="open"
        @keydown.up.prevent="open"
        @keydown.escape="close"
      >
        <span :class="{ 'text-[var(--text-tertiary)]': !selectedLabel }">
          {{ selectedLabel || placeholder || '' }}
        </span>
        <svg class="w-5 h-5 flex-shrink-0 text-[var(--text-tertiary)] transition-transform duration-200" :class="{ 'rotate-180': isOpen }" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      <!-- Dropdown Panel -->
      <Teleport to="body">
        <div
          v-show="isOpen"
          ref="panelRef"
          class="fixed z-30 bg-[var(--bg-card)] border border-[var(--border-input)] rounded-input shadow-card overflow-hidden transition-opacity duration-150"
          :class="isOpen ? 'opacity-100' : 'opacity-0 pointer-events-none'"
          :style="panelStyle"
        >
          <ul class="py-1 max-h-60 overflow-y-auto">
            <li
              v-if="placeholder"
              class="px-4 py-2.5 text-sm text-[var(--text-tertiary)] cursor-default select-none"
              @click="select('')"
            >
              {{ placeholder }}
            </li>
            <li
              v-for="(option, index) in options"
              :key="option.value"
              class="px-4 py-2.5 text-sm cursor-pointer select-none transition-colors duration-150"
              :class="[
                hoveredIndex === index
                  ? 'bg-[var(--color-primary-light)] text-brand font-medium'
                  : 'text-[var(--text-primary)]'
              ]"
              @mouseenter="hoveredIndex = index"
              @mouseleave="hoveredIndex = -1"
              @click="select(option.value)"
            >
              {{ option.label }}
            </li>
          </ul>
        </div>
      </Teleport>
    </div>

    <!-- Error Message -->
    <p v-if="error" class="mt-1 text-xs text-red-500">{{ error }}</p>
  </div>
</template>

<script setup lang="ts">
/**
 * Select - 自定义下拉选择组件
 *
 * 替代原生 <select>，hover 样式完全由 CSS token 控制，适配深色/浅色主题
 */
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'

interface Option {
  value: string | number
  label: string
}

interface Props {
  modelValue: string | number
  label?: string
  options: Option[]
  placeholder?: string
  disabled?: boolean
  required?: boolean
  error?: string
}

const props = withDefaults(defineProps<Props>(), {
  disabled: false,
  required: false,
})

const emit = defineEmits(['update:modelValue'])

const id = `select-${Math.random().toString(36).slice(2, 9)}`
const isOpen = ref(false)
const hoveredIndex = ref(-1)
const triggerRef = ref<HTMLElement | null>(null)
const panelRef = ref<HTMLElement | null>(null)
const panelStyle = ref<Record<string, string>>({})

const selectedLabel = computed(() => {
  const opt = props.options.find(o => o.value === props.modelValue)
  return opt?.label ?? ''
})

function computePosition() {
  if (!triggerRef.value) return
  const rect = triggerRef.value.getBoundingClientRect()
  panelStyle.value = {
    top: `${rect.bottom + 4}px`,
    left: `${rect.left}px`,
    width: `${rect.width}px`,
  }
}

function toggle() {
  if (props.disabled) return
  isOpen.value ? close() : open()
}

function open() {
  if (props.disabled) return
  isOpen.value = true
  hoveredIndex.value = -1
  nextTick(computePosition)
}

function close() {
  isOpen.value = false
  hoveredIndex.value = -1
}

function select(value: string | number) {
  emit('update:modelValue', value)
  close()
}

function onClickOutside(e: MouseEvent) {
  const target = e.target as HTMLElement
  if (
    isOpen.value &&
    !triggerRef.value?.contains(target) &&
    !panelRef.value?.contains(target)
  ) {
    close()
  }
}

onMounted(() => {
  document.addEventListener('mousedown', onClickOutside)
  window.addEventListener('resize', close)
  window.addEventListener('scroll', close, true)
})

onUnmounted(() => {
  document.removeEventListener('mousedown', onClickOutside)
  window.removeEventListener('resize', close)
  window.removeEventListener('scroll', close, true)
})
</script>
