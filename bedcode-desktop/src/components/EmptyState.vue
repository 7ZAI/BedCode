<template>
  <div class="flex flex-col items-center justify-center py-12 px-4">
    <!-- Icon -->
    <div class="mb-4" :class="iconSizeClass">
      <slot name="icon">
        <!-- 默认图标：空文件夹 -->
        <svg
          class="text-[var(--text-tertiary)]"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
          />
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M8 12h.01M12 12h.01M16 12h.01"
          />
        </svg>
      </slot>
    </div>

    <!-- Title -->
    <h3 class="text-lg font-medium text-[var(--text-primary)] mb-2">{{ title }}</h3>

    <!-- Description -->
    <p v-if="description" class="text-sm text-center max-w-sm mb-6 text-[var(--text-secondary)]">
      {{ description }}
    </p>

    <!-- Action Button -->
    <slot name="action">
      <Button
        v-if="actionLabel"
        :variant="actionVariant"
        size="md"
        @click="$emit('action')"
      >
        {{ actionLabel }}
      </Button>
    </slot>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import Button from './Button.vue'

interface Props {
  title: string
  description?: string
  icon?: 'folder' | 'search' | 'error' | 'data' | 'custom'
  iconSize?: 'sm' | 'md' | 'lg' | 'xl'
  actionLabel?: string
  actionVariant?: 'primary' | 'secondary' | 'danger' | 'ghost'
}

const props = withDefaults(defineProps<Props>(), {
  icon: 'folder',
  iconSize: 'lg',
  actionVariant: 'primary',
})

defineEmits(['action'])

const iconSizeClass = computed(() => {
  switch (props.iconSize) {
    case 'sm':
      return 'w-8 h-8'
    case 'md':
      return 'w-12 h-12'
    case 'lg':
      return 'w-16 h-16'
    case 'xl':
      return 'w-20 h-20'
    default:
      return 'w-16 h-16'
  }
})
</script>