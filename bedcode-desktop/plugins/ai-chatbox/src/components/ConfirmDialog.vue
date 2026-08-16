<template>
  <!-- 删除等破坏性操作的确认弹窗（自绘覆盖层，禁原生 confirm） -->
  <Teleport to="body">
    <Transition name="confirm-fade" appear>
      <div class="fixed inset-0 z-[9999] flex items-center justify-center p-6">
      <!-- 半透明遮罩：mousedown 遮罩关闭（弹窗主体是 sibling 且层级更高，点击不触达遮罩） -->
      <div class="absolute inset-0 bg-black/50 backdrop-blur-sm" @mousedown="onCancel"></div>

        <!-- 弹窗主体 -->
        <div
          role="alertdialog"
          aria-modal="true"
          class="relative w-full max-w-xs bg-[var(--bg-card)] border border-[var(--border)] rounded-card shadow-card p-5"
        >
          <h4 class="text-sm font-medium text-[var(--text-primary)] mb-2">{{ title }}</h4>
          <p class="text-xs text-[var(--text-secondary)] leading-relaxed break-words mb-5">{{ body }}</p>
          <div class="flex justify-end gap-2">
            <button
              class="h-[34px] px-4 text-sm rounded-btn bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-input)] transition-colors"
              @click="onCancel"
            >
              {{ cancelText }}
            </button>
            <!-- --color-danger 双主题均为固定红（不随主题反色），白字对比度恒定达标 -->
            <button
              class="h-[34px] px-4 text-sm rounded-btn bg-[var(--color-danger)] text-white hover:opacity-90 transition-opacity"
              @click="onConfirm"
            >
              {{ confirmText }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * ConfirmDialog — 插件本地确认弹窗
 *
 * 自绘覆盖层（Teleport 到 body，避开宿主 overflow 容器），标题 + 正文 +
 * 确认/取消按钮；点击遮罩或取消关闭。破坏性操作统一经此组件确认。
 * 组件在插件本地实现，等第二个插件需要时再提升进 SDK。
 */
import { useI18n } from 'vue-i18n'

const props = defineProps<{
  title: string
  body: string
}>()

const emit = defineEmits<{
  confirm: []
  cancel: []
}>()

const { t } = useI18n()

const confirmText = t('desktop.plugin.aiChatbox.delete')
const cancelText = t('desktop.plugin.aiChatbox.cancel')

function onConfirm(): void {
  emit('confirm')
}

function onCancel(): void {
  emit('cancel')
}
</script>

<style scoped>
/* 遮罩与弹窗淡入（GPU 合成属性 only） */
.confirm-fade-enter-active,
.confirm-fade-leave-active {
  transition: opacity 0.2s;
}
.confirm-fade-enter-from,
.confirm-fade-leave-to {
  opacity: 0;
}
</style>
