<script setup lang="ts">
/**
 * 单 CLI 卡片：官方图标 + 状态徽章 + 版本/安装方式 + 双安装警告
 *
 * 徽章色语义：ok=success；双安装=warning；error=danger；其余中性
 */
import { computed, inject } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { CliDetectInfo, CliId } from '../types'
import CliIcon from './CliIcon.vue'

const props = defineProps<{ cliId: CliId; info: CliDetectInfo | null }>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

const NAMES: Record<CliId, string> = {
  claude: 'Claude Code',
  codex: 'Codex CLI',
  opencode: 'OpenCode',
  pi: 'Pi',
}

const badge = computed(() => {
  const s = props.info?.status ?? 'idle'
  if (s === 'ok') return { tone: props.info?.dual ? 'warn' : 'ok', label: t('hub.card.installed') }
  if (s === 'not-installed') return { tone: 'neutral', label: t('hub.card.notInstalled') }
  if (s === 'error') return { tone: 'err', label: t('hub.card.error') }
  if (s === 'detecting') return { tone: 'neutral', label: t('hub.card.detecting') }
  return { tone: 'neutral', label: t('hub.card.idle') }
})

const methodLabel = computed(() => {
  const m = props.info?.method
  return m && m !== 'unknown' ? t(`hub.card.method.${m}`) : null
})
</script>

<template>
  <div class="ah-card ah-cli-card">
    <div class="ah-cli-head">
      <span class="ah-cli-name">
        <CliIcon :cli-id="cliId" />
        <span class="ah-cli-name-text">{{ NAMES[cliId] }}</span>
      </span>
      <span class="ah-cli-tag" :class="badge.tone">
        <span class="ah-cli-dot"></span>{{ badge.label }}
      </span>
    </div>

    <div class="ah-cli-meta">
      <span class="ah-mono">{{ props.info?.version ?? t('hub.card.versionUnknown') }}</span>
      <span v-if="methodLabel" class="ah-cli-method">{{ methodLabel }}</span>
    </div>

    <div v-if="props.info?.error" class="ah-cli-error">{{ props.info.error }}</div>

    <div v-if="props.info?.dual" class="ah-cli-dual">
      <div>{{ t('hub.card.dualWarning', { n: props.info.paths.length }) }}</div>
      <div v-for="p in props.info.paths" :key="p" class="ah-mono ah-cli-path">{{ p }}</div>
    </div>
  </div>
</template>
