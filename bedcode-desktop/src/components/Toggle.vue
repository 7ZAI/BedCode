<template>
  <label class="flex items-center gap-3 cursor-pointer">
    <div class="relative">
      <input
        type="checkbox"
        :checked="modelValue"
        :disabled="disabled"
        class="sr-only"
        @change="$emit('update:modelValue', !modelValue)"
      />
      <div
        class="relative w-10 h-5 rounded-[4px] border transition-colors"
        :class="[
          modelValue
            ? 'bg-[var(--color-primary)] border-[var(--color-primary)]'
            : 'bg-[var(--bg-page)] border-[var(--border-strong)]',
          { 'opacity-50 cursor-not-allowed': disabled }
        ]"
      ></div>
      <div
        class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
        :class="[
          modelValue
            ? 'left-[22px] bg-[var(--color-primary-contrast)]'
            : 'left-[3px] bg-[var(--border-strong)]'
        ]"
      ></div>
    </div>
    <span v-if="label" class="text-sm text-[var(--text-secondary)]">{{ label }}</span>
  </label>
</template>

<script setup lang="ts">
interface Props {
  modelValue: boolean
  label?: string
  disabled?: boolean
}

withDefaults(defineProps<Props>(), {
  disabled: false,
})

defineEmits(['update:modelValue'])
</script>
