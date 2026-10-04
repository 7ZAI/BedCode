<script setup lang="ts">
/**
 * 概览分区（票据 02 交付范围 + 票据 03 测速行）：目录授权横幅 + 环境条
 * （含 npm 源测速与推荐）+ 四张 CLI 卡片
 *
 * 票 07：CLI 卡片的第六态「已装 · 未初始化」由 usage 域给出（父层经
 * `sessionStates` 下传），本分区不自行探测会话数据。
 *
 * 卸载（本次新增）：卡片上的「卸载」经 `uninstall` 事件上抛，父层执行
 * 命令并回传结果；本分区只负责把 busy / uninstalling / 失败信号下传给卡。
 */
import { computed, inject } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { AgentHubState, CliId, InstallDomainState } from '../types'
import type { CliSessionState } from '../composables/useUsage'
import CliCard from './CliCard.vue'

const props = defineProps<{
  state: AgentHubState | null
  detecting: boolean
  installState: InstallDomainState | null
  speedTesting: boolean
  /** 各 CLI 的会话数据状态（票 07；缺项=不下结论） */
  sessionStates?: Partial<Record<CliId, CliSessionState>>
  /** 卸载失败信号：{ cli, error }（guest 拒绝/异常；瞬态提示，成功后由父层清除）
   * error 为友好 i18n 文案（ADR 0030：guest 业务码优先，原文不携带） */
  uninstallFailed?: { cli: CliId; error: string } | null
}>()
const emit = defineEmits<{
  detect: []
  auth: []
  'speed-test': []
  'goto-install': []
  uninstall: [cli: CliId]
}>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string) => context.i18n.t(key)

const CLI_IDS: CliId[] = ['claude', 'codex', 'opencode', 'pi']

/** 平台名（std::env::consts::OS 值域）→ 展示名；未知值回退原样 */
const OS_LABELS: Record<string, string> = {
  linux: 'Linux',
  windows: 'Windows',
  macos: 'macOS',
  android: 'Android',
  ios: 'iOS',
  freebsd: 'FreeBSD',
  openbsd: 'OpenBSD',
  netbsd: 'NetBSD',
  dragonfly: 'DragonFly BSD',
  solaris: 'Solaris',
}
function osLabel(os: string | null | undefined): string | null {
  if (!os) return null
  return OS_LABELS[os] ?? os
}

const envRows = computed(() => {
  const env = props.state?.env
  return [
    { label: t('hub.env.os'), value: osLabel(env?.os) },
    { label: t('hub.env.arch'), value: env?.arch },
    { label: t('hub.env.osVersion'), value: env?.osVersion },
    { label: t('hub.env.shell'), value: env?.shell },
    { label: t('hub.env.node'), value: env?.node },
    { label: t('hub.env.npm'), value: env?.npm },
    { label: t('hub.env.pnpm'), value: env?.pnpm },
    { label: t('hub.env.registry'), value: env?.registry },
    { label: t('hub.env.python'), value: env?.python },
  ]
})

/** 是否有任意在途 run（安装/更新/卸载共用同一 run 管线，busy 期间全禁） */
const busy = computed(() => !!props.installState?.active)

/** 正在被卸载的 CLI（active run 属主；其余卡片照常禁用） */
const uninstalling = computed<CliId | null>(() => {
  const a = props.installState?.active
  return a?.action === 'uninstall' ? (a.cli as CliId) : null
})

/** node 环境是否就绪（npm-global 卸载依赖） */
const nodeReady = computed(() => !!props.state?.env?.node)

const speed = computed(() => props.installState?.mirror?.speed ?? null)
const speedDone = computed(() => speed.value?.status === 'ok')
const speedFailed = computed(() => speed.value?.status === 'error')

/** 内置源展示名（i18n；自定义源统一 "hub.speed.source.custom"） */
const SOURCE_LABELS: Record<string, string> = {
  npmmirror: 'hub.speed.source.npmmirror',
  npmjs: 'hub.speed.source.npmjs',
  huawei: 'hub.speed.source.huawei',
  tencent: 'hub.speed.source.tencent',
  yarn: 'hub.speed.source.yarn',
}
function sourceLabel(id: string): string {
  return t(SOURCE_LABELS[id] ?? 'hub.speed.source.custom')
}

/** 推荐源（最快可达者，来自多源测速列表） */
const recommended = computed(() => {
  if (!speedDone.value || !speed.value?.recommend) return null
  return (speed.value.sources ?? []).find((s) => s.id === speed.value?.recommend) ?? null
})
const recommendMirror = computed(() => speed.value?.recommend === 'npmmirror')
</script>

<template>
  <div>
    <div v-if="state && !state.authGranted" class="ah-banner">
      <span class="ah-banner-ic">⚠</span>
      <span class="ah-banner-text">{{ t('hub.auth.banner') }}</span>
      <button type="button" class="ah-btn ah-btn-sm" @click="emit('auth')">
        {{ t('hub.auth.action') }}
      </button>
    </div>

    <div class="ah-card">
      <div class="ah-env-head">
        <span class="ah-section-title">{{ t('hub.env.title') }}</span>
        <button
          type="button"
          class="ah-btn ah-btn-ghost ah-btn-sm"
          :disabled="detecting"
          @click="emit('detect')"
        >
          {{ detecting ? t('hub.env.detecting') : t('hub.env.detect') }}
        </button>
      </div>
      <div class="ah-env-rows">
        <div v-for="row in envRows" :key="row.label" class="ah-env-row">
          <span class="ah-env-label">{{ row.label }}</span>
          <span class="ah-env-value ah-mono">{{ row.value ?? t('hub.env.none') }}</span>
        </div>
      </div>

      <!-- npm 源测速（原型 b-ov：两源计时 + 推荐；动作在安装与更新页） -->
      <div class="ah-speed-line">
        <span v-if="speedDone && recommended" class="ah-speed-text">
          {{ t('hub.speed.recommended') }}
          <b class="ah-mono">{{ sourceLabel(recommended.id) }} {{ recommended.ms ?? '—' }} ms</b>
          <span class="ah-cli-tag ah-speed-tag" :class="recommendMirror ? 'warn' : 'ok'">
            <span class="ah-cli-dot"></span>{{ recommendMirror ? t('hub.speed.recommendMirror') : t('hub.speed.recommendOfficial') }}
          </span>
        </span>
        <span v-else-if="speedFailed" class="ah-speed-text ah-cli-error">{{ t('hub.speed.fail') }}</span>
        <span v-else class="ah-speed-text"></span>
        <span class="ah-speed-line-btns">
          <button
            type="button"
            class="ah-btn ah-btn-ghost ah-btn-sm"
            :disabled="speedTesting"
            @click="emit('speed-test')"
          >
            {{ speedTesting ? t('hub.speed.testing') : t('hub.speed.action') }}
          </button>
          <button
            v-if="recommendMirror"
            type="button"
            class="ah-btn ah-btn-ghost ah-btn-sm"
            @click="emit('goto-install')"
          >
            {{ t('hub.speed.goto') }}
          </button>
        </span>
      </div>
    </div>

    <div class="ah-grid">
      <CliCard
        v-for="id in CLI_IDS"
        :key="id"
        :cli-id="id"
        :info="state?.clis?.[id] ?? null"
        :session-state="props.sessionStates?.[id]"
        :busy="busy"
        :uninstalling="uninstalling === id"
        :uninstall-failed="props.uninstallFailed?.cli === id ? props.uninstallFailed.error : undefined"
        :node-ready="nodeReady"
        @uninstall="(cli) => emit('uninstall', cli)"
      />
    </div>
  </div>
</template>
