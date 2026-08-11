<template>
  <Teleport to="body">
    <Transition name="loading-dialog-fade">
      <div
        v-if="visible"
        class="fixed inset-0 z-[100] flex items-center justify-center p-4 mobile-ui"
      >
        <!-- 遮罩：半透明 + 模糊，阻断底层交互 -->
        <div class="absolute inset-0 backdrop-blur-sm" style="background: var(--mobile-overlay)"></div>

        <!-- 弹窗卡片：会话页面保持可见，仅中央弹窗提示 -->
        <div
          class="relative rounded-2xl px-8 py-6 shadow-xl flex flex-col items-center gap-4 min-w-[220px] max-w-[85vw]"
          style="background: var(--mobile-bg-card); border: 1px solid var(--mobile-border)"
        >
          <div
            class="w-10 h-10 border-4 border-current border-t-transparent rounded-full animate-spin"
            style="color: var(--mobile-accent)"
          ></div>
          <p class="text-sm font-medium text-center" style="color: var(--mobile-text-secondary)">{{ message }}</p>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 弹窗式 Loading — 遮罩 + 居中卡片（区别于全页面遮罩）。
 * 用于连接中 / 终端准备中等需要阻断交互、但保留页面上下文的场景。
 */
defineProps<{
  visible: boolean
  message: string
}>()
</script>

<style scoped>
.loading-dialog-fade-enter-active,
.loading-dialog-fade-leave-active {
  transition: opacity 0.25s ease;
}

.loading-dialog-fade-enter-from,
.loading-dialog-fade-leave-to {
  opacity: 0;
}
</style>
