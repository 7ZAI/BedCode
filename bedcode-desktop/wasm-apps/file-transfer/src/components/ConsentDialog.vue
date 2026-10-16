<script setup lang="ts">
/**
 * ConsentDialog — 首连确认内容组件（spec 决策 6）
 *
 * 经宿主全局弹窗（context.ui.showDialog 组件模式）渲染：宿主提供遮罩/卡片/
 * 队列/z-index，本组件只负责内容——设备名（无名以短指纹兜底 + 身份提示）、
 * 完整节点 ID 指纹核对区、秒级倒计时；接受 / 拒绝（含关闭）均经 useConsent
 * 结算并推进队列。编排常驻于插件激活期，弹窗由 index.ts 根据
 * currentRequest 开关（用户不在任何页面都能看到并操作，无需跳转面板）。
 *
 * 注意：宿主卡片自带背景/圆角/内边距（showDialog bodyClass），本组件不再
 * 包裹卡片，仅保留「位置相对」的 close 按钮锚点。
 */
import { computed, inject, onBeforeUnmount, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { useConsent, consentDisplayName } from '../composables/useConsent'
import { groupFingerprint } from '../utils/format'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const { currentRequest, remainingSeconds, accept, deny } = useConsent(context)
const request = currentRequest

/** 展示名：有设备名用名，无名回退短指纹 */
const displayName = computed(() => (request.value ? consentDisplayName(request.value) : ''))

/** 完整节点 ID 按 4 字符分组（换行落在组边界，对比友好） */
const fpGroups = computed(() => groupFingerprint(request.value?.nodeId ?? ''))

/** 复制指纹反馈态（2s 后复位） */
const copied = ref(false)
let copyTimer: ReturnType<typeof setTimeout> | null = null

async function copyFingerprint(): Promise<void> {
  const id = request.value?.nodeId
  if (!id) return
  try {
    await navigator.clipboard.writeText(id)
    copied.value = true
    if (copyTimer) clearTimeout(copyTimer)
    copyTimer = setTimeout(() => {
      copied.value = false
    }, 2000)
  } catch (e) {
    console.warn('[File Transfer] copy fingerprint failed:', e)
  }
}

onBeforeUnmount(() => {
  if (copyTimer) clearTimeout(copyTimer)
})

function handleAccept(): void {
  accept().catch((e: unknown) => {
    console.error('[File Transfer] consent accept failed:', e)
  })
}

function handleDeny(): void {
  deny().catch((e: unknown) => {
    console.error('[File Transfer] consent deny failed:', e)
  })
}
</script>

<template>
  <div v-if="request" class="ft-consent-card">
    <!-- 关闭等同拒绝（与宿主 Modal close → deny 语义一致） -->
    <button
      class="ft-mini-btn ft-consent-close"
      :title="t('transfer.consent.close')"
      @click="handleDeny"
    >
      <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M18 6L6 18M6 6l12 12"
        />
      </svg>
    </button>

    <div class="ft-consent-head">
      <span class="ft-dialog-title">{{ t('transfer.consent.title') }}</span>
      <span
        class="ft-dialog-countdown"
        :class="{ 'ft-dialog-countdown--urgent': remainingSeconds <= 10 }"
      >
        {{ t('transfer.consent.countdown', { seconds: remainingSeconds }) }}
      </span>
    </div>

    <div class="ft-consent-content">
      <!-- 设备图标 -->
      <span class="ft-consent-icon">
        <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M9 17V5a2 2 0 012-2h4a2 2 0 012 2v12m-8 0h8m-8 0a2 2 0 01-2 2H6a2 2 0 01-2-2v-3a2 2 0 012-2h1m10 3h1a2 2 0 002-2v-1"
          />
        </svg>
      </span>

      <p class="ft-consent-body">
        {{ t('transfer.consent.body', { name: displayName }) }}
      </p>
      <!-- 无名设备身份提示：宁可多一分核对，不可误信陌生节点 -->
      <p v-if="!request.deviceName" class="ft-consent-hint">
        {{ t('transfer.consent.namelessHint') }}
      </p>

      <!-- 完整节点 ID 指纹核对区：4 字符分组展示（换行落组边界）+ 一键复制 -->
      <div class="ft-consent-fp">
        <div class="ft-consent-fp-head">
          <span class="ft-consent-fp-label">
            {{ t('transfer.consent.fingerprintLabel') }}
          </span>
          <button
            class="ft-text-btn ft-consent-fp-copy"
            :title="t('transfer.consent.copyHint')"
            @click="copyFingerprint"
          >
            {{ copied ? t('transfer.consent.copied') : t('transfer.consent.copy') }}
          </button>
        </div>
        <code class="ft-consent-fp-code">
          <span v-for="(group, i) in fpGroups" :key="i" class="ft-consent-fp-block">
            {{ group }}
          </span>
        </code>
      </div>
    </div>

    <div class="ft-dialog-actions">
      <button class="ft-btn" @click="handleDeny">
        {{ t('transfer.consent.deny') }}
      </button>
      <button class="ft-btn ft-btn--primary" @click="handleAccept">
        {{ t('transfer.consent.accept') }}
      </button>
    </div>
  </div>
</template>