<template>
  <div class="select-root">
    <label v-if="label" :for="id" class="block text-xs font-medium mb-1.5 text-mobile-secondary select-label">
      {{ label }}
      <span v-if="required" class="text-mobile-error">*</span>
    </label>

    <div ref="triggerRef" class="relative">
      <!-- Trigger -->
      <button
        :id="id"
        type="button"
        :disabled="disabled"
        class="select-trigger"
        :class="[
          triggerSizeCls,
          error ? 'select-trigger--error' : 'select-trigger--normal',
          { 'select-trigger--disabled': disabled },
        ]"
        @click="toggle"
        @keydown.down.prevent="open"
        @keydown.up.prevent="open"
        @keydown.escape="close"
      >
        <span
          class="min-w-0 truncate"
          :class="selectedLabel ? 'text-mobile-primary' : 'text-[var(--mobile-input-placeholder)]'"
        >
          {{ selectedLabel || placeholder || '' }}
        </span>
        <svg
          class="w-4 h-4 flex-shrink-0 text-mobile-muted transition-transform duration-200"
          :class="{ 'rotate-180': isOpen }"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      <!-- 下拉面板：Teleport 到 body 规避 overflow 裁剪，safe-stack overlay 层 z-50 -->
      <Teleport to="body">
        <div
          v-show="isOpen"
          ref="panelRef"
          class="select-panel fixed z-50"
          :style="panelStyle"
        >
          <ul class="py-1 max-h-[45vh] overflow-y-auto">
            <li
              v-if="placeholder"
              class="select-option select-option--placeholder"
              :class="optionRowCls"
              @click="select('')"
            >
              {{ placeholder }}
            </li>
            <li
              v-for="(option, index) in options"
              :key="option.value"
              class="select-option"
              :class="[
                optionRowCls,
                option.value === modelValue ? 'select-option--selected' : '',
                hoveredIndex === index ? 'select-option--hover' : '',
              ]"
              @mouseenter="hoveredIndex = index"
              @mouseleave="hoveredIndex = -1"
              @click="select(option.value)"
            >
              <span class="min-w-0 truncate">{{ option.label }}</span>
            </li>
          </ul>
        </div>
      </Teleport>
    </div>

    <!-- Error Message -->
    <p v-if="error" class="mt-1 text-xs text-mobile-error">{{ error }}</p>
  </div>
</template>

<script setup lang="ts">
/**
 * Select - 移动端共享自定义下拉选择组件
 *
 * 替代原生 <select>，外观完全由 --mobile-* token 控制，适配深浅主题。
 * 宿主与插件共享（@bedcode/plugin-sdk-mobile/ui）。
 *
 * - 默认 md：触发器与选项行 44px 触摸目标、字号 14px；面板空间不足时自动向上展开
 * - size="sm"：插件紧凑布局（36px/12px，如聊天头部工具条）
 * - open 事件：下拉展开时触发（插件可借此静默刷新选项，无原生组件禁用打断问题）
 */
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'

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
  /** md：宿主表单默认（44px/14px）；sm：插件紧凑布局（36px/12px） */
  size?: 'md' | 'sm'
  /** 面板弹出方向：auto=空间不足自动向上（默认）；top/bottom=固定方向（输入栏等贴底场景用 top） */
  placement?: 'auto' | 'top' | 'bottom'
}

const props = withDefaults(defineProps<Props>(), {
  disabled: false,
  required: false,
  size: 'md',
  placement: 'auto',
})

const emit = defineEmits(['update:modelValue', 'open'])

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

const triggerSizeCls = computed(() =>
  props.size === 'sm' ? 'select-trigger--sm' : 'select-trigger--md',
)
const optionRowCls = computed(() =>
  props.size === 'sm' ? 'select-option--sm' : 'select-option--md',
)

/** 面板与触发器的间距 */
const PANEL_GAP = 4

function computePosition() {
  const trigger = triggerRef.value
  const panel = panelRef.value
  if (!trigger || !panel) return
  const rect = trigger.getBoundingClientRect()
  const panelHeight = panel.offsetHeight
  // 下方空间不足时向上展开，避免面板超出视口底部（移动端小屏常见）；
  // placement 显式指定时优先：输入栏贴容器底部等场景固定向上，不受视口高度误判影响
  const openUpward =
    props.placement === 'top' ||
    (props.placement === 'auto' && rect.bottom + PANEL_GAP + panelHeight > window.innerHeight)
  const top = openUpward
    ? Math.max(PANEL_GAP, rect.top - panelHeight - PANEL_GAP)
    : rect.bottom + PANEL_GAP
  panelStyle.value = {
    top: `${top}px`,
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

<style scoped>
/* 触发器：44px 触摸目标，字号 14px，随屏幕宽度 flow 缩放 */
.select-trigger {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  text-align: left;
  background: var(--mobile-input-bg);
  transition: border-color 0.15s ease;
  cursor: pointer;
  outline: none;
}

.select-trigger--normal {
  border: 1px solid var(--mobile-input-border);
}

.select-trigger--normal:focus-visible,
.select-trigger--normal:focus {
  border-color: var(--mobile-input-focus);
}

.select-trigger--error {
  border: 1px solid var(--mobile-error);
}

.select-trigger--disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.select-trigger--md {
  min-height: clamp(2.75rem, 2.75rem + (100vw - 400px) / 800 * 8, 3.25rem);
  padding: 0 0.75rem;
  font-size: var(--font-size-base); /* 14px */
  border-radius: 0.625rem;
}

/* sm：插件紧凑布局（聊天头部工具条等） */
.select-trigger--sm {
  min-height: clamp(2.25rem, 2.25rem + (100vw - 400px) / 800 * 4, 2.5rem);
  padding: 0 0.5rem;
  font-size: var(--font-size-sm);
  border-radius: 0.5rem;
}

/* 面板：卡片底色 + 边框 + 阴影，overlay 层浮于页面内容之上 */
.select-panel {
  background: var(--mobile-bg-card);
  border: 1px solid var(--mobile-border);
  border-radius: 0.75rem;
  box-shadow: var(--mobile-card-shadow);
  overflow: hidden;
}

/* 选项行：44px 触摸目标，字号 14px */
.select-option {
  display: flex;
  align-items: center;
  padding: 0 0.75rem;
  cursor: pointer;
  user-select: none;
  color: var(--mobile-text-primary);
  transition: background-color 0.15s ease, color 0.15s ease;
}

.select-option--md {
  min-height: 2.75rem; /* 44px */
  font-size: var(--font-size-base); /* 14px */
}

.select-option--sm {
  min-height: 2.25rem; /* 36px */
  font-size: var(--font-size-sm);
}

.select-option--hover {
  background: var(--mobile-accent-muted);
}

/* 当前选中项以 accent 高亮，方便移动端快速识别 */
.select-option--selected {
  color: var(--mobile-accent);
  font-weight: 500;
}

.select-option--placeholder {
  color: var(--mobile-text-muted);
  cursor: default;
}
</style>
