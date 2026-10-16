<template>
  <Teleport to="body">
    <Transition name="boot-fade">
      <div
        v-if="visible"
        class="fixed inset-0 z-[100] bg-page flex flex-col items-center justify-center gap-4"
        @click="state.boot.value = false"
      >
        <!-- 品牌标志：与应用图标（src-tauri/icons/icon.svg）逐字同源——暖炭斜向渐变底
             + 白色 `>` 提示符 + 光标横线（旧标志是四笔画 W + 琥珀菱形，已退役）。
             底与图形都走品牌层固定色，不跟主题反转为浅底白字。 -->
        <svg
          class="boot-logo [--logo-bg-start:#2E2A22] [--logo-bg-end:#0A0907] [--logo-fg:#FFFFFF] dark:[--logo-bg-start:#FAF9F7] dark:[--logo-bg-end:#E7E4DC] dark:[--logo-fg:#1C1917]"
          viewBox="0 0 100 100"
          aria-hidden="true"
        >
          <defs>
            <linearGradient id="boot-logo-bg" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stop-color="var(--logo-bg-start)" />
              <stop offset="100%" stop-color="var(--logo-bg-end)" />
            </linearGradient>
          </defs>
          <rect width="100" height="100" rx="18" fill="url(#boot-logo-bg)" />
          <path d="M 24 18 L 59 50 L 24 82 L 32 74 L 51 50 L 32 26 Z" fill="var(--logo-fg)" />
          <path d="M 51 60 L 84 62 L 53 65 Z" fill="var(--logo-fg)" />
        </svg>
        <div class="line">{{ t('shell.boot.line') }}</div>
        <div class="bar"><i :style="{ width: progress + '%' }"></i></div>
        <div class="log">
          <div v-for="step in steps" :key="step.key">
            <span class="dot" :class="{ ok: step.done, running: step.active }">{{
              step.done ? '✓' : step.active ? '·' : ''
            }}</span>
            {{ t(step.key, step.params) }}
          </div>
        </div>
        <div class="note">{{ t('shell.boot.skip') }}</div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * ShellBootScreen — 启动屏（真实初始化步骤驱动，非演示文案）
 *
 * 步骤 = 壳的实际加载链：应用注册表（plugin_list_loaded）→ 授权概览 → 就绪。
 * 插件清单加载完成后自动推进；点击任意处可跳过。
 */
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useShellState } from '../useShellState'
import { useShellApps } from '../useShellApps'

const { t } = useI18n()
const state = useShellState()
const appsStore = useShellApps()

const visible = computed(() => state.boot.value)
const pluginsLoaded = computed(() => !appsStore.loading.value && appsStore.plugins.value.length > 0)
const bootStartedAt = ref(0)

/** 步骤状态机：注册表 → 授权 → 就绪（0/1/2） */
const stepIndex = ref(0)
let timer: ReturnType<typeof setInterval> | null = null

const steps = computed(() => [
  {
    key: 'shell.boot.stepRegistry',
    params: { count: appsStore.plugins.value.length },
    done: stepIndex.value >= 1,
    active: stepIndex.value === 0,
  },
  {
    key: 'shell.boot.stepAuth',
    params: {},
    done: stepIndex.value >= 2,
    active: stepIndex.value === 1,
  },
  {
    key: 'shell.boot.stepReady',
    params: {},
    done: stepIndex.value >= 3,
    active: stepIndex.value === 2,
  },
])

const progress = computed(() => {
  if (!visible.value) return 100
  const elapsed = Date.now() - bootStartedAt.value
  return Math.min(100, Math.round((elapsed / 2200) * 100))
})

/** 就绪后落一条驻留通知（运行时就绪） */
function emitReadyNotif() {
  if (!visible.value) return
  state.pushNotif({
    kind: 'info',
    sticky: true,
    icon: 'check',
    source: t('shell.notif.hostSource'),
    title: t('shell.notif.ready.title'),
    body: t('shell.notif.ready.body', { count: appsStore.plugins.value.length }),
  })
}

watch(pluginsLoaded, (done) => {
  if (done) {
    stepIndex.value = Math.max(stepIndex.value, 1)
    // 授权概览加载（幂等；失败不阻断启动）
    void import('@/plugin/commands').then(({ pluginAuthOverview }) =>
      pluginAuthOverview()
        .then(() => {
          stepIndex.value = Math.max(stepIndex.value, 2)
          stepIndex.value = 3
          emitReadyNotif()
        })
        .catch(() => {
          stepIndex.value = 3
        }),
    )
  }
})

onMounted(() => {
  bootStartedAt.value = Date.now()
  // 壳启动即拉应用清单（与启动屏步骤联动）
  void appsStore.load()
  timer = setInterval(() => {
    // 进度条驱动（re-render 用）
    if (visible.value && progress.value >= 100) {
      state.boot.value = false
    }
  }, 120)
  // 保底：3s 后无论加载链是否就绪都收起启动屏（仅启动屏展示，
  // 授权裁决本身仍 fail-closed，见 AGENTS §8）
  setTimeout(() => {
    if (visible.value) state.boot.value = false
  }, 3000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<style scoped>
/* 启动屏品牌标志：尺寸与图标的满幅圆角方一致（图标底板 rx=18 / 100 网格 = 18%），
   底与图形由 SVG 自绘（品牌固定色），不再用主题色的 .glyph 底板包一层 */
.boot-logo {
  width: 52px;
  height: 52px;
  flex-shrink: 0;
}
.line {
  font-family: var(--font-mono, ui-monospace, 'SF Mono', Menlo, Consolas, monospace);
  font-size: calc(12px * var(--ui-scale));
  color: var(--text-secondary);
}
.bar {
  width: 280px;
  height: 3px;
  border-radius: 2px;
  background: var(--border);
  overflow: hidden;
}
.bar i {
  display: block;
  height: 100%;
  background: var(--color-primary);
  border-radius: 2px;
  transition: width 0.3s;
}
.log {
  display: flex;
  flex-direction: column;
  gap: 3px;
  font-family: var(--font-mono, ui-monospace, 'SF Mono', Menlo, Consolas, monospace);
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-tertiary);
  min-width: 280px;
}
.log div {
  display: flex;
  gap: 7px;
}
.log .dot {
  width: 12px;
  text-align: center;
}
.log .dot.ok {
  color: var(--color-success);
}
.log .dot.running {
  color: var(--color-warning);
}
.note {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-tertiary);
}
.boot-fade-enter-active,
.boot-fade-leave-active {
  transition: opacity 0.3s ease;
}
.boot-fade-enter-from,
.boot-fade-leave-to {
  opacity: 0;
}
</style>
