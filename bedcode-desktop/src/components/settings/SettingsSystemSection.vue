<template>
  <!-- ==================== SYSTEM ==================== -->
  <section>
    <h3 class="wb-section-title">{{ t('settings.system.title') }}</h3>
    <div
      class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]"
    >
      <!-- 防止休眠：方角开关 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <div>
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.system.preventSleep')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.system.preventSleepDesc') }}
          </p>
        </div>
        <button
          class="relative w-10 h-5 rounded-[4px] border transition-colors flex-shrink-0"
          :class="
            preventSleep
              ? 'bg-[var(--color-primary)] border-[var(--color-primary)]'
              : 'bg-[var(--bg-page)] border-[var(--border-strong)]'
          "
          role="switch"
          :aria-checked="preventSleep"
          @click="preventSleep = !preventSleep"
        >
          <span
            class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
            :class="
              preventSleep
                ? 'left-[22px] bg-[var(--color-primary-contrast)]'
                : 'left-[3px] bg-[var(--border-strong)]'
            "
          />
        </button>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
/**
 * 设置页 — 系统分组（SettingsView 拆分产物）
 *
 * 防止休眠开关，直写 store，由父组件 deep watch 防抖持久化。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/settings'

const { t } = useI18n()
const settingsStore = useSettingsStore()

const preventSleep = computed({
  get: () => settingsStore.settings.network.prevent_sleep ?? true,
  set: (value: boolean) => {
    settingsStore.settings.network.prevent_sleep = value
  },
})
</script>
