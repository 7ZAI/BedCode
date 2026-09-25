<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="modelValue"
        class="fixed inset-0 z-50 flex items-center justify-center p-4"
        @click.self="closeOnBackdrop && close()"
      >
        <!-- Backdrop（safe-stack：overlay 层 z-50 + blur） -->
        <div class="absolute inset-0 bg-black/50 backdrop-blur-sm"></div>

        <!-- Modal Content -->
        <div
          class="relative rounded-card shadow-2xl border bg-card border-[var(--border)]"
          :class="[sizeClass]"
        >
          <!-- Header -->
          <div v-if="title || $slots.header" class="px-6 py-4 border-b border-[var(--border)]">
            <slot name="header">
              <h3 class="text-lg font-semibold text-[var(--text-primary)]">{{ title }}</h3>
            </slot>
          </div>

          <!-- Body -->
          <div class="flex flex-col overflow-hidden" :class="bodyMaxHeightClass">
            <div class="flex-1 overflow-y-auto p-6">
              <slot></slot>
            </div>
          </div>

          <!-- Footer -->
          <div v-if="$slots.footer" class="px-6 py-4 border-t border-[var(--border)]">
            <slot name="footer"></slot>
          </div>

          <!-- Close Button -->
          <button
            v-if="closable"
            class="absolute top-4 right-4 text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors"
            @click="close()"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * PluginModal — 插件侧通用弹窗外壳（宿主 `Modal.vue` 的逐类复制，票 13）
 *
 * 为什么复制而不是复用：宿主 `@/components/Modal.vue` 是宿主内部模块，插件前端
 * 禁止引宿主模块（spec D2）。此处复制同一套 Tailwind 类 / token / 过渡名，弹窗
 * 外观与动画与宿主一致（尺寸档位、遮罩、关闭按钮、body 最大高度全同）。
 *
 * 票 14：会话页与设备/配对页共用同一外壳（原 `SessionModal.vue` 改名为此名，
 * 避免域前缀误导——它与业务域无关）。
 */
import { computed } from 'vue'

interface Props {
  modelValue: boolean
  title?: string
  size?: 'sm' | 'md' | 'lg' | 'xl' | 'full'
  closable?: boolean
  closeOnBackdrop?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  size: 'md',
  closable: true,
  closeOnBackdrop: true,
})

const emit = defineEmits(['update:modelValue', 'close'])

const sizeClass = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'w-full max-w-sm'
    case 'md':
      return 'w-full max-w-md'
    case 'lg':
      return 'w-full max-w-lg'
    case 'xl':
      return 'w-full max-w-xl'
    case 'full':
      return 'w-full max-w-4xl'
    default:
      return 'w-full max-w-md'
  }
})

const bodyMaxHeightClass = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'max-h-[60vh]'
    case 'md':
      return 'max-h-[70vh]'
    case 'lg':
      return 'max-h-[75vh]'
    case 'xl':
    case 'full':
      return 'max-h-[80vh]'
    default:
      return 'max-h-[70vh]'
  }
})

function close() {
  emit('update:modelValue', false)
  emit('close')
}
</script>

<style scoped>
.modal-enter-active,
.modal-leave-active {
  transition: all 0.2s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-from > div:last-child,
.modal-leave-to > div:last-child {
  transform: scale(0.95);
}
</style>
