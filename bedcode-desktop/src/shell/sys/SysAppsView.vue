<template>
  <div class="flex flex-col gap-2.5">
    <!-- 排序说明（spec §9.1：始终允许最宽松置顶；运行中不可卸载；启用方向先过审批门） -->
    <ShellCard title="" :desc="t('shell.apps.sortHint')" />

    <div class="amgr">
      <!-- ======== 左列表（按授权风险排序） ======== -->
      <div class="alist" role="listbox" :aria-label="t('shell.apps.listAria')">
        <button
          v-for="app in sortedApps"
          :key="app.id"
          class="arow"
          :class="{ on: selected?.id === app.id }"
          role="option"
          :aria-selected="selected?.id === app.id"
          @click="appsStore.detailAppId.value = app.id"
        >
          <ShellAppTile
            :icon="shellIconForApp(app.id)"
            :tone="appsStore.tones.value[app.id] ?? 'system'"
            :size="30"
          />
          <span class="nm">
            <b class="truncate">{{ app.name }}</b>
            <span class="truncate"
              >{{ sourceLabel(app) }} · {{ sizeLabel(app) }} · {{ stateText(app) }}</span
            >
          </span>
          <span class="sdot" :class="appsStore.displayStates.value[app.id]"></span>
        </button>
        <button class="arow" @click="installPrompt">
          <span class="glyph-box tone-system shrink-0" style="width: 30px; height: 30px">
            <ShellIcon name="plus" :size="14" />
          </span>
          <span class="nm">
            <b>{{ t('shell.apps.install') }}</b>
            <span>{{ t('shell.apps.installDesc') }}</span>
          </span>
        </button>
      </div>

      <!-- ======== 右详情 ======== -->
      <div v-if="selected" class="adetail">
        <!-- 信息 -->
        <ShellCard :title="selected.name" :icon="shellIconForApp(selected.id)">
          <div class="pcard-head">
            <ShellAppTile
              :icon="shellIconForApp(selected.id)"
              :tone="appsStore.tones.value[selected.id] ?? 'system'"
              :size="34"
            />
            <div class="nm">
              <b class="truncate">{{ selected.id }} · v{{ selected.version }}</b>
              <span class="truncate">{{ selected.description }}</span>
            </div>
            <span class="shell-chip" :class="stateChipClass(displayState)">
              {{ stateChipText(displayState) }}
            </span>
          </div>

          <div class="grid2">
            <div class="shell-kv">
              <b>{{ t('shell.apps.kvSource') }}</b
              ><span>{{ sourceLabel(selected) }}</span>
            </div>
            <div class="shell-kv">
              <b>{{ t('shell.apps.kvSize') }}</b
              ><span>{{ sizeLabel(selected) }}</span>
            </div>
            <div class="shell-kv">
              <b>{{ t('shell.apps.kvViews') }}</b
              ><span>{{ selected.contributes?.views?.length ?? 0 }}</span>
            </div>
            <div class="shell-kv">
              <b>{{ t('shell.apps.kvType') }}</b
              ><span>{{ selected.pluginType }}</span>
            </div>
          </div>

          <!-- 生命周期 -->
          <div class="flex flex-wrap gap-2">
            <template v-if="displayState === 'approval'">
              <button
                class="shell-btn"
                :disabled="appsStore.busyId.value === selected.id"
                @click="state.approveAndStart(selected.id)"
              >
                <ShellIcon name="check" :size="13" /> {{ t('shell.apps.approveStart') }}
              </button>
            </template>
            <template v-else-if="isRunning">
              <button
                class="shell-btn ghost"
                :disabled="appsStore.busyId.value === selected.id"
                @click="state.stopApp(selected.id)"
              >
                <ShellIcon name="stop" :size="12" /> {{ t('shell.apps.stop') }}
              </button>
              <button class="shell-btn" @click="state.openApp(selected.id)">
                <ShellIcon name="play" :size="13" /> {{ t('shell.apps.open') }}
              </button>
            </template>
            <template v-else>
              <button
                class="shell-btn"
                :disabled="appsStore.busyId.value === selected.id"
                @click="state.openApp(selected.id)"
              >
                <ShellIcon name="play" :size="13" /> {{ t('shell.apps.startOpen') }}
              </button>
            </template>
            <button
              class="shell-btn danger-outline"
              :disabled="isRunning"
              :title="isRunning ? t('shell.apps.uninstallDisabled') : ''"
              @click="askUninstall(selected.id)"
            >
              <ShellIcon name="trash" :size="13" /> {{ t('shell.apps.uninstall') }}
            </button>
          </div>

          <div v-if="displayState === 'approval'" class="shell-callout danger">
            <ShellIcon name="warn" :size="14" />
            <span>{{
              t('shell.apps.approvalCallout', {
                perms: selected.permissions.length,
                risks: riskCount,
              })
            }}</span>
          </div>
        </ShellCard>

        <!-- 授权策略 -->
        <ShellCard :title="t('shell.apps.strategyTitle')" icon="shield">
          <div v-for="res in AUTH_RESOURCES" :key="res" class="strat-row">
            <span class="lb">
              <ShellIcon :name="res === 'fs' ? 'folder' : 'network'" :size="13" />
              {{ t('settings.authorization.resource.' + res) }}
            </span>
            <ShellSeg
              :options="strategyOptions"
              :model-value="strategyOf(authOf(selected.id), res)"
              risky
              risky-value="always_allow"
              @update:model-value="onSetStrategy(res, $event)"
            />
          </div>
          <p class="note">{{ t('shell.apps.strategyHint') }}</p>
        </ShellCard>

        <!-- 权限位（manifest 声明 · 只读） -->
        <ShellCard :title="t('shell.apps.permsTitle')" icon="layers">
          <div class="flex flex-wrap gap-1.5">
            <span
              v-for="p in selected.permissions"
              :key="p"
              class="shell-pill"
              :class="{ risk: isHighRisk(p) }"
            >
              {{ p }}
            </span>
          </div>
          <p class="note">{{ t('shell.apps.permsHint') }}</p>
        </ShellCard>

        <!-- 应用配置（manifest contributes.configuration 驱动的 schema 控件） -->
        <ShellCard :title="t('shell.apps.configTitle')" icon="gear">
          <template v-if="configSchema && Object.keys(configSchema.properties).length">
            <div class="flex flex-col gap-2.5">
              <div v-for="(prop, key) in configSchema.properties" :key="key" class="cfg-row">
                <span v-if="prop.type === 'boolean'" class="row-sw">
                  <span class="lb"
                    ><b>{{ prop.title }}</b
                    ><span class="mono">{{ key }}</span></span
                  >
                  <label class="shell-sw">
                    <input
                      type="checkbox"
                      :checked="configValues[key] === true"
                      :aria-label="prop.title"
                      @change="configValues[key] = !configValues[key]"
                    />
                    <span class="track"></span><span class="knob"></span>
                  </label>
                </span>
                <template v-else>
                  <span class="cfg-lb"
                    >{{ prop.title }} <span class="mono muted">{{ key }}</span></span
                  >
                  <select
                    v-if="prop.enum"
                    class="shell-inp"
                    :value="configValues[key] ?? prop.default"
                    :aria-label="prop.title"
                    @change="configValues[key] = ($event.target as HTMLSelectElement).value"
                  >
                    <option v-for="opt in prop.enum" :key="opt" :value="opt">{{ opt }}</option>
                  </select>
                  <input
                    v-else-if="prop.type === 'number' && prop.minimum !== undefined"
                    class="shell-inp"
                    type="number"
                    :min="prop.minimum"
                    :max="prop.maximum"
                    :value="configValues[key] ?? prop.default"
                    :aria-label="prop.title"
                    @input="configValues[key] = Number(($event.target as HTMLInputElement).value)"
                  />
                  <input
                    v-else
                    class="shell-inp"
                    :value="configValues[key] ?? prop.default"
                    :aria-label="prop.title"
                    @input="configValues[key] = ($event.target as HTMLInputElement).value"
                  />
                </template>
              </div>
            </div>
            <div class="flex gap-2">
              <button class="shell-mini" :disabled="savingConfig" @click="saveConfig">
                {{ t('shell.apps.configSave') }}
              </button>
              <button class="shell-mini" @click="loadConfigValues(true)">
                {{ t('shell.apps.configReset') }}
              </button>
            </div>
          </template>
          <p v-else class="note">{{ t('shell.apps.configEmpty') }}</p>
        </ShellCard>
      </div>
      <div v-else class="adetail">
        <div class="empty-note">{{ t('shell.apps.noSelection') }}</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * SysAppsView — 系统页「应用管理」（左列表 + 右详情，参考方案 B 管理中心形态）
 *
 * 数据源：useShellApps（plugin_list_loaded）+ plugin_auth_overview（风险排序 / 策略）。
 * 生命周期动作（启停 / 审批 / 卸载）与主屏同源（useShellState / useShellApps），
 * 通知语义一致：停用落事件通知、审批走整单裁决弹窗。
 */
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  pluginAuthOverview,
  pluginAuthSetStrategy,
  pluginStorageGet,
  pluginStorageSet,
  ensureHostCredential,
} from '@/plugin/commands'
import {
  AUTH_RESOURCES,
  STRATEGY_TIERS,
  strategyOf,
  type PluginAuthOverview,
} from '@/utils/authPolicy'
import { useToast } from '@/composables/useToast'
import { showUserError } from '@/utils/userError'
import { isHighRisk } from '../state'
import { isRunningState } from '../state'
import { useShellState } from '../useShellState'
import { useShellApps } from '../useShellApps'
import ShellIcon from '../components/ShellIcon.vue'
import ShellCard from '../components/ShellCard.vue'
import ShellAppTile from '../components/ShellAppTile.vue'
import ShellSeg from '../components/ShellSeg.vue'
import type { PluginConfiguration, PluginInfo } from '@/plugin/types'

