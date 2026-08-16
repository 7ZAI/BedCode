<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="request"
        class="fixed inset-0 z-50 flex items-center justify-center p-4"
        @click.self="deny"
      >
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/50 backdrop-blur-sm"></div>

        <!-- Dialog -->
        <div class="relative w-full max-w-sm rounded-card shadow-2xl border bg-card border-[var(--border)]">
          <!-- Header -->
          <div class="px-6 py-4 border-b border-[var(--border)]">
            <h3 class="text-lg font-semibold text-[var(--text-primary)]">
              {{ t('desktop.plugin.fsAuthTitle') }}
            </h3>
          </div>

          <!-- Body -->
          <div class="p-6 space-y-4">
            <!-- Description -->
            <p class="text-sm text-[var(--text-secondary)]">
              {{ t('desktop.plugin.fsAuthRequest', {
                plugin: request.pluginId,
                operation: operationLabel
              }) }}
            </p>

            <!-- Path display -->
            <div class="space-y-1">
              <span class="text-xs text-[var(--text-tertiary)]">
                {{ pathCount > 1 ? t('desktop.plugin.fsAuthPaths', { count: pathCount }) : t('desktop.plugin.fsAuthPath') }}
              </span>
              <div
                v-if="pathCount > 1"
                class="max-h-40 overflow-y-auto p-2 rounded-input bg-[var(--bg-input)] text-xs text-[var(--text-primary)] space-y-1"
              >
                <div
                  v-for="p in paths"
                  :key="p"
                  class="break-all font-mono leading-relaxed"
                >
                  {{ p }}
                </div>
              </div>
              <div
                v-else
                class="p-2 rounded-input bg-[var(--bg-input)] text-xs text-[var(--text-primary)] break-all font-mono"
              >
                {{ paths[0] || request.path }}
              </div>
            </div>

            <!-- Remember checkbox -->
            <label class="flex items-center gap-2 cursor-pointer select-none">
              <input
                v-model="remember"
                type="checkbox"
                class="w-4 h-4 rounded accent-[var(--color-primary)]"
              />
              <span class="text-sm text-[var(--text-secondary)]">
                {{ t('desktop.plugin.fsAuthRemember') }}
              </span>
            </label>
          </div>

          <!-- Footer -->
          <div class="flex items-center justify-end gap-3 px-6 py-4 border-t border-[var(--border)]">
            <button
              @click="deny"
              class="px-4 h-9 rounded-btn text-sm font-medium bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-input)] transition-colors duration-200"
            >
              {{ t('desktop.plugin.fsAuthDeny') }}
            </button>
            <button
              @click="allow"
              class="px-4 h-9 rounded-btn text-sm font-medium bg-brand text-white hover:bg-[var(--color-primary-hover)] transition-colors duration-200"
            >
              {{ t('desktop.plugin.fsAuthAllow') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * FsAuthDialog — 文件系统授权弹窗
 *
 * 监听 plugin:fs-auth-request 事件，显示授权请求弹窗，
 * 用户选择后通过 plugin_fs_auth_respond Tauri command 回调宿主
 */
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'

const { t } = useI18n()

interface FsAuthRequest {
  requestId: string
  pluginId: string
  path?: string
  /** 批量授权：未授权路径数组（桌面 1.0 后新增，优先于 path） */
  paths?: string[]
  operation: string
}

const request = ref<FsAuthRequest | null>(null)
const remember = ref(true)

/** 待展示路径列表（批量事件用 paths，兼容旧事件回退 path） */
const paths = computed(() => {
  if (!request.value) return []
  return request.value.paths?.length ? request.value.paths : [request.value.path || '']
})

const pathCount = computed(() => paths.value.length)

const operationLabel = computed(() => {
  if (!request.value) return ''
  return request.value.operation === 'write'
    ? t('desktop.plugin.fsAuthWrite')
    : t('desktop.plugin.fsAuthRead')
})

let unlisten: UnlistenFn | null = null

onMounted(async () => {
  unlisten = await listen<FsAuthRequest>('plugin:fs-auth-request', (event) => {
    request.value = event.payload
    remember.value = true
  })
})

onUnmounted(() => {
  unlisten?.()
})

async function allow() {
  if (!request.value) return
  const { requestId } = request.value
  request.value = null
  await invoke('plugin_fs_auth_respond', {
    requestId,
    allowed: true,
    remember: remember.value,
  })
}

async function deny() {
  if (!request.value) return
  const { requestId } = request.value
  request.value = null
  await invoke('plugin_fs_auth_respond', {
    requestId,
    allowed: false,
    remember: false,
  })
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
