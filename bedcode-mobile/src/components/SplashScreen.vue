<template>
  <Teleport to="body">
    <div
      class="splash-root mobile-app mobile-ui z-[100]"
      :class="{ exiting: exiting }"
      :style="{ '--splash-exit-ms': `${exitFadeMs}ms` }"
      role="status"
      aria-live="polite"
      :aria-label="tagline"
    >
      <!-- 氛围层:琥珀辉光 + 扫描线(纯装饰) -->
      <div class="glow" aria-hidden="true" />
      <div class="scanlines" aria-hidden="true" />

      <!-- 品牌区:App 图标同款终端符号 ›_ -->
      <div class="brand">
        <span ref="brandWrap" class="brand-inner" :class="{ hop: hopping }">
          <svg class="logo r" :class="{ in: logoIn }" viewBox="0 0 96 96" aria-hidden="true">
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
        <div class="wordmark r" :class="{ in: wordIn }">BEDCODE</div>
        <div class="tagline r" :class="{ in: tagIn }">{{ tagline }}</div>
      </div>

      <!-- 开机日志:打字行 + 任务行(完成打勾) + 就绪行 -->
      <div class="bootlog font-mono" aria-hidden="true">
        <div class="line r" :class="{ in: typedLineIn }">
          <span class="prompt">$</span>
          <span>{{ typedText }}</span>
          <span v-if="typedCaretOn" class="caret-inline" />
        </div>

        <div v-for="line in labels" :key="line.id" class="line r" :class="{ in: shown[line.id] }">
          <span class="tri">▸</span>
          <span class="task">{{ line.label }}</span>
          <span class="dots" />
          <span class="ok" :class="{ show: okShown[line.id] }">✓</span>
        </div>

        <div class="line r" :class="{ in: doneShown }">
          <span class="prompt">$</span>
          <span class="ok-msg">{{ readyText }}</span>
          <span class="block-caret" />
        </div>
      </div>

      <!-- 终端方块进度条:随任务行揭示分批点亮,琥珀脉冲头块停在已点亮前沿 -->
      <div class="progress" aria-hidden="true">
        <span
          v-for="b in blockCount"
          :key="b"
          class="block"
          :class="{ on: b <= litBlocks, head: b === litBlocks && litBlocks > 0 && litBlocks < blockCount }"
        />
        <span class="pct">{{ pctText }}</span>
      </div>

      <div class="footer" aria-hidden="true">
        <span>BedCode v{{ version }}</span>
        <span>LAN · WS</span>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 开屏动画(Terminal Boot)——「终端开机自检」
 *
 * 职责:展示启动进度叙事(品牌 ›_ 符号 + 开机日志 + 方块进度条),
 * 并决定退出时刻:最短展示时长从挂载(可见)起算,保证开机叙事完整播放;
 * 就绪后让就绪行定格一拍再交接;未就绪时按兜底时长强制淡出。
 * 淡出动画播完 emit('closed'),由父组件 v-if 卸载。
 *
 * 根元素带 mobile-app mobile-ui 类:Teleport 到 body 后仍能命中
 * mobile.css 的浅色 token 作用域(html:not(.dark) .mobile-ui)。
 */
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import pkg from '../../package.json'
import { SPLASH_CONFIG } from '@/config/splash'
import {
  computeSplashExitAt,
  readyAt as startupReadyAt,
  startupReady,
  taskCompletedAt,
} from '@/composables/useAppStartup'

const props = withDefaults(
  defineProps<{
    /** 固定最短显示时长(ms),默认取 SPLASH_CONFIG */
    minDurationMs?: number
    /** 最长兜底时长(ms),超过强制进入首页 */
    maxDurationMs?: number
  }>(),
  {
    minDurationMs: SPLASH_CONFIG.minDurationMs,
    maxDurationMs: SPLASH_CONFIG.maxDurationMs,
  },
)

const emit = defineEmits<{ closed: [] }>()

const { t } = useI18n()