const { t } = useI18n()
const state = useShellState()
const appsStore = useShellApps()
const toast = useToast()

const authOverviews = ref<PluginAuthOverview[]>([])
const configSchema = ref<PluginConfiguration | null>(null)
const configValues = ref<Record<string, any>>({})
const savingConfig = ref(false)

/** 按授权风险排序（始终允许置顶；同档按名称）——读模型为空时按安装顺序兜底 */
const sortedApps = computed<PluginInfo[]>(() => {
  const list = appsStore.plugins.value
  if (!authOverviews.value.length) return list
  const rank = new Map(authOverviews.value.map((a) => [a.pluginId, sortKey(a)]))
  return [...list].sort((x, y) => (rank.get(x.id) ?? 0) - (rank.get(y.id) ?? 0))
})

const selected = computed(() => appsStore.appOf(appsStore.detailAppId.value ?? ''))
const displayState = computed(() => appsStore.displayStates.value[selected.value?.id ?? ''])
const isRunning = computed(() => isRunningState(selected.value?.state))
const riskCount = computed(() => (selected.value ? appsStore.riskCount(selected.value) : 0))

function sortKey(a: PluginAuthOverview): number {
  // 与 authPolicy.riskRank 同口径（always_allow=0 置顶）
  const r = (res: string) =>
    strategyOf(a, res) === 'always_allow' ? 0 : strategyOf(a, res) === 'always_ask' ? 2 : 1
  return Math.min(r('fs'), r('network'))
}

