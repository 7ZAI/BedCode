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
      <!-- 开关样式与桌面端 SettingsView 对齐：方形墨色轨道 + 方形滑块 -->
      <div
        class="w-10 h-5 rounded-[4px] border transition-colors"
        :class="[
          modelValue
            ? 'bg-[var(--mobile-accent)] border-[var(--mobile-accent)]'
            : 'bg-[var(--mobile-bg-primary)] border-[var(--mobile-border-hover)]',
          { 'opacity-50 cursor-not-allowed': disabled }
        ]"
      ></div>
      <div
        class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
        :class="[
          modelValue
            ? 'left-[22px] bg-[var(--mobile-text-on-accent)]'
            : 'left-[3px] bg-[var(--mobile-bg-tertiary)]'
        ]"
      ></div>
    </div>
    <span v-if="label" class="text-sm text-[var(--mobile-text-secondary)]">{{ label }}</span>
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
