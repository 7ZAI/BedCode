<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="visible"
        class="fixed inset-0 z-50 flex items-center justify-center p-4"
        @click.self="cancel"
      >
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/50 backdrop-blur-sm"></div>

        <!-- Dialog -->
        <div class="relative w-full max-w-sm rounded-card shadow-2xl border bg-card border-[var(--border)]">
          <!-- Header -->
          <div class="px-5 py-4 border-b border-[var(--border)]">
            <h3 class="text-base font-semibold text-[var(--text-primary)]">
              {{ $t('desktop.session.confirmExitTitle') }}
            </h3>
          </div>

          <!-- Body -->
          <div class="px-5 py-4">
            <p class="text-sm text-[var(--text-secondary)] mb-3">
              {{ $t('desktop.session.confirmExitMsg', { count: sessions.length }) }}
            </p>
            <ul class="space-y-1.5 max-h-32 overflow-y-auto">
              <li
                v-for="session in sessions"
                :key="session.id"
                class="flex items-center gap-2 text-sm text-[var(--text-primary)]"
              >
                <span class="w-2 h-2 rounded-full bg-green-500 shrink-0"></span>
                <span class="truncate">{{ session.name }}</span>
              </li>
            </ul>
          </div>

          <!-- Footer -->
          <div class="px-5 py-3 border-t border-[var(--border)] flex justify-end gap-2">
            <Button variant="ghost" size="sm" @click="cancel">
              {{ $t('common.button.cancel') }}
            </Button>
            <Button variant="danger" size="sm" @click="forceExit">
              {{ $t('desktop.session.confirmExitForce') }}
            </Button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 退出确认弹窗 — 有运行中会话时展示，防止误关闭
 */
import { invoke } from '@tauri-apps/api/core'
import Button from '@/components/Button.vue'

interface RunningSession {
  id: string
  name: string
  status: string
}

const props = defineProps<{
  visible: boolean
  sessions: RunningSession[]
}>()

const emit = defineEmits<{
  (e: 'update:visible', value: boolean): void
}>()

function cancel() {
  emit('update:visible', false)
}

async function forceExit() {
  try {
    await invoke('confirm_window_close')
  } catch (e) {
    console.error('Failed to confirm window close:', e)
  }
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

.modal-enter-from > :last-child,
.modal-leave-to > :last-child {
  transform: scale(0.95);
}
</style>
