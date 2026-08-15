<template>
  <Teleport to="body">
    <Transition name="sheet">
      <div v-if="modelValue" class="fixed inset-0 z-50">
        <!-- 遮罩：点击关闭（即改即存，无未保存状态） -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay)]" @click="emit('update:modelValue', false)"></div>

        <!-- 底部面板：代码渲染 + 思考模式配置（插件级全局配置，storage key `config`） -->
        <div
          class="sheet-panel absolute left-0 right-0 bottom-0 max-h-[80dvh] overflow-y-auto rounded-t-2xl bg-[var(--mobile-bg-card)] border-t border-[var(--mobile-border)] shadow-[var(--mobile-card-shadow)]"
          :style="{ paddingBottom: `max(1rem, ${safeAreaBottom}px)` }"
        >
          <!-- 拖拽把手 -->
          <div class="flex justify-center pt-2.5 pb-1">
            <div class="w-10 h-1 rounded-full bg-[var(--mobile-border-strong)]"></div>
          </div>

          <div class="px-4 pb-2">
            <div class="flex items-center justify-between">
              <h2 class="text-base font-semibold text-[var(--mobile-text-primary)]">
                {{ t('mobile.plugin.aiChatbox.pluginSettings') }}
              </h2>
              <button
                class="w-11 h-11 -mr-2 flex items-center justify-center text-[var(--mobile-text-secondary)] active:opacity-80 rounded-xl transition-opacity"
                :aria-label="t('mobile.plugin.aiChatbox.close')"
                @click="emit('update:modelValue', false)"
              >
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>

            <!-- 分组：代码渲染 -->
            <div class="mt-3">
              <h3 class="text-xs font-medium text-[var(--mobile-text-secondary)] mb-2">
                {{ t('mobile.plugin.aiChatbox.codeRendering') }}
              </h3>
              <div class="space-y-3">
                <!-- 字体大小：− / 数值 / ＋（11-18px 步进 1） -->
                <div class="flex items-center justify-between min-h-11">
                  <span class="text-[var(--font-size-sm)] text-[var(--mobile-text-primary)]">
                    {{ t('mobile.plugin.aiChatbox.codeFontSize') }}
                  </span>
                  <div class="flex items-center gap-1">
                    <button
                      class="w-11 h-11 flex items-center justify-center rounded-lg border border-[var(--mobile-border)] text-[var(--mobile-text-primary)] active:bg-[var(--mobile-bg-tertiary)] transition-colors disabled:opacity-30"
                      :disabled="cfg.codeFontSize <= CODE_FONT_SIZE_MIN"
                      :aria-label="t('mobile.plugin.aiChatbox.decrease')"
                      @click="update({ codeFontSize: cfg.codeFontSize - 1 })"
                    >
                      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M20 12H4" />
                      </svg>
                    </button>
                    <span class="w-9 text-center text-[var(--font-size-sm)] text-[var(--mobile-text-primary)] tabular-nums">
                      {{ cfg.codeFontSize }}
                    </span>
                    <button
                      class="w-11 h-11 flex items-center justify-center rounded-lg border border-[var(--mobile-border)] text-[var(--mobile-text-primary)] active:bg-[var(--mobile-bg-tertiary)] transition-colors disabled:opacity-30"
                      :disabled="cfg.codeFontSize >= CODE_FONT_SIZE_MAX"
                      :aria-label="t('mobile.plugin.aiChatbox.increase')"
                      @click="update({ codeFontSize: cfg.codeFontSize + 1 })"
                    >
                      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4" />
                      </svg>
                    </button>
                  </div>
                </div>

                <!-- 行距：自绘滑块拖动（0.5-2.0，步进 0.1，即拖即存） -->
                <div class="flex items-center justify-between min-h-11 gap-3">
                  <span class="text-[var(--font-size-sm)] text-[var(--mobile-text-primary)]">
                    {{ t('mobile.plugin.aiChatbox.codeLineHeight') }}
                  </span>
                  <div class="flex items-center gap-2 flex-1 min-w-0">
                    <div
                      ref="lhTrackRef"
                      class="relative h-11 flex-1 min-w-0 flex items-center cursor-pointer touch-none"
                      @pointerdown="onLhPointerDown"
                      @pointermove="onLhPointerMove"
                      @pointerup="onLhPointerEnd"
                      @pointercancel="onLhPointerEnd"
                    >
                      <!-- 轨道 -->
                      <div class="absolute left-0 right-0 h-1 rounded-full bg-[var(--mobile-border)] pointer-events-none"></div>
                      <!-- 填充段 -->
                      <div
                        class="absolute h-1 rounded-full bg-[var(--mobile-accent)] pointer-events-none"
                        :style="{ width: lhFillPercent }"
                      ></div>
                      <!-- 滑块 thumb（视觉 20px；拖拽热区为整行 44px） -->
                      <div
                        class="absolute w-5 h-5 rounded-full bg-[var(--mobile-accent)] border-2 border-[var(--mobile-bg-card)] shadow-sm pointer-events-none"
                        :style="{ left: `calc(${lhFillPercent} - 10px)` }"
                      ></div>
                    </div>
                    <span class="w-9 flex-shrink-0 text-right text-[var(--font-size-sm)] text-[var(--mobile-text-primary)] tabular-nums">
                      {{ cfg.codeLineHeight.toFixed(1) }}
                    </span>
                  </div>
                </div>

                <!-- 代码主题：展开式下拉（跟随 / 浅色 / 深色） -->
                <div>
                  <span class="text-[var(--font-size-sm)] text-[var(--mobile-text-primary)]">
                    {{ t('mobile.plugin.aiChatbox.codeTheme') }}
                  </span>
                  <div class="mt-1.5 rounded-lg border border-[var(--mobile-border)] bg-[var(--mobile-bg-tertiary)] overflow-hidden">
                    <button
                      class="w-full h-11 px-3 flex items-center justify-between text-[var(--font-size-sm)] text-[var(--mobile-text-primary)] active:bg-[var(--mobile-bg-secondary)] transition-colors"
                      @click="toggleDropdown('codeTheme')"
                    >
                      <span>{{ currentThemeLabel }}</span>
                      <svg
                        class="w-4 h-4 text-[var(--mobile-text-secondary)] transition-transform duration-200"
                        :class="{ 'rotate-180': openDropdown === 'codeTheme' }"
                        fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2"
                      >
                        <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
                      </svg>
                    </button>
                    <Transition name="dropdown">
                      <div v-if="openDropdown === 'codeTheme'" class="border-t border-[var(--mobile-border)]">
                        <button
                          v-for="opt in themeOptions"
                          :key="opt.value"
                          class="w-full h-11 px-3 flex items-center justify-between text-[var(--font-size-sm)] text-left transition-colors"
                          :class="cfg.codeTheme === opt.value
                            ? 'text-[var(--mobile-accent)] font-medium'
                            : 'text-[var(--mobile-text-secondary)] active:bg-[var(--mobile-bg-secondary)]'"
                          @click="selectTheme(opt.value)"
                        >
                          <span>{{ opt.label }}</span>
                          <svg v-if="cfg.codeTheme === opt.value" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                          </svg>
                        </button>
                      </div>
                    </Transition>
                  </div>
                </div>
              </div>
            </div>

            <!-- 分组：思考模式 -->
            <div class="mt-4">
              <h3 class="text-xs font-medium text-[var(--mobile-text-secondary)] mb-2">
                {{ t('mobile.plugin.aiChatbox.thinking') }}
              </h3>
              <div class="space-y-3">
                <div class="flex items-center justify-between min-h-11">
                  <span class="text-[var(--font-size-sm)] text-[var(--mobile-text-primary)]">
                    {{ t('mobile.plugin.aiChatbox.thinkingMode') }}
                  </span>
                  <SegmentedControl
                    :options="thinkingModeOptions"
                    :model-value="cfg.thinkingMode"
                    @change="v => update({ thinkingMode: v })"
                  />
                </div>

                <!-- 推理强度：仅思考模式为「强制开启」时生效 -->
                <div v-if="cfg.thinkingMode === 'enabled'" class="flex items-center justify-between min-h-11">
                  <span class="text-[var(--font-size-sm)] text-[var(--mobile-text-primary)]">
                    {{ t('mobile.plugin.aiChatbox.reasoningEffort') }}
                  </span>
                  <SegmentedControl
                    :options="effortOptions"
                    :model-value="cfg.reasoningEffort"
                    @change="v => update({ reasoningEffort: v })"
                  />
                </div>

                <div class="flex items-center justify-between min-h-11">
                  <span class="text-[var(--font-size-sm)] text-[var(--mobile-text-primary)]">
                    {{ t('mobile.plugin.aiChatbox.showReasoning') }}
                  </span>
                  <SegmentedControl
                    :options="showReasoningOptions"
                    :model-value="cfg.showReasoning ? 'on' : 'off'"
                    @change="v => update({ showReasoning: v === 'on' })"
                  />
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * PluginSettingsSheet — 移动端插件级配置弹层（底部面板）
 *
 * 移动宿主暂无插件配置页（桌面端由宿主 PluginConfigView schema 渲染），
 * 此处提供同一份 contributes.configuration 的编辑入口：代码渲染
 * （字体大小 / 行距滑块 / 高亮主题下拉）+ 思考模式。即改即存——每次
 * 变更直接写宿主 storage（key `config`），与桌面配置页同一数据源。
 */
