<template>
  <div class="form-group">
    <label
      v-if="label"
      :for="id"
      class="block text-xs font-medium mb-1.5 text-[var(--text-secondary)]"
    >
      {{ label }}
      <span v-if="required" class="text-red-500">*</span>
    </label>

    <div class="relative">
      <input
        :id="id"
        :type="type"
        :value="modelValue"
        :placeholder="placeholder"
        :disabled="disabled"
        :required="required"
        class="w-full h-[var(--input-height)] border rounded-input px-4 transition-all duration-200 outline-none bg-[var(--bg-input)] text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)] focus:border-brand focus:shadow-input-focus shadow-xs dark:shadow-none"
        :class="[
          error ? 'border-red-500' : 'border-[var(--border-input)]',
          { 'pr-16': $slots.suffix },
          { 'opacity-50 cursor-not-allowed': disabled },
        ]"
        @input="handleInput"
      />

      <div
        v-if="$slots.suffix"
        class="absolute right-3 top-1/2 -translate-y-1/2 text-[var(--text-tertiary)]"
      >
        <slot name="suffix"></slot>
      </div>
    </div>

    <p v-if="error" class="mt-1 text-xs text-red-500">{{ error }}</p>
    <p v-else-if="help" class="mt-1 text-xs text-[var(--text-tertiary)]">{{ help }}</p>
  </div>
</template>

<script setup lang="ts">
/**
 * TextInput — 插件侧文本输入（宿主 `Input.vue` 的逐类复制，票 13）
 *
 * 为什么复制而不是复用：宿主 `@/components/Input.vue` 是宿主内部模块，插件前端
 * 禁止引宿主模块（spec D2 取数/依赖收口，插件工程契约测试 C4 强校验）。此处只
 * 复制**同一套 Tailwind 类与 token**（宿主 tailwind 扫描插件源码 + 全局样式在
 * 宿主文档内生效），保证像素与宿主输入框一致；行为差异为零（无 prefix 插槽——
 * 会话表单不需要）。
 */
interface Props {
  modelValue: string | number
  label?: string
  type?: 'text' | 'password' | 'email' | 'number' | 'url'
  placeholder?: string
  disabled?: boolean
  required?: boolean
  error?: string
  help?: string
}

const props = withDefaults(defineProps<Props>(), {
  type: 'text',
  disabled: false,
  required: false,
})

const emit = defineEmits<{
  'update:modelValue': [value: string | number]
}>()

const id = `session-input-${Math.random().toString(36).slice(2, 9)}`

function handleInput(e: Event) {
  const target = e.target as HTMLInputElement
  if (props.type === 'number') {
    emit('update:modelValue', target.value === '' ? '' : Number(target.value))
  } else {
    emit('update:modelValue', target.value)
  }
}
</script>
