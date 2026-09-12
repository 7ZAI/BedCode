<script setup lang="ts">
/**
 * 安装与更新分区（票据 03）：镜像/测速卡片 + CLI 安装/更新行 + 执行输出控制台
 *
 * 设计真源：原型 `.scratch/agent-hub/prototype/index.html` #b-in。
 * 动作一律 emit 给宿主组件（useInstall 单实例在 AgentHubView），
 * 状态经 `plugin:agent-hub:install` 事件回流。
 */
import { computed, inject, nextTick, ref, watch } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { AgentHubState, CliDetectInfo, CliId, CliUpdateInfo, InstallDomainState, MirrorTarget } from '../types'
import CliIcon from './CliIcon.vue'

const props = defineProps<{
  detection: AgentHubState | null
  state: InstallDomainState | null
  output: string | null
  checking: boolean
  speedTesting: boolean
}>()

const emit = defineEmits<{
  'speed-test': []
  'apply-mirror': [target: MirrorTarget]
  restore: []
  'check-updates': []
  install: [cli: CliId, useMirror: boolean]
  cancel: []
  auth: []
}>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

const CLI_IDS: CliId[] = ['claude', 'codex', 'opencode', 'pi']
const NAMES: Record<CliId, string> = {
  claude: 'Claude Code',
  codex: 'Codex CLI',
  opencode: 'OpenCode',
  pi: 'Pi',
}

/**
 * 目录授权结果：fs 类动作（安装/换源/还原）依赖授权过的宿主 fs 通道；
 * 拒绝时置灰降级（避免逐次弹窗），测速/检查更新走 host-http 不受限
 */
const authGranted = computed(() => props.detection?.authGranted ?? false)

const nodeReady = computed(() => !!props.detection?.env?.node)

/** 本次安装临时镜像（spec：默认临时 --registry=npmmirror） */
const useMirror = ref(true)
/** 持久切换两击确认（避免引入 ui:dialog 权限；4s 未确认自动复位） */
const confirmPending = ref(false)
let confirmTimer: ReturnType<typeof setTimeout> | null = null

/**
 * node 缺失降级：每家的展示用命令经 guest `describe-install` 解析
 * （同一 recipe 白名单真源，前端不复刻包名）；随临时镜像开关重取
 */
const displayCommands = ref<Partial<Record<CliId, string>>>({})
const copiedCli = ref<CliId | null>(null)
let copiedTimer: ReturnType<typeof setTimeout> | null = null

async function loadDisplayCommands() {
  if (nodeReady.value) return
  for (const cli of CLI_IDS) {
    try {
      const d = await context.commands.execute('agent-hub.describe-install', {
        cli,
        mirror: useMirror.value,
      })
      if (d?.command) displayCommands.value[cli] = d.command as string
    } catch (e) {
      // 解析失败（如 opencode standalone / claude unknown method）：行内不展示命令
      console.error('[Agent Hub] describe-install failed', cli, e)
    }
  }
}

watch([nodeReady, useMirror], loadDisplayCommands, { immediate: true })

async function copyCommand(cli: CliId, command: string) {
  try {
    await navigator.clipboard.writeText(command)
    copiedCli.value = cli
    if (copiedTimer) clearTimeout(copiedTimer)
    copiedTimer = setTimeout(() => {
      copiedCli.value = null
    }, 2000)
  } catch (e) {
    console.error('[Agent Hub] copy command failed', cli, e)
  }
}

const speed = computed(() => props.state?.mirror?.speed ?? null)
const npmrc = computed(() => props.state?.mirror?.npmrc ?? null)
const activeRun = computed(() => props.state?.active ?? null)
const lastRun = computed(() => props.state?.last ?? null)

/** 当前生效源（npm config 实测值优先，npmrc 文件值兜底） */
const currentRegistry = computed(() => props.detection?.env?.registry ?? npmrc.value?.fileRegistry ?? null)

const speedDone = computed(() => speed.value?.status === 'ok')
const speedFailed = computed(() => speed.value?.status === 'error')

const recommendLabel = computed(() => {
  if (!speedDone.value) return null
  return speed.value?.recommend === 'npmmirror'
    ? t('hub.speed.recommendMirror')
    : t('hub.speed.recommendOfficial')
})

/** 持久切换仅在测速推荐 npmmirror 且文件源尚未是 npmmirror 时可用 */
const persistVisible = computed(
  () => speedDone.value && speed.value?.recommend === 'npmmirror' && npmrc.value?.fileRegistry !== 'https://registry.npmmirror.com',
)

function onPersistClick() {
  if (!confirmPending.value) {
    confirmPending.value = true
    if (confirmTimer) clearTimeout(confirmTimer)
    confirmTimer = setTimeout(() => {
      confirmPending.value = false
    }, 4000)
    return
  }
  confirmPending.value = false
  if (confirmTimer) clearTimeout(confirmTimer)
  emit('apply-mirror', 'npmmirror')
}

