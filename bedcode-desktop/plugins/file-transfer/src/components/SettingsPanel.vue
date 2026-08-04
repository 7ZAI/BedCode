<script setup lang="ts">
/**
 * SettingsPanel — 插件设置（覆盖层面板）
 *
 * 共享目录列表管理（添加 = 系统目录选择器）、下载目录、并发数（1..8）、
 * 安全告知常驻文案（spec §10 transfer.settings.plainWarning）。
 * 纯展示组件，写操作经 emit 交给父级 composable。
 */
import { inject } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-desktop'
import type { Settings } from '../types'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const props = defineProps<{
  settings: Settings
}>()

const emit = defineEmits<{
  (e: 'addRoot'): void
  (e: 'removeRoot', dir: string): void
  (e: 'pickDownloadDir'): void
  (e: 'setConcurrency', n: number): void
  (e: 'close'): void
}>()

function onConcurrencyChange(e: Event): void {
  const value = Number((e.target as HTMLSelectElement).value)
  emit('setConcurrency', value)
}
</script>

<template>
  <div class="ft-settings-backdrop">
    <div class="ft-settings">
      <div class="ft-topbar">
        <h2 class="ft-settings-title">{{ t('transfer.topbar.settings') }}</h2>
        <div class="ft-spacer"></div>
        <button class="ft-btn" @click="emit('close')">{{ t('transfer.topbar.closeSettings') }}</button>
      </div>

      <div class="ft-settings-body">
        <!-- 共享目录 -->
        <section>
          <h3 class="ft-settings-section-title">{{ t('transfer.settings.sharedRoots') }}</h3>
          <div v-if="settings.roots.length === 0" class="ft-dir-value ft-dir-value--empty">
            {{ t('transfer.settings.noRoots') }}
          </div>
          <div v-else class="ft-root-list">
            <div v-for="root in settings.roots" :key="root" class="ft-root-item">
              <span class="ft-root-path" :title="root">{{ root }}</span>
              <button
                class="ft-mini-btn"
                :title="t('transfer.settings.removeRoot')"
                @click="emit('removeRoot', root)"
              >
                ✕
              </button>
            </div>
          </div>
          <div class="ft-settings-row ft-settings-row--push">
            <button class="ft-btn" @click="emit('addRoot')">
              {{ t('transfer.settings.addRoot') }}
            </button>
          </div>
        </section>

        <!-- 下载目录 -->
        <section>
          <h3 class="ft-settings-section-title">{{ t('transfer.settings.downloadDir') }}</h3>
          <div class="ft-settings-row">
            <span
              class="ft-dir-value"
              :class="{ 'ft-dir-value--empty': !settings.downloadDir }"
            >
              {{ settings.downloadDir || t('transfer.settings.noDownloadDir') }}
            </span>
            <button class="ft-btn" @click="emit('pickDownloadDir')">
              {{ t('transfer.settings.chooseDir') }}
            </button>
          </div>
        </section>

        <!-- 并发数 -->
        <section>
          <h3 class="ft-settings-section-title">{{ t('transfer.settings.concurrency') }}</h3>
          <select
            class="ft-select"
            :value="settings.concurrency"
            @change="onConcurrencyChange"
          >
            <option v-for="n in 8" :key="n" :value="n">{{ n }}</option>
          </select>
        </section>

        <!-- 安全告知（spec §10 常驻） -->
        <div class="ft-warning">⚠️ {{ t('transfer.settings.plainWarning') }}</div>
      </div>
    </div>
  </div>
</template>
