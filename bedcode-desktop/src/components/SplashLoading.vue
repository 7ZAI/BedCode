<template>
  <Teleport to="body">
    <Transition name="splash">
      <div
        v-if="visible"
        class="splash-root fixed inset-0 z-[100] flex flex-col items-center justify-center overflow-hidden"
        role="status"
        :aria-label="statusText"
      >
        <!-- 呼吸辉光层（装饰，屏幕阅读器忽略） -->
        <div class="splash-halo" aria-hidden="true"></div>

        <div class="splash-content relative z-[1] flex flex-col items-center">
          <!-- Logo（品牌 glyph 的下划线光标会像活终端光标一样闪烁） -->
          <div class="splash-rise mb-7">
            <slot name="logo">
              <div
                class="splash-logo flex h-[72px] w-[72px] items-center justify-center rounded-2xl"
              >
                <svg
                  class="h-11 w-11 text-[var(--splash-text)]"
                  viewBox="0 0 100 100"
                  fill="currentColor"
                  aria-hidden="true"
                >
                  <!-- 提示符箭头 -->
                  <path d="M 24 18 L 59 50 L 24 82 L 32 74 L 51 50 L 32 26 Z" />
                  <!-- 下划线光标：慢速硬闪烁，呼应终端光标 -->
                  <path class="splash-glyph-caret" d="M 51 60 L 84 62 L 53 65 Z" />
                </svg>
              </div>
            </slot>
          </div>

          <!-- 品牌名 + 副标语 -->
          <div
            class="splash-rise splash-rise-1 mb-1.5 text-xl font-semibold tracking-[0.18em] text-[var(--splash-text)]"
          >
            BedCode
          </div>
          <div
            class="splash-rise splash-rise-2 mb-10 text-xs tracking-wider text-[var(--splash-text-dim)]"
          >
            {{ t('desktop.splash.tagline') }}
          </div>

          <!-- 技术内容列：整组以屏幕中线为中心 -->
          <div class="splash-technical flex flex-col items-center">
            <!-- 终端启动行：$ 提示符 + 打字机命令 + 块状光标 -->
            <div
              class="splash-rise splash-rise-3 wb-mono flex h-5 items-center text-[calc(13px*var(--ui-scale))]"
            >
              <span class="text-[var(--splash-text-faint)]">$&nbsp;</span
              ><span class="splash-typed text-[var(--splash-text)]">bedcode</span
              ><span class="splash-caret" aria-hidden="true"></span>
            </div>

            <!-- Boot log 阶段栈（可选，替代状态文本） -->
            <div v-if="bootLogs.length" class="splash-bootlog wb-mono mt-5 flex flex-col gap-1.5" role="log">
              <div
                v-for="(log, i) in bootLogs"
                :key="i"
                class="bootlog-row"
                :data-delay="i + 1"
              >
                <span class="mark" aria-hidden="true">✓</span>
                <span class="text">{{ log }}</span>
              </div>
            </div>

            <!-- 状态文本（无 boot log 时显示） -->
            <p
              v-if="statusText && !bootLogs.length"
              class="splash-rise splash-rise-3 mt-4 text-sm text-[var(--splash-text-dim)]"
            >
              {{ statusText }}
            </p>

            <!-- 段式进度指示（可选） -->
            <div v-if="showSegments" class="splash-progress-track mt-6 flex gap-1 items-center justify-center" aria-hidden="true">
              <span
                v-for="i in segmentCount"
                :key="i"
                class="seg"
                :class="{ done: i <= progressSegments }"
                :data-delay="i"
              ></span>
            </div>

            <!-- 进度条（可选，传统方式） -->
            <template v-if="showProgress">
              <div class="mt-6 h-1 w-48 overflow-hidden rounded-full bg-[var(--splash-track)]">
                <div
                  class="splash-progress h-full rounded-full bg-[var(--splash-text)] opacity-80"
                  :style="{ width: `${progress}%` }"
                ></div>
              </div>
              <p class="wb-mono mt-2 text-[calc(11px*var(--ui-scale))] text-[var(--splash-text-dim)]">
                {{ progress }}%
              </p>
            </template>
          </div>
        </div>

        <!-- 底部系统状态微信息 -->
        <div class="splash-footer wb-mono" aria-hidden="true">
          <span class="text">{{ footerText }}</span>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * SplashLoading - 桌面端启动画面（终端 Boot 风格）
 *
 * 固定深色品牌渐变背景（镜像 src-tauri/icons/icon.svg，不随主题切换，
 * 避免启动期主题闪烁），内容为品牌 glyph + 打字机启动行 + boot log 阶段栈 + 可选进度条。
 * 通过 visible 控制显隐；宿主在初始化完成后置 false 触发淡出。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

