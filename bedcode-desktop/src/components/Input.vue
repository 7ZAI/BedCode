<template>
  <div class="form-group" :class="$attrs.class">
    <label v-if="label" :for="id" class="block text-xs font-medium mb-1.5 text-[var(--text-secondary)]">
      {{ label }}
      <span v-if="required" class="text-red-500">*</span>
    </label>

    <div class="relative">
      <!-- Prefix Icon -->
      <div v-if="$slots.prefix" class="absolute left-3 top-1/2 -translate-y-1/2 text-[var(--text-tertiary)]">
        <slot name="prefix"></slot>
      </div>

      <!-- Input -->
      <input
        :id="id"
        :type="type"
        :value="modelValue"
        :placeholder="placeholder"
        :disabled="disabled"
        :readonly="readonly"
        :required="required"
        class="w-full h-[var(--input-height)] border rounded-input px-4 transition-all duration-200 outline-none bg-[var(--bg-input)] text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)] focus:border-brand focus:shadow-input-focus shadow-xs dark:shadow-none"
        :class="[
          error ? 'border-red-500' : 'border-[var(--border-input)]',
          { 'pl-10': $slots.prefix },
          { 'pr-10': $slots.suffix },
          { 'opacity-50 cursor-not-allowed': disabled }
        ]"
        @input="handleInput"
        @blur="emit('blur', $event)"
      />

      <!-- Suffix Icon -->
      <div v-if="$slots.suffix" class="absolute right-3 top-1/2 -translate-y-1/2 text-[var(--text-tertiary)]">
        <slot name="suffix"></slot>
      </div>
    </div>

    <!-- Error Message -->
    <p v-if="error" class="mt-1 text-xs text-red-500">{{ error }}</p>

    <!-- Help Text -->
    <p v-else-if="help" class="mt-1 text-xs text-[var(--text-tertiary)]">{{ help }}</p>
  </div>
</template>

<script setup lang="ts">
/**
 * Input - 自定义输入框组件
 *
 * 替代原生 <input>，统一样式和主题适配
 */
import { v4 as uuidv4 } from 'uuid'

interface Props {
  modelValue: string | number
  label?: string
  type?: 'text' | 'password' | 'email' | 'number' | 'url'
  placeholder?: string
  disabled?: boolean
  readonly?: boolean
  required?: boolean
  error?: string
  help?: string
}

const props = withDefaults(defineProps<Props>(), {
  type: 'text',
  disabled: false,
  readonly: false,
  required: false,
})

const emit = defineEmits<{
  'update:modelValue': [value: string | number]
  'blur': [event: FocusEvent]
}>()

const id = uuidv4()

function handleInput(e: Event) {
  const target = e.target as HTMLInputElement
  if (props.type === 'number') {
    const val = target.value === '' ? '' : Number(target.value)
    emit('update:modelValue', val)
  } else {
    emit('update:modelValue', target.value)
  }
}
</script>
