<script setup lang="ts">
/**
 * PromptHost — 渲染 dialogService 的 prompt 队列（fileService.pick 系列使用）
 */
import { ref, watch } from 'vue'
import { dialogService, resolveTop } from '../mock/dialog-service'

const value = ref('')

function confirm() {
  resolveTop(value.value)
  value.value = ''
}

function cancel() {
  resolveTop(null)
  value.value = ''
}

// 队列顶部条目变化时同步输入框初值
watch(
  () => dialogService.queue.value[0]?.value,
  (v) => {
    if (v !== undefined) value.value = v
  },
)
</script>

<template>
  <Teleport to="body">
    <div
      v-if="dialogService.queue.value.length"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4"
      @click.self="cancel()"
    >
      <div class="w-full max-w-sm bg-card border border-[var(--border)] rounded-card shadow-card-hover p-5">
        <h3 class="text-sm font-semibold mb-1">{{ dialogService.queue.value[0]?.title }}</h3>
        <p v-if="dialogService.queue.value[0]?.message" class="text-xs text-[var(--text-secondary)] mb-4 leading-relaxed">
          {{ dialogService.queue.value[0]?.message }}
        </p>
        <input
          v-model="value"
          class="w-full bg-[var(--bg-input)] border border-[var(--border-input)] rounded-input px-3 py-2 text-sm text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)] focus:border-[var(--color-primary)] outline-none transition-colors duration-200"
          :placeholder="dialogService.queue.value[0]?.placeholder"
          @keydown.enter="confirm()"
        />
        <div class="flex justify-end gap-2 mt-4">
          <button
            class="px-4 py-2 rounded-btn text-sm text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors duration-200"
            @click="cancel()"
          >
            取消
          </button>
          <button
            class="px-4 py-2 rounded-btn text-sm font-medium bg-[var(--color-primary)] text-white hover:opacity-90 transition-opacity duration-200"
            @click="confirm()"
          >
            确定
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
