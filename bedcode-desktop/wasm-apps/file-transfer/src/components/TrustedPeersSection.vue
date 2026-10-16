<script setup lang="ts">
/**
 * TrustedPeersSection — 可信对端管理分区（桌面设置覆盖层，spec 决策 8）
 *
 * 列出全部可信对端（设备名 / 短指纹 / 加入时间），支持逐项撤销：行内撤销
 * 按钮弹出后果说明确认框（撤销后对方重连需重新确认），确认后调撤销命令并
 * 从列表移除；取消无副作用。加载失败如实呈现错误态 + 重试，不吞成空白。
 *
 * 编排逻辑在 useTrustedPeers；本组件只负责渲染与确认框状态。
 */
import { computed, inject, onMounted, ref, watch } from 'vue'
import type {
  PluginContext,
  PluginDialogHandle,
  PluginDialogOptions,
} from '@binblink/bedcode-plugin-sdk-desktop'
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
/** 确认后撤销提交中（按钮禁用防重复提交） */
const confirming = ref(false)
/** 撤销确认宿主全局弹窗句柄 */
let confirmDialog: PluginDialogHandle | null = null

function buildConfirmOptions(peer: TrustedPeer): PluginDialogOptions {
  return {
    // 警告三角图标（Heroicons outline exclamation-triangle）
    icon: 'M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z',
    title: t('transfer.trusted.revokeTitle'),
    message: t('transfer.trusted.revokeBody', { name: displayName(peer) }),
    actions: [
      { label: t('transfer.trusted.cancel'), kind: 'default', disabled: confirming.value, onClick: cancelConfirm },
      {
        label: t('transfer.trusted.revoke'),
        kind: 'danger',
        disabled: confirming.value,
        onClick: confirmRevoke,
      },
    ],
    // 遮罩/Escape 等同取消（沿用旧行为）
    onClose: () => {
      confirmTarget.value = null
      if (confirmDialog) {
        confirmDialog = null
      }
    },
  }
}

watch(
  () => [confirmTarget.value, confirming.value] as const,
  ([target]) => {
    if (target && !confirmDialog) {
      confirmDialog = context.ui.showDialog(buildConfirmOptions(target))
    } else if (target && confirmDialog) {
      // 确认中状态变化（禁用按钮）也经 update 联动
      confirmDialog.update(buildConfirmOptions(target))
    } else if (!target && confirmDialog) {
      confirmDialog.close()
      confirmDialog = null
    }
  },
)

/** 跟随宿主语言本地化加入时间 */
function formatDate(dateStr: string): string {
  const locale = context.i18n.getI18n()?.global?.locale
  const lang = typeof locale === 'string' ? locale : locale?.value
  return formatTrustedDate(dateStr, lang ?? 'zh-CN')
}

/** 展示名兜底：无名条目以「未知设备」呈现，短指纹单独展示供核对 */
function displayName(peer: TrustedPeer): string {
  return peer.displayName || t('transfer.peer.unknown')
}

function openConfirm(peer: TrustedPeer): void {
  confirmTarget.value = peer
}

function cancelConfirm(): void {
  confirmTarget.value = null
}

async function confirmRevoke(): Promise<void> {
  const target = confirmTarget.value
  if (!target || confirming.value) return
  confirming.value = true
  try {
    const ok = await revoke(target.nodeId)
    // 失败：抛错使宿主弹窗保持打开可重试（composable 已就地提示 errorKey）
    if (!ok) throw new Error('revoke failed')
    confirmTarget.value = null
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
  <section class="ft-settings-section">
    <h3 class="ft-settings-section-title">{{ t('transfer.trusted.title') }}</h3>

    <!-- 加载中 -->
    <div v-if="loadState === 'loading'" class="ft-trusted-state">
      {{ t('transfer.trusted.loading') }}
    </div>

    <!-- 加载失败：如实呈现错误态 + 重试入口 -->
    <div v-else-if="loadState === 'error'" class="ft-trusted-error">
      <span class="ft-trusted-error-text">{{ t(errorKey) }}</span>
      <button class="ft-btn" @click="refresh">{{ t('transfer.trusted.retry') }}</button>
    </div>

    <!-- 空态 -->
    <div v-else-if="peers.length === 0" class="ft-trusted-state">
      {{ t('transfer.trusted.empty') }}
    </div>

    <!-- 列表态 -->
    <template v-else>
      <div class="ft-root-list">
        <div v-for="peerItem in peers" :key="peerItem.nodeId" class="ft-root-item ft-trusted-item">
          <div class="ft-trusted-info">
            <span class="ft-trusted-name" :title="displayName(peerItem)">
              {{ displayName(peerItem) }}
            </span>
            <code class="ft-trusted-fp">{{ peerItem.fingerprintShort }}</code>
          </div>
          <div class="ft-trusted-meta">
            <span class="ft-trusted-added">
              {{ t('transfer.trusted.addedAt', { time: formatDate(peerItem.addedAt) }) }}
            </span>
            <button
              class="ft-text-btn ft-text-btn--ghost"
              :disabled="revokingIds.has(peerItem.nodeId)"
              @click="openConfirm(peerItem)"
            >
              {{ t('transfer.trusted.revoke') }}
            </button>
          </div>
        </div>
      </div>
      <!-- 撤销失败：保留条目并就地提示（重试走行内撤销按钮） -->
      <p v-if="revokeFailed" class="ft-trusted-revoke-failed">
        {{ t(errorKey) }}
      </p>
      <p class="ft-settings-helper">{{ t('transfer.devices.trustHint') }}</p>
    </template>
  </section>
</template>