const REDUCED = window.matchMedia('(prefers-reduced-motion: reduce)').matches

// ==================== 展示状态 ====================

const exiting = ref(false)
const logoIn = ref(false)
const wordIn = ref(false)
const tagIn = ref(false)
const typedLineIn = ref(false)
const typedCaretOn = ref(true)
const doneShown = ref(false)
const hopping = ref(false)
const typedText = ref('')
/** 挂载前已完成的任务行,进入后按 stagger 揭示;之后的由 watch 驱动 */
const shown = reactive<Record<string, boolean>>(
  Object.fromEntries(SPLASH_CONFIG.lines.map((line) => [line.id, false])),
)
const okShown = reactive<Record<string, boolean>>(
  Object.fromEntries(SPLASH_CONFIG.lines.map((line) => [line.id, false])),
)

const brandWrap = ref<HTMLElement | null>(null)

// ==================== 文案(i18n 可配置) ====================

const tagline = computed(() => t(SPLASH_CONFIG.taglineKey))
const readyText = computed(() => t(SPLASH_CONFIG.readyKey))
const fullTyped = computed(() => t(SPLASH_CONFIG.typedCommandKey))
const labels = computed(() =>
  SPLASH_CONFIG.lines.map((line) => ({ id: line.id, label: t(line.labelKey) })),
)
const version = pkg.version

// ==================== 进度 ====================

// 跟随任务行揭示节奏(而非真实完成时刻):挂载前已完成的任务按 stagger 逐行
// 揭示,方块随之分批点亮——避免挂载瞬间直接跳满,进度与 ✓ 弹出保持同步
const blockCount = 14
const revealedCount = computed(
  () => SPLASH_CONFIG.lines.filter((line) => shown[line.id]).length,
)
const litBlocks = computed(() =>
  Math.round((revealedCount.value / SPLASH_CONFIG.lines.length) * blockCount),
)
const pctText = computed(() =>
  `${Math.round((revealedCount.value / SPLASH_CONFIG.lines.length) * 100)}%`,
)

// ==================== 时间线引擎 ====================

const TYPE_INTERVAL = 40
const PRE_START = 1400
const PRE_STEP = 180
const OK_DELAY = 240
/** 就绪行展示后的定格时长:让「就绪」停留一拍再交接(对齐原型 ~1.25s) */
const NARRATIVE_HOLD_MS = 1100
/** 挂载后最短可见时长:防止挂载偏晚时开屏一闪而过(硬兜底始终优先) */
const MIN_VISIBLE_MS = 900
const exitFadeMs = SPLASH_CONFIG.exitFadeMs

const timers = new Set<number>()
let exitTimer: number | null = null
let mountAt = 0
/** 最近一次任务行 ✓ 的展示时刻(绝对);就绪行需排在其后 */
let lastOkAt = 0
/** 打字动画收尾时刻(挂载起算);就绪行不得早于它出现 */
let typingEndMs = 0
/** 就绪行的排程展示时刻(绝对);作为退出定格的起点 */
let doneRevealAt: number | null = null
/** 已排程揭示的任务行数:挂载后完成的行接在队尾,保持节奏连续 */
let revealSlot = 0

function at(delay: number, fn: () => void): void {
  const id = window.setTimeout(() => {
    timers.delete(id)
    fn()
  }, delay)
  timers.add(id)
}

function hop(): void {
  if (REDUCED || !brandWrap.value) return
  hopping.value = false
  // 强制重排以重启 CSS 动画
  void brandWrap.value.offsetWidth
  hopping.value = true
}

function revealLine(id: string): void {
  shown[id] = true
  lastOkAt = Math.max(lastOkAt, performance.now() + OK_DELAY)
  at(OK_DELAY, () => {
    okShown[id] = true
    hop()
  })
}

/**
 * 按固定节奏揭示任务行:挂载前已完成的排在前队,挂载后完成的接在队尾
 * (节奏窗口已过则立即揭示)。进度方块随揭示分批点亮,与 ✓ 弹出同步
 */
