<template>
  <div
    class="bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 flex items-center gap-3 shadow-[var(--mobile-card-shadow)] hover:border-[var(--mobile-border-hover)] hover:shadow-[var(--mobile-card-shadow-hover)] transition-[border-color,box-shadow] duration-300 cursor-pointer group active:opacity-90"
    @click="$emit('click')"
  >
    <!-- Icon -->
    <div
      :class="[
        'w-12 h-12 rounded-xl flex items-center justify-center shrink-0 transition-colors',
        device.isOnline || isDiscovered ? 'bg-[var(--mobile-accent-muted)] border border-[color:color-mix(in_srgb,var(--mobile-accent)_20%,transparent)]' : 'bg-[var(--mobile-bg-elevated)] border border-[var(--mobile-border)]'
      ]"
    >
      <svg class="w-6 h-6" :class="device.isOnline || isDiscovered ? 'text-[var(--mobile-accent)]' : 'text-[var(--mobile-text-muted)]'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
      </svg>
    </div>

    <!-- Info -->
    <div class="flex-1 min-w-0">
      <p class="font-medium text-base text-[var(--mobile-text-primary)] truncate">{{ device.name }}</p>
      <p class="text-[var(--mobile-text-muted)] text-sm truncate">
        <template v-if="isDiscovered">
          {{ device.address }}:{{ device.port }}
        </template>
        <template v-else>
          {{ device.isOnline ? t('mobile.deviceCard.online') : t('mobile.deviceCard.offline') }}
        </template>
      </p>
    </div>

    <!-- Status Indicator -->
    <div v-if="!isDiscovered" class="flex items-center gap-2">
      <div
        :class="[
          'w-2.5 h-2.5 rounded-full',
          device.isOnline ? 'bg-[var(--mobile-success)] shadow-[0_0_8px_rgba(16,185,129,0.5)] animate-pulse' : 'bg-[var(--mobile-text-muted)]'
        ]"
      ></div>
    </div>

    <!-- Arrow -->
    <svg class="w-5 h-5 text-[var(--mobile-text-disabled)] group-hover:text-[var(--mobile-accent)] transition-colors" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
    </svg>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

defineProps<{
  device: {
    id?: string
    name: string
    address?: string
    port?: number
    isOnline?: boolean
  }
  isDiscovered?: boolean
}>()

defineEmits<{
  click: []
}>()
</script>