// ==================== CLI 行状态机 ====================

type RowAction = 'install' | 'update' | 'latest' | 'manual' | 'waiting'

interface Row {
  cli: CliId
  name: string
  info: CliDetectInfo | null
  update: CliUpdateInfo | null
  hint: string | null
  action: RowAction
  disabled: boolean
}

const rows = computed<Row[]>(() =>
  CLI_IDS.map((cli) => {
    const info = props.detection?.clis?.[cli] ?? null
    const update = props.state?.updates?.[cli] ?? null
    const busy = activeRun.value !== null

    const row: Row = { cli, name: NAMES[cli], info, update, hint: null, action: 'waiting', disabled: true }

    if (!authGranted.value) {
      // 页顶已有授权横幅，行内不重复提示
      return row
    }
    if (!info || info.status === 'idle' || info.status === 'detecting') {
      row.hint = t('hub.card.detecting')
      return row
    }
    if (info.status === 'error') {
      row.hint = info.error ?? t('hub.card.error')
      return row
    }
    if (!nodeReady.value) {
      row.hint = t('hub.inst.nodeGuide')
      return row
    }
    if (info.status === 'not-installed') {
      row.action = 'install'
      row.disabled = busy
      return row
    }
    // 已安装：opencode standalone 生效 → 手动提示；否则按 outdated 出按钮
    if (cli === 'opencode' && info.method === 'standalone') {
      row.action = 'manual'
      row.hint = t('hub.inst.manualHint')
      return row
    }
    if (update?.latest && update.outdated === false) {
      row.action = 'latest'
      row.disabled = busy
      return row
    }
    row.action = 'update'
    row.disabled = busy
    return row
  }),
)

// ==================== 输出控制台 ====================

const consoleEl = ref<HTMLElement | null>(null)
const consoleText = computed(() => {
  if (activeRun.value) return props.output
  return lastRun.value?.output ?? props.output
})
const consoleCommand = computed(() => activeRun.value?.command ?? lastRun.value?.command ?? null)
const consoleStatus = computed<'running' | 'ok' | 'error' | 'cancelled' | null>(() => {
  if (activeRun.value) return 'running'
  if (!lastRun.value) return null
  if (lastRun.value.ok) return 'ok'
  return lastRun.value.cancelled ? 'cancelled' : 'error'
})

// 输出增长时贴底滚动（终端回显惯例）
watch(
  () => consoleText.value,
  async () => {
    await nextTick()
    const el = consoleEl.value
    if (el) el.scrollTop = el.scrollHeight
  },
)
</script>