function scheduleReveal(id: string): void {
  const slotAt = mountAt + PRE_START + revealSlot * PRE_STEP
  revealSlot += 1
  const delay = Math.max(0, slotAt - performance.now())
  at(delay, () => revealLine(id))
  lastOkAt = Math.max(lastOkAt, performance.now() + delay + OK_DELAY)
}

function revealDone(): void {
  // 就绪即收束打字(后续字符定时器因长度不匹配自动失效)
  typedText.value = fullTyped.value
  typedCaretOn.value = false
  doneShown.value = true
  doneRevealAt = performance.now()
}

/**
 * 排程就绪行:等打字与任务行揭示收尾后再展示,
 * 避免"就绪"先于全部 ✓ 出现、或快启动时动画被立即截断
 */
function scheduleDone(readyTime: number): void {
  const finish = Math.max(readyTime + OK_DELAY, lastOkAt, mountAt + typingEndMs)
  doneRevealAt = finish + 80
  at(Math.max(0, finish + 80 - performance.now()), revealDone)
}

// ==================== 退出调度(时长策略) ====================

function scheduleExit(): void {
  if (exitTimer !== null) {
    window.clearTimeout(exitTimer)
    exitTimer = null
  }
  if (mountAt === 0) return

  // 最短展示从挂载(真正可见)起算:若从应用打开起算,静态首屏的加载耗时
  // 会预先烧掉固定显示时长,挂载后动画还没开播就被判超时提前退出
  const policyExitAt = computeSplashExitAt({
    now: performance.now(),
    readyAt: startupReadyAt.value,
    minExitAt: mountAt + props.minDurationMs,
    maxExitAt: props.maxDurationMs,
  })
  // 就绪后兜底上限让位于完整叙事(就绪行定格一拍再交接);
  // 未就绪时硬兜底按时生效,防止启动卡死困住用户
  const narrativeExitAt = doneRevealAt !== null ? doneRevealAt + NARRATIVE_HOLD_MS : null
  const hardCap =
    startupReady.value && narrativeExitAt !== null
      ? Math.max(props.maxDurationMs, narrativeExitAt)
      : props.maxDurationMs
  // 最短可见保障 + 硬兜底始终获胜
  const exitAt = Math.min(
    Math.max(policyExitAt, mountAt + MIN_VISIBLE_MS, narrativeExitAt ?? 0),
    hardCap,
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
    // 减少动态:跳过打字与 stagger,已完成的任务行直接全显
    typedText.value = fullTyped.value
    SPLASH_CONFIG.lines.forEach((line) => {
      if (taskCompletedAt[line.id] != null) {
        shown[line.id] = true
        okShown[line.id] = true
      }
    })
    if (startupReady.value) revealDone()
  } else {
    at(60, () => (logoIn.value = true))
    at(300, () => (wordIn.value = true))
    at(440, () => (tagIn.value = true))
    at(580, () => (typedLineIn.value = true))

    typingEndMs = 700 + fullTyped.value.length * TYPE_INTERVAL + 100
    fullTyped.value.split('').forEach((ch, i) => {
      at(700 + i * TYPE_INTERVAL, () => {
        if (typedText.value.length === i) typedText.value += ch
      })
    })

    // 挂载前已完成的任务行按固定节奏揭示(动画完整感);之后完成的接队尾
    SPLASH_CONFIG.lines.forEach((line) => {
      if (taskCompletedAt[line.id] != null) scheduleReveal(line.id)
    })

    if (startupReady.value) scheduleDone(performance.now())
  }

  scheduleExit()
})

// 挂载后陆续完成的任务行:接在揭示节奏队尾(节奏已过则立即揭示)
watch(
  taskCompletedAt,
  () => {
    for (const line of SPLASH_CONFIG.lines) {
      if (taskCompletedAt[line.id] != null && !shown[line.id]) {
        if (REDUCED) {
          shown[line.id] = true
          okShown[line.id] = true
        } else {
          scheduleReveal(line.id)
        }
      }
    }
  },
  { deep: true },
)

