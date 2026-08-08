<script setup lang="ts">
/**
 * SettingsView — 演示设置页：主题切换（html.dark 类，与宿主一致）+ 说明
 */
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

function toggleTheme() {
  document.documentElement.classList.toggle('dark')
}
</script>

<template>
  <div class="p-6 space-y-4 max-w-xl">
    <h2 class="text-lg font-semibold">{{ t('devshell.nav.settings') }}</h2>

    <div class="bg-card border border-[var(--border)] rounded-card p-4 shadow-card space-y-3">
      <div class="flex items-center justify-between">
        <span class="text-sm">{{ t('devshell.settings.theme') }}</span>
        <button
          class="px-3 py-1.5 rounded-btn text-xs bg-[var(--color-primary)] text-white hover:opacity-90 transition-opacity duration-200"
          @click="toggleTheme()"
        >
          {{ t('devshell.settings.theme.dark') }} / {{ t('devshell.settings.theme.light') }}
        </button>
      </div>
      <p class="text-xs text-[var(--text-tertiary)] leading-relaxed">{{ t('devshell.settings.note') }}</p>
    </div>
  </div>
</template>
