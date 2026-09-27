<script setup lang="ts">
/**
 * 单 CLI 卡片：官方图标 + 状态徽章 + 版本/安装方式 + 双安装警告
 *
 * 徽章色语义：ok=success；双安装=warning；error=danger；其余中性
 *
 * 票 07 新增第六态「已装 · 未初始化」：`sessionState === 'empty'`（装了但
 * 扫描后零会话数据，如本机 codex —— `~/.codex/` 存在却从未跑过会话）。
 * 该信号由 usage 域给出（`useUsage.cliSessionState`），本组件不自行探测；
 * 三种未定状态（未扫描 / 未授权 / 扫描中）由上游归为 `unknown`，此时仍走
 * 常规「已装」——**宁可少一次提醒也不误报**。
 */
import { computed, inject } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { CliDetectInfo, CliId } from '../types'
import type { CliSessionState } from '../composables/useUsage'
import CliIcon from './CliIcon.vue'

const props = defineProps<{
  cliId: CliId
  info: CliDetectInfo | null
  /** usage 域的会话数据状态（缺省=不下结论） */
  sessionState?: CliSessionState
}>()

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
  if (s === 'ok') {
    // **双安装优先于第六态**：双安装是可行动问题（PATH 生效的是哪一个），
    // 「没数据」只是缺席信号。两者同现时用可行动的那条。
    if (props.info?.dual) return { tone: 'warn', label: t('hub.card.installed') }
    // 已装但未初始化：中性色（不是错误也不是警告，是「还没数据」）
    if (props.sessionState === 'empty') {
      return { tone: 'neutral', label: t('hub.card.installedNoSessions') }
    }
    return { tone: 'ok', label: t('hub.card.installed') }
  }
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
  <div class="ah-card">
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

    <div v-if="props.info?.error" class="ah-cli-error">{{ t('hub.card.error') }}</div>

    <div v-if="props.info?.dual" class="ah-cli-dual">
      <div>{{ t('hub.card.dualWarning', { n: props.info.paths.length }) }}</div>
      <div v-for="p in props.info.paths" :key="p" class="ah-mono ah-cli-path">{{ p }}</div>
    </div>
  </div>
</template>
