<!--
  PROTOTYPE — 一次性原型切换条（原型专用，勿在生产代码引用）

  固定在屏幕底部中央，左右箭头循环切换设计变体。
  仅开发构建渲染（import.meta.env.PROD 时为空壳），避免原型误合并后泄漏给用户。
-->
<template>
  <div
    v-if="isVisible"
    class="proto-switcher fixed bottom-4 left-1/2 -translate-x-1/2 z-[9999] flex items-center gap-1 rounded-full border border-white/15 bg-zinc-900/95 shadow-[0_8px_32px_rgba(0,0,0,0.6)] px-2 py-1.5 backdrop-blur-md"
  >
    <button class="proto-arrow" aria-label="上一个变体" @click="$emit('prev')">
      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M15 19l-7-7 7-7" />
      </svg>
    </button>
    <div class="px-2 min-w-[13rem] text-center">
      <div class="text-[11px] font-semibold text-white tracking-wide">{{ label }}</div>
      <div class="text-[10px] text-zinc-400 leading-tight">原型 · 左右键切换 · ?variant={{ modelValue }}</div>
    </div>
    <button class="proto-arrow" aria-label="下一个变体" @click="$emit('next')">
      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M9 5l7 7-7 7" />
      </svg>
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

defineProps<{ modelValue: string; label: string }>()
defineEmits<{ prev: []; next: [] }>()

/** 生产构建隐藏（import.meta.env 只能出现在 script 中，不能出现在模板表达式里） */
const isVisible = computed(() => !import.meta.env.PROD)
</script>

<style scoped>
.proto-arrow {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2rem;
  height: 2rem;
  border-radius: 9999px;
  color: #d4d4d8;
  transition: background-color 0.15s ease;
}
.proto-arrow:active {
  background: rgba(255, 255, 255, 0.12);
}
</style>
