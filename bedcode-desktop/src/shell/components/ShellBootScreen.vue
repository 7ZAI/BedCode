<template>
  <Teleport to="body">
    <Transition name="boot-fade">
      <div
        v-if="visible"
        class="fixed inset-0 z-[100] bg-page flex flex-col items-center justify-center gap-4"
        @click="state.boot.value = false"
      >
        <span class="glyph">
          <svg width="30" height="30" viewBox="0 0 100 100" aria-hidden="true">
            <g stroke="currentColor" stroke-width="13" stroke-linecap="round" fill="none">
              <line x1="23.2" y1="33" x2="36.8" y2="67" />
              <line x1="39.3" y1="67.1" x2="48.7" y2="46.9" />
              <line x1="51.3" y1="46.9" x2="60.7" y2="67.1" />
              <line x1="63.2" y1="67" x2="76.8" y2="33" />
            </g>
            <path d="M50 36.5 L56.5 43 L50 49.5 L43.5 43 Z" fill="#FF9E2C" />
          </svg>
        </span>
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
.glyph {
  width: 52px;
  height: 52px;
  border-radius: var(--radius-app-icon);
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--color-primary);
  color: var(--color-primary-contrast);
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
