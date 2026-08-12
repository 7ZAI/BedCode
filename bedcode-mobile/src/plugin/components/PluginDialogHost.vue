<!--
  插件对话框渲染宿主 — 消费 pluginDialogHost.queue
  复用宿主移动端弹窗样式（--mobile-* CSS 变量），支持 confirm / prompt / 通用对话框
-->
<script setup lang="ts">
/**
 * Plugin Dialog Host Component
 *
 * 从 pluginDialogHost.queue 读取待展示对话框，按顺序渲染。
 * 单例组件，由 App.vue 挂载一次。
 */
import { ref } from 'vue'
import { pluginDialogHost } from '../dialog-host'

const inputValue = ref('')

function confirm(item: { id: number; kind: string; options: any }): void {
  const value = item.kind === 'prompt' ? inputValue.value : undefined
  pluginDialogHost.resolveTop('confirm', value)
  inputValue.value = ''
}

function cancel(): void {
  pluginDialogHost.resolveTop('cancel')
  inputValue.value = ''
}
</script>

<template>
  <Teleport to="body">
    <Transition name="center-modal">
      <div
        v-if="pluginDialogHost.queue.value.length > 0"
        class="fixed inset-0 z-[100] flex items-center justify-center mobile-ui"
      >
        <!-- Backdrop -->
        <div
          class="absolute inset-0 bg-[var(--mobile-overlay)] backdrop-blur-sm"
          @click="pluginDialogHost.queue.value[0].options.dismissible && cancel()"
        ></div>

        <!-- Panel -->
        <div
          class="relative w-full max-w-sm mx-4 mb-[var(--safe-area-bottom,0px)] bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl overflow-hidden shadow-xl modal-panel"
        >
          <template v-for="item in pluginDialogHost.queue.value" :key="item.id">
            <template v-if="item === pluginDialogHost.queue.value[0]">
              <!-- Header -->
              <div class="px-6 pt-6 pb-2">
                <div class="flex items-center gap-3 mb-2">
                  <div
                    :class="[
                      'w-10 h-10 rounded-xl flex items-center justify-center flex-shrink-0',
                      item.options.variant === 'danger'
                        ? 'bg-[var(--mobile-danger-bg)]'
                        : item.options.variant === 'warning'
                          ? 'bg-[var(--mobile-warning-muted)]'
                          : 'bg-[var(--mobile-accent-muted)]',
                    ]"
                  >
                    <svg
                      :class="[
                        'w-5 h-5',
                        item.options.variant === 'danger'
                          ? 'text-[var(--mobile-danger-color)]'
                          : item.options.variant === 'warning'
                            ? 'text-[var(--mobile-warning)]'
                            : 'text-[var(--mobile-accent)]',
                      ]"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        v-if="item.options.variant === 'danger'"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z"
                      />
                      <path
                        v-else-if="item.options.variant === 'warning'"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                      />
                      <path
                        v-else
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                      />
                    </svg>
                  </div>
                  <h3 class="text-base font-semibold text-[var(--mobile-text-primary)]">
                    {{ item.options.title || 'BedCode' }}
                  </h3>
                </div>
              </div>

              <!-- Body -->
              <div class="px-6 pb-2">
                <p
                  v-if="item.options.message"
                  class="text-sm leading-relaxed text-[var(--mobile-text-secondary)]"
                >
                  {{ item.options.message }}
                </p>
                <input
                  v-if="item.kind === 'prompt'"
                  v-model="inputValue"
                  :placeholder="item.options.inputPlaceholder"
                  class="mt-3 w-full rounded-xl border border-[var(--mobile-border)] bg-[var(--mobile-bg-input)] px-4 py-2.5 text-sm text-[var(--mobile-text-primary)] outline-none focus:border-[var(--mobile-accent)]"
                />
              </div>

              <!-- Footer -->
              <div class="flex gap-3 px-6 py-4">
                <button
                  class="flex-1 rounded-xl border border-[var(--mobile-border)] py-2.5 text-sm font-medium text-[var(--mobile-text-secondary)] active:opacity-70"
                  @click="cancel"
                >
                  {{ item.options.cancelText || $t('mobile.plugin.dialog.cancel') }}
                </button>
                <button
                  :class="[
                    'flex-1 rounded-xl py-2.5 text-sm font-medium text-[var(--mobile-text-on-accent)] active:opacity-70',
                    item.options.variant === 'danger'
                      ? 'bg-[var(--mobile-danger)]'
                      : item.options.variant === 'warning'
                        ? 'bg-[var(--mobile-warning)]'
                        : 'bg-[var(--mobile-accent)]',
                  ]"
                  @click="confirm(item)"
                >
                  {{ item.options.confirmText || $t('mobile.plugin.dialog.confirm') }}
                </button>
              </div>
            </template>
          </template>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
