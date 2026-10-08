<template>
  <div class="flex flex-col items-center justify-center flex-1 min-h-0 gap-3 px-8 text-center">
    <ShellAppIcon v-if="app" :app="app" :size="56" />

    <div>
      <p class="text-[var(--font-size-base)] font-medium text-[var(--mobile-text-primary)]">
        {{ t('shell.run.reservedTitle') }}
      </p>
      <p class="mt-1 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
        {{ t('shell.run.reservedHint') }}
      </p>
    </div>

    <!-- 预留位只展示平台掌握的事实，用于接入期对账（确认挂载的是哪个应用） -->
    <p v-if="app" class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] break-all">
      {{ t('shell.run.reservedStatus', { state: stateLabel, count: app.permissions.length, id: app.id }) }}
    </p>

    <!-- 虚线框：明确「这里是将来应用界面的位置」，而不是渲染失败的白屏 -->
    <div
      class="w-full mt-1 rounded-[12px] border border-dashed border-[var(--mobile-border-hover)] py-6 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]"
    >
      {{ app ? app.name : t('shell.run.notFound') }}
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 运行面预留位
 * -----------------------------------------------------------------------------
 * 原型的终端 / AI Chatbox / 文件传输三个界面都是**应用内页面**，平台不实现它们：
 * 这些界面属于各自的 wasm-app，接入后由应用注册 surface 组件渲染到这里。
 *
 * 因此这里只做三件事：
 *   ① 标明这是预留挂载点（不是加载失败）
 *   ② 显示平台掌握的应用事实，便于接入期对账
 *   ③ 给出明确的下一步（应用需注册运行面）
 *
 * 一旦应用注册了 surface，本组件自动让位（见 ShellAppSurface 的优先级）。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ShellApp } from '../types'
import ShellAppIcon from './ShellAppIcon.vue'

const props = defineProps<{ app?: ShellApp }>()

const { t } = useI18n()

const stateLabel = computed(() => {
  switch (props.app?.state) {
    case 'running':
      return t('shell.common.running')
    case 'disabled':
      return t('shell.common.disabled')
    case 'error':
      return t('shell.common.error')
    default:
      return t('shell.common.stopped')
  }
})
</script>