// 就绪:按收尾节奏展示就绪行,并按策略重算退出时刻
watch(startupReady, (ready) => {
  if (!ready) return
  scheduleDone(performance.now())
  scheduleExit()
})

onBeforeUnmount(() => {
  timers.forEach((id) => window.clearTimeout(id))
  timers.clear()
  if (exitTimer !== null) window.clearTimeout(exitTimer)
})
</script>

<style scoped>
/* ==================== 根布局 ==================== */

.splash-root {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
  padding: calc(env(safe-area-inset-top, 0px) + 56px) 28px calc(env(safe-area-inset-bottom, 0px) + 28px);
  background: linear-gradient(
    160deg,
    var(--mobile-bg-secondary) 0%,
    var(--mobile-bg-primary) 55%,
    var(--mobile-terminal-bg) 100%
  );
  container-type: size;
  overflow: hidden;
}

.splash-root.exiting {
  animation: splashExit var(--splash-exit-ms, 500ms) ease-in forwards;
  will-change: transform, opacity;
  /* 淡出期间不再拦截触摸：即使极端情况下未及时卸载，也不会盖住主界面交互 */
  pointer-events: none;
}

/* 浅色下渐变反转为「上亮下暗」:三枚 token 的明度顺序在两套主题间相反,
   直接换序复用,保持与原型一致的光从顶部来的方向感。
   注意:不能用 `:global(html:not(.dark)) .splash-root` 写法——scoped 编译器
   会丢掉 :global() 之后的后代选择器,把规则错挂到 html 本体上(见
   scanlines 的 opacity:.06 曾导致整页 6% 透明的灰蓝罩层);裸祖先选择器
   会被正确编译为 `html:not(.dark) .splash-root[data-v-xxx]` */
html:not(.dark) .splash-root {
  background: linear-gradient(
    160deg,
    var(--mobile-terminal-bg) 0%,
    var(--mobile-bg-primary) 55%,
    var(--mobile-bg-secondary) 100%
  );
}

@keyframes splashExit {
  to {
    opacity: 0;
    transform: scale(1.045);
  }
}

/* ==================== 氛围层 ==================== */

.glow {
  position: absolute;
  left: 50%;
  top: 20cqh;
  width: 250px;
  height: 250px;
  transform: translate(-50%, -50%);
  background: radial-gradient(
    circle,
    color-mix(in srgb, var(--mobile-warning) 13%, transparent) 0%,
    transparent 58%
  );
  animation: breathe 4.6s ease-in-out infinite alternate;
  pointer-events: none;
}

@keyframes breathe {
  from {
    opacity: 0.45;
  }
  to {
    opacity: 0.8;
  }
}

.scanlines {
  position: absolute;
  inset: 0;
  background: repeating-linear-gradient(0deg, var(--mobile-border) 0 1px, transparent 1px 3px);
  opacity: 0.14;
  pointer-events: none;
}

/* 浅色下扫描线减弱,避免纸面脏感(写法注意同上,勿用 :global() 后代形式) */
html:not(.dark) .scanlines {
  opacity: 0.06;
}

/* ==================== 品牌区 ==================== */

.brand {
  margin-top: 11cqh;
  text-align: center;
}

.brand-inner {
  display: inline-block;
}

.brand-inner.hop {
  animation: hop 0.32s cubic-bezier(0.34, 1.56, 0.64, 1);
}

@keyframes hop {
  0% {
    transform: translateX(0);
  }
  40% {
    transform: translateX(4px);
  }
  100% {
    transform: translateX(0);
  }
}

.logo {
  width: 84px;
  height: 84px;
  color: var(--mobile-text-primary);
  display: inline-block;
}

.logo-caret {
  stroke-dasharray: 200;
  stroke-dashoffset: 200;
}

