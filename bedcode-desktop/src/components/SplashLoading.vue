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
              ><span class="splash-typed text-[var(--splash-text)]">bedcode</span>
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
 * 背景随主题切换：浅色为纸面墨色（跟随当前色板 token），深色沿用固定品牌暖白渐变
 * （镜像 src-tauri/icons/icon.svg）。主题 class 由 App.vue onMounted 的 setupTheme()
 * 落定，早于开屏最短 900ms 展示窗口；Vue 挂载前的静态首屏只能跟随系统深浅色，
 * 见 index.html 内的 prefers-color-scheme 分支。
 * 内容为品牌 glyph + 打字机启动行 + boot log 阶段栈 + 可选进度条。
 * 通过 visible 控制显隐；宿主在初始化完成后置 false 触发淡出。
 */
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { getVersion } from '@tauri-apps/api/app'

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
  /** 底部 footer 文案；缺省时组件用运行时应用版本号动态生成（bedcode v<version> · LAN remote terminal） */
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
  // 空串触发组件内部动态生成（运行时应用版本号），见下方 footerText computed
  footerText: '',
})

const { t } = useI18n()

const statusText = computed(() => props.status || t('desktop.splash.status'))

// 版本号动态化：从 Tauri 运行时取应用版本（单一真源 tauri.conf.json / package.json），
// 禁止在组件里 hard code 版本号。非 Tauri 环境（vitest、浏览器预览）拿不到版本，
// 降级为不带版本号的 footer，避免启动期报错。
const appVersion = ref('')
onMounted(async () => {
  try {
    appVersion.value = await getVersion()
  } catch {
    // 非 Tauri 运行环境：保留空版本号，footer 自动降级
  }
})

const footerText = computed(() =>
  props.footerText ||
  (appVersion.value ? `bedcode v${appVersion.value} · LAN remote terminal` : 'bedcode · LAN remote terminal'),
)
</script>

<style scoped>
/* ==================== 开屏色板 ==================== */
/* 开屏专属变量，仅在本组件内定义一次，避免污染全局 token 命名空间。
   两套主题的色值全部收敛在这两个变量块，下方规则一律引用变量，不再出现字面色值。

   浅色（默认，即 :root 非 dark）：纸面底 + 墨色字，端点直接取当前色板 token，
   故 warm/cool/forest/… 色板切换时开屏一起跟随。环境高光用卡片白而非暗色光晕，
   霓虹类发光（text-shadow / drop-shadow）整体置 transparent——纸面上低透明度墨色
   既脏又糊，层次改由边框与投影承担（同移动端 SplashScreen 的浅色处理思路）。
   文字透明度按小字号 AA 反推：墨色 78% 在 #f5f4f0 上约 7:1、65% 约 4.8:1，
   比深色的 55% / 35% 各提一档（深色底 #15130f 上同样透明度对比度天然更高）。

   深色（html.dark）：固定品牌渐变常量，镜像 src-tauri/icons/icon.svg 两端色值，
   不随色板切换——保持该主题既有观感不变（暖白文字 #ece8dc = 深色 --text-primary）。
   注：祖先选择器必须裸写 `html.dark`，不能用 `:global(html.dark) .splash-root`——
   scoped 编译器会丢掉 :global() 之后的后代选择器（移动端 SplashScreen 踩过，整页罩层）。 */
