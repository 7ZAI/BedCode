<template>
  <div class="flex flex-col gap-2.5">
    <!-- 待审批横幅（整单裁决入口；批准后审批通知自然消失——动态派生无生命周期） -->
    <div v-if="approvals.length" class="shell-callout warn">
      <ShellIcon name="warn" :size="14" />
      <span>
        <b>{{ t('shell.perms.pending', { count: approvals.length }) }}</b
        >： {{ approvals.map((a) => a.name).join('、') }}。
        {{ t('shell.perms.pendingHint') }}
      </span>
      <button
        class="shell-mini shrink-0 ml-auto"
        :disabled="appsStore.busyId.value !== null"
        @click="approveFirst"
      >
        {{ t('shell.perms.pendingGo') }}
      </button>
    </div>

    <!-- 加载骨架 -->
    <div v-if="loading && !overviews.length" class="flex flex-col gap-2.5">
      <div
        v-for="i in 3"
        :key="i"
        class="h-24 rounded-[10px] animate-pulse bg-[var(--bg-card)] border border-[var(--border)]"
      ></div>
    </div>

    <!-- 每应用授权卡 -->
    <ShellCard v-for="app in sortedOverviews" :key="app.pluginId" :title="app.name" icon="shield">
      <div class="sub-line">
        <span
          class="shell-chip"
          :class="stateChipClass(appsStore.displayStates.value[app.pluginId])"
        >
          {{ stateChipText(appsStore.displayStates.value[app.pluginId]) }}
        </span>
        <span class="sub-note">
          {{
            t('shell.perms.appMeta', {
              perms: grantedCount(app),
              risks: riskCountOf(app),
              records: app.records.length,
            })
          }}
        </span>
        <span class="grow"></span>
        <button class="shell-mini" @click="jumpManage(app.pluginId)">
          {{ t('shell.perms.manage') }}
        </button>
      </div>

      <!-- 策略分段控件（fs / network 两资源三档） -->
      <div v-for="res in AUTH_RESOURCES" :key="res" class="strat-row">
        <span class="lb">
          <ShellIcon :name="res === 'fs' ? 'folder' : 'network'" :size="13" />
          {{ t('settings.authorization.resource.' + res) }}
        </span>
        <ShellSeg
          :options="strategyOptions"
          :model-value="strategyOf(app, res)"
          risky
          risky-value="always_allow"
          @update:model-value="onSetStrategy(app, res, $event)"
        />
      </div>

      <!-- 四分区计数 -->
      <div class="flex flex-wrap items-center gap-1.5">
        <span class="shell-chip ok">{{
          t('shell.perms.secUser', { count: userGrantedRecords(app).length })
        }}</span>
        <span class="shell-chip warn">{{
          t('shell.perms.secAuto', { count: autoAllowedRecords(app).length })
        }}</span>
        <span class="shell-chip info">{{
          t('shell.perms.secFirstParty', { count: firstPartyDirsOf(app).length })
        }}</span>
        <span class="shell-chip danger">{{
          t('shell.perms.secDeny', { count: deniedRecords(app).length })
        }}</span>
      </div>

      <!-- 分区记录明细（复用既有四分区组件，展示口径与设置页逐字一致） -->
      <AuthSection
        :title="t('settings.authorization.sections.userGranted')"
        :empty-text="t('settings.authorization.sections.empty')"
        :has-records="userGrantedRecords(app).length > 0"
      >
        <AuthRecordRow
          v-for="r in userGrantedRecords(app)"
          :key="r.id"
          :record="r"
          :busy="busyKey === recKey(app, r)"
          @revoke="revokeRecord(app, $event)"
        />
      </AuthSection>
      <AuthSection
        :title="t('settings.authorization.sections.autoAllowed')"
        :empty-text="t('settings.authorization.sections.empty')"
        :has-records="autoAllowedRecords(app).length > 0"
      >
        <AuthRecordRow
          v-for="r in autoAllowedRecords(app)"
          :key="r.id"
          :record="r"
          unconfirmed
          :busy="busyKey === recKey(app, r)"
          @revoke="revokeRecord(app, $event)"
        />
      </AuthSection>
      <AuthSection
        :title="t('settings.authorization.sections.firstParty')"
        :empty-text="t('settings.authorization.sections.firstPartyEmpty')"
        :has-records="firstPartyDirsOf(app).length > 0"
      >
        <AuthFirstPartyRow
          v-for="e in firstPartyDirsOf(app)"
          :key="e.kind + e.value"
          :entry="e"
          :busy="busyKey === fpKey(app, e)"
          @revoke="revokeFirstParty(app, $event)"
        />
      </AuthSection>
      <AuthSection
        :title="t('settings.authorization.sections.denied')"
        :empty-text="t('settings.authorization.sections.empty')"
        :has-records="deniedRecords(app).length > 0"
      >
        <AuthRecordRow
          v-for="r in deniedRecords(app)"
          :key="r.id"
          :record="r"
          :busy="busyKey === recKey(app, r)"
          @revoke="removeDeny(app, $event)"
        />
      </AuthSection>
    </ShellCard>
  </div>
</template>