function authOf(pluginId: string): PluginAuthOverview {
  return (
    authOverviews.value.find((a) => a.pluginId === pluginId) ?? {
      pluginId,
      name: '',
      strategies: [],
      records: [],
      firstPartyDirs: [],
    }
  )
}

const strategyOptions = computed(() =>
  STRATEGY_TIERS.map((s) => ({ value: s, label: t('settings.authorization.strategy.' + s) })),
)

async function onSetStrategy(resource: string, tier: string): Promise<void> {
  const app = selected.value
  if (!app) return
  if (tier === 'always_allow' && strategyOf(authOf(app.id), resource) !== 'always_allow') {
    state.dialog.value = 'strategy-confirm'
    state.dialogAppId.value = app.id
    state.dialogResource.value = resource
    return
  }
  try {
    await pluginAuthSetStrategy(app.id, resource, tier as never)
    toast.success(t('settings.authorization.strategyControl.saved'))
    await loadAuth()
  } catch (e) {
    showUserError(e)
  }
}

function sourceLabel(app: PluginInfo): string {
  const map: Record<string, string> = {
    builtin: t('shell.apps.source.builtin'),
    scanned: t('shell.apps.source.scanned'),
    wasm: t('shell.apps.source.wasm'),
    'user-installed': t('shell.apps.source.user'),
  }
  return map[app.source] ?? app.source
}
function sizeLabel(app: PluginInfo): string {
  const bytes = app.sizeBytes ?? 0
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`
  return `${bytes} B`
}
function stateText(app: PluginInfo): string {
  return stateChipText(appsStore.displayStates.value[app.id])
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

function shellIconForApp(pluginId: string): string {
  const SET = ['terminal', 'ai', 'transfer', 'hub', 'folder', 'bolt']
  let h = 0
  for (let i = 0; i < pluginId.length; i++) h = (h * 31 + pluginId.charCodeAt(i)) >>> 0
  return SET[h % SET.length]
}

/** 卸载二次确认（弹窗在 ShellDialogLayer） */
function askUninstall(pluginId: string) {
  state.dialog.value = 'uninstall'
  state.dialogAppId.value = pluginId
}

function installPrompt() {
  toast.info(t('shell.apps.installToast'))
}

async function loadAuth(): Promise<void> {
  try {
    authOverviews.value = await pluginAuthOverview()
  } catch (e) {
    showUserError(e)
  }
}

/** 配置：manifest contributes.configuration → 值读写插件存储 key 'config'（与 PluginConfigView 同通道） */
async function loadConfigValues(reset = false): Promise<void> {
  const app = selected.value
  if (!app) {
    configSchema.value = null
    return
  }
  configSchema.value = app.contributes?.configuration ?? null
  try {
    const saved = reset
      ? undefined
      : await pluginStorageGet(app.id, 'config', await ensureHostCredential())
    const defaults = Object.fromEntries(
      Object.entries(app.contributes?.configuration?.properties ?? {}).map(([k, p]) => [
        k,
        p.default,
      ]),
    )
    configValues.value = saved ? { ...defaults, ...saved } : defaults
  } catch (e) {
    showUserError(e)
  }
}

async function saveConfig(): Promise<void> {
  const app = selected.value
  if (!app || savingConfig.value) return
  savingConfig.value = true
  try {
    await pluginStorageSet(
      app.id,
      'config',
      { ...configValues.value },
      await ensureHostCredential(),
    )
    toast.success(t('shell.apps.configSaved'))
  } catch (e) {
    showUserError(e)
  } finally {
    savingConfig.value = false
  }
}

// 选中应用变化 → 重载配置
watch(
  () => appsStore.detailAppId.value,
  () => void loadConfigValues(),
)

onMounted(() => {
  void loadAuth()
  void loadConfigValues()
})
</script>

<style scoped>
.amgr {
  display: flex;
  gap: 10px;
  align-items: flex-start;
}
.alist {
  width: 240px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.arow {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 7px 9px;
  border-radius: var(--radius-button);
  border: 1px solid transparent;
  text-align: left;
  cursor: pointer;
  width: 100%;
  transition:
    background-color 0.15s,
    border-color 0.15s;
}
.arow:hover {
  background: var(--bg-hover);
}
.arow.on {
  background: var(--bg-card);
  border-color: var(--border-strong);
  box-shadow: var(--shadow-card);
}
.arow .nm {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.arow .nm b {
  font-size: calc(12px * var(--ui-scale));
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.arow .nm span {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.adetail {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.pcard-head {
  display: flex;
  align-items: center;
  gap: 10px;
}
.pcard-head .nm {
  flex: 1;
  min-width: 0;
}
.pcard-head .nm b {
  display: block;
  font-size: calc(13px * var(--ui-scale));
  font-weight: 600;
  color: var(--text-primary);
}
.pcard-head .nm span {
  display: block;
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-tertiary);
}
.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 4px 10px;
}
@media (max-width: 780px) {
  /* 结构性响应：左列表右详情塌成单列（列表在上、详情在下） */
  .amgr {
    flex-direction: column;
  }
  .alist {
    width: 100%;
    flex-direction: row;
    overflow-x: auto;
    padding-bottom: 4px;
  }
  .arow {
    width: auto;
    flex: 0 0 auto;
  }
  .arow .nm span {
    display: none;
  }
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
.note {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-tertiary);
  line-height: 1.55;
}
.empty-note {
  padding: 24px;
  text-align: center;
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-tertiary);
}
.cfg-row {
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.cfg-lb {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-secondary);
}
.cfg-row .row-sw {
  padding: 4px 0;
}
.cfg-row .row-sw .lb b {
  font-size: calc(12px * var(--ui-scale));
}
.mono {
  font-family: var(--font-mono, ui-monospace, 'SF Mono', Menlo, Consolas, monospace);
}
.muted {
  color: var(--text-tertiary);
}
.shell-inp {
  height: 28px;
  padding: 0 9px;
  background: var(--bg-input);
  border: 1px solid var(--border-input);
  border-radius: var(--radius-input);
  color: var(--text-primary);
  outline: none;
  font-size: calc(12px * var(--ui-scale));
  width: 100%;
  transition:
    border-color 0.2s,
    box-shadow 0.2s;
}
.shell-inp:focus {
  border-color: var(--border-strong);
  box-shadow: var(--shadow-input-focus);
}
</style>
