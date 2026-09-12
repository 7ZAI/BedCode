<script setup lang="ts">
/**
 * Agent Hub 面板（变体 B：顶部 pill 分段导航）
 *
 * 设计真源：用户评审定稿原型 `.scratch/agent-hub/prototype/index.html`
 * （#variant=b）。六分区中票据 02 交付概览、票据 03 交付安装与更新、
 * 票据 04 交付 Skills、票据 05 交付供应商、票据 06 交付使用统计与会话
 * 日志（07 收尾 opencode/codex）。useDetection / useInstall 均为单实例：
 * 事件订阅与输出轮询在此层持有，各分区经 props + emits 交互。
 */
import { computed, inject, onMounted, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { useDetection } from '../composables/useDetection'
import { useInstall } from '../composables/useInstall'
import { useSkills } from '../composables/useSkills'
import { useProviders } from '../composables/useProviders'
import { useUsage } from '../composables/useUsage'
import type { HubTab } from '../types'
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

const tabs = computed(() => [
  { id: 'overview' as const, label: context.i18n.t('hub.tab.overview') },
  { id: 'install' as const, label: context.i18n.t('hub.tab.install') },
  { id: 'skills' as const, label: context.i18n.t('hub.tab.skills') },
  { id: 'providers' as const, label: context.i18n.t('hub.tab.providers') },
  { id: 'stats' as const, label: context.i18n.t('hub.tab.stats') },
  { id: 'logs' as const, label: context.i18n.t('hub.tab.logs') },
])

onMounted(() => {
  refresh()
  install.refresh()
})

function gotoInstall() {
  activeTab.value = 'install'
}

/** 统计明细行点击 → 跳日志分区并打开该会话 */
function gotoLogs(sessionId: number) {
  activeTab.value = 'logs'
  void usage.openSession(sessionId)
}
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

    <OverviewTab
      v-if="activeTab === 'overview'"
      :state="state"
      :detecting="detecting"
      :install-state="install.state.value"
      :speed-testing="install.speedTesting.value"
      @detect="detect"
      @auth="requestAuth"
      @speed-test="install.speedTest"
      @goto-install="gotoInstall"
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
    <StatsTab v-else-if="activeTab === 'stats'" :usage="usage" @goto-logs="gotoLogs" />
    <SessionLogsTab v-else :usage="usage" />
  </div>
</template>