<script setup lang="ts">
/**
 * SysPermsView — 系统页「权限与授权」
 *
 * 数据源：plugin_auth_overview（策略 + 记录 + 第一方免询问清单）+ 壳应用注册表投影。
 * 分区口径 / 排序 / 撤销语义全部复用 authPolicy.ts 与既有组件（AuthSection /
 * AuthRecordRow / AuthFirstPartyRow），与设置页「应用授权」逐字一致——安全闸门的
 * 呈现必须单一口径（spec §9.2 四分区互斥完备）。
 */
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  pluginAuthOverview,
  pluginAuthRemoveRecord,
  pluginAuthRevoke,
  pluginAuthSetStrategy,
} from '@/plugin/commands'
import {
  AUTH_RESOURCES,
  STRATEGY_TIERS,
  autoAllowedRecords,
  deniedRecords,
  firstPartyDirsOf,
  sortAppsByRisk,
  strategyOf,
  userGrantedRecords,
  type AuthRecord,
  type FirstPartyDirEntry,
  type PluginAuthOverview,
} from '@/utils/authPolicy'
import { useToast } from '@/composables/useToast'
import { showUserError } from '@/utils/userError'
import { useShellState } from '../useShellState'
import { useShellApps } from '../useShellApps'
import ShellIcon from '../components/ShellIcon.vue'
import ShellCard from '../components/ShellCard.vue'
import ShellSeg from '../components/ShellSeg.vue'
import AuthSection from '@/components/AuthSection.vue'
import AuthRecordRow from '@/components/AuthRecordRow.vue'
import AuthFirstPartyRow from '@/components/AuthFirstPartyRow.vue'

const { t } = useI18n()
const state = useShellState()
const appsStore = useShellApps()
const toast = useToast()

const overviews = ref<PluginAuthOverview[]>([])
const loading = ref(false)
const busyKey = ref<string | null>(null)

const approvals = computed(() => appsStore.approvals.value)

/** 按风险排序（始终允许置顶；同档按名称，顺序确定） */
const sortedOverviews = computed(() => sortAppsByRisk(overviews.value))

const strategyOptions = computed(() =>
  STRATEGY_TIERS.map((s) => ({ value: s, label: t('settings.authorization.strategy.' + s) })),
)

async function load(): Promise<void> {
  loading.value = true
  try {
    overviews.value = await pluginAuthOverview()
  } catch (e) {
    showUserError(e)
  } finally {
    loading.value = false
  }
}

/** 策略切换：切「始终允许」走二次确认（弹窗在 ShellDialogLayer）；其余直写 */
async function onSetStrategy(
  app: PluginAuthOverview,
  resource: string,
  tier: string,
): Promise<void> {
  if (tier === 'always_allow' && strategyOf(app, resource) !== 'always_allow') {
    state.dialog.value = 'strategy-confirm'
    state.dialogAppId.value = app.pluginId
    state.dialogResource.value = resource
    return
  }
  try {
    await pluginAuthSetStrategy(app.pluginId, resource, tier as never)
    toast.success(t('settings.authorization.strategyControl.saved'))
    await load()
  } catch (e) {
    showUserError(e)
  }
}

async function revokeRecord(app: PluginAuthOverview, record: AuthRecord): Promise<void> {
  busyKey.value = recKey(app, record)
  try {
    await pluginAuthRevoke(app.pluginId, record.resource, record.target)
    toast.success(
      t(
        record.resource === 'network'
          ? 'settings.authorization.records.networkRevoked'
          : 'settings.authorization.records.revoked',
      ),
    )
    await load()
  } catch (e) {
    showUserError(e)
  } finally {
    busyKey.value = null
  }
}

async function removeDeny(app: PluginAuthOverview, record: AuthRecord): Promise<void> {
  busyKey.value = recKey(app, record)
  try {
    await pluginAuthRemoveRecord(app.pluginId, record.resource, record.target)
    toast.success(t('settings.authorization.records.denyRemoved'))
    await load()
  } catch (e) {
    showUserError(e)
  } finally {
    busyKey.value = null
  }
}

/** 内置免询问撤销：target 传 `~/` 前缀形态，宿主展开为绝对路径（见 AuthorizationView 同口径） */
async function revokeFirstParty(app: PluginAuthOverview, entry: FirstPartyDirEntry): Promise<void> {
  if (entry.kind !== 'home') return
  busyKey.value = fpKey(app, entry)
  try {
    await pluginAuthRevoke(app.pluginId, 'fs', `~/${entry.value}`)
    toast.success(t('settings.authorization.sections.firstPartyRevoked'))
    await load()
  } catch (e) {
    showUserError(e)
  } finally {
    busyKey.value = null
  }
}

function recKey(app: PluginAuthOverview, r: AuthRecord): string {
  return `${app.pluginId}:${r.resource}:${r.target}:${r.id}`
}
function fpKey(app: PluginAuthOverview, e: FirstPartyDirEntry): string {
  return `fp:${app.pluginId}:${e.kind}:${e.value}`
}

function grantedCount(app: PluginAuthOverview): number {
  return appsStore.appOf(app.pluginId)?.permissions.length ?? 0
}
function riskCountOf(app: PluginAuthOverview): number {
  const perms = appsStore.appOf(app.pluginId)?.permissions ?? []
  return perms.filter(
    (p) =>
      p === 'process:run' || p === 'pty:spawn' || p === 'terminal:input' || p === 'database:main',
  ).length
}

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

/** 待审批：去审批 = 打开第一个待审批应用的审批弹窗 */
function approveFirst() {
  const first = approvals.value[0]
  if (!first) return
  state.dialog.value = 'approval'
  state.dialogAppId.value = first.id
}

function jumpManage(pluginId: string) {
  state.openSys('apps')
  appsStore.detailAppId.value = pluginId
}

onMounted(load)
</script>

<style scoped>
.sub-line {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.sub-line .sub-note {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-tertiary);
}
.sub-line .grow {
  flex: 1;
}
.strat-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 6px 0;
}
.strat-row + .strat-row {
  border-top: 1px solid var(--border);
}
.strat-row .lb {
  font-size: calc(12px * var(--ui-scale));
  width: 92px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--text-primary);
}
</style>
