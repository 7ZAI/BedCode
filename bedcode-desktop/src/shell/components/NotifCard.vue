<template>
  <div class="ntf" data-testid="shell-notif-card">
    <span class="ntf-ic" :class="toneClass">
      <ShellIcon :name="notif.icon" :size="14" />
    </span>
    <div class="ntf-b">
      <div class="ntf-top">
        <b class="truncate">{{ notif.title }}</b>
        <span v-if="timeText" class="ntf-time">{{ timeText }}</span>
      </div>
      <div class="ntf-src">{{ notif.source }}</div>
      <div v-if="notif.body" class="ntf-body">{{ notif.body }}</div>
      <div v-if="notif.kind === 'progress'" class="ntf-progress">
        <div class="bar"><i :style="{ width: notif.progress + '%' }"></i></div>
        <span>{{ notif.progress }}%</span>
      </div>
      <div v-if="notif.actions?.length" class="ntf-acts">
        <button
          v-for="a in notif.actions"
          :key="a.id"
          class="shell-mini"
          @click="$emit('action', a.id)"
        >
          {{ a.label }}
        </button>
      </div>
    </div>
    <button
      v-if="!notif.sticky"
      class="ntf-x"
      :aria-label="t('shell.notif.dismiss', { title: notif.title })"
      @click="$emit('dismiss')"
    >
      <ShellIcon name="x" :size="11" />
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * NotifCard — 单条通知卡（通知中心内部组件，参照手机系统通知栏结构：
 * 来源图标 + 标题/时间 + 来源 + 内容 + 动作行 + 单条关闭；驻留项无 ×）
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import ShellIcon from './ShellIcon.vue'
import type { HostNotif } from '../types'

const props = defineProps<{
  notif: HostNotif
}>()

defineEmits<{
  dismiss: []
  action: [id: string]
}>()

const { t } = useI18n()

const toneClass = computed(() => {
  const k = props.notif.kind
  return k === 'progress' ? 'tone-success' : k === 'info' ? 'tone-system' : 'tone-info'
})

const timeText = computed(() => {
  const mins = props.notif.mins
  if (mins === null || mins === undefined) return ''
  if (mins === 0) return t('shell.notif.justNow')
  return t('shell.notif.minutesAgo', { mins })
})
</script>

<style scoped>
.ntf {
  display: flex;
  align-items: flex-start;
  gap: 9px;
  padding: 10px;
  border-radius: var(--radius-card);
  background: var(--bg-sidebar);
  border: 1px solid var(--border);
}
.ntf.attn {
  border-color: color-mix(in srgb, var(--color-warning) 45%, transparent);
  background: var(--color-warning-light);
}
.ntf-ic {
  width: 28px;
  height: 28px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}
.ntf-b {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.ntf-top {
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.ntf-top b {
  flex: 1;
  min-width: 0;
  font-size: calc(12px * var(--ui-scale));
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ntf-time {
  font-size: calc(10px * var(--ui-scale));
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.ntf-src {
  font-size: calc(10px * var(--ui-scale));
  color: var(--text-tertiary);
}
.ntf-body {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-secondary);
  line-height: 1.5;
}
.ntf-progress {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 2px;
}
.ntf-progress .bar {
  flex: 1;
  height: 3px;
  border-radius: 2px;
  background: var(--border);
  overflow: hidden;
}
.ntf-progress .bar i {
  display: block;
  height: 100%;
  border-radius: 2px;
  background: var(--color-success);
}
.ntf-progress span {
  font-size: calc(10px * var(--ui-scale));
  color: var(--text-secondary);
  flex-shrink: 0;
}
.ntf-acts {
  display: flex;
  gap: 6px;
  margin-top: 4px;
}
.ntf-x {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  color: var(--text-tertiary);
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  transition:
    background-color 0.15s,
    color 0.15s;
}
.ntf-x:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}
</style>