.splash-root {
  --splash-bg-from: var(--bg-page);
  --splash-bg-to: var(--bg-sidebar);
  --splash-tile-from: var(--bg-card);
  --splash-tile-to: var(--bg-page);
  --splash-text: var(--text-primary);
  --splash-text-dim: color-mix(in srgb, var(--text-primary) 78%, transparent);
  --splash-text-faint: color-mix(in srgb, var(--text-primary) 65%, transparent);
  --splash-border: color-mix(in srgb, var(--text-primary) 14%, transparent);
  --splash-track: color-mix(in srgb, var(--text-primary) 16%, transparent);
  --splash-success: color-mix(in srgb, var(--color-success) 65%, black);
  --splash-bloom: var(--color-primary-contrast);
  --splash-ambient: var(--text-primary);
  --splash-vignette: color-mix(in srgb, var(--text-primary) 8%, transparent);
  --splash-shadow: 0 12px 40px color-mix(in srgb, var(--text-primary) 14%, transparent);
  --splash-glow: transparent;

  /* 三层背景：中央高光 + 底部 vignette + 主渐变 */
  background:
    radial-gradient(ellipse 68% 46% at 50% 40%, color-mix(in srgb, var(--splash-bloom) 6%, transparent), transparent 70%),
    radial-gradient(ellipse 100% 60% at 50% 110%, var(--splash-vignette), transparent 60%),
    linear-gradient(135deg, var(--splash-bg-from), var(--splash-bg-to));
}

html.dark .splash-root {
  --splash-bg-from: #2e2a22;
  --splash-bg-to: #0a0907;
  --splash-tile-from: #2e2a22;
  --splash-tile-to: #0a0907;
  --splash-text: #ece8dc;
  --splash-text-dim: rgba(236, 232, 220, 0.55);
  --splash-text-faint: rgba(236, 232, 220, 0.35);
  --splash-border: rgba(236, 232, 220, 0.09);
  --splash-track: rgba(236, 232, 220, 0.12);
  --splash-success: rgba(140, 212, 138, 0.92);
  --splash-bloom: #ece8dc;
  --splash-ambient: #ece8dc;
  --splash-vignette: rgba(0, 0, 0, 0.6);
  --splash-shadow: 0 12px 40px rgba(0, 0, 0, 0.55);
  /* 深色下的发光色：暖白霓虹，呼应终端荧光（深色文字阴影的唯一来源） */
  --splash-glow: rgba(236, 232, 220, 0.22);
}

/* 呼吸辉光层：极克制（opacity 0.4 → 0.55，周期 8s），仅烘托 logo 周围；
   浅色下 ambient 为墨色，读作 logo 周围的柔和环境阴影而非光晕 */
.splash-halo {
  position: absolute;
  left: 50%;
  top: 40%;
  transform: translate(-50%, -50%);
  width: 520px;
  height: 520px;
  border-radius: 50%;
  background: radial-gradient(circle at 50% 50%, color-mix(in srgb, var(--splash-ambient) 9%, transparent), color-mix(in srgb, var(--splash-ambient) 2%, transparent) 40%, transparent 65%);
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
    linear-gradient(145deg, color-mix(in srgb, var(--splash-bloom) 8%, transparent), transparent 55%),
    linear-gradient(135deg, var(--splash-tile-from), var(--splash-tile-to));
  border: 1px solid var(--splash-border);
  box-shadow:
    var(--splash-shadow),
    inset 0 1px 0 color-mix(in srgb, var(--splash-bloom) 6%, transparent);
}

/* Logo 外围氛围层：深色读作外发光，浅色下 ambient 为墨色 → 柔影 */
.splash-logo::before {
  content: "";
  position: absolute;
  inset: -12px;
  border-radius: 1.3rem;
  background: radial-gradient(circle at 50% 50%, color-mix(in srgb, var(--splash-ambient) 13%, transparent), transparent 70%);
  filter: blur(16px);
  z-index: -1;
  pointer-events: none;
}

.splash-logo svg {
  filter: drop-shadow(0 0 6px var(--splash-glow));
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
  text-shadow: 0 0 10px var(--splash-glow);
}

/* 提示符 $ 微微发光（浅色下 glow 为 transparent，即不做霓虹处理） */
.splash-root .text-\[var\(--splash-text-faint\)\] {
  text-shadow: 0 0 6px var(--splash-glow);
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
  text-shadow: 0 0 6px color-mix(in srgb, var(--splash-success) 44%, transparent);
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
  box-shadow: 0 0 4px color-mix(in srgb, var(--splash-text) 35%, transparent);
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

  .splash-glyph-caret,
  .splash-halo {
    animation: none !important;
  }

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
