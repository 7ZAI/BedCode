<template>
  <Teleport to="body">
    <!-- 遮罩与面板各自过渡：遮罩淡入、面板上滑，避免两者绑定同一时长显得拖沓 -->
    <Transition name="shell-scrim">
      <div
        v-if="open"
        class="fixed inset-0 z-50"
        style="background: var(--mobile-overlay)"
        @click="emit('close')"
      />
    </Transition>
    <Transition name="shell-sheet">
      <div
        v-if="open"
        class="shell-ui fixed inset-x-0 bottom-0 z-50 flex flex-col rounded-t-[16px] border-t border-[var(--mobile-border)] bg-[var(--mobile-bg-card)] px-4 pt-2 pb-4"
        role="dialog"
        aria-modal="true"
        :aria-label="title"
        :style="{ paddingBottom: `calc(16px + ${bottomInset}px)` }"
        @keydown.esc="emit('close')"
      >
        <!-- 抓手：视觉上暗示可下滑关闭 -->
        <div class="mx-auto mb-3 h-1 w-9 rounded-full" style="background: var(--mobile-border-active)" aria-hidden="true" />

        <div v-if="title || subtitle" class="pb-3">
          <h3 class="text-[var(--font-size-lg)] font-semibold text-[var(--mobile-text-primary)]">
            {{ title }}
          </h3>
          <p v-if="subtitle" class="mt-0.5 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
            {{ subtitle }}
          </p>
        </div>

        <div class="min-h-0 overflow-y-auto">
          <slot />
        </div>

        <div v-if="$slots.footer" class="pt-3">
          <slot name="footer" />
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 底部抽屉（胶囊菜单 / 授权弹窗共用）
 *
 * 统一 Teleport 到 body + z-50（safe-stack 约定）：壳内屏幕是 overflow-hidden 的
 * 层叠容器，抽屉留在原地会被裁掉。底部内边距叠加安全区（Android WebView 不支持
 * env()，取宿主注入的 JS 值）。
 */
import { computed, inject, type Ref } from 'vue'

defineProps<{
  open: boolean
  title?: string
  subtitle?: string
}>()

const emit = defineEmits<{ close: [] }>()

// 安全区由 App.vue 注入（与 MobileNav 同一来源），缺省 0 不影响桌面预览。
// 用 computed 而非 setup 期快照：安全区在 App 挂载后才就绪，快照会恒为 0
const safeArea = inject<Ref<{ top: number; bottom: number; navigationBar?: number }> | undefined>(
  'safeArea',
  undefined,
)
const bottomInset = computed(() => {
  const value = safeArea?.value
  return value?.navigationBar ?? value?.bottom ?? 0
})
</script>