import { computed, defineComponent, h, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { PluginConfig } from '../types'
import {
  CODE_FONT_SIZE_MAX,
  CODE_FONT_SIZE_MIN,
  CODE_LINE_HEIGHT_MAX,
  CODE_LINE_HEIGHT_MIN,
} from '../types'

/** 分段选择器（自绘，无原生控件外观；与宿主 token 体系一致） */
const SegmentedControl = defineComponent({
  name: 'SegmentedControl',
  props: {
    options: { type: Array as () => { value: string; label: string }[], required: true },
    modelValue: { type: String, required: true },
  },
  emits: ['change'],
  setup(props, { emit }) {
    return () =>
      h(
        'div',
        {
          class:
            'flex items-center gap-0.5 p-0.5 rounded-lg border border-[var(--mobile-border)] bg-[var(--mobile-bg-tertiary)]',
        },
        (props.options as { value: string; label: string }[]).map(opt =>
          h(
            'button',
            {
              class:
                'h-9 px-2.5 min-w-[3.25rem] rounded-md text-[var(--font-size-xs)] transition-colors duration-150 ' +
                (opt.value === props.modelValue
                  ? 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] font-medium'
                  : 'text-[var(--mobile-text-secondary)] active:bg-[var(--mobile-bg-secondary)]'),
              onClick: () => emit('change', opt.value),
            },
            opt.label,
          ),
        ),
      )
  },
})

