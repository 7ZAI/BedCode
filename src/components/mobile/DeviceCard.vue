<template>
  <div
    class="bg-dark-800 rounded-xl p-4 flex items-center gap-3 active:bg-dark-700 transition-colors"
    @click="$emit('click')"
  >
    <!-- Icon -->
    <div
      :class="[
        'w-12 h-12 rounded-xl flex items-center justify-center',
        device.isOnline || isDiscovered ? 'bg-primary-900' : 'bg-dark-700'
      ]"
    >
      <svg class="w-6 h-6" :class="device.isOnline || isDiscovered ? 'text-primary-400' : 'text-dark-400'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
      </svg>
    </div>

    <!-- Info -->
    <div class="flex-1 min-w-0">
      <p class="font-medium truncate">{{ device.name }}</p>
      <p class="text-dark-400 text-sm truncate">
        <template v-if="isDiscovered">
          {{ device.address }}:{{ device.port }}
        </template>
        <template v-else>
          {{ device.isOnline ? '在线' : '离线' }}
        </template>
      </p>
    </div>

    <!-- Status Indicator -->
    <div v-if="!isDiscovered" class="flex items-center gap-2">
      <div
        :class="[
          'w-2.5 h-2.5 rounded-full',
          device.isOnline ? 'bg-green-500 animate-pulse' : 'bg-dark-500'
        ]"
      ></div>
    </div>

    <!-- Arrow -->
    <svg class="w-5 h-5 text-dark-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
    </svg>
  </div>
</template>

<script setup lang="ts">
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
