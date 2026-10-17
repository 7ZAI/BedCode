<template>
  <div
    v-if="missing && !dismissed"
    class="px-4 py-2.5 flex items-center justify-between gap-3 bg-[var(--color-warning-light)] border-b border-[var(--border)]"
    role="alert"
  >
    <div class="flex items-center gap-2.5 min-w-0">
      <!-- 警告图标（内联 SVG，项目标准：stroke 线性图标，w-4 h-4） -->
      <svg
        class="w-4 h-4 flex-shrink-0 text-[var(--color-warning)]"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        viewBox="0 0 24 24"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"
        />
      </svg>
      <div class="min-w-0">
        <span class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)]">
          {{ t('desktop.diagnostics.builtinPluginsMissing') }}
        </span>
        <p class="text-xs text-[var(--text-secondary)] mt-0.5 truncate">
          {{ t('desktop.diagnostics.builtinPluginsMissingDesc') }}
        </p>
      </div>
    </div>
    <button
      class="flex-shrink-0 w-6 h-6 rounded-[4px] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-page)] hover:text-[var(--text-primary)] transition-colors"
      :aria-label="t('common.button.close')"
      @click="dismissBanner"
    >
      <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
      </svg>
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * 内置插件缺失警告横幅 — 宿主诊断（缺口 B 前端面）
 *
 * 安装资源不完整（内置插件目录缺失/被隔离）时，应用会零插件空壳启动；本横幅
 * 在布局顶部向用户明示原因并引导重装。判据与探测见 useBuiltinPluginsHealth。
 * 可关闭（会话内不再重弹）；token 化 warning 风格，暗色模式自动跟随。
 */
import { useI18n } from 'vue-i18n'
import { useBuiltinPluginsHealth } from '@/composables/useBuiltinPluginsHealth'

const { t } = useI18n()
const { missing, dismissed, dismissBanner } = useBuiltinPluginsHealth()
</script>
