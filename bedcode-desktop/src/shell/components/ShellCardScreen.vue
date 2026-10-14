<template>
  <section
    v-if="activeKey"
    :key="activeKey"
    class="card-screen"
    :aria-label="state.cardTitleOf(activeKey)"
  >
    <!-- 应用卡：宿主给屏幕与闸门，内容挂插件注册视图（appbar 属于应用内容层） -->
    <template v-if="kind === 'app'">
      <div class="cardframe">
        <div class="appbar">
          <ShellAppTile
            v-if="app"
            :icon="appIcon(app)"
            :tone="appsStore.tones.value[app.id] ?? 'system'"
            :size="22"
          />
          <span class="nm truncate">
            {{ app?.name ?? '' }}
          </span>
          <span v-if="app && app.contributes?.commands?.length" class="sub shrink-0">
            {{ subText }}
          </span>
          <span class="grow"></span>
          <span
            v-if="app"
            class="shell-chip"
            :class="stateChipClass(displayState)"
            style="font-size: calc(11px * var(--ui-scale))"
          >
            {{ stateChipText(displayState) }}
          </span>
          <button
            v-if="isRunning && app"
            class="shell-mini danger"
            :title="t('shell.app.stop')"
            @click="state.stopApp(app.id)"
          >
            <ShellIcon name="stop" :size="12" /> {{ t('shell.app.stop') }}
          </button>
        </div>

        <!-- 审批态：审批屏卡（批准 = 整单裁决，不做逐位勾选） -->
        <div v-if="displayState === 'approval' && app" class="empty-state">
          <ShellIcon name="warn" :size="30" :stroke-width="1.4" />
          <div class="t-warn">{{ t('shell.app.approval.title') }}</div>
          <p class="note">
            {{ t('shell.app.approval.desc', { perms: app.permissions.length, risks: riskCount }) }}
          </p>
          <div class="flex gap-2">
            <button class="shell-btn ghost" @click="openPermsPage">
              <ShellIcon name="shield" :size="13" /> {{ t('shell.app.approval.viewPerms') }}
            </button>
            <button
              class="shell-btn"
              :disabled="appsStore.busyId.value === app.id"
              @click="state.approveAndStart(app.id)"
            >
              <ShellIcon name="check" :size="13" /> {{ t('shell.app.approval.approveStart') }}
            </button>
          </div>
        </div>

        <!-- error 态 -->
        <div v-else-if="displayState === 'error' && app" class="empty-state">
          <ShellIcon name="warn" :size="30" :stroke-width="1.4" />
          <div class="t-warn">{{ t('shell.app.error.title') }}</div>
          <p class="note">{{ t('shell.app.error.desc') }}</p>
          <button class="shell-btn ghost" @click="goHome">
            {{ t('shell.app.error.backHome') }}
          </button>
        </div>

        <!-- dormant：未启动占位（点「启动并打开」） -->
        <div v-else-if="displayState === 'dormant' && app" class="empty-state">
          <ShellAppTile
            :icon="appIcon(app)"
            :tone="appsStore.tones.value[app.id] ?? 'system'"
            :size="44"
          />
          <p class="note">{{ t('shell.app.dormant.desc') }}</p>
          <button
            class="shell-btn"
            :disabled="appsStore.busyId.value === app.id"
            @click="state.openApp(app.id)"
          >
            <ShellIcon name="play" :size="13" /> {{ t('shell.app.dormant.start') }}
          </button>
        </div>

        <!-- 正常态：挂插件注册视图（PluginViewHost 只读承载；无视图时给占位） -->
        <div v-else class="appbody">
          <PluginViewHost
            v-if="viewSpec"
            :plugin-id="viewSpec.pluginId"
            :view-id="viewSpec.viewId"
          />
          <div v-else class="empty-state">
            <ShellAppTile
              v-if="app"
              :icon="appIcon(app)"
              :tone="appsStore.tones.value[app.id] ?? 'system'"
              :size="44"
            />
            <p class="note">{{ t('shell.app.noView.desc') }}</p>
            <button class="shell-btn ghost" @click="goHome">
              {{ t('shell.app.noView.backHome') }}
            </button>
          </div>
        </div>
      </div>
    </template>

    <!-- 系统页卡：页头 + 三页互切 tab（切换 = 激活另一张系统页卡，不弹栈——spec-c §1） -->
    <template v-else>
      <div class="sysframe">
        <div class="sys-head">
          <ShellAppTile :icon="sys.icon" tone="system" :size="34" />
          <div class="min-w-0">
            <div class="sys-t">{{ t(sys.nameKey) }}</div>
            <div class="note">{{ t(sys.descKey) }}</div>
          </div>
          <span class="grow"></span>
          <span class="shell-chip">{{ t('shell.sys.chip') }}</span>
        </div>
        <div class="sys-tabs" role="tablist">
          <button
            v-for="page in SYS_PAGES"
            :key="page.id"
            class="sys-tab"
            :class="{ on: page.id === sysPageId }"
            role="tab"
            :aria-selected="page.id === sysPageId"
            @click="state.openSys(page.id)"
          >
            <ShellIcon :name="page.icon" :size="13" /> {{ t(page.nameKey) }}
          </button>
        </div>
        <div class="sys-body">
          <div class="sys-wrap">
            <SysAppsView v-if="sysPageId === 'apps'" />
            <SysPermsView v-else-if="sysPageId === 'perms'" />
            <SysSettingsView v-else />
          </div>
        </div>
      </div>
    </template>
  </section>
