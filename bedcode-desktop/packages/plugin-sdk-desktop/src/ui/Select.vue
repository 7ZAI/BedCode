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
        class="w-full border text-left transition-all duration-200 outline-none cursor-pointer bg-[var(--bg-input)] text-[var(--text-primary)] focus:border-brand focus:shadow-input-focus shadow-xs dark:shadow-none flex items-center justify-between gap-2"
        :class="[
          triggerSizeCls,
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
          class="fixed z-[60] bg-[var(--bg-card)] border border-[var(--border-input)] rounded-input shadow-card overflow-hidden transition-opacity duration-150"
          :class="isOpen ? 'opacity-100' : 'opacity-0 pointer-events-none'"
          :style="panelStyle"
        >
          <ul class="py-1 overflow-y-auto" :style="panelListStyle">
            <li
              v-if="placeholder"
              :class="['text-[var(--text-tertiary)] cursor-default select-none', optionRowCls]"
              @click="select('')"
            >
              {{ placeholder }}
            </li>
            <li
              v-for="(option, index) in options"
              :key="option.value"
              :class="[
                optionRowCls,
                'cursor-pointer select-none transition-colors duration-150',
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
 * Select - 宿主共享自定义下拉选择组件
 *
 * 替代原生 <select>，hover 样式完全由 CSS token 控制，适配深色/浅色主题。
 * 同时提供给插件 SDK（@binblink/plugin-sdk-desktop/ui）供插件引用。
 *
 * - size="sm"：插件紧凑布局（32px/12px）；默认 md 与宿主表单一致（--input-height）
 * - open 事件：下拉展开时触发（插件可借此静默刷新选项，无原生组件禁用打断问题）
 */
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { computeSelectPosition, SELECT_MAX_PANEL_HEIGHT } from './select-position'

export interface SelectOption {
  value: string | number
  label: string
}

export interface Props {
  modelValue: string | number
  label?: string
  options: SelectOption[]
  placeholder?: string
  disabled?: boolean
  required?: boolean
  error?: string
  /** md：宿主表单默认（--input-height 36px）；sm：插件紧凑布局（32px/12px） */
  size?: 'md' | 'sm'
}

const props = withDefaults(defineProps<Props>(), {
  disabled: false,
  required: false,
  size: 'md',
})

const emit = defineEmits(['update:modelValue', 'open'])

const id = `select-${Math.random().toString(36).slice(2, 9)}`
const isOpen = ref(false)
const hoveredIndex = ref(-1)
const triggerRef = ref<HTMLElement | null>(null)
const panelRef = ref<HTMLElement | null>(null)
const panelStyle = ref<Record<string, string>>({})
const panelListStyle = ref<Record<string, string>>({})

const selectedLabel = computed(() => {
  const opt = props.options.find(o => o.value === props.modelValue)
  return opt?.label ?? ''
})

// 尺寸分支：sm 为插件紧凑布局（与插件控件 h-8/text-xs 一致），md 与宿主表单控件一致
const triggerSizeCls = computed(() =>
  props.size === 'sm'
    ? 'h-8 px-2 text-xs rounded-[6px]'
    : 'h-[var(--input-height)] px-4 text-sm rounded-input',
)
const optionRowCls = computed(() =>
  props.size === 'sm' ? 'px-2 py-1.5 text-xs' : 'px-4 py-2.5 text-sm',
)

function computePosition() {
  if (!triggerRef.value) return
  const rect = triggerRef.value.getBoundingClientRect()
  // 面板未渲染/高度不可测时退回设计高度，规则仍然成立
  const panelHeight = panelRef.value?.getBoundingClientRect().height || SELECT_MAX_PANEL_HEIGHT
  const pos = computeSelectPosition(
    { top: rect.top, bottom: rect.bottom, left: rect.left, width: rect.width },
    { width: window.innerWidth, height: window.innerHeight },
    panelHeight,
  )
  panelStyle.value = {
    top: `${pos.top}px`,
    left: `${pos.left}px`,
    width: `${rect.width}px`,
  }
  panelListStyle.value = {
    maxHeight: `${pos.maxHeight}px`,
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
  emit('open')
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

/**
 * 滚动关闭：scroll 事件不冒泡，但捕获阶段监听（capture:true）仍会收到
 * 任意后代元素的滚动。面板自身的选项列表滚动（选项过多需滚动查看）不应
 * 关闭下拉——否则一滚面板就消失，无法选中可视区外的选项；
 * 其余滚动（页面/输入区/消息列表）视为失焦关闭，保持原意图
 */
function onScroll(e: Event) {
  if (panelRef.value?.contains(e.target as Node)) return
  close()
}

onMounted(() => {
  document.addEventListener('mousedown', onClickOutside)
  window.addEventListener('resize', close)
  window.addEventListener('scroll', onScroll, true)
})

onUnmounted(() => {
  document.removeEventListener('mousedown', onClickOutside)
  window.removeEventListener('resize', close)
  window.removeEventListener('scroll', onScroll, true)
})
</script>
