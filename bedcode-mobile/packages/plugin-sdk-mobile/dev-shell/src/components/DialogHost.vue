<script setup lang="ts">
/**
 * DialogHost — 渲染 dialogService 队列（dialog / confirm / prompt）+ toasts
 *
 * 与宿主 PluginDialogHost 同形状：模块级队列 + resolveTop 完成 Promise。
 * 移动端样式（--mobile-* token）+ Teleport to body（safe-stack z-50）。
 */
import { ref, watch } from 'vue'
import { dialogService, resolveTop } from '../mock/dialog-service'

const promptValue = ref('')
const showPromptError = ref(false)

// 队列顶部条目变化时预填 inputValue（与宿主 showPrompt 默认值语义一致）
watch(
  () => dialogService.queue.value[0]?.options.inputValue,
  (v) => {
    if (v !== undefined) promptValue.value = v
    showPromptError.value = false
  },
)

function confirmTop(action: 'confirm' | 'cancel') {
  const top = dialogService.queue.value[0]
  if (!top) return
  if (top.kind === 'prompt' && action === 'confirm' && !promptValue.value) {
    showPromptError.value = true
    return
  }
  showPromptError.value = false
  resolveTop(action, top.kind === 'prompt' ? promptValue.value : undefined)
  promptValue.value = ''
}
</script>

<template>
  <Teleport to="body">
    <!-- 对话框队列（mobile-ui：Teleport 到 body 后仍需继承 --mobile-* token 的浅色作用域） -->
    <div
      v-if="dialogService.queue.value.length"
      class="mobile-ui fixed inset-0 z-50 flex items-end sm:items-center justify-center bg-[var(--mobile-overlay)] backdrop-blur-sm p-4"
      @click.self="dialogService.queue.value[0]?.options.dismissible && confirmTop('cancel')"
    >
      <div
        class="w-full max-w-sm rounded-2xl bg-[var(--mobile-bg-elevated)] border border-[var(--mobile-border)] shadow-2xl p-5"
      >
        <h3 class="text-base font-semibold text-[var(--mobile-text-primary)] mb-1">
          {{ dialogService.queue.value[0]?.options.title || '' }}
        </h3>
        <p v-if="dialogService.queue.value[0]?.options.message" class="text-sm text-[var(--mobile-text-secondary)] mb-4 leading-relaxed">
          {{ dialogService.queue.value[0]?.options.message }}
        </p>

        <input
          v-if="dialogService.queue.value[0]?.kind === 'prompt'"
          v-model="promptValue"
          class="w-full bg-[var(--mobile-input-bg)] border rounded-lg px-3 py-2.5 text-sm text-[var(--mobile-text-primary)] placeholder:text-[var(--mobile-input-placeholder)] outline-none mb-1 transition-colors duration-200"
          :class="showPromptError ? 'border-[var(--mobile-error)]' : 'border-[var(--mobile-input-border)] focus:border-[var(--mobile-input-focus)]'"
          :placeholder="dialogService.queue.value[0]?.options.inputPlaceholder"
          @keydown.enter="confirmTop('confirm')"
        />
        <p v-if="showPromptError" class="text-[11px] text-[var(--mobile-error)] mb-2">请输入内容</p>

        <div class="flex justify-end gap-2 mt-4">
          <button
            v-if="dialogService.queue.value[0]?.options.cancelText !== undefined || dialogService.queue.value[0]?.kind !== 'dialog'"
            class="px-4 py-2 rounded-lg text-sm text-[var(--mobile-text-secondary)] hover:text-[var(--mobile-text-primary)] transition-colors duration-200"
            @click="confirmTop('cancel')"
          >
            {{ dialogService.queue.value[0]?.options.cancelText || '取消' }}
          </button>
          <button
            class="px-4 py-2 rounded-lg text-sm font-medium bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] transition-colors duration-200"
            @click="confirmTop('confirm')"
          >
            {{ dialogService.queue.value[0]?.options.confirmText || '确定' }}
          </button>
        </div>
      </div>
    </div>

    <!-- Toasts（mobile-ui：Teleport 到 body 后仍需继承 --mobile-* token 的浅色作用域） -->
    <div class="mobile-ui fixed top-14 left-0 right-0 z-50 flex flex-col items-center gap-2 pointer-events-none px-4">
      <TransitionGroup name="toast">
        <div
          v-for="toast in dialogService.toasts.value"
          :key="toast.id"
          class="px-4 py-2 rounded-xl text-sm shadow-lg backdrop-blur-xl pointer-events-auto max-w-sm"
          :class="{
            info: 'bg-[var(--mobile-bg-elevated)]/95 text-[var(--mobile-text-primary)] border border-[var(--mobile-border)]',
            success: 'bg-[var(--mobile-success-muted)]/95 text-[var(--mobile-success)] border border-[var(--mobile-success)]/30',
            warning: 'bg-[var(--mobile-warning-muted)]/95 text-[var(--mobile-warning)] border border-[var(--mobile-warning)]/30',
            error: 'bg-[var(--mobile-error-muted)]/95 text-[var(--mobile-error)] border border-[var(--mobile-error)]/30',
          }[toast.type]"
        >
          {{ toast.message }}
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}
</style>
