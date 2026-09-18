<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="visible" class="fixed inset-0 z-[100] mobile-ui">
        <!-- 聚光灯遮罩：四块暗化区域围出目标高亮孔，整体 pointer-events none
             不阻挡对真实元素的操作（交互步骤要求用户在真机元素上完成手势） -->
        <div class="absolute inset-0 pointer-events-none">
          <div class="tour-dim" :style="dimStyle('top')"></div>
          <div class="tour-dim" :style="dimStyle('bottom')"></div>
          <div class="tour-dim" :style="dimStyle('left')"></div>
          <div class="tour-dim" :style="dimStyle('right')"></div>
          <div
            v-if="hole"
            class="absolute rounded-xl border-2 border-[var(--mobile-accent)] shadow-[0_0_0_9999px_rgba(0,0,0,0)]"
            :style="holeStyle"
          ></div>
        </div>

        <!-- 引导卡片：定位在目标附近，pointer-events auto -->
        <div
          v-if="card"
          class="absolute tour-card bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 shadow-xl"
          :style="cardStyle"
        >
          <div class="flex items-center justify-between mb-2">
            <span class="text-xs font-medium text-[var(--mobile-accent)]">{{ stepLabel }}</span>
            <button
              class="text-xs text-[var(--mobile-text-muted)] active:opacity-70 px-1.5 py-1 rounded-md"
              @click="finish"
            >
              {{ t('mobile.terminal.onboardingSkip') }}
            </button>
          </div>
          <p class="text-sm font-semibold text-[var(--mobile-text-primary)]">{{ t(step.titleKey) }}</p>
          <p class="text-xs text-[var(--mobile-text-secondary)] mt-1.5 leading-relaxed">{{ t(step.descKey) }}</p>
          <p
            v-if="step.tryHintKey"
            class="text-xs font-medium text-[var(--mobile-accent)] mt-2"
          >
            {{ t('mobile.terminal.onboardingTry') }}{{ t(step.tryHintKey) }}
          </p>
          <button
            class="mt-3 w-full py-2.5 rounded-lg bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] text-sm font-semibold active:opacity-80 transition-opacity"
            @click="advance"
          >
            {{ isLast ? t('mobile.terminal.onboardingDone') : t('mobile.terminal.onboardingNext') }}
          </button>
          <button
            v-if="isLast"
            class="mt-2 w-full py-2 rounded-lg border border-[var(--mobile-border)] bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-secondary)] text-xs active:opacity-80 transition-opacity"
            @click="emit('open-help')"
          >
            {{ t('mobile.terminal.onboardingFullGuide') }}
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 终端新手引导 · 聚光灯分步导览（首次进入终端页自动展示，设置里可重开）
 *
 * 与纯文字卡片的区别：每一步用聚光灯高亮**真实 UI 元素**，操作型步骤要求
 * 用户在真机元素上完成手势（快捷条滑动/打开面板/面板翻页），由轮询检测
 * 完成后自动进入下一步；信息型步骤（自定义命令/发送与执行）用「下一步」
 * 推进。遮罩 pointer-events none，不阻挡对真实元素的操作。
 *
 * 展示与否由 assistStore.settings.terminalOnboardingPending 控制：关闭/
 * 跳过/完成均清除标记（本设备展示过一次），设置里可重新开启。
 */
import { computed, ref, watch, onBeforeUnmount } from 'vue'
import { useI18n } from 'vue-i18n'
import { TERMINAL_ONBOARDING_STEPS } from '@/config/terminalOnboardingSteps'

const { t } = useI18n()

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  close: []
  'open-help': []
}>()

/** 轮询间隔：目标矩形重测量 + 交互完成检测 */
const POLL_INTERVAL_MS = 250

const stepIndex = ref(0)
const hole = ref<{ left: number; top: number; width: number; height: number } | null>(null)
const card = ref<{ left: number; top: number } | null>(null)
/** 交互基线：进入步骤时记录（快捷条 scrollLeft），与当前值比对判定完成 */
let baseline = 0
/** 出现后消失型检测的状态（自动补全面板/选择模式/菜单/侧栏），步骤切换时重置 */
const flags = ref<Record<string, boolean>>({})
let pollTimer: ReturnType<typeof setInterval> | null = null

/**
 * 「出现后消失」完成语义：目标元素出现（用户做了动作）记 saw，之后消失
 * （用户完成/退出动作）才算完成——引导不会在用户操作中途抢跑推进
 */
function sawThenGone(selector: string, key: string): boolean {
  const present = !!document.querySelector(selector)
  if (present) {
    flags.value[key] = true
    return false
  }
  return !!flags.value[key]
}

/** 快捷键面板轮播当前激活页（圆点 index，0 起） */
function activeDotIndex(): number {
  const dots = [...document.querySelectorAll('.carousel-dots .dot')]
  return dots.findIndex((d) => d.classList.contains('active'))
}

