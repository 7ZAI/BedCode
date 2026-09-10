<template>
  <Teleport to="body">
    <div
      class="splash-native-root mobile-app mobile-ui z-[100]"
      :class="{ exiting: exiting }"
      :style="{ '--splash-exit-ms': `${exitFadeMs}ms` }"
      role="status"
      aria-live="polite"
      aria-label="BedCode"
    >
      <!-- 氛围层:极淡的琥珀辉光(纯装饰,比叙事版更克制) -->
      <div class="glow" aria-hidden="true" />

      <!-- 品牌区:原生系统开屏同款 —— 居中 ›_ 符号 + 字标,无叙事元素 -->
      <div class="brand">
        <span class="brand-inner" :class="{ in: logoIn }">
          <svg class="logo" viewBox="0 0 96 96" aria-hidden="true">
            <polyline
              class="logo-caret"
              points="20,22 52,52 20,82"
              fill="none"
              stroke="currentColor"
              stroke-width="9"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
            <rect class="logo-cursor" x="62" y="68" width="22" height="10" rx="5" fill="currentColor" />
          </svg>
        </span>
        <div class="wordmark" :class="{ in: wordIn }">BEDCODE</div>
      </div>

      <div class="footer" :class="{ in: footerIn }" aria-hidden="true">v{{ version }}</div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 开屏候选 2:原生 Android 系统开屏样式复刻(纯色底 + 品牌符号)
 *
 * 背景:Android 12+ 系统开屏由系统强制存在、且无法承载叙事动画,
 * 此候选页把「原生开屏的样子」搬到前端:纯色底(墨纸色 token,与
 * 原生 windowBackground 的 bedcode_launch_bg 同源)+ 居中品牌 ›_ 符号
 * + 字标。入场只有一次克制的淡入缩放与光标闪烁,无开机日志/进度条。
 *
 * 与 SplashScreen.vue(候选 1,终端叙事动画)保持同一对外契约:相同 props
 * (min/max 时长)与 closed 事件、同一退出时刻策略(useAppStartup.computeSplashExitAt),
 * 由 App.vue 依据 config/splash.ts 的 ACTIVE_SPLASH_CANDIDATE 二选一渲染,
 * 切换开屏样式只改配置一处。
 *
 * 静态页无需叙事时间,固定最短展示取比叙事版短的值(2s),避免纯背景长时间
 * 空挂;退出仍遵守兜底时长与「就绪即退」策略。
 */
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import pkg from '../../package.json'
import { SPLASH_CONFIG } from '@/config/splash'
import { computeSplashExitAt, readyAt as startupReadyAt, startupReady } from '@/composables/useAppStartup'

const props = withDefaults(
  defineProps<{
    /** 固定最短显示时长(ms),默认 2s(静态页短于叙事版) */
    minDurationMs?: number
    /** 最长兜底时长(ms),超过强制进入首页 */
    maxDurationMs?: number
  }>(),
  {
    minDurationMs: 2000,
    maxDurationMs: SPLASH_CONFIG.maxDurationMs,
  },
)

const emit = defineEmits<{ closed: [] }>()

const REDUCED = window.matchMedia('(prefers-reduced-motion: reduce)').matches

// ==================== 展示状态 ====================

const exiting = ref(false)
const logoIn = ref(false)
const wordIn = ref(false)
const footerIn = ref(false)

const version = pkg.version

// ==================== 时间线引擎 ====================

const exitFadeMs = SPLASH_CONFIG.exitFadeMs
/** 挂载后最短可见时长:防止挂载偏晚时开屏一闪而过(硬兜底始终优先) */
const MIN_VISIBLE_MS = 900

const timers = new Set<number>()
let exitTimer: number | null = null
let mountAt = 0

function at(delay: number, fn: () => void): void {
  const id = window.setTimeout(() => {
    timers.delete(id)
    fn()
  }, delay)
  timers.add(id)
}

// ==================== 退出调度(时长策略,与候选 1 一致) ====================

function scheduleExit(): void {
  if (exitTimer !== null) {
    window.clearTimeout(exitTimer)
    exitTimer = null
  }
  if (mountAt === 0) return

  // 策略退出时刻(最短展示从挂载起算,就绪/兜底见 computeSplashExitAt)
  const policyExitAt = computeSplashExitAt({
    now: performance.now(),
    readyAt: startupReadyAt.value,
    minExitAt: mountAt + props.minDurationMs,
    maxExitAt: props.maxDurationMs,
  })
  // 最短可见保障 + 兜底上限始终获胜(无叙事定格,比候选 1 少 NARRATIVE_HOLD)
  const exitAt = Math.min(
    Math.max(policyExitAt, mountAt + MIN_VISIBLE_MS),
    props.maxDurationMs,
  )
  exitTimer = window.setTimeout(beginExit, Math.max(0, exitAt - performance.now()))
}

function beginExit(): void {
  exiting.value = true
  // 动画播完(留 60ms 余量)再通知父组件卸载
  at(exitFadeMs + 60, () => emit('closed'))
}

