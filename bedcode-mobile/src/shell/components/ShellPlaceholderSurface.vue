<template>
  <div class="flex flex-col items-center justify-center flex-1 min-h-0 gap-3 px-8 text-center">
    <ShellAppIcon v-if="app" :app="app" :size="56" />

    <div>
      <p class="shell-type-label text-[var(--mobile-text-primary)]">
        {{ t('shell.run.noSurface') }}
      </p>
      <p class="mt-1 shell-type-caption text-[var(--mobile-text-secondary)]">
        {{ t('shell.run.noSurfaceHint') }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 运行面缺位提示
 * -----------------------------------------------------------------------------
 * 应用尚未向壳注册运行界面时渲染它。只说两件用户能感知的事：现在没有界面、
 * 原因是该应用没提供——不再讲「这是将来的挂载点」「平台只提供挂载与生命周期」
 * 这类方案期说明（2026-10-14 修订：宿主文案不写平台实现）。
 *
 * 一旦应用注册了 surface，本组件自动让位（见 ShellAppSurface 的优先级）。
 */
import { useI18n } from 'vue-i18n'
import type { ShellApp } from '../types'
import ShellAppIcon from './ShellAppIcon.vue'

defineProps<{ app?: ShellApp }>()

const { t } = useI18n()
</script>
