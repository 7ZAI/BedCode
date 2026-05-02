<template>
  <div class="form-group">
    <label v-if="label" :for="id" class="block text-dark-300 text-sm mb-2">
      {{ label }}
      <span v-if="required" class="text-red-400">*</span>
    </label>

    <div class="relative">
      <!-- Prefix Icon -->
      <div v-if="$slots.prefix" class="absolute left-3 top-1/2 -translate-y-1/2 text-dark-400">
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
        class="w-full bg-dark-700 border rounded-lg px-4 py-2 text-white placeholder-dark-400 focus:border-primary-500 focus:ring-1 focus:ring-primary-500 outline-none transition-colors"
        :class="[
          error ? 'border-red-500' : 'border-dark-600',
          { 'pl-10': $slots.prefix },
          { 'pr-10': $slots.suffix },
          { 'opacity-50 cursor-not-allowed': disabled }
        ]"
        @input="$emit('update:modelValue', ($event.target as HTMLInputElement).value)"
      />

      <!-- Suffix Icon -->
      <div v-if="$slots.suffix" class="absolute right-3 top-1/2 -translate-y-1/2 text-dark-400">
        <slot name="suffix"></slot>
      </div>
    </div>

    <!-- Error Message -->
    <p v-if="error" class="mt-1 text-sm text-red-400">{{ error }}</p>

    <!-- Help Text -->
    <p v-else-if="help" class="mt-1 text-sm text-dark-500">{{ help }}</p>
  </div>
</template>

<script setup lang="ts">
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

withDefaults(defineProps<Props>(), {
  type: 'text',
})

defineEmits(['update:modelValue'])

const id = uuidv4()
</script>
