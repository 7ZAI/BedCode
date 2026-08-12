<template>
  <div>
    <label class="text-[var(--mobile-text-muted)] text-sm mb-1.5 block">{{ t('mobile.toolbox.repeatable') }}</label>
    <div class="flex gap-2">
      <button
        type="button"
        class="flex-1 min-h-11 flex items-center justify-center py-2.5 rounded-xl text-xs font-medium transition-colors"
        :class="modelValue
          ? 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] shadow-[0_1px_4px_color-mix(in_srgb,var(--mobile-accent)_40%,transparent)]'
          : 'border border-[var(--mobile-border-hover)] bg-[var(--mobile-bg-primary)] text-[var(--mobile-text-secondary)]'"
        @click="emit('update:modelValue', true)"
      >
        {{ t('mobile.toolbox.repeatableOn') }}
      </button>
      <button
        type="button"
        class="flex-1 min-h-11 flex items-center justify-center py-2.5 rounded-xl text-xs font-medium transition-colors"
        :class="!modelValue
          ? 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] shadow-[0_1px_4px_color-mix(in_srgb,var(--mobile-accent)_40%,transparent)]'
          : 'border border-[var(--mobile-border-hover)] bg-[var(--mobile-bg-primary)] text-[var(--mobile-text-secondary)]'"
        @click="emit('update:modelValue', false)"
      >
        {{ t('mobile.toolbox.repeatableOff') }}
      </button>
    </div>
    <p class="text-xs mt-1" style="color: var(--mobile-text-disabled)">
      {{ modelValue ? t('mobile.toolbox.repeatableOnHint') : t('mobile.toolbox.repeatableOffHint') }}
    </p>
  </div>
</template>

<script setup lang="ts">
/**
 * RepeatableToggle - 预设任务「可重复/不可重复」属性选择
 *
 * 自绘 segmented 双按钮（禁原生控件外观），供创建/编辑预设任务入口复用
 * （TaskEditDialog 与 PresetTasksView 顶部编辑区）。
 */
import { useI18n } from 'vue-i18n'

defineProps<{
  /** 当前值：true=可重复，false=不可重复 */
  modelValue: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
}>()

const { t } = useI18n()
</script>