interface Props {
  visible: boolean
  /** 自定义状态文案；缺省回退到 i18n 的 desktop.splash.status */
  status?: string
  showProgress?: boolean
  progress?: number
  /** Boot log 阶段文案列表；提供时显示 boot log 阶段栈替代状态文本 */
  bootLogs?: string[]
  /** 是否显示段式进度指示 */
  showSegments?: boolean
  /** 段式进度指示总段数 */
  segmentCount?: number
  /** 已完成段数 */
  progressSegments?: number
  /** 底部 footer 文案 */
  footerText?: string
}

const props = withDefaults(defineProps<Props>(), {
  status: '',
  showProgress: false,
  progress: 0,
  bootLogs: () => [],
  showSegments: false,
  segmentCount: 4,
  progressSegments: 0,
  footerText: 'bedcode v1.0.0 · LAN remote terminal',
})

const { t } = useI18n()

const statusText = computed(() => props.status || t('desktop.splash.status'))
</script>

<style scoped>
/* ==================== 品牌常量 ==================== */
/* 启动画面固定深色，颜色为品牌资产常量而非主题 token：
   渐变两端镜像 src-tauri/icons/icon.svg 背景，文字取深色主题 --text-primary 同值。
   仅在本组件内定义一次，避免污染全局 token 命名空间。 */
.splash-root {
  --splash-bg-from: #2e2a22;
  --splash-bg-to: #0a0907;
  --splash-text: #ece8dc;
  --splash-text-dim: rgba(236, 232, 220, 0.55);
  --splash-text-faint: rgba(236, 232, 220, 0.35);
  --splash-border: rgba(236, 232, 220, 0.09);
  --splash-track: rgba(236, 232, 220, 0.12);
  --splash-success: rgba(140, 212, 138, 0.92);

  /* 三层背景：中央辉光 + 底部 vignette + 品牌渐变 */
  background:
    radial-gradient(ellipse 68% 46% at 50% 40%, rgba(236, 232, 220, 0.06), transparent 70%),
    radial-gradient(ellipse 100% 60% at 50% 110%, rgba(0, 0, 0, 0.6), transparent 60%),
    linear-gradient(135deg, var(--splash-bg-from), var(--splash-bg-to));
}

/* 呼吸辉光层：极克制（opacity 0.4 → 0.55，周期 8s），仅烘托 logo 周围 */
.splash-halo {
  position: absolute;
  left: 50%;
  top: 40%;
  transform: translate(-50%, -50%);
  width: 520px;
  height: 520px;
  border-radius: 50%;
  background: radial-gradient(circle at 50% 50%, rgba(236, 232, 220, 0.09), rgba(236, 232, 220, 0.02) 40%, transparent 65%);
  filter: blur(36px);
  opacity: 0.4;
  animation: splash-halo-breath 8s ease-in-out infinite;
  pointer-events: none;
}

@keyframes splash-halo-breath {
  0%, 100% { opacity: 0.4; transform: translate(-50%, -50%) scale(0.97); }
  50%      { opacity: 0.55; transform: translate(-50%, -50%) scale(1.03); }
}

.splash-logo {
  position: relative;
  background:
    linear-gradient(145deg, rgba(236, 232, 220, 0.08), transparent 55%),
    linear-gradient(135deg, var(--splash-bg-from), var(--splash-bg-to));
  border: 1px solid var(--splash-border);
  box-shadow:
    0 12px 40px rgba(0, 0, 0, 0.55),
    inset 0 1px 0 rgba(236, 232, 220, 0.06);
}

/* Logo 外发光 */
.splash-logo::before {
  content: "";
  position: absolute;
  inset: -12px;
  border-radius: 1.3rem;
  background: radial-gradient(circle at 50% 50%, rgba(236, 232, 220, 0.13), transparent 70%);
  filter: blur(16px);
  z-index: -1;
  pointer-events: none;
}

.splash-logo svg {
  filter: drop-shadow(0 0 6px rgba(236, 232, 220, 0.22));
}

/* ==================== 入场编排 ==================== */

