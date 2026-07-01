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
        class="w-10 h-6 rounded-full transition-colors"
        :class="[
          modelValue ? 'bg-primary-600' : 'bg-slate-300 dark:bg-dark-600',
          { 'opacity-50 cursor-not-allowed': disabled }
        ]"
      ></div>
      <div
        class="absolute top-1 w-4 h-4 bg-white rounded-full transition-transform shadow"
        :class="[modelValue ? 'translate-x-5 left-1' : 'translate-x-0 left-1']"
      ></div>
    </div>
    <span v-if="label" class="text-sm text-slate-700 dark:text-dark-300">{{ label }}</span>
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
