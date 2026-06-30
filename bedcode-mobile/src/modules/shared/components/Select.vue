<template>
  <div class="form-group">
    <label v-if="label" :for="id" class="block text-sm mb-2 text-slate-700 dark:text-dark-300">
      {{ label }}
      <span v-if="required" class="text-red-500">*</span>
    </label>

    <div class="relative">
      <select
        :id="id"
        :value="modelValue"
        :disabled="disabled"
        :required="required"
        class="w-full border rounded-lg px-4 py-2 text-sm transition-colors outline-none appearance-none cursor-pointer bg-white dark:bg-dark-700 text-slate-900 dark:text-white focus:border-primary-500 focus:ring-1 focus:ring-primary-500 shadow-xs dark:shadow-none"
        :class="[
          error ? 'border-red-500' : 'border-slate-300 dark:border-dark-600',
          { 'opacity-50 cursor-not-allowed': disabled }
        ]"
        @change="$emit('update:modelValue', ($event.target as HTMLSelectElement).value)"
      >
        <option v-if="placeholder" value="" disabled>{{ placeholder }}</option>
        <option v-for="option in options" :key="option.value" :value="option.value">
          {{ option.label }}
        </option>
      </select>

      <!-- Dropdown Icon -->
      <div class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-slate-400 dark:text-dark-400">
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
        </svg>
      </div>
    </div>

    <!-- Error Message -->
    <p v-if="error" class="mt-1 text-sm text-red-500">{{ error }}</p>
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