@keyframes splash-rise {
  from {
    opacity: 0;
    transform: translateY(14px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.splash-rise {
  animation: splash-rise 0.3s cubic-bezier(0.22, 1, 0.36, 1) both;
}

.splash-rise-1 {
  animation-delay: 0.08s;
}

.splash-rise-2 {
  animation-delay: 0.14s;
}

.splash-rise-3 {
  animation-delay: 0.22s;
}

/* ==================== 终端光标 / 打字机 ==================== */

@keyframes splash-typing {
  from {
    width: 0;
  }
  to {
    width: 7ch;
  }
}

/* "bedcode" 共 7ch，等宽字体下按字符步进打出 */
.splash-typed {
  display: inline-block;
  overflow: hidden;
  white-space: nowrap;
  vertical-align: bottom;
  animation: splash-typing 0.6s steps(7, end) 0.45s both;
  text-shadow: 0 0 10px rgba(236, 232, 220, 0.22);
}

/* 提示符 $ 微微发光 */
.splash-root .text-\[var\(--splash-text-faint\)\] {
  text-shadow: 0 0 6px rgba(236, 232, 220, 0.2);
}

@keyframes splash-caret-blink {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0;
  }
}

.splash-caret {
  display: inline-block;
  width: 0.55em;
  height: 1.05em;
  margin-left: 3px;
  background: var(--splash-text);
  /* 磷光效果：box-shadow 让光标像 CRT 磷光余晖 */
  box-shadow: 0 0 8px rgba(236, 232, 220, 0.7);
  animation: splash-caret-blink 1s steps(1) infinite;
}

.splash-glyph-caret {
  animation: splash-caret-blink 1.2s steps(1) 0.9s infinite;
}

/* ==================== Boot log 阶段栈 ==================== */

.splash-bootlog {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

/* Boot log 行：grid 三列（1fr | auto | 1fr），text 居中列严格居中于屏幕中线；
   mark 在左列右对齐（靠 text 左侧），作为左装饰不推走 text */
.bootlog-row {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: center;
  font-size: calc(11px * var(--ui-scale));
  color: var(--splash-text-dim);
  line-height: 1.4;
  opacity: 0;
  transform: translateY(4px);
  animation: bootlog-appear 0.3s cubic-bezier(0.22, 1, 0.36, 1) both;
}

.bootlog-row .mark {
  grid-column: 1;
  justify-self: end;
  display: inline-flex;
  align-items: center;
  font-weight: 500;
  color: var(--splash-success);
  text-shadow: 0 0 6px rgba(140, 212, 138, 0.4);
  padding-right: 0.55rem;
}

.bootlog-row .text {
  grid-column: 2;
  color: var(--splash-text-dim);
  text-align: center;
}

/* Boot log 分四阶段，形成节奏 */
.bootlog-row[data-delay='1'] { animation-delay: 0.65s; }
.bootlog-row[data-delay='2'] { animation-delay: 0.95s; }
.bootlog-row[data-delay='3'] { animation-delay: 1.25s; }
.bootlog-row[data-delay='4'] { animation-delay: 1.55s; }

@keyframes bootlog-appear {
  to { opacity: 1; transform: translateY(0); }
}

/* ==================== 段式进度指示 ==================== */

.splash-progress-track {
  display: flex;
  gap: 4px;
  align-items: center;
  justify-content: center;
}

.splash-progress-track .seg {
  width: 22px;
  height: 2px;
  background: var(--splash-track);
  border-radius: 1px;
  opacity: 0;
  animation: seg-appear 0.35s ease-out both, seg-fill 0.35s ease-out both;
}

.splash-progress-track .seg.done {
  background: var(--splash-text);
  box-shadow: 0 0 4px rgba(236, 232, 220, 0.35);
}

/* 段与 boot log 同步入场 */
.splash-progress-track .seg[data-delay='1'] { animation-delay: 0.65s, 0.65s; }
.splash-progress-track .seg[data-delay='2'] { animation-delay: 0.95s, 0.95s; }
.splash-progress-track .seg[data-delay='3'] { animation-delay: 1.25s, 1.25s; }
.splash-progress-track .seg[data-delay='4'] { animation-delay: 1.55s, 1.55s; }

@keyframes seg-appear {
  from { opacity: 0; }
  to { opacity: 1; }
}

@keyframes seg-fill {
  from { transform: scaleX(0); transform-origin: left; }
  to { transform: scaleX(1); transform-origin: left; }
}

/* ==================== Footer ==================== */

.splash-footer {
  position: absolute;
  bottom: 1.5rem;
  left: 0;
  right: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 10px;
  color: var(--splash-text-faint);
  letter-spacing: 0.05em;
  opacity: 0;
  animation: bootlog-appear 0.4s ease-out 1.75s both;
}

.splash-footer .text {
  text-align: center;
}

/* ==================== 进度条 ==================== */

.splash-progress {
  transition: width 300ms ease;
}

/* ==================== 淡出 ==================== */

.splash-leave-active {
  transition: opacity 0.3s ease, transform 0.3s ease;
}

.splash-leave-to {
  opacity: 0;
  transform: scale(1.015);
}

/* ==================== 减弱动态效果 ==================== */
/* 收敛入场与循环动画到首帧终态；光标类停闪并保持可见 */
@media (prefers-reduced-motion: reduce) {
  .splash-rise,
  .splash-typed,
  .bootlog-row,
  .splash-progress-track .seg,
  .splash-footer {
    animation-duration: 0.01ms !important;
    animation-delay: 0ms !important;
    animation-iteration-count: 1 !important;
  }

  .splash-caret,
  .splash-glyph-caret,
  .splash-halo {
    animation: none !important;
  }

  .splash-caret,
  .splash-glyph-caret {
    opacity: 1;
  }

  .bootlog-row,
  .splash-progress-track .seg,
  .splash-footer {
    opacity: 1;
    transform: none;
  }
}
</style>
