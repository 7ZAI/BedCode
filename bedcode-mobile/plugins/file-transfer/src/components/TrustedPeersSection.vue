<script setup lang="ts">
/**
 * TrustedPeersSection — 可信对端管理分区（移动设置二级页，spec 决策 8）
 *
 * 列出全部可信对端（设备名 / 短指纹 / 加入时间），支持逐项撤销：行内撤销
 * 按钮经插件对话框 API 弹出后果说明确认框（撤销后对方重连需重新确认），
 * 确认后调撤销命令并从列表移除；取消无副作用。加载失败如实呈现错误态 +
 * 重试入口，不吞成空白。
 *
 * 编排逻辑在 useTrustedPeers；本组件只负责渲染与确认框调用。
 * 样式复用宿主 settings-group/settings-row 设计语言 + --mobile-* token，
 * 触控目标 ≥44px。
 */
import { computed, inject, onMounted, ref } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'
import {
  formatTrustedDate,
  useTrustedPeers,
  type TrustedPeer,
} from '../composables/useTrustedPeers'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const { peers, loadState, errorKey, revokingIds, refresh, revoke } = useTrustedPeers(context)

/** 两步确认目标；null = 无确认框（取消即清空，无副作用） */
const confirmTarget = ref<TrustedPeer | null>(null)
/** 确认后撤销提交中（行内按钮禁用防重复提交） */
const confirming = ref(false)

/** 跟随当前语言本地化加入时间（插件 i18n API 无 locale 读取能力时回退浏览器语言） */
function formatDate(dateStr: string): string {
  // 移动端 I18nAPI 未暴露宿主 vue-i18n 实例：防御性探测 getI18n（桌面端形态），
  // 缺失时回退浏览器语言，最终由 formatTrustedDate 兑底默认格式化
  const api = context.i18n as unknown as { getI18n?: () => unknown }
  const global = (api.getI18n?.() as { global?: { locale?: unknown } } | undefined)?.global
  const loc = global?.locale
  const lang = typeof loc === 'string' ? loc : (loc as { value?: string } | undefined)?.value
  return formatTrustedDate(dateStr, lang || navigator.language)
}

/** 展示名兜底：无名条目以「未知设备」呈现，短指纹单独展示供核对 */
function displayName(peer: TrustedPeer): string {
  return peer.displayName || t('transfer.peer.unknown')
}

/** 行内撤销按钮 → 两步撤销第二步：对话框展示后果说明 */
function openConfirm(peer: TrustedPeer): void {
  confirmTarget.value = peer
  void context.dialogs
    .showConfirm({
      title: t('transfer.trusted.revokeTitle'),
      message: t('transfer.trusted.revokeBody', { name: displayName(peer) }),
      variant: 'danger',
      confirmText: t('transfer.trusted.revoke'),
      cancelText: t('transfer.trusted.cancel'),
      dismissible: true,
    })
    .then((confirmed) => {
      // 对话框已关闭：清空目标（取消/关闭均无副作用），仅 confirmed 时执行撤销
      const target = confirmTarget.value
      confirmTarget.value = null
      if (confirmed !== true || !target) return
      return handleRevoke(target)
    })
    .catch(() => {
      confirmTarget.value = null
    })
}

async function handleRevoke(target: TrustedPeer): Promise<void> {
  confirming.value = true
  try {
    const ok = await revoke(target.nodeId)
    if (ok) {
      context.dialogs.showToast(
        t('transfer.trusted.revokedToast', { name: displayName(target) }),
        'success',
      )
    }
    // 失败保留条目并就地提示（errorKey），可再次点击行内撤销重试
  } finally {
    confirming.value = false
  }
}

/** 撤销失败提示（与列表加载失败区分，不覆盖错误态分支） */
const revokeFailed = computed(() => errorKey.value === 'transfer.trusted.revokeFailed')

onMounted(() => {
  void refresh()
})
</script>