interface SegmentedOption<T extends string> {
  value: T
  label: string
}

const props = defineProps<{
  modelValue: boolean
  config: PluginConfig
  /** 安全区底部高度（宿主注入；与 ChatView 同源） */
  safeAreaBottom?: number
}>()

const emit = defineEmits<{
  'update:modelValue': [open: boolean]
  /** 配置变更（组件内部即改即存，父级仅透传） */
  change: [next: PluginConfig]
}>()

const { t } = useI18n()

const cfg = computed(() => props.config)

function update(patch: Partial<PluginConfig>): void {
  emit('change', { ...cfg.value, ...patch })
}

// ==================== 行距滑块（自绘：轨道 + 填充 + thumb，pointer 事件拖动） ====================

const lhTrackRef = ref<HTMLElement | null>(null)
const lhDragging = ref(false)

/** 行距填充比例（0-1，驱动轨道填充宽度与 thumb 位置） */
const lhFillPercent = computed(() => {
  const ratio = (cfg.value.codeLineHeight - CODE_LINE_HEIGHT_MIN) / (CODE_LINE_HEIGHT_MAX - CODE_LINE_HEIGHT_MIN)
  return `${Math.min(Math.max(ratio, 0), 1) * 100}%`
})

/** clientX → 行距值（夹取 [0.5, 2] 并保留一位小数；步进 0.1） */
function lhValueFromClientX(clientX: number): number {
  const track = lhTrackRef.value
  if (!track) return cfg.value.codeLineHeight
  const rect = track.getBoundingClientRect()
  if (rect.width <= 0) return cfg.value.codeLineHeight
  const ratio = Math.min(Math.max((clientX - rect.left) / rect.width, 0), 1)
  const value = CODE_LINE_HEIGHT_MIN + ratio * (CODE_LINE_HEIGHT_MAX - CODE_LINE_HEIGHT_MIN)
  return Math.round(value * 10) / 10
}