// ==================== 生命周期与联动 ====================

onMounted(() => {
  mountAt = performance.now()

  if (REDUCED) {
    // 减少动态:跳过入场动画,直接呈现终态
    logoIn.value = true
    wordIn.value = true
    footerIn.value = true
  } else {
    at(80, () => (logoIn.value = true))
    at(340, () => (wordIn.value = true))
    at(520, () => (footerIn.value = true))
  }

  scheduleExit()
})

// 就绪:按策略重算退出时刻(无叙事,就绪后按 min 即可退)
watch(startupReady, () => scheduleExit())

onBeforeUnmount(() => {
  timers.forEach((id) => window.clearTimeout(id))
  timers.clear()
  if (exitTimer !== null) window.clearTimeout(exitTimer)
})
</script>

<style scoped>
/* ==================== 根布局 ==================== */

.splash-native-root {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  /* 纯色墨纸底:与 Android 原生 windowBackground(bedcode_launch_bg)同源,
     原生窗口 → 前端开屏无缝衔接,见 themes.xml 恢复清单注释 */
  background: var(--mobile-bg-primary);
  padding: calc(env(safe-area-inset-top, 0px) + 24px) 28px calc(env(safe-area-inset-bottom, 0px) + 24px);
  overflow: hidden;
}

.splash-native-root.exiting {
  animation: splashExit var(--splash-exit-ms, 500ms) ease-in forwards;
  will-change: transform, opacity;
  /* 淡出期间不再拦截触摸：即使极端情况下未及时卸载，也不会盖住主界面交互 */
  pointer-events: none;
}

@keyframes splashExit {
  to {
    opacity: 0;
    transform: scale(1.045);
  }
}

/* ==================== 氛围层(极淡辉光,克制于叙事版) ==================== */

.glow {
  position: absolute;
  left: 50%;
  top: 42%;
  width: 280px;
  height: 280px;
  transform: translate(-50%, -50%);
  background: radial-gradient(
    circle,
    color-mix(in srgb, var(--mobile-warning) 9%, transparent) 0%,
    transparent 60%
  );
  animation: breathe 5.2s ease-in-out infinite alternate;
  pointer-events: none;
}

@keyframes breathe {
  from {
    opacity: 0.35;
  }
  to {
    opacity: 0.65;
  }
}

/* ==================== 品牌区 ==================== */

.brand {
  text-align: center;
}

.brand-inner {
  display: inline-block;
  opacity: 0;
  transform: scale(0.82);
  transition:
    opacity 0.5s cubic-bezier(0.22, 1, 0.36, 1),
    transform 0.5s cubic-bezier(0.22, 1, 0.36, 1);
}

.brand-inner.in {
  opacity: 1;
  transform: scale(1);
}

.logo {
  width: 96px;
  height: 96px;
  color: var(--mobile-text-primary);
  display: inline-block;
}

.logo-caret {
  stroke-dasharray: 200;
  stroke-dashoffset: 200;
}

.brand-inner.in .logo-caret {
  animation: draw 0.6s 0.1s cubic-bezier(0.5, 0, 0.2, 1) forwards;
}

@keyframes draw {
  to {
    stroke-dashoffset: 0;
  }
}

.logo-cursor {
  opacity: 0;
}

.brand-inner.in .logo-cursor {
  animation:
    cursorIn 0.3s 0.55s both,
    blink 1.1s 1.15s infinite;
}

@keyframes cursorIn {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

@keyframes blink {
  0%,
  49% {
    opacity: 1;
  }
  50%,
  100% {
    opacity: 0;
  }
}

.wordmark {
  margin-top: 22px;
  font-size: var(--font-size-base);
  font-weight: 600;
  color: var(--mobile-text-primary);
  letter-spacing: 0.42em;
  text-indent: 0.42em;
  opacity: 0;
  transform: translateY(10px);
  transition:
    opacity 0.5s cubic-bezier(0.22, 1, 0.36, 1),
    transform 0.5s cubic-bezier(0.22, 1, 0.36, 1);
}

.wordmark.in {
  opacity: 1;
  transform: none;
}

/* ==================== 版本脚注(调试用途,淡入最晚) ==================== */

.footer {
  position: absolute;
  bottom: calc(env(safe-area-inset-bottom, 0px) + 18px);
  font-size: var(--font-size-xs);
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--mobile-text-muted);
  opacity: 0;
  transition: opacity 0.5s cubic-bezier(0.22, 1, 0.36, 1);
}

.footer.in {
  opacity: 1;
}

/* ==================== 无障碍:跟随系统减少动态 ==================== */

@media (prefers-reduced-motion: reduce) {
  .splash-native-root *,
  .splash-native-root *::before,
  .splash-native-root *::after {
    animation: none !important;
    transition: none !important;
  }

  /* 描边/淡入类动画被禁用后直接呈现终态 */
  .logo-caret {
    stroke-dashoffset: 0;
  }

  .logo-cursor {
    opacity: 1;
  }
}
</style>