.logo.in .logo-caret {
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

.logo.in .logo-cursor {
  animation: cursorIn 0.3s 0.55s both, blink 1.1s 1.15s infinite;
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
  margin-top: 18px;
  font-size: var(--font-size-base);
  font-weight: 600;
  color: var(--mobile-text-primary);
  letter-spacing: 0.42em;
  text-indent: 0.42em;
}

.tagline {
  margin-top: 9px;
  font-size: var(--font-size-xs);
  color: var(--mobile-text-muted);
  letter-spacing: 0.22em;
  text-indent: 0.22em;
}

/* ==================== 通用入场 ==================== */

.r {
  opacity: 0;
  transform: translateY(12px);
  transition:
    opacity 0.5s cubic-bezier(0.22, 1, 0.36, 1),
    transform 0.5s cubic-bezier(0.22, 1, 0.36, 1);
}

.r.in {
  opacity: 1;
  transform: none;
}

/* ==================== 开机日志 ==================== */

.bootlog {
  margin-top: auto;
  font-size: var(--font-size-sm);
  line-height: 2.1;
  color: var(--mobile-text-secondary);
  max-width: 96%;
}

.line {
  display: flex;
  align-items: baseline;
  gap: 9px;
  white-space: nowrap;
}

.prompt {
  color: var(--mobile-warning);
  font-weight: 700;
}

.tri {
  color: var(--mobile-text-muted);
}

.task {
  color: var(--mobile-text-secondary);
}

.dots {
  flex: 1;
  min-width: 14px;
  border-bottom: 1px dotted var(--mobile-border-hover);
  opacity: 0.55;
  transform: translateY(-4px);
}

.ok {
  color: var(--mobile-success);
  font-weight: 700;
  opacity: 0;
  transform: scale(0.4);
}

.ok.show {
  opacity: 1;
  transform: scale(1);
  animation: okPop 0.32s cubic-bezier(0.34, 1.56, 0.64, 1);
}

@keyframes okPop {
  0% {
    opacity: 0;
    transform: scale(0.4);
  }
  62% {
    opacity: 1;
    transform: scale(1.22);
  }
  100% {
    opacity: 1;
    transform: scale(1);
  }
}

.ok-msg {
  color: var(--mobile-success);
}

.caret-inline {
  display: inline-block;
  width: 7px;
  height: 14px;
  background: var(--mobile-warning);
  vertical-align: -2px;
  animation: blink 1.06s infinite;
}

.block-caret {
  display: inline-block;
  width: 9px;
  height: 15px;
  background: var(--mobile-warning);
  vertical-align: -2px;
  animation: blink 1.06s infinite;
}

/* ==================== 进度条 ==================== */

.progress {
  margin-top: 20px;
  display: flex;
  align-items: center;
  gap: 3px;
}

.block {
  width: 14px;
  height: 7px;
  border-radius: 1.5px;
  background: var(--mobile-accent-muted);
  transition: background-color 0.18s;
}

.block.on {
  background: var(--mobile-text-primary);
}

.block.head {
  background: var(--mobile-warning);
  animation: headPulse 0.9s ease-in-out infinite;
}

@keyframes headPulse {
  0%,
  100% {
    box-shadow: 0 0 0 0 transparent;
    opacity: 1;
  }
  50% {
    box-shadow: 0 0 9px 1px color-mix(in srgb, var(--mobile-warning) 40%, transparent);
    opacity: 0.72;
  }
}

.pct {
  margin-left: 10px;
  font-size: var(--font-size-xs);
  color: var(--mobile-text-muted);
  min-width: 34px;
}

/* ==================== 底栏 ==================== */

.footer {
  margin-top: 16px;
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  font-size: var(--font-size-xs);
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--mobile-text-muted);
}

/* ==================== 无障碍:跟随系统减少动态 ==================== */

@media (prefers-reduced-motion: reduce) {
  .splash-root *,
  .splash-root *::before,
  .splash-root *::after {
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

  .r {
    opacity: 1;
    transform: none;
  }
}
</style>
