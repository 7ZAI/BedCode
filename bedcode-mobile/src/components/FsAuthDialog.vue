<template>
  <Teleport to="body">
    <Transition name="center-modal">
      <div
        v-if="request"
        class="fixed inset-0 z-50 flex items-center justify-center mobile-ui"
        @click.self="deny"
      >
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay)] backdrop-blur-sm"></div>

        <!-- Panel -->
        <div
          class="relative w-full max-w-sm mx-4 mb-[var(--safe-area-bottom,0px)] bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl overflow-hidden shadow-xl modal-panel"
        >
          <!-- Header -->
          <div class="px-6 pt-6 pb-2">
            <div class="flex items-center gap-3 mb-2">
              <div
                class="w-10 h-10 rounded-xl flex items-center justify-center flex-shrink-0 bg-[var(--mobile-accent-muted)]"
              >
                <svg
                  class="w-5 h-5 text-[var(--mobile-accent)]"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z"
                  />
                </svg>
              </div>
              <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)]">
                {{ t('mobile.plugin.fsAuthTitle') }}
              </h3>
            </div>
            <p class="text-[var(--mobile-text-secondary)] text-sm leading-relaxed">
              {{ t('mobile.plugin.fsAuthRequest', {
                plugin: request.pluginId,
                operation: operationLabel
              }) }}
            </p>
          </div>

          <!-- Body -->
          <div class="px-6 py-2 space-y-3">
            <span class="text-xs text-[var(--mobile-text-muted)]">
              {{ pathCount > 1
                ? t('mobile.plugin.fsAuthPaths', { count: pathCount })
                : t('mobile.plugin.fsAuthPath') }}
            </span>
            <div
              v-if="pathCount > 1"
              class="max-h-40 overflow-y-auto p-2 rounded-xl bg-[var(--mobile-input-bg)] text-xs text-[var(--mobile-text-primary)] space-y-1"
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
              class="p-2 rounded-xl bg-[var(--mobile-input-bg)] text-xs text-[var(--mobile-text-primary)] break-all font-mono"
            >
              {{ paths[0] || request.path }}
            </div>

            <!-- Remember toggle（自绘开关，不占原生 checkbox 外观） -->
            <div class="pt-1">
              <Toggle v-model="remember" :label="t('mobile.plugin.fsAuthRemember')" />
            </div>
          </div>

          <!-- Actions -->
          <div class="flex gap-3 px-6 py-5">
            <button
              class="flex-1 bg-[var(--mobile-input-bg)] text-[var(--mobile-text-secondary)] rounded-xl font-medium active:opacity-80 transition-colors duration-200 confirm-btn-height"
              @click="deny"
            >
              {{ t('mobile.plugin.fsAuthDeny') }}
            </button>
            <button
              class="flex-1 bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] rounded-xl font-medium active:opacity-80 transition-colors duration-200 confirm-btn-height"
              @click="allow"
            >
              {{ t('mobile.plugin.fsAuthAllow') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * FsAuthDialog — 移动端文件系统授权弹窗
 *
 * 监听 plugin:fs-auth-request 事件，展示插件目录授权请求（含批量路径），
 * 用户选择后经 plugin_fs_auth_respond Tauri command 回调宿主。
 * 由 App.vue 全局挂载一次（与 PluginDialogHost 并列）。
 */
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import Toggle from '@/components/Toggle.vue'

const { t } = useI18n()

interface FsAuthRequest {
  requestId: string
  pluginId: string
  path?: string
  /** 批量授权：未授权路径数组（优先于 path 兼容字段） */
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
    ? t('mobile.plugin.fsAuthWrite')
    : t('mobile.plugin.fsAuthRead')
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
/* 按钮高度与 ConfirmDialog 保持一致（44px 触摸目标下限） */
.confirm-btn-height {
  height: clamp(2.5rem, 2.75rem, 3rem);
}
</style>