/** 按 checkKind 组装当前步骤的完成检测（i18n key 前缀在此统一拼接） */
function buildStep(index: number) {
  const def = TERMINAL_ONBOARDING_STEPS[index]
  const k = (key: string) => `mobile.terminal.${key}`
  const check = (() => {
    switch (def.checkKind) {
      case 'quickBarScroll':
        return () => {
          const qb = document.querySelector('.quick-bar')
          return qb ? Math.abs((qb as HTMLElement).scrollLeft - baseline) > 4 : false
        }
      case 'sawGone':
        return () => sawThenGone(def.appearSelector ?? def.targetSelector, `saw${index}`)
      case 'panelDotIndex':
        return () => activeDotIndex() === 1
      default:
        return undefined
    }
  })()
  return {
    targetSelector: def.targetSelector,
    titleKey: k(def.titleKey),
    descKey: k(def.descKey),
    tryHintKey: def.tryHintKey ? k(def.tryHintKey) : undefined,
    check,
  }
}

const step = computed(() => buildStep(Math.min(stepIndex.value, TERMINAL_ONBOARDING_STEPS.length - 1)))
const isLast = computed(() => stepIndex.value >= TERMINAL_ONBOARDING_STEPS.length - 1)
const stepLabel = computed(() =>
  t('mobile.terminal.onboardingStepOf', {
    current: stepIndex.value + 1,
    total: TERMINAL_ONBOARDING_STEPS.length,
  }),
)

/** 测量目标元素：更新聚光灯孔与引导卡片位置（目标缺失则跳到下一步） */
function measure() {
  const el = document.querySelector(step.value.targetSelector)
  if (!el) {
    // 目标不存在（面板被关闭等）：自动推进，避免引导卡死
    advance()
    return
  }
  const rect = el.getBoundingClientRect()
  const pad = 6
  hole.value = {
    left: Math.max(0, rect.left - pad),
    top: Math.max(0, rect.top - pad),
    width: rect.width + pad * 2,
    height: rect.height + pad * 2,
  }
  // 卡片优先放目标下方，空间不足放上方；水平钳制在视口内
  const vw = window.innerWidth
  const cardW = Math.min(vw - 24, 340)
  const below = rect.bottom + pad + 12
  const top = below + 180 < window.innerHeight ? below : Math.max(12, rect.top - pad - 12 - 190)
  const left = Math.min(Math.max(12, rect.left), vw - cardW - 12)
  card.value = { left, top }
}

/** 聚光灯四块暗化区域（围绕高亮孔） */
function dimStyle(side: 'top' | 'bottom' | 'left' | 'right'): Record<string, string> {
  if (!hole.value) return { display: 'none' }
  const h = hole.value
  const style: Record<string, string> = { background: 'var(--mobile-overlay-heavy)' }
  if (side === 'top') Object.assign(style, { left: 0, top: 0, right: 0, height: `${h.top}px` })
  if (side === 'bottom') {
    Object.assign(style, {
      left: 0,
      right: 0,
      bottom: 0,
      top: `${h.top + h.height}px`,
    })
  }
  if (side === 'left') Object.assign(style, { left: 0, top: `${h.top}px`, width: `${h.left}px`, height: `${h.height}px` })
  if (side === 'right') {
    Object.assign(style, {
      left: `${h.left + h.width}px`,
      top: `${h.top}px`,
      right: 0,
      height: `${h.height}px`,
    })
  }
  return style
}

const holeStyle = computed(() => {
  if (!hole.value) return {}
  const h = hole.value
  return {
    left: `${h.left}px`,
    top: `${h.top}px`,
    width: `${h.width}px`,
    height: `${h.height}px`,
  }
})

const cardStyle = computed(() => {
  if (!card.value) return { display: 'none' }
  return { left: `${card.value.left}px`, top: `${card.value.top}px`, width: `${Math.min(window.innerWidth - 24, 340)}px` }
})

function advance() {
  if (isLast.value) {
    finish()
    return
  }
  stepIndex.value++
}

function finish() {
  emit('close')
}

watch(() => props.visible, (visible) => {
  if (visible) {
    stepIndex.value = 0
    measure()
    startPolling()
  } else {
    stopPolling()
  }
})

watch(stepIndex, () => {
  // 进入新步骤：重置「出现后消失」状态；记录交互基线（快捷条 scrollLeft）
  flags.value = {}
  const el = document.querySelector(step.value.targetSelector)
  baseline = step.value.targetSelector === '.quick-bar' && el ? (el as HTMLElement).scrollLeft : 0
  measure()
})

function startPolling() {
  stopPolling()
  pollTimer = setInterval(() => {
    measure()
    // 交互步骤：检测到用户完成手势 → 自动进入下一步
    if (step.value.check?.()) advance()
  }, POLL_INTERVAL_MS)
}

function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

onBeforeUnmount(stopPolling)
</script>

<style scoped>
.tour-dim {
  position: absolute;
}
</style>