</template>

<script setup lang="ts">
/**
 * ShellCardScreen — 卡片全屏（应用视图 / 系统页共用承载）
 *
 * 应用卡 = 宿主屏幕 + 闸门 + 插件注册视图挂载槽位；系统页卡 = 宿主自有管理面
 * （应用管理 / 权限与授权 / 设置）。appbar 属于应用内容层，不是窗口装饰。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import PluginViewHost from '@/plugin/components/PluginViewHost.vue'
import { appDisplayState, isRunningState } from '../state'
import { SYS_PAGES } from '../state'
import { useShellState } from '../useShellState'
import { useShellApps } from '../useShellApps'
import ShellIcon from './ShellIcon.vue'
import ShellAppTile from './ShellAppTile.vue'
import SysAppsView from '../sys/SysAppsView.vue'
import SysPermsView from '../sys/SysPermsView.vue'
import SysSettingsView from '../sys/SysSettingsView.vue'
import type { PluginInfo } from '@/plugin/types'

const { t } = useI18n()
const state = useShellState()
const appsStore = useShellApps()

const activeKey = computed(() => state.activeCard.value)
const kind = computed(() =>
  activeKey.value ? (activeKey.value.startsWith('app:') ? 'app' : 'sys') : 'sys',
)
const appId = computed(() =>
  activeKey.value ? activeKey.value.slice(activeKey.value.indexOf(':') + 1) : '',
)
const app = computed(() => appsStore.appOf(appId.value))
const displayState = computed(() => appDisplayState(app.value?.state))
const isRunning = computed(() => isRunningState(app.value?.state))

const sysPageId = computed(() => appId.value)
const sys = computed(() => SYS_PAGES.find((s) => s.id === sysPageId.value) ?? SYS_PAGES[0])

const riskCount = computed(() => (app.value ? appsStore.riskCount(app.value) : 0))
const subText = computed(() =>
  app.value
    ? t('shell.app.subCommands', { count: app.value.contributes?.commands?.length ?? 0 })
    : '',
)

/** 插件注册的第一个可挂载视图（sidebar / page 型，按注册顺序） */
const viewSpec = computed(() => {
  const a = app.value
  if (!a || !isRunning.value) return null
  const views = a.contributes?.views ?? []
  const first = views.find((v) => v.type === 'sidebar') ?? views.find((v) => v.type === 'toolbox')
  return first ? { pluginId: a.id, viewId: first.id } : null
})

function stateChipText(st: string): string {
  const map: Record<string, string> = {
    running: t('shell.state.running'),
    degraded: t('shell.state.degraded'),
    dormant: t('shell.state.dormant'),
    approval: t('shell.state.approval'),
    error: t('shell.state.error'),
  }
  return map[st] ?? st
}
function stateChipClass(st: string): string {
  const map: Record<string, string> = {
    running: 'ok',
    degraded: 'warn',
    approval: 'danger',
    error: 'danger',
    dormant: '',
  }
  return map[st] ?? ''
}

/** 应用 → 壳图标名（与主屏同一哈希，见 state.ts） */
function appIcon(a: PluginInfo): string {
  const SET = ['terminal', 'ai', 'transfer', 'hub', 'folder', 'bolt']
  let h = 0
  for (let i = 0; i < a.id.length; i++) h = (h * 31 + a.id.charCodeAt(i)) >>> 0
  return SET[h % SET.length]
}

function openPermsPage() {
  state.openSys('perms')
}
function goHome() {
  state.goHome()
}
</script>

<style scoped>
.card-screen {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.cardframe {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--bg-card);
}
.appbar {
  height: 44px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 14px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-card);
}
.appbar .nm {
  font-size: calc(13px * var(--ui-scale));
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.appbar .sub {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-tertiary);
}
.appbar .grow {
  flex: 1;
  min-width: 0;
}
.appbody {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

/* 空态 / 审批屏 */
.empty-state {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--text-tertiary);
  text-align: center;
  padding: 24px;
}
.empty-state .t-warn {
  font-size: calc(15px * var(--ui-scale));
  color: var(--color-warning);
  font-weight: 600;
}
.empty-state .note {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-tertiary);
  line-height: 1.55;
  max-width: 380px;
}

/* 系统页 */
.sysframe {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--bg-page);
}
.sys-head {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 16px 0;
}
.sys-head .sys-t {
  font-size: calc(16px * var(--ui-scale));
  font-weight: 650;
  color: var(--text-primary);
}
.sys-head .note {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-tertiary);
  line-height: 1.55;
}
.sys-tabs {
  flex-shrink: 0;
  display: flex;
  gap: 2px;
  padding: 8px 16px 0;
  border-bottom: 1px solid var(--border);
}
.sys-tab {
  height: 32px;
  padding: 0 12px;
  font-size: calc(12px * var(--ui-scale));
  color: var(--text-secondary);
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  display: flex;
  align-items: center;
  gap: 7px;
  transition:
    color 0.15s,
    border-color 0.15s;
}
.sys-tab:hover {
  color: var(--text-primary);
}
.sys-tab.on {
  color: var(--text-primary);
  border-bottom-color: var(--color-primary);
  font-weight: 600;
}
.sys-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 12px 16px 18px;
}
.sys-wrap {
  max-width: 880px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
</style>