<template>
  <div class="ah-inst">
    <div v-if="!authGranted" class="ah-banner">
      <span class="ah-banner-ic">⚠</span>
      <span class="ah-banner-text">{{ t('hub.auth.banner') }}</span>
      <button type="button" class="ah-btn ah-btn-sm" @click="emit('auth')">
        {{ t('hub.auth.action') }}
      </button>
    </div>

    <!-- 镜像 / 测速 -->
    <div class="ah-card">
      <div class="ah-inst-head">
        <span class="ah-section-title">{{ t('hub.speed.title') }}</span>
        <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" :disabled="speedTesting" @click="emit('speed-test')">
          {{ speedTesting ? t('hub.speed.testing') : t('hub.speed.action') }}
        </button>
      </div>

      <div class="ah-speed-rows">
        <div class="ah-env-row">
          <span class="ah-env-label">{{ t('hub.speed.current') }}</span>
          <span class="ah-env-value ah-mono">{{ currentRegistry ?? '—' }}</span>
        </div>
        <div v-if="speed?.npmjsMs != null || speedFailed" class="ah-env-row">
          <span class="ah-env-label">{{ t('hub.speed.official') }}</span>
          <span class="ah-env-value ah-mono">{{ speed?.npmjsMs != null ? `${speed.npmjsMs} ms` : t('hub.speed.fail') }}</span>
        </div>
        <div v-if="speed?.npmmirrorMs != null || speedFailed" class="ah-env-row">
          <span class="ah-env-label">{{ t('hub.speed.mirror') }}</span>
          <span class="ah-env-value ah-mono">{{ speed?.npmmirrorMs != null ? `${speed.npmmirrorMs} ms` : t('hub.speed.fail') }}</span>
        </div>
      </div>

      <div v-if="speed?.error" class="ah-cli-error">{{ speed.error }}</div>

      <div v-if="speedDone || npmrc?.backupExists" class="ah-speed-actions">
        <span v-if="recommendLabel" class="ah-cli-tag" :class="speed?.recommend === 'npmmirror' ? 'warn' : 'ok'">
          <span class="ah-cli-dot"></span>{{ recommendLabel }}
        </span>
        <span class="ah-speed-actions-btns">
          <button
            v-if="persistVisible"
            type="button"
            class="ah-btn ah-btn-ghost ah-btn-sm"
            :class="{ 'ah-btn-warn': confirmPending }"
            :disabled="!authGranted"
            @click="onPersistClick"
          >
            {{ confirmPending ? t('hub.mirror.persistConfirm') : t('hub.mirror.persist') }}
          </button>
          <button
            v-if="npmrc?.backupExists"
            type="button"
            class="ah-btn ah-btn-ghost ah-btn-sm"
            :disabled="!authGranted"
            @click="emit('restore')"
          >
            {{ t('hub.mirror.restore') }}
          </button>
        </span>
      </div>
    </div>

    <!-- CLI 安装 / 更新 -->
    <div class="ah-card">
      <div class="ah-inst-head">
        <span class="ah-section-title">{{ t('hub.inst.title') }}</span>
        <button
          type="button"
          class="ah-btn ah-btn-ghost ah-btn-sm"
          :disabled="checking || activeRun !== null"
          @click="emit('check-updates')"
        >
          {{ checking ? t('hub.inst.checking') : t('hub.inst.check') }}
        </button>
      </div>

      <label class="ah-mirror-tmp">
        <input v-model="useMirror" type="checkbox" />
        <span>{{ t('hub.mirror.tmp') }}</span>
      </label>

      <div class="ah-inst-rows">
        <div v-for="row in rows" :key="row.cli" class="ah-inst-row">
          <span class="ah-cli-name">
            <CliIcon :cli-id="row.cli" />
            <span class="ah-cli-name-text">{{ row.name }}</span>
          </span>
          <span class="ah-inst-versions ah-mono">
            {{ row.info?.version ?? '—' }}
            <template v-if="row.update?.latest">
              → {{ row.update.latest }}
              <span v-if="row.update.outdated" class="ah-inst-outdated">↓</span>
            </template>
          </span>
          <span class="ah-inst-action">
            <span v-if="row.hint" class="ah-inst-hint">{{ row.hint }}</span>
            <button
              v-if="row.action === 'install'"
              type="button"
              class="ah-btn ah-btn-primary ah-btn-sm"
              :disabled="row.disabled || !authGranted"
              @click="emit('install', row.cli, useMirror)"
            >
              {{ t('hub.inst.install') }}
            </button>
            <button
              v-else-if="row.action === 'update'"
              type="button"
              class="ah-btn ah-btn-ghost ah-btn-sm"
              :disabled="row.disabled || !authGranted"
              @click="emit('install', row.cli, useMirror)"
            >
              {{ t('hub.inst.update') }}
            </button>
            <span v-else-if="row.action === 'latest'" class="ah-cli-tag ok">
              <span class="ah-cli-dot"></span>{{ t('hub.inst.latest') }}
            </span>
            <span v-else-if="row.action === 'waiting' && nodeReady" class="ah-cli-tag">
              <span class="ah-cli-dot"></span>{{ t('hub.card.detecting') }}
            </span>
          </span>
          <!-- node 缺失降级：展示白名单解析命令 + 复制（spec §4.2），整行第二行 -->
          <div v-if="!nodeReady && displayCommands[row.cli]" class="ah-inst-degraded">
            <span class="ah-mono ah-inst-degraded-cmd">{{ displayCommands[row.cli] }}</span>
            <button
              type="button"
              class="ah-btn ah-btn-ghost ah-btn-sm"
              @click="copyCommand(row.cli, displayCommands[row.cli]!)"
            >
              {{ copiedCli === row.cli ? t('hub.inst.copied') : t('hub.inst.copy') }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- 执行输出控制台 -->
    <div v-if="consoleStatus" class="ah-card">
      <div class="ah-inst-head">
        <span class="ah-section-title">{{ t('hub.inst.console') }}</span>
        <span class="ah-speed-actions-btns">
          <span class="ah-cli-tag" :class="{ ok: consoleStatus === 'ok', warn: consoleStatus === 'running', err: consoleStatus === 'error' }">
            <span class="ah-cli-dot"></span>
            {{ consoleStatus === 'running' ? t('hub.inst.running') : consoleStatus === 'ok' ? t('hub.inst.done') : consoleStatus === 'cancelled' ? t('hub.inst.cancelled') : t('hub.inst.failed') }}
          </span>
          <button v-if="activeRun && !activeRun.cancelRequested" type="button" class="ah-btn ah-btn-ghost ah-btn-sm" @click="emit('cancel')">
            {{ t('hub.inst.cancel') }}
          </button>
        </span>
      </div>
      <div v-if="consoleCommand" class="ah-console-cmd ah-mono">$ {{ consoleCommand }}</div>
      <pre ref="consoleEl" class="ah-console ah-mono">{{ consoleText || t('hub.inst.outputEmpty') }}</pre>
      <div v-if="lastRun?.error" class="ah-cli-error">{{ lastRun.error }}</div>
    </div>
  </div>
</template>
