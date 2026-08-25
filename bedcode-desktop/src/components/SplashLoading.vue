<template>
  <Teleport to="body">
    <Transition name="splash">
      <div
        v-if="visible"
        class="splash-root fixed inset-0 z-[100] flex flex-col items-center justify-center overflow-hidden"
        role="status"
        :aria-label="statusText"
      >
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

        <!-- 终端启动行：$ 提示符 + 打字机命令 + 块状光标 -->
        <div
          class="splash-rise splash-rise-3 wb-mono flex h-5 items-center text-[calc(13px*var(--ui-scale))]"
        >
          <span class="text-[var(--splash-text-faint)]">$&nbsp;</span
          ><span class="splash-typed text-[var(--splash-text)]">bedcode</span
          ><span class="splash-caret" aria-hidden="true"></span>
        </div>

        <!-- 状态文本 -->
        <p
          v-if="statusText"
          class="splash-rise splash-rise-3 mt-4 text-sm text-[var(--splash-text-dim)]"
        >
          {{ statusText }}
        </p>

        <!-- 进度条（可选） -->
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
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * SplashLoading - 桌面端启动画面（终端 Boot 风格）
 *
 * 固定深色品牌渐变背景（镜像 src-tauri/icons/icon.svg，不随主题切换，
 * 避免启动期主题闪烁），内容为品牌 glyph + 打字机启动行 + 可选进度条。
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
}

const props = withDefaults(defineProps<Props>(), {
  status: '',
  showProgress: false,
  progress: 0,
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

  /* 中央微弱辉光营造纵深，底层为品牌渐变 */
  background:
    radial-gradient(ellipse 62% 48% at 50% 42%, rgba(236, 232, 220, 0.05), transparent 70%),
    linear-gradient(135deg, var(--splash-bg-from), var(--splash-bg-to));
}

.splash-logo {
  background: linear-gradient(145deg, rgba(236, 232, 220, 0.06), transparent 55%),
    linear-gradient(135deg, var(--splash-bg-from), var(--splash-bg-to));
  border: 1px solid var(--splash-border);
  box-shadow: 0 10px 36px rgba(0, 0, 0, 0.4);
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
  animation: splash-caret-blink 1s steps(1) infinite;
}

.splash-glyph-caret {
  animation: splash-caret-blink 1.2s steps(1) 0.9s infinite;
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
  .splash-typed {
    animation-duration: 0.01ms !important;
    animation-delay: 0ms !important;
    animation-iteration-count: 1 !important;
  }

  .splash-caret,
  .splash-glyph-caret {
    animation: none !important;
    opacity: 1;
  }
}
</style>
