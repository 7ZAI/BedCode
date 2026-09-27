<script setup lang="ts">
/**
 * Agent Hub 面板（变体 B：顶部 pill 分段导航）
 *
 * 设计真源：用户评审定稿原型 `.scratch/agent-hub/prototype/index.html`
 * （#variant=b）。六分区中票据 02 交付概览、票据 03 交付安装与更新、
 * 票据 04 交付 Skills、票据 05 交付供应商、票据 06 交付使用统计与会话
 * 日志（07 收尾 opencode/codex）。useDetection / useInstall 均为单实例：
 * 事件订阅与输出轮询在此层持有，各分区经 props + emits 交互。
 *
 * 分区职责（统计改版后）：**统计 = 聚合看板**（汇总 / 趋势 / 节奏 / 分布），
 * **日志 = 会话级明细**（分页表格 + 二级详情）。两者不再展示同一份会话
 * 列表，因此本层不再持有 `goto-logs` 跳转。
 */
import { computed, inject, onMounted, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { useDetection } from '../composables/useDetection'
import { useInstall } from '../composables/useInstall'
import { useSkills } from '../composables/useSkills'
import { useProviders } from '../composables/useProviders'
import { useUsage } from '../composables/useUsage'
import type { CliId, HubTab } from '../types'
import OverviewTab from './OverviewTab.vue'
import InstallTab from './InstallTab.vue'
import SkillsTab from './SkillsTab.vue'
import ProvidersTab from './ProvidersTab.vue'
import StatsTab from './StatsTab.vue'
import SessionLogsTab from './SessionLogsTab.vue'

const context = inject<PluginContext>('pluginContext')!
const {
  state,
  detecting,
  refresh,
  detect,
  requestAuth,
} = useDetection(context)
const install = useInstall(context)
const skills = useSkills(context)
const providers = useProviders(context)
const usage = useUsage(context)

const activeTab = ref<HubTab>('overview')

/** 卸载失败信号（guest 拒绝/异常；瞬态提示，下次操作自动清除） */
const uninstallFailed = ref<CliId | null>(null)

/** 概览卡片卸载：发命令给 guest，失败则落瞬态提示（友好 i18n 在卡片层） */
async function handleUninstall(cli: CliId) {
  uninstallFailed.value = null
  const ok = await install.uninstall(cli)
  if (!ok) uninstallFailed.value = cli
}

const tabs = computed(() => [
  { id: 'overview' as const, label: context.i18n.t('hub.tab.overview') },
  { id: 'install' as const, label: context.i18n.t('hub.tab.install') },
  { id: 'skills' as const, label: context.i18n.t('hub.tab.skills') },
  { id: 'providers' as const, label: context.i18n.t('hub.tab.providers') },
  { id: 'logs' as const, label: context.i18n.t('hub.tab.logs') },
  { id: 'stats' as const, label: context.i18n.t('hub.tab.stats') },
])

onMounted(() => {
  refresh()
  install.refresh()
})

function gotoInstall() {
  activeTab.value = 'install'
}

/**
 * 各 CLI 的会话数据状态（票 07：概览卡片第六态「已装 · 未初始化」）
 *
 * 由 usage 域单实例计算后下传——统计与概览两个分区共享同一个 useUsage，
 * 不会重复拉取。未扫描 / 未授权 / 扫描中 → `unknown`（不下结论）。
 */
const CLI_IDS: CliId[] = ['claude', 'codex', 'opencode', 'pi']
const sessionStates = computed(() => {
  const out: Partial<Record<CliId, ReturnType<typeof usage.cliSessionState>>> = {}
  for (const id of CLI_IDS) out[id] = usage.cliSessionState(id)
  return out
})
</script>

<template>
  <div class="ah-view">
    <div class="ah-tabs" role="tablist">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        type="button"
        class="ah-tab"
        :class="{ active: activeTab === tab.id }"
        role="tab"
        :aria-selected="activeTab === tab.id"
        @click="activeTab = tab.id"
      >
        {{ tab.label }}
      </button>
    </div>

    <Transition name="ah-page" mode="out-in">
      <OverviewTab
      v-if="activeTab === 'overview'"
      :state="state"
      :detecting="detecting"
      :install-state="install.state.value"
      :speed-testing="install.speedTesting.value"
      :session-states="sessionStates"
      :uninstall-failed="uninstallFailed"
      @detect="detect"
      @auth="requestAuth"
      @speed-test="install.speedTest"
      @goto-install="gotoInstall"
      @uninstall="handleUninstall"
    />
    <InstallTab
      v-else-if="activeTab === 'install'"
      :detection="state"
      :state="install.state.value"
      :output="install.output.value"
      :checking="install.checking.value"
      :speed-testing="install.speedTesting.value"
      @speed-test="install.speedTest"
      @apply-mirror="install.applyMirror"
      @restore="install.restoreNpmrc"
      @check-updates="install.checkUpdates"
      @install="install.install"
      @cancel="install.cancelRun"
      @auth="requestAuth"
    />
    <SkillsTab v-else-if="activeTab === 'skills'" :detection="state" :skills="skills" />
    <ProvidersTab v-else-if="activeTab === 'providers'" :detection="state" :providers="providers" />
    <SessionLogsTab v-else-if="activeTab === 'logs'" :usage="usage" />
    <StatsTab v-else :usage="usage" />
    </Transition>
  </div>
</template>
