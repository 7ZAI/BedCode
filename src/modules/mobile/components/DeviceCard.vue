<template>
  <div
    class="bg-[#12121a] border border-cyan-500/10 rounded-xl p-4 flex items-center gap-3 hover:border-cyan-500/30 transition-all duration-300 cursor-pointer group"
    @click="$emit('click')"
  >
    <!-- Icon -->
    <div
      :class="[
        'w-12 h-12 rounded-xl flex items-center justify-center shrink-0 transition-colors',
        device.isOnline || isDiscovered ? 'bg-cyan-500/10 border border-cyan-500/20' : 'bg-gray-800 border border-gray-700'
      ]"
    >
      <svg class="w-6 h-6" :class="device.isOnline || isDiscovered ? 'text-cyan-400' : 'text-gray-500'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
      </svg>
    </div>

    <!-- Info -->
    <div class="flex-1 min-w-0">
      <p class="font-medium text-white truncate">{{ device.name }}</p>
      <p class="text-gray-500 text-sm truncate">
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
          device.isOnline ? 'bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.5)] animate-pulse' : 'bg-gray-600'
        ]"
      ></div>
    </div>

    <!-- Arrow -->
    <svg class="w-5 h-5 text-gray-600 group-hover:text-cyan-400 transition-colors" fill="none" stroke="currentColor" viewBox="0 0 24 24">
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
