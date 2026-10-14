<template>
  <div class="settings-page flex flex-col gap-4 px-4 pb-6">
    <p v-if="ctrl.error.value" class="rounded-lg px-3 py-2 text-xs" :style="warnStyle">
      {{ t('settings.loadFailed') }}
    </p>

    <!-- ==================== 业务设置分组 ==================== -->
    <section v-for="group in SETTING_GROUPS" :key="group.id" class="flex flex-col gap-1.5">
      <h2 class="text-xs px-1" :style="{ color: 'var(--mobile-text-secondary)' }">
        {{ t(group.titleKey) }}
      </h2>
      <div
        class="rounded-xl overflow-hidden"
        :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)' }"
      >
        <div
          v-for="(item, index) in group.items"
          :key="item.key"
          class="flex items-center justify-between gap-3 px-3 min-h-[48px] py-2"
          :style="index > 0 ? { borderTop: '1px solid var(--mobile-group-border)' } : {}"
        >
          <span class="flex-1 min-w-0 text-sm" :style="{ color: 'var(--mobile-text-primary)' }">
            {{ t(item.labelKey) }}
          </span>

          <!-- 布尔项：复用应用内开关（终端域 Toggle，契约与宿主设置页一致） -->
          <Toggle
            v-if="item.kind === 'boolean'"
            :model-value="ctrl.values[item.key] === true"
            :disabled="ctrl.saving.value"
            @update:model-value="onToggle(item.key, $event)"
          />

          <!-- 数值项：步进按钮（区间在 settingsModel 归一，UI 只管给合法输入） -->
          <div v-else-if="item.kind === 'number'" class="flex items-center gap-2 flex-shrink-0">
            <button
              type="button"
              class="w-9 h-9 rounded-lg text-lg leading-none transition-colors active:opacity-70"
              :style="stepperStyle"
              :aria-label="`${t(item.labelKey)} -1`"
              :disabled="ctrl.saving.value || Number(ctrl.values[item.key]) <= (item.min ?? 0)"
              @click="onStep(item.key, -1)"
            >
              −
            </button>
            <span
              class="min-w-[3ch] text-center text-sm tabular-nums"
              :style="{ color: 'var(--mobile-text-primary)' }"
            >
              {{ ctrl.values[item.key] }}
            </span>
            <button
              type="button"
              class="w-9 h-9 rounded-lg text-lg leading-none transition-colors active:opacity-70"
              :style="stepperStyle"
              :aria-label="`${t(item.labelKey)} +1`"
              :disabled="ctrl.saving.value || Number(ctrl.values[item.key]) >= (item.max ?? Infinity)"
              @click="onStep(item.key, 1)"
            >
              +
            </button>
          </div>

          <!-- 枚举项：分段选择 -->
          <div v-else class="flex items-center gap-1 flex-shrink-0">
            <button
              v-for="opt in item.options ?? []"
              :key="opt.value"
              type="button"
              class="min-h-[32px] px-3 rounded-lg text-xs transition-colors"
              :style="segmentStyle(item.key === opt.value)"
              :disabled="ctrl.saving.value"
              @click="onSelect(item.key, opt.value)"
            >
              {{ t(opt.labelKey) }}
            </button>
          </div>
        </div>
      </div>
    </section>

    <!-- ==================== 重置 ==================== -->
    <button
      type="button"
      class="min-h-[48px] rounded-xl text-sm font-medium transition-colors active:opacity-80"
      :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)', color: 'var(--mobile-error)' }"
      :disabled="ctrl.saving.value"
      @click="onReset"
    >
      {{ t('settings.actions.reset') }}
    </button>

    <p class="text-xs px-1" :style="{ color: 'var(--mobile-text-secondary)' }">
      {{ t('settings.platformHint') }}
    </p>
  </div>
</template>

<script setup lang="ts">
/**
 * 业务设置页（票 2026-10-10：全量 UI 下沉 —— 设置域）
 *
 * 本页只管**业务设置**（连接策略 / 通知 / 终端上限 / 首选认证）。
 * 平台设置（外观、语言、字号缩放、关于、出站授权、链路加密）由宿主壳持有，
 * 见 spec §2 设置切分口径——本页底部给出去处说明，不重复实现一套。
 *
 * 数据面：useAppSettings（宿主通用 KV 桥）；本组件不直连任何 Tauri 命令。
 */
import { onMounted } from 'vue'
import Toggle from '../terminal/components/Toggle.vue'
import { SETTING_GROUPS, SETTINGS_BY_KEY } from './settingsModel'
import { useAppSettings } from './useAppSettings'

const ctrl = useAppSettings()
const t = ctrl.t

const warnStyle = {
  background: 'var(--mobile-bg-card)',
  color: 'var(--mobile-error)',
}

const stepperStyle = {
  background: 'var(--mobile-bg-primary)',
  border: '1px solid var(--mobile-border)',
  color: 'var(--mobile-text-primary)',
}

function segmentStyle(active: boolean) {
  return {
    background: active ? 'var(--mobile-accent)' : 'var(--mobile-bg-primary)',
    color: active ? 'var(--mobile-text-on-accent)' : 'var(--mobile-text-secondary)',
  }
}

function onToggle(key: string, next: boolean): void {
  void ctrl.set(key, next).catch(() => {
    ctrl.dialogs.showToast(t('settings.saveFailed'), 'error')
  })
}

function onStep(key: string, delta: number): void {
  const current = Number(ctrl.values[key]) || 0
  void ctrl.set(key, current + delta).catch(() => {
    ctrl.dialogs.showToast(t('settings.saveFailed'), 'error')
  })
}

function onSelect(key: string, value: string): void {
  void ctrl.set(key, value).catch(() => {
    ctrl.dialogs.showToast(t('settings.saveFailed'), 'error')
  })
}

async function onReset(): Promise<void> {
  const confirmed = await ctrl.dialogs.showConfirm({
    message: t('settings.actions.resetConfirm'),
    confirmText: t('settings.actions.reset'),
  })
  if (!confirmed) return
  try {
    await ctrl.reset()
    ctrl.dialogs.showToast(t('settings.actions.resetDone'), 'success')
  } catch {
    ctrl.dialogs.showToast(t('settings.saveFailed'), 'error')
  }
}

onMounted(() => {
  void ctrl.load()
})

// 防御：定义表里的键必须都能在运行时查到（改表漏改归一会静默写空）
if (import.meta.env.DEV) {
  for (const group of SETTING_GROUPS) {
    for (const item of group.items) {
      if (!SETTINGS_BY_KEY[item.key]) {
        ctrl.logger.error(`[settings] definition table broken: missing index for ${item.key}`)
      }
    }
  }
}
</script>