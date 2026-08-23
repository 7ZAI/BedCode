<template>
  <SettingsSubPage :title="$t('settings.peerReceive.title')">
    <div class="px-4 py-4 space-y-3">
      <!-- 接收策略：三段自绘分段控件（非原生 radio；44px 触达） -->
      <div class="settings-group p-4">
        <p class="text-sm font-medium text-[var(--mobile-text-primary)]">
          {{ $t('settings.peerReceive.policyLabel') }}
        </p>
        <div
          class="mt-3 flex rounded-xl border border-[var(--mobile-border)] overflow-hidden"
          role="radiogroup"
          :aria-label="$t('settings.peerReceive.policyLabel')"
        >
          <button
            v-for="(mode, index) in policyModes"
            :key="mode.value"
            class="flex-1 min-h-[44px] px-2 text-[13px] font-medium transition-colors duration-200 active:opacity-80"
            :class="[
              index > 0 ? 'border-l border-[var(--mobile-border)]' : '',
              policyMode === mode.value
                ? 'text-[var(--mobile-text-on-accent)]'
                : 'text-[var(--mobile-text-secondary)]',
            ]"
            :style="
              policyMode === mode.value ? { background: 'var(--mobile-accent)' } : {}
            "
            role="radio"
            :aria-checked="policyMode === mode.value"
            @click="policyMode = mode.value"
          >
            {{ mode.label }}
          </button>
        </div>
        <p class="mt-2 text-xs" style="color: var(--mobile-row-sub)">
          {{ policyDesc }}
        </p>
      </div>

      <!-- 询问超时：预设时长 chips（仅 ask 模式有意义） -->
      <div
        class="settings-group p-4"
        :class="policyMode !== 'ask' ? 'opacity-50' : ''"
      >
        <div class="flex items-center justify-between gap-3">
          <p class="text-sm font-medium text-[var(--mobile-text-primary)]">
            {{ $t('settings.peerReceive.timeoutLabel') }}
          </p>
          <span class="text-xs" style="color: var(--mobile-row-sub)">
            {{ askTimeoutSecs }}{{ $t('settings.peerReceive.timeoutUnit') }}
          </span>
        </div>
        <div class="mt-3 flex flex-wrap gap-2">
          <button
            v-for="secs in timeoutPresets"
            :key="secs"
            class="min-h-[44px] px-4 rounded-xl text-[13px] font-mono border transition-colors duration-200 active:opacity-80"
            :class="
              askTimeoutSecs === secs
                ? 'border-transparent text-[var(--mobile-text-on-accent)]'
                : 'border-[var(--mobile-border)] text-[var(--mobile-text-secondary)]'
            "
            :style="
              askTimeoutSecs === secs ? { background: 'var(--mobile-accent)' } : {}
            "
            :disabled="policyMode !== 'ask'"
            @click="askTimeoutSecs = secs"
          >
            {{ secs }}{{ $t('settings.peerReceive.timeoutUnit') }}
          </button>
        </div>
      </div>
    </div>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 文件接收设置二级页面 — 对等网络接收策略（issue 10）
 *
 * 全局接收策略单开关（每次询问/直接接收/直接拒绝）+ 询问超时窗口；
 * 变更即经 setPolicy 持久化并对运行中节点热生效，失败回滚并提示。
 */
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import { usePeerReceiving, type PeerReceivePolicyMode } from '@/composables/usePeerReceiving'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const toast = useToast()
const receiving = usePeerReceiving()
const settings = receiving.settings

onMounted(() => {
  // 幂等单例启动：拉取接收设置快照（已启动则为 no-op）
  void receiving.start()
})

const policyMode = ref<PeerReceivePolicyMode>('ask')
const askTimeoutSecs = ref(60)
/** 预设询问窗口（秒；crate 校验范围 10..=600） */
const timeoutPresets = [15, 30, 60, 120, 300]

const policyModes = computed(() => [
  { value: 'ask' as PeerReceivePolicyMode, label: t('settings.peerReceive.policyAsk') },
  {
    value: 'always_accept' as PeerReceivePolicyMode,
    label: t('settings.peerReceive.policyAlwaysAccept'),
  },
  {
    value: 'always_deny' as PeerReceivePolicyMode,
    label: t('settings.peerReceive.policyAlwaysDeny'),
  },
])

/** 当前策略的说明文案（随选中项切换） */
const policyDesc = computed(() => {
  switch (policyMode.value) {
    case 'always_accept':
      return t('settings.peerReceive.policyAlwaysAcceptDesc')
    case 'always_deny':
      return t('settings.peerReceive.policyAlwaysDenyDesc')
    default:
      return t('settings.peerReceive.policyAskDesc')
  }
})

watch(
  settings,
  (value) => {
    policyMode.value = value.policyMode
    askTimeoutSecs.value = value.askTimeoutSecs
  },
  { immediate: true },
)

watch([policyMode, askTimeoutSecs], async ([mode, secs], [prevMode, prevSecs]) => {
  if (mode === prevMode && secs === prevSecs) return
  const ok = await receiving.setPolicy(mode, secs)
  if (ok) toast.info(t('settings.peerReceive.savedToast'))
  else toast.error(t('settings.peerReceive.savedFailedToast'))
})
</script>
