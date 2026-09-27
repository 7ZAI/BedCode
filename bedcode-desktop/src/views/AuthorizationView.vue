<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左返回+标题+应用数，右刷新 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-2.5 min-w-0">
        <button
          class="h-7 w-7 rounded-[6px] border border-[var(--border)] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors shrink-0"
          :title="$t('settings.authorization.back')"
          @click="goBack"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.75"
              d="M15 19l-7-7 7-7"
            />
          </svg>
        </button>
        <h1
          class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)] truncate"
        >
          {{ $t('settings.authorization.title') }}
        </h1>
        <span
          v-if="!loading"
          class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] shrink-0"
        >
          {{ $t('settings.authorization.appCount', { count: apps.length }) }}
        </span>
      </div>
      <div class="flex items-center gap-2">
        <button class="wb-btn-ghost" :disabled="loading" @click="load()">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.75"
              d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
            />
          </svg>
          {{ $t('settings.authorization.refresh') }}
        </button>
      </div>
    </div>

    <!-- ==================== 内容区：全部已安装 wasm 应用（按风险排序） ==================== -->
    <div class="flex-1 overflow-auto p-5">
      <div class="max-w-3xl mx-auto">
        <!-- 加载态：骨架（首次进入无数据时才显示，刷新时保留旧列表避免跳动） -->
        <div v-if="loading && apps.length === 0" class="space-y-2">
          <div
            v-for="i in 3"
            :key="i"
            class="h-14 rounded-[10px] animate-pulse bg-[var(--bg-card)] border border-[var(--border)]"
          ></div>
        </div>

        <!-- 空态：没有可管理的应用 -->
        <div v-else-if="apps.length === 0" class="py-12 text-center">
          <p class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)]">
            {{ $t('settings.authorization.empty') }}
          </p>
          <p class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] mt-1">
            {{ $t('settings.authorization.emptyHint') }}
          </p>
        </div>

        <!-- 应用列表：始终允许（免询问自动放行）置顶 -->
        <div
          v-else
          class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)] overflow-hidden"
        >
          <!-- data-testid：渲染契约测试的行锚点（本页唯一的测试钩子） -->
          <div v-for="app in sortedApps" :key="app.pluginId" data-testid="auth-app-row">
            <div class="flex items-center gap-3 px-4 py-3">
              <PluginIcon :name="app.name" :plugin-id="app.pluginId" />
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span
                    class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate"
                  >
                    {{ app.name }}
                  </span>
                  <span
                    class="wb-mono text-[calc(10px*var(--ui-scale))] text-[var(--text-tertiary)] truncate"
                  >
                    {{ app.pluginId }}
                  </span>
                </div>
                <!-- 本阶段只呈现档位徽标与记录数，策略控件在票 03 接入 -->
                <div class="mt-1.5 flex items-center gap-x-4 gap-y-1 flex-wrap">
                  <span
                    v-for="resource in AUTH_RESOURCES"
                    :key="resource"
                    class="inline-flex items-center gap-1.5"
                  >
                    <span class="text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)]">
                      {{ $t(`settings.authorization.resource.${resource}`) }}
                    </span>
                    <span
                      class="px-1.5 py-0.5 rounded text-[calc(10px*var(--ui-scale))] font-medium shrink-0"
                      :class="strategyClass(strategyOf(app, resource))"
                    >
                      {{ $t(`settings.authorization.strategy.${strategyOf(app, resource)}`) }}
                    </span>
                    <span class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">
                      {{
                        $t('settings.authorization.recordCount', {
                          count: recordCount(app, resource),
                        })
                      }}
                    </span>
                  </span>
                </div>
              </div>
              <button
                class="w-5 h-5 flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors shrink-0"
                :aria-expanded="expanded[app.pluginId] === true"
                :title="$t('settings.authorization.records.toggle')"
                @click="toggleExpanded(app.pluginId)"
              >
                <svg
                  class="w-3.5 h-3.5 transition-transform duration-200"
                  :class="{ 'rotate-90': expanded[app.pluginId] }"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M9 5l7 7-7 7"
                  />
                </svg>
              </button>
            </div>

            <!-- 展开：策略控件 + 按资源分区列记录 + 逐条撤销（票 02 文件 / 票 05 网络） -->
            <div v-if="expanded[app.pluginId]" class="px-4 pb-3 space-y-2">
              <!-- 策略档位（票 03）：三档都展示，本票只放开「默认」与「总是询问」；
                   「始终允许」的免询问放行语义属票 04，禁用并在 title 里说明原因 -->
              <div
                v-for="resource in AUTH_RESOURCES"
                :key="`strategy-${resource}`"
                class="rounded-[8px] border border-[var(--border)] bg-[var(--bg-page)] px-3 py-2.5"
              >
                <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">
                  {{
                    $t('settings.authorization.strategyControl.title', {
                      resource: $t(`settings.authorization.resource.${resource}`),
                    })
                  }}
                </p>
                <div class="mt-1.5 flex items-center gap-1.5 flex-wrap">
                  <button
                    v-for="tier in STRATEGY_TIERS"
                    :key="`${resource}-${tier}`"
                    data-testid="auth-strategy-option"
                    :data-resource="resource"
                    :data-tier="tier"
                    :disabled="busyStrategy === strategyKey(app, resource)"
                    :title="$t(`settings.authorization.strategyControl.hint.${tier}`)"
                    class="shrink-0 h-6 px-2 rounded-[6px] border text-[calc(11px*var(--ui-scale))] transition-colors disabled:opacity-50"
                    :class="
                      tier === strategyOf(app, resource)
                        ? 'border-[var(--color-primary)] bg-[var(--bg-hover)] text-[var(--text-primary)]'
                        : 'border-[var(--border)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]'
                    "
                    @click="requestStrategy(app, resource, tier)"
                  >
                    {{ $t(`settings.authorization.strategy.${tier}`) }}
                  </button>
                </div>
              </div>

              <div
                v-for="resource in RECORD_RESOURCES"
                :key="resource"
                class="rounded-[8px] border border-[var(--border)] bg-[var(--bg-page)] px-3 py-2.5"
              >
                <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">
                  {{ $t(`settings.authorization.records.${resource}Title`) }}
                </p>
                <p
                  v-if="resourceRecords(app, resource).length === 0"
                  class="mt-1.5 text-[calc(11px*var(--ui-scale)] text-[var(--text-tertiary)]"
                >
                  {{ $t(`settings.authorization.records.${resource}Empty`) }}
                </p>
                <div v-else class="mt-1.5 space-y-1.5">
                  <div
                    v-for="record in resourceRecords(app, resource)"
                    :key="record.id"
                    data-testid="auth-record-row"
                    class="flex items-center gap-2"
                  >
                    <span
                      class="wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-primary)] break-all flex-1 min-w-0"
                    >
                      {{ record.target }}
                    </span>
                    <!-- 免询问自动放行记录必须带「未经确认」标记并与用户确认记录视觉区分
                         （spec §9.4；与详情页 AuthRecordRow 同一口径） -->
                    <span
                      v-if="isUnconfirmed(record)"
                      class="shrink-0 px-1.5 py-0.5 rounded text-[calc(10px*var(--ui-scale))] font-medium bg-amber-50 dark:bg-amber-500/10 text-amber-600 dark:text-amber-400"
                    >
                      {{ $t('settings.authorization.sections.unconfirmed') }}
                    </span>
                    <span
                      class="shrink-0 px-1.5 py-0.5 rounded text-[calc(10px*var(--ui-scale))] font-medium"
                      :class="effectClass(record.effect)"
                    >
                      {{ effectLabel(record.effect) }}
                    </span>
                    <span
                      v-if="opsLabel(record.ops)"
                      class="shrink-0 px-1.5 py-0.5 rounded text-[calc(10px*var(--ui-scale))] bg-[var(--bg-hover)] text-[var(--text-secondary)]"
                    >
                      {{ opsLabel(record.ops) }}
                    </span>
                    <span
                      class="shrink-0 px-1.5 py-0.5 rounded text-[calc(10px*var(--ui-scale))] bg-[var(--bg-hover)] text-[var(--text-tertiary)]"
                    >
                      {{ sourceLabel(record.source) }}
                    </span>
                    <!-- 撤销（删 allow + 落 deny）/ 移除 deny（spec §8.4 的两种出口） -->
                    <button
                      class="shrink-0 h-6 px-2 rounded-[6px] border border-[var(--border-strong)] text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors disabled:opacity-50"
                      :disabled="busyKey === recordKey(app, record)"
                      @click="isDeny(record) ? removeDeny(app, record) : revoke(app, record)"
                    >
                      {{
                        isDeny(record)
                          ? $t('settings.authorization.records.removeDeny')
                          : $t('settings.authorization.records.revoke')
                      }}
                    </button>
                  </div>
                </div>
              </div>

              <!-- 内置免询问分区（票 08，spec §7「必须配套」的可见性补救）：不列出来
                   就是用户看不见的特权——看目录清单会以为该应用只能碰自己授权过的地方。
                   行标记与详情页共用 AuthFirstPartyRow（两处各写一份必然漂移），
                   撤销同样落一条 deny 记录。 -->
              <div
                data-testid="auth-first-party-section"
                class="rounded-[8px] border border-[var(--border)] bg-[var(--bg-page)] px-3 py-2.5"
              >
                <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">
                  {{ $t('settings.authorization.sections.firstParty') }}
                </p>
                <p
                  v-if="firstPartyDirsOf(app).length === 0"
                  class="mt-1.5 text-[calc(11px*var(--ui-scale)] text-[var(--text-tertiary)]"
                >
                  {{ $t('settings.authorization.sections.firstPartyEmpty') }}
                </p>
                <div v-else class="mt-1">
                  <AuthFirstPartyRow
                    v-for="entry in firstPartyDirsOf(app)"
                    :key="entry.kind + entry.value"
                    :entry="entry"
                    :busy="busyKey === firstPartyKey(app, entry)"
                    @revoke="revokeFirstParty(app, $event)"
                  />
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 切到「始终允许」的二次确认（票 04）：该档是风险最高的一档，必须让用户在
         改档前看到语义边界（spec §4.3）——切档不等于一次性全盘授权，但访问会累积 -->
    <Modal
      :model-value="confirmTarget !== null"
      size="sm"
      :title="$t('settings.authorization.strategyControl.confirmTitle')"
      @update:model-value="
        (visible: boolean) => {
          if (!visible) cancelAlwaysAllow()
        }
      "
    >
      <p
        data-testid="auth-strategy-confirm"
        class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] leading-relaxed"
      >
        {{
          $t('settings.authorization.strategyControl.confirmBody', {
            name: confirmTarget?.app.name ?? '',
            resource: $t(`settings.authorization.resource.${confirmTarget?.resource ?? 'fs'}`),
          })
        }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-2">
          <Button
            variant="ghost"
            size="sm"
            data-testid="auth-strategy-confirm-cancel"
            @click="cancelAlwaysAllow"
          >
            {{ $t('common.button.cancel') }}
          </Button>
          <Button
            variant="danger"
            size="sm"
            data-testid="auth-strategy-confirm-ok"
            @click="confirmAlwaysAllow"
          >
            {{ $t('settings.authorization.strategyControl.confirmOk') }}
          </Button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
/**
 * AuthorizationView — 设置页「应用授权」二级页（授权策略增强 · 票 01 / 02 / 03 / 04 / 05）
 *
 * 列出全部已安装 wasm 应用及其授权概览：每类受管资源（文件 / 网络）的策略档位徽标
 * + 记录条数，按风险排序（始终允许置顶，spec §9.1）。
 *
 * 数据源是宿主唯一读模型命令 `plugin_auth_overview`（spec §9.3）：本页不另写查询。
 * 每行可展开：**策略档位控件**（票 03 三档，票 04 起「始终允许」可选但需二次确认）+
 * 按资源分区列记录并逐条管理（票 02 文件目录 / 票 05 网络 origin）：
 * - 「取消授权」= 删 allow 记录 + 落一条 deny（该目标后续访问被直接拒绝，spec §8.4）；
 * - 「移除拒绝」= 只删 deny，回到「默认」档的未覆盖状态（同一节要求的另一种出口）。
 *
 * 归属说明：授权是引擎侧安全闸门事实（ADR 0022 §5.1.3），管理入口按用户裁定放在
 * 设置页二级位置，不占一级菜单（一般用户不关心权限，权限不是业务）。
 */
import { computed, onActivated, onMounted, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import Button from '@/components/Button.vue'
import Modal from '@/components/Modal.vue'
import PluginIcon from '@/components/PluginIcon.vue'
import AuthFirstPartyRow from '@/components/AuthFirstPartyRow.vue'
import {
  pluginAuthOverview,
  pluginAuthRemoveRecord,
  pluginAuthRevoke,
  pluginAuthSetStrategy,
} from '@/plugin/commands'
import { useToast } from '@/composables/useToast'
import { showUserError } from '@/utils/userError'
import {
  AUTH_RESOURCES,
  STRATEGY_TIERS,
  effectKeySuffix,
  firstPartyDirsOf,
  opsKeySuffix,
  recordCount,
  recordsOf,
  sortAppsByRisk,
  sourceKeySuffix,
  strategyOf,
  type AuthRecord,
  type AuthStrategy,
  type FirstPartyDirEntry,
  type PluginAuthOverview,
} from '@/utils/authPolicy'

/**
 * 展开面板里逐个渲染的资源分区（i18n key 后缀用：`<resource>Title` / `<resource>Empty`）
 *
 * 顺序 = `AUTH_RESOURCES`（文件在前、网络在后）：两类分区的行结构完全相同，
 * 共用同一段标记（`record.target` / effect / ops / source + 逐条操作），
 * 网络记录的 `ops` 恒空 → 徽标自动不渲染（`opsLabel` 返回空串）。
 */
const RECORD_RESOURCES = AUTH_RESOURCES

const router = useRouter()
const { t } = useI18n()
const toast = useToast()

/** 读模型数据（已按 plugin_id 去重，宿主保证一个应用一条） */
const apps = ref<PluginAuthOverview[]>([])
const loading = ref(false)

/** 按风险排序的展示序列（始终允许置顶；排序语义见 utils/authPolicy） */
const sortedApps = computed(() => sortAppsByRisk(apps.value))

/** 返回设置页（本页是设置页的二级入口，返回目标确定，不用 history.back） */
function goBack(): void {
  router.push({ name: 'settings' })
}

/** 档位徽标样式：始终允许用警示色（风险提示），其余中性偏弱化 */
function strategyClass(strategy: AuthStrategy): string {
  if (strategy === 'always_allow') {
    return 'bg-amber-50 dark:bg-amber-500/10 text-amber-600 dark:text-amber-400'
  }
  if (strategy === 'always_ask') {
    return 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
  }
  return 'bg-[var(--bg-hover)] text-[var(--text-tertiary)]'
}

/** 记录效果徽标样式：硬拒绝用警示色（与档位徽标同一套色彩语义） */
function effectClass(effect: string): string {
  if (effectKeySuffix(effect) === 'deny') {
    return 'bg-amber-50 dark:bg-amber-500/10 text-amber-600 dark:text-amber-400'
  }
  return 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
}

async function load(): Promise<void> {
  loading.value = true
  try {
    apps.value = await pluginAuthOverview()
  } catch (e) {
    // 票 01（ADR 0030）：统一消费层——友好文案 + 日志，永不渲染错误原文
    showUserError(e)
  } finally {
    loading.value = false
  }
}

// ==================== 授权记录展开与逐条管理（票 02） ====================

/** 展开状态（每个应用独立；默认收起——列表首屏只回答「谁有什么档位 / 几条记录」） */
const expanded = reactive<Record<string, boolean>>({})

/** 正在执行的记录操作 key（同一行按钮防重复点击） */
const busyKey = ref<string | null>(null)

/** 正在提交的档位设置 key（应用 × 资源，防重复点击） */
const busyStrategy = ref<string | null>(null)

function toggleExpanded(pluginId: string): void {
  expanded[pluginId] = !expanded[pluginId]
}

// ==================== 策略档位（票 03 / 04） ====================

/** 档位控件的操作 key（应用 × 资源） */
function strategyKey(app: PluginAuthOverview, resource: string): string {
  return `${app.pluginId}:${resource}`
}

/**
 * 待二次确认的档位切换（`null` = 没有待确认项）
 *
 * 只有「切到始终允许」这一种会挂起：它是三档里唯一放宽询问的一档（spec §4.3），
 * 破坏性操作前确认是既有交互惯例；改回更保守的档位不设门槛（收紧无需阻拦）。
 */
const confirmTarget = ref<{
  app: PluginAuthOverview
  resource: string
  tier: AuthStrategy
} | null>(null)

/**
 * 档位按钮入口：同档不重复写库；「始终允许」先走二次确认，其余直接写
 *
 * 「已是当前档位」的点击必须静默返回（一次点击不该产生无意义的写库与刷新），
 * 而「已确认过」与「已拒绝过」的意图差异只在确认弹窗出现前的一瞬，不做记忆。
 */
function requestStrategy(app: PluginAuthOverview, resource: string, tier: AuthStrategy): void {
  if (tier === strategyOf(app, resource)) return
  if (tier === 'always_allow') {
    confirmTarget.value = { app, resource, tier }
    return
  }
  void applyStrategy(app, resource, tier)
}

/** 写档位（成功 toast + 重拉读模型；不乐观更新——档位徽标跟真源走） */
async function applyStrategy(
  app: PluginAuthOverview,
  resource: string,
  tier: AuthStrategy,
): Promise<void> {
  busyStrategy.value = strategyKey(app, resource)
  try {
    await pluginAuthSetStrategy(app.pluginId, resource, tier)
    toast.success(t('settings.authorization.strategyControl.saved'))
    await load()
  } catch (e) {
    showUserError(e)
  } finally {
    busyStrategy.value = null
  }
}

/** 二次确认通过：关闭弹窗后按用户确认的档位写库 */
async function confirmAlwaysAllow(): Promise<void> {
  const target = confirmTarget.value
  confirmTarget.value = null
  if (target) await applyStrategy(target.app, target.resource, target.tier)
}

/** 二次确认取消 / 点遮罩关闭：不写库、不刷新（用户没做决定） */
function cancelAlwaysAllow(): void {
  confirmTarget.value = null
}

/**
 * 某应用某资源的授权记录（票 02 文件目录 / 票 05 网络 origin）
 *
 * 排序：已授权在前、硬拒绝在后（与 spec §9.2 的四分区心智一致——「哪些能用」
 * 先答，「哪些被拒绝」是收敛信息），同组按落账时间；`recordsOf` 已返回新数组，
 * 这里就地排序不会动读模型。
 */
function resourceRecords(app: PluginAuthOverview, resource: string): AuthRecord[] {
  return recordsOf(app, resource).sort((a, b) => {
    const denyDelta = Number(isDeny(a)) - Number(isDeny(b))
    return denyDelta !== 0 ? denyDelta : a.createdAt - b.createdAt
  })
}

function recordKey(app: PluginAuthOverview, record: AuthRecord): string {
  return `${app.pluginId}:${record.id}`
}

function isDeny(record: AuthRecord): boolean {
  return effectKeySuffix(record.effect) === 'deny'
}

/** 免询问自动放行的记录（来源 `always_allow`）：标「未经确认」（spec §9.4） */
function isUnconfirmed(record: AuthRecord): boolean {
  return sourceKeySuffix(record.source) === 'always_allow'
}

/** 效果徽标文案：未知值显示原文（不吞掉宿主将来的新取值） */
function effectLabel(effect: string): string {
  const key = effectKeySuffix(effect)
  return key ? t(`settings.authorization.records.effect.${key}`) : effect
}

/** 操作集文案：空集（网络记录）不渲染徽标 */
function opsLabel(ops: string[]): string {
  const key = opsKeySuffix(ops)
  return key ? t(`settings.authorization.records.ops.${key}`) : ''
}

/** 来源文案：未知来源显示原文（错标成「用户确认」比标丑严重） */
function sourceLabel(source: string): string {
  const key = sourceKeySuffix(source)
  return key ? t(`settings.authorization.records.source.${key}`) : source
}

/** 撤销该目标授权：删除 allow 记录 + 落一条 deny 记录（spec §8.4） */
async function revoke(app: PluginAuthOverview, record: AuthRecord): Promise<void> {
  const key = recordKey(app, record)
  busyKey.value = key
  try {
    await pluginAuthRevoke(app.pluginId, record.resource, record.target)
    // 提示按资源分开说：目录被拒与地址被拒不是一回事（票 05 加网络分区时补的）
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

/** 移除拒绝记录：只删 deny，回到未覆盖状态（spec §8.4 的恢复出口） */
async function removeDeny(app: PluginAuthOverview, record: AuthRecord): Promise<void> {
  const key = recordKey(app, record)
  busyKey.value = key
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

// ==================== 内置免询问项（票 08） ====================

/**
 * 内置免询问项的操作 key（应用 × 条目）
 *
 * 与记录行的 key 不同命名空间：同一个 `value` 可能既是一条记录的 target 又是
 * 一个第一方条目，两个按钮共用一个 key 会让其中一侧在提交期间被误禁用。
 */
function firstPartyKey(app: PluginAuthOverview, entry: FirstPartyDirEntry): string {
  return `${app.pluginId}:first-party:${entry.value}`
}

/**
 * 撤销内置免询问项（仅 home 形态，spec §7）
 *
 * 撤销 = 落一条 `effect='deny'` 记录，由判定链第 1 步拦截（deny 优先于第一方层）。
 * target 传 `~/` 前缀形态：前端拿不到 `$HOME`，宿主 `AuthPolicyStore::revoke` 展开成
 * 绝对路径再落账——直接落 `~/` 字面量会永远匹配不上（静默撤销无效）。
 * project-segment 形态按设计只读（`AuthFirstPartyRow` 不渲染按钮），这里再挡一道：
 * 命令面不得被插件/前端直接调用去撤销一个落不成记录的段名。
 */
async function revokeFirstParty(app: PluginAuthOverview, entry: FirstPartyDirEntry): Promise<void> {
  if (entry.kind !== 'home') return
  const key = firstPartyKey(app, entry)
  busyKey.value = key
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

onMounted(() => {
  void load()
})

// KeepAlive 缓存恢复（从设置页往返）时 onMounted 不再执行；授权记录会随插件运行
// 变化（票 02+ 起还有撤销操作），缓存恢复时重新拉取，避免显示离开前的旧快照。
// 首次挂载时 onActivated 也会触发一次（onMounted 已加载过），用标志位跳过双次拉取。
let activatedOnce = false
onActivated(() => {
  if (activatedOnce) void load()
  activatedOnce = true
})
</script>