function onLhPointerDown(e: PointerEvent): void {
  lhDragging.value = true
  // 捕获指针：拖出轨道范围仍持续更新；松手/取消统一复位
  ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
  update({ codeLineHeight: lhValueFromClientX(e.clientX) })
}

function onLhPointerMove(e: PointerEvent): void {
  if (!lhDragging.value) return
  update({ codeLineHeight: lhValueFromClientX(e.clientX) })
}

function onLhPointerEnd(): void {
  lhDragging.value = false
}

// ==================== 代码主题下拉（展开式选项列表） ====================

const openDropdown = ref<'codeTheme' | null>(null)

function toggleDropdown(key: 'codeTheme'): void {
  openDropdown.value = openDropdown.value === key ? null : key
}

const themeOptions: SegmentedOption<PluginConfig['codeTheme']>[] = [
  { value: 'auto', label: t('mobile.plugin.aiChatbox.codeThemeAuto') },
  { value: 'light', label: t('mobile.plugin.aiChatbox.codeThemeLight') },
  { value: 'dark', label: t('mobile.plugin.aiChatbox.codeThemeDark') },
  { value: 'github-light', label: t('mobile.plugin.aiChatbox.codeThemeGithubLight') },
  { value: 'github-dark', label: t('mobile.plugin.aiChatbox.codeThemeGithubDark') },
  { value: 'dracula', label: t('mobile.plugin.aiChatbox.codeThemeDracula') },
]

const currentThemeLabel = computed(
  () => themeOptions.find(opt => opt.value === cfg.value.codeTheme)?.label ?? themeOptions[0].label,
)

function selectTheme(value: PluginConfig['codeTheme']): void {
  update({ codeTheme: value })
  openDropdown.value = null
}

// ==================== 思考模式 ====================

const thinkingModeOptions: SegmentedOption<PluginConfig['thinkingMode']>[] = [
  { value: 'default', label: t('mobile.plugin.aiChatbox.thinkingDefault') },
  { value: 'enabled', label: t('mobile.plugin.aiChatbox.thinkingEnabled') },
  { value: 'disabled', label: t('mobile.plugin.aiChatbox.thinkingDisabled') },
]

const effortOptions: SegmentedOption<PluginConfig['reasoningEffort']>[] = [
  { value: 'low', label: t('mobile.plugin.aiChatbox.effortLow') },
  { value: 'high', label: t('mobile.plugin.aiChatbox.effortHigh') },
  { value: 'max', label: t('mobile.plugin.aiChatbox.effortMax') },
]

const showReasoningOptions: SegmentedOption<'on' | 'off'>[] = [
  { value: 'on', label: t('mobile.plugin.aiChatbox.show') },
  { value: 'off', label: t('mobile.plugin.aiChatbox.hide') },
]
</script>

<style scoped>
/* 底部弹层过渡：淡入 + 上滑（GPU 合成属性） */
.sheet-enter-active,
.sheet-leave-active {
  transition: opacity 0.2s ease;
}
.sheet-enter-active .sheet-panel,
.sheet-leave-active .sheet-panel {
  transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}
.sheet-enter-from,
.sheet-leave-to {
  opacity: 0;
}
.sheet-enter-from .sheet-panel,
.sheet-leave-to .sheet-panel {
  transform: translateY(100%);
}
/* 下拉选项展开：淡入 + 轻微下移 */
.dropdown-enter-active,
.dropdown-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}
.dropdown-enter-from,
.dropdown-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
