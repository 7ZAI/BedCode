<template>
  <button
    type="button"
    class="w-full text-left rounded-[12px] border border-[var(--mobile-border)] bg-[var(--mobile-bg-card)] p-3.5 transition-colors duration-200 active:bg-[var(--mobile-group-row-active)]"
    :style="{ boxShadow: 'var(--mobile-card-shadow)' }"
    @click="$emit('open', app.id)"
  >
    <span class="flex items-center gap-1.5 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
      <span class="w-1.5 h-1.5 rounded-full flex-shrink-0" :style="{ background: dotColor }" aria-hidden="true" />
      {{ t('shell.home.providedBy', { name: app.name }) }}
    </span>

    <span class="mt-1.5 flex items-center gap-2">
      <ShellAppIcon :app="app" :size="28" />
      <span class="text-[var(--font-size-base)] font-semibold text-[var(--mobile-text-primary)] truncate">
        {{ app.name }}
      </span>
      <ShellChip :label="stateLabel" :tone="stateTone" class="ml-auto flex-shrink-0" />
    </span>

    <span class="mt-1.5 flex items-center justify-between text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
      <span class="truncate">{{ t('shell.apps.permissionCount', { count: app.permissions.length }) }}</span>
      <span>{{ t('shell.home.openApp') }}</span>
    </span>
  </button>
</template>

<script setup lang="ts">
/**
 * 平台默认快捷卡片
 *
 * 应用没有贡献自己的首页卡片时，平台用这张卡兜底——空白不是选项：
 * 应用装了却在首页看不见，用户会以为没装上。卡片只展示平台掌握的事实
 * （名称 / 运行态 / 权限项数），不编造应用内部的业务数据。
 *
 * 原型里终端的「N 个活跃会话」、文件传输的进度条、AI Chatbox 的引用句都是
 * **应用自带卡片**，属于各 wasm-app 自己的 slot，平台不实现；应用接入并
 * registerSlot 后，这张兜底卡自动让位（见 ShellHomeScreen 的 slotCards）。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ShellApp } from '../types'
import ShellAppIcon from './ShellAppIcon.vue'
import ShellChip from './ShellChip.vue'

const props = defineProps<{ app: ShellApp }>()
defineEmits<{ open: [appId: string] }>()

const { t } = useI18n()

const dotColor = computed(() =>
  props.app.state === 'running' ? 'var(--mobile-success)' : 'var(--mobile-text-disabled)',
)

const stateLabel = computed(() => {
  switch (props.app.state) {
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

const stateTone = computed<'success' | 'warn' | 'danger' | 'neutral'>(() => {
  switch (props.app.state) {
    case 'running':
      return 'success'
    case 'disabled':
      return 'warn'
    case 'error':
      return 'danger'
    default:
      return 'neutral'
  }
})
</script>
