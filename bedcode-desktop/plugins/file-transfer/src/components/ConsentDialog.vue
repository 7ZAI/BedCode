<script setup lang="ts">
/**
 * ConsentDialog — 首连确认富弹窗（spec 决策 6）
 *
 * 渲染 useConsent 单例中的当前待确认请求：设备名（无名以短指纹兜底 +
 * 身份提示）、完整节点 ID 指纹核对区、秒级倒计时；接受 / 拒绝 / 关闭
 * （含点击遮罩）均经 useConsent 结算并推进队列。编排常驻于插件激活期，
 * 本组件只负责渲染当前项——用户不在文件传输面板时经状态栏项跳转过来
 * 后即可见可操作。
 */
import { computed, inject } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import { useConsent, consentDisplayName } from '../composables/useConsent'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const { currentRequest, remainingSeconds, accept, deny } = useConsent(context)
const request = currentRequest

/** 展示名：有设备名用名，无名回退短指纹 */
const displayName = computed(() => (request.value ? consentDisplayName(request.value) : ''))

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
  <Teleport to="body">
    <Transition name="ft-dialog">
      <div
        v-if="request"
        class="ft-dialog-overlay"
        role="dialog"
        aria-modal="true"
        @click.self="handleDeny"
      >
        <div class="ft-dialog-card ft-consent-card">
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

            <!-- 完整节点 ID 指纹核对区 -->
            <div class="ft-consent-fp">
              <span class="ft-consent-fp-label">
                {{ t('transfer.consent.fingerprintLabel') }}
              </span>
              <code class="ft-consent-fp-code">{{ request.nodeId }}</code>
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
      </div>
    </Transition>
  </Teleport>
</template>
