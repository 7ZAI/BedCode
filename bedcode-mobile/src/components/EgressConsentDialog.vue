<template>
  <Teleport to="body">
    <Transition name="center-modal">
      <div
        v-if="current"
        class="fixed inset-0 z-[120] flex items-center justify-center mobile-ui"
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
                    stroke-width="1.75"
                    d="M12 21a9 9 0 100-18 9 9 0 000 18zm0 0a8.949 8.949 0 01-3-5.618m3 5.618a8.949 8.949 0 013-5.618M12 3c2.5 2.9 4 6.3 4 9s-1.5 6.1-4 9m-4-9c0-2.7 1.5-6.1 4-9"
                  />
                </svg>
              </div>
              <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)]">
                {{ t('mobile.egress.title') }}
              </h3>
            </div>
            <p class="text-[var(--mobile-text-secondary)] text-sm leading-relaxed">
              {{ requestDescription }}
            </p>
          </div>

          <!-- Body：目标地址 + 「不再询问」 -->
          <div class="px-6 py-2 space-y-3">
            <div class="p-3 rounded-xl bg-[var(--mobile-input-bg)] text-xs">
              <span class="text-[var(--mobile-text-muted)]">
                {{ t('mobile.egress.targetUrl') }}
              </span>
              <div class="mt-1.5 flex items-baseline gap-0.5 break-all font-mono leading-relaxed">
                <span class="text-[var(--mobile-accent)] font-medium">{{ current.host }}</span>
                <span class="text-[var(--mobile-text-primary)]">{{ current.path }}</span>
              </div>
            </div>

            <!-- 不再询问（持久授权，落 Rust 持久层） -->
            <div class="pt-1">
              <Toggle v-model="persist" :label="t('mobile.egress.remember')" />
            </div>
          </div>

          <!-- Actions -->
          <div class="flex gap-3 px-6 py-5">
            <button
              class="flex-1 bg-[var(--mobile-input-bg)] text-[var(--mobile-text-secondary)] rounded-xl font-medium active:opacity-80 transition-colors duration-200 confirm-btn-height"
              @click="deny"
            >
              {{ t('mobile.egress.deny') }}
            </button>
            <button
              class="flex-1 bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] rounded-xl font-medium active:opacity-80 transition-colors duration-200 confirm-btn-height"
              @click="allow"
            >
              {{ t('mobile.egress.allow') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * EgressConsentDialog — 外网访问授权弹窗（Egress L3，spec §5.6 / D7 / D8）
 *
 * 监听 `egress_consent_request`（Rust egress.rs request_consent emit，payload
 * { request_id, url, host, path, source }），懒触发：请求到达时入队展示，
 * 用户确认/拒绝经 `egress_consent_resolve` 回 Rust 裁决（oneshot 通道）。
 * 请求可能并发到达（宿主 useUpdateChecker + 插件 ai-chatbox 等），故维护
 * FIFO 队列：当前请求结算后才弹下一个；Rust 侧每个 pending 有 30s 超时，
 * 本组件用同值计时器在超时点自动收起（Rust 已按拒绝结算，回执静默无害）。
 * 授权记忆粒度（D7）：会话级默认；勾选「不再询问」→ persist=true 落 Rust
 * 持久层（egress_grants.json），不落 localStorage（§8 安全红线）。
 * 由 App.vue 全局挂载一次（与 FsAuthDialog 并列）。
 */
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { logger } from '@/utils/frontendLogger'
import Toggle from '@/components/Toggle.vue'

const { t } = useI18n()

/** Rust egress.rs ConsentRequest 的 emit payload 形状 */
interface EgressConsentRequest {
  request_id: string
  url: string
  host: string
  path: string
  /** 调用方来源：宿主 'host' / 插件 'plugin:<id>' */
  source: string
}

/** 弹窗展示超时：与 Rust egress.rs CONSENT_TIMEOUT（30s）同值 */
const CONSENT_TIMEOUT_MS = 30_000

/** 待授权请求队列（FIFO；懒触发，一次弹一个） */
const queue = ref<EgressConsentRequest[]>([])
const current = computed(() => queue.value[0] ?? null)
/** 「不再询问」勾选（持久授权；每次弹出复位） */
const persist = ref(false)

/** 请求方描述：宿主调用 / 插件调用 / 未知来源兜底 */
const requestDescription = computed(() => {
  const req = current.value
  if (!req) return ''
  if (req.source === 'host') return t('mobile.egress.requestByHost')
  if (req.source.startsWith('plugin:')) {
    return t('mobile.egress.requestByPlugin', { plugin: req.source.slice('plugin:'.length) })
  }
  return t('mobile.egress.requestByUnknown', { source: req.source })
})

let unlisten: UnlistenFn | null = null
/** 当前弹窗超时计时器（Rust 侧同值超时视为拒绝；到点自动收起推进队列） */
let dismissTimer: ReturnType<typeof setTimeout> | null = null

onMounted(async () => {
  unlisten = await listen<EgressConsentRequest>('egress_consent_request', (event) => {
    enqueue(event.payload)
  })
})

onUnmounted(() => {
  unlisten?.()
  if (dismissTimer) clearTimeout(dismissTimer)
})

function enqueue(req: EgressConsentRequest) {
  // 去重：同一 request_id 事件重发（防御）不重复入队，避免悬挂未结算事务
  if (queue.value.some((q) => q.request_id === req.request_id)) return
  queue.value.push(req)
  if (queue.value.length === 1) {
    persist.value = false
    startDismissTimer()
  }
}

/** 结算当前请求并推进队列（结果回执在 allow/deny 中 invoke） */
function advance() {
  queue.value.shift()
  persist.value = false
  if (dismissTimer) {
    clearTimeout(dismissTimer)
    dismissTimer = null
  }
  if (queue.value.length > 0) startDismissTimer()
}

function startDismissTimer() {
  dismissTimer = setTimeout(() => {
    // Rust 侧同值超时已按拒绝结算（pending 已清）；此处仅收起弹窗推进队列，
    // 不 invoke 回执——request_id 已不存在，回执仅产生 warn 日志
    dismissTimer = null
    advance()
  }, CONSENT_TIMEOUT_MS)
}

async function allow() {
  const req = current.value
  if (!req) return
  const remember = persist.value
  advance()
  try {
    await invoke('egress_consent_resolve', {
      requestId: req.request_id,
      allow: true,
      persist: remember,
    })
  } catch (e) {
    logger.error('[EgressConsent] Resolve allow failed:', e)
  }
}

async function deny() {
  const req = current.value
  if (!req) return
  advance()
  try {
    await invoke('egress_consent_resolve', {
      requestId: req.request_id,
      allow: false,
      persist: false,
    })
  } catch (e) {
    logger.error('[EgressConsent] Resolve deny failed:', e)
  }
}
</script>

<style scoped>
/* 按钮高度与 ConfirmDialog / FsAuthDialog 保持一致（44px 触摸目标下限） */
.confirm-btn-height {
  height: clamp(2.5rem, 2.75rem, 3rem);
}
</style>
