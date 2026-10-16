<template>
  <div class="shell-seg" role="group">
    <button
      v-for="opt in options"
      :key="opt.value"
      class="seg-btn"
      :class="{ on: opt.value === modelValue, risky: risky && opt.value === riskyValue }"
      :aria-pressed="opt.value === modelValue"
      @click="$emit('update:modelValue', opt.value)"
    >
      {{ opt.label }}
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * ShellSeg — 分段控件（主题三档 / 授权策略三档等）
 * 形状见 shell.css .shell-seg；risky 变体标红（如「始终允许」档）。
 */
defineProps<{
  options: Array<{ value: string; label: string }>
  modelValue: string
  /** 风险档标记：为 true 时 value === riskyValue 的按钮显示危险色 */
  risky?: boolean
  riskyValue?: string
}>()

defineEmits<{
  'update:modelValue': [value: string]
}>()
</script>

<style scoped>
.seg-btn.on.risky {
  background: var(--color-danger-light);
  color: var(--color-danger);
  box-shadow: none;
}
</style>
