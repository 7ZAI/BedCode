<template>
  <button
    type="button"
    class="w-full rounded-[14px] p-3.5 text-left transition-colors active:opacity-90"
    :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)' }"
  >
    <span
      v-if="label"
      class="block text-[11px] font-medium uppercase tracking-wide"
      :style="{ color: 'var(--mobile-text-secondary)' }"
    >
      {{ label }}
    </span>
    <span class="block mt-1 text-xl font-semibold tabular-nums" :style="{ color: 'var(--mobile-text-primary)' }">
      {{ count }}
    </span>
  </button>
</template>

<script setup lang="ts">
/**
 * 宿主壳首页快捷卡片（terminal-session 贡献的「活跃会话数」槽位）
 *
 * 壳 Home 直接渲染（不经 PluginViewHost），不依赖 pluginContext——只读
 * mobileApi.activeSessions（宿主连接域投影）计数。文案一律走宿主共享 i18n
 * 实例（本域在 activate 期已注册插件前缀键）；插件键取不到时退回宿主自有的
 * 槽位标题键（同义且双语），仍取不到则留空（模板隐藏标签，不落硬编码文案）。
 */
import { computed } from 'vue'
import { getI18n, getMobileApi } from '@binblink/bedcode-plugin-sdk-mobile'

defineProps<{ app: any }>()

const api = getMobileApi()
const count = computed(() => api.activeSessions.value?.length ?? 0)

/** 插件前缀键（本域 activate 期注册；键名见 host/i18n.ts） */
const PREFIX = 'com.bedcode.terminal-session.'
/** 兜底键：宿主壳首页自有标题文案（双语，壳不依赖插件注册顺序） */
const HOST_FALLBACK_KEY = 'shell.home.slotsTitle'

const label = computed(() => {
  const i18n: any = getI18n()
  if (!i18n?.global?.t) return ''
  try {
    const key = `${PREFIX}hub.slotTitle`
    const text: string = i18n.global.t(key)
    if (text && text !== key) return text
    const hostText: string = i18n.global.t(HOST_FALLBACK_KEY)
    return hostText && hostText !== HOST_FALLBACK_KEY ? hostText : ''
  } catch {
    return ''
  }
})
</script>