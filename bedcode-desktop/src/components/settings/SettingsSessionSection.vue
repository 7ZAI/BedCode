<template>
  <!-- ==================== SESSION ==================== -->
  <section>
    <h3 class="wb-section-title">{{ t('settings.session.title') }}</h3>
    <div
      class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]"
    >
      <!-- 默认执行环境：分段控件 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
          t('settings.session.defaultEnvironment')
        }}</span>
        <div
          class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0"
        >
          <button
            v-for="opt in availableEnvironmentOptions"
            :key="opt.value"
            class="h-8 px-4 text-xs font-medium wb-mono transition-colors"
            :class="
              defaultEnvironment === opt.value
                ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
            "
            @click="defaultEnvironment = opt.value"
          >
            {{ opt.label }}
          </button>
        </div>
      </div>

      <!-- 默认启动命令 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
          t('settings.session.defaultCommand')
        }}</span>
        <input
          type="text"
          :value="settingsStore.settings.session.default_command || ''"
          class="h-8 w-56 px-2.5 rounded-[6px] wb-mono bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
          @input="
            settingsStore.settings.session.default_command = ($event.target as HTMLInputElement).value
          "
        />
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
/**
 * 设置页 — 会话分组（SettingsView 拆分产物）
 *
 * 默认执行环境（仅展示当前宿主平台可用的环境）+ 默认启动命令，均直写 store，
 * 由父组件 deep watch 防抖持久化。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/settings'
import { useAvailableEnvironments } from '@/composables/useAvailableEnvironments'
import i18n from '@/locales'

const { t } = useI18n()
const settingsStore = useSettingsStore()

const environmentOptions = computed(() => [
  { value: 'windows', label: i18n.global.t('desktop.form.windowsNative') },
  { value: 'wsl2', label: 'WSL2' },
  { value: 'linux', label: i18n.global.t('desktop.form.linuxNative') },
])

// 仅展示当前宿主平台可用的执行环境；未识别平台（macOS 等）下落到 windows 与老数据兼容
const { availableValues } = useAvailableEnvironments()
const availableEnvironmentOptions = computed(() =>
  environmentOptions.value.filter((opt) => availableValues.value.includes(opt.value as any)),
)

const defaultEnvironment = computed({
  get: () => {
    const stored = settingsStore.settings.session.default_environment || 'windows'
    // 老用户存储的值（如 'wsl2'）在 Linux 平台上无效时，返回平台默认环境
    return availableValues.value.includes(stored as any) ? stored : (availableValues.value[0] ?? 'windows')
  },
  set: (value: string) => {
    settingsStore.settings.session.default_environment = value
  },
})
</script>
