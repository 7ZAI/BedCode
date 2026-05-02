<template>
  <div class="form-group">
    <label v-if="label" :for="id" class="block text-dark-300 text-sm mb-2">
      {{ label }}
      <span v-if="required" class="text-red-400">*</span>
    </label>

    <select
      :id="id"
      :value="modelValue"
      :disabled="disabled"
      :required="required"
      class="w-full bg-dark-700 border border-dark-600 rounded-lg px-4 py-2 text-white focus:border-primary-500 focus:ring-1 focus:ring-primary-500 outline-none transition-colors appearance-none cursor-pointer"
      :class="{ 'opacity-50 cursor-not-allowed': disabled }"
      @change="$emit('update:modelValue', ($event.target as HTMLSelectElement).value)"
    >
      <option v-if="placeholder" value="" disabled>{{ placeholder }}</option>
      <option v-for="option in options" :key="option.value" :value="option.value">
        {{ option.label }}
      </option>
    </select>

    <!-- Dropdown Icon -->
    <div class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-dark-400">
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
      </svg>
    </div>

    <!-- Error Message -->
    <p v-if="error" class="mt-1 text-sm text-red-400">{{ error }}</p>
  </div>
</template>

<script setup lang="ts">
import { v4 as uuidv4 } from 'uuid'

interface Option {
  value: string | number
  label: string
}

interface Props {
  modelValue: string | number
  label?: string
  options: Option[]
  placeholder?: string
  disabled?: boolean
  required?: boolean
  error?: string
}

withDefaults(defineProps<Props>(), {
  disabled: false,
  required: false,
})

defineEmits(['update:modelValue'])

const id = uuidv4()
</script>

<style scoped>
select {
  background-image: none;
}
</style>