<template>
  <section class="space-y-2">
    <h2 class="settings-section-title">{{ t('transfer.trusted.title') }}</h2>

    <!-- 加载中 -->
    <div v-if="loadState === 'loading'" class="settings-group ft-trusted-state">
      {{ t('transfer.trusted.loading') }}
    </div>

    <!-- 加载失败：如实呈现错误态 + 重试入口 -->
    <div v-else-if="loadState === 'error'" class="ft-trusted-error">
      <span class="ft-trusted-error-text">{{ t(errorKey) }}</span>
      <button
        class="ft-touch-btn ft-trusted-retry rounded-xl border border-[var(--mobile-border)] bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-primary)] active:opacity-80 transition-opacity"
        @click="refresh"
      >
        {{ t('transfer.trusted.retry') }}
      </button>
    </div>

    <!-- 空态 -->
    <div v-else-if="peers.length === 0" class="settings-group ft-trusted-state">
      {{ t('transfer.trusted.empty') }}
    </div>

    <!-- 列表态 -->
    <template v-else>
      <div class="settings-group">
        <div v-for="peerItem in peers" :key="peerItem.nodeId" class="settings-row">
          <div class="flex items-center gap-2 flex-1 min-w-0">
            <svg
              class="w-4 h-4 flex-shrink-0 text-[var(--mobile-accent)]"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.75c0 5.592 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.57-.598-3.75h-.152c-3.196 0-6.1-1.248-8.25-3.285z"
              />
            </svg>
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-1.5 min-w-0">
                <span class="settings-label truncate">{{ displayName(peerItem) }}</span>
                <code class="ft-trusted-fp flex-shrink-0">{{ peerItem.fingerprintShort }}</code>
              </div>
              <p class="settings-desc truncate">
                {{ t('transfer.trusted.addedAt', { time: formatDate(peerItem.addedAt) }) }}
              </p>
            </div>
          </div>
          <button
            class="flex-shrink-0 ft-trusted-revoke-btn"
            :disabled="revokingIds.has(peerItem.nodeId)"
            @click="openConfirm(peerItem)"
          >
            {{ t('transfer.trusted.revoke') }}
          </button>
        </div>
      </div>
      <!-- 撤销失败就地提示（条目保留，可再次点击行内撤销重试） -->
      <p v-if="revokeFailed" class="ft-trusted-revoke-failed">
        {{ t(errorKey) }}
      </p>
      <p class="settings-desc ft-trusted-hint">{{ t('transfer.devices.trustHint') }}</p>
    </template>
  </section>
</template>

<style scoped>
/* 空态 / 加载态占位（居中弱化文案） */
.ft-trusted-state {
  padding: 2rem 1rem;
  text-align: center;
  font-size: var(--font-size-sm);
  color: var(--mobile-text-muted);
}

/* 加载失败错误条：危险色如实呈现 + 行内重试（44px 触控目标） */
.ft-trusted-error {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.75rem 1rem;
  border-radius: 0.75rem;
  border: 1px solid color-mix(in srgb, var(--mobile-error) 40%, transparent);
  background: color-mix(in srgb, var(--mobile-error) 8%, transparent);
}

.ft-trusted-error-text {
  flex: 1;
  min-width: 0;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-text-primary);
}

.ft-trusted-retry {
  min-height: 2.75rem;
  padding: 0 1rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  font-weight: 500;
}

/* 短指纹（等宽字体核对区） */
.ft-trusted-fp {
  font-family: Consolas, Monaco, monospace;
  font-size: clamp(0.625rem, 0.6875rem + (100vw - 360px) / 800, 0.75rem);
  color: var(--mobile-text-secondary);
}

/* 行内撤销按钮：44px 触控目标 + danger 配色（与设置页删除按钮同语言，
   自包含定义——ft-settings-remove-btn 是 SettingsSection scoped 类，对子组件不生效） */
.ft-trusted-revoke-btn {
  min-height: 2.75rem;
  min-width: 2.75rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0 0.875rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-error);
  border: 1px solid var(--mobile-error-muted);
  background: transparent;
  transition: opacity 0.15s ease;
}

.ft-trusted-revoke-btn:active {
  opacity: 0.8;
}

.ft-trusted-revoke-btn:disabled {
  opacity: 0.45;
}

/* 分区底部提示行（次级说明文字，与设置页 hint 同视觉语言） */
.ft-trusted-hint {
  margin-top: 0.25rem;
}

/* 撤销失败就地提示（条目保留，可再次点击行内撤销重试） */
.ft-trusted-revoke-failed {
  margin: 0;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-error);
}
</style>
