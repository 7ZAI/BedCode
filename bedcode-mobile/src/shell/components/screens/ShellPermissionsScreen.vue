<template>
  <div class="flex flex-col h-full min-h-0">
    <!-- ==================== 页头 ==================== -->
    <header class="flex items-center gap-2 px-2 pt-2 pb-1 flex-shrink-0">
      <button
        type="button"
        class="flex items-center justify-center rounded-[10px] min-w-[var(--mobile-touch-target-min)] min-h-[var(--mobile-touch-target-min)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
        :aria-label="t('shell.detail.back')"
        @click="nav.back()"
      >
        <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M15 6l-6 6 6 6" />
        </svg>
      </button>
      <h1 class="shell-type-display flex-1 min-w-0 truncate">
        {{ t('shell.settings.permissions') }}
      </h1>
    </header>

    <!-- ==================== 视图切换（按应用 / 按权限） ====================
         两个视图是同一份权限真源的两个投影方向：
         · 按应用（默认）：一行一应用，跨权限组不重复——这是「审计」主视图
         · 按权限：反查「哪些应用持有某权限」，行尾带授予状态 -->
    <div class="flex items-center px-4 pt-1 pb-1 flex-shrink-0">
      <div class="shell-segmented" role="group" :aria-label="t('shell.permissions.viewSwitch')">
        <button
          type="button"
          :aria-pressed="view === 'apps'"
          @click="view = 'apps'"
        >
          {{ t('shell.permissions.viewApps') }}
        </button>
        <button
          type="button"
          :aria-pressed="view === 'groups'"
          @click="view = 'groups'"
        >
          {{ t('shell.permissions.viewGroups') }}
        </button>
      </div>
    </div>

    <div class="flex-1 min-h-0 overflow-y-auto pb-6 shell-content-cap">
      <!-- ==================== 视图：按应用 ==================== -->
      <template v-if="view === 'apps'">
        <!-- 待审批（需要用户动作，置顶）。审批是整单的（后端 plugin_approve：全部
             声明权限 + 内容哈希钉扎），因此这里不做逐项勾选。 -->
        <template v-if="pendingApps.length > 0">
          <ShellSection
            :title="t('shell.permissions.pending')"
            :note="t('shell.permissions.pendingNote')"
            class="shell-reveal"
          />
          <div class="px-4 pb-1 flex flex-col gap-2">
            <div
              v-for="app in pendingApps"
              :key="app.id"
              class="flex items-center gap-3 rounded-[12px] border px-3 py-3"
              :style="{
                background: 'var(--mobile-warning-muted)',
                borderColor: 'var(--mobile-warning)',
              }"
            >
              <ShellAppIcon :app="app" :size="32" />
              <div class="flex-1 min-w-0">
                <div class="shell-type-label text-[var(--mobile-text-primary)] truncate">
                  {{ app.name }}
                </div>
                <div class="shell-type-caption text-[var(--mobile-text-secondary)] truncate">
                  {{ t('shell.permissions.pendingBody', { count: app.permissions.length }) }}
                </div>
              </div>
              <ShellChip
                :label="t('shell.permissions.approveStart')"
                tone="accent"
                clickable
                @click="approveAndStart(app.id)"
              />
            </div>
          </div>
        </template>

        <!-- 应用权限清单：每行一个应用，摘要一行说清授予情况；完整的勾选与审计
             在详情页（openDetail），总览不复制明细——明细两份必然漂移 -->
        <ShellSection
          v-if="appRows.length > 0"
          :title="t('shell.permissions.appSection')"
          class="shell-reveal"
        />
        <div class="px-4">
          <ShellGroupCard v-if="appRows.length > 0">
            <button
              v-for="app in appRows"
              :key="app.id"
              type="button"
              class="flex items-center gap-3 w-full px-3 py-3 text-left transition-colors duration-200 active:bg-[var(--mobile-group-row-active)] min-h-[var(--mobile-touch-target-min)]"
              @click="nav.openDetail(app.id)"
            >
              <ShellAppIcon :app="app" :size="32" />
              <span class="flex-1 min-w-0">
                <span class="block shell-type-label text-[var(--mobile-text-primary)] truncate">
                  {{ app.name }}
                </span>
                <span class="block shell-type-caption text-[var(--mobile-text-secondary)] truncate">
                  {{ permissionSummary(app) }}
                </span>
              </span>
              <svg class="flex-shrink-0" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" :style="{ color: 'var(--mobile-text-secondary)' }">
                <path d="M9 6l6 6-6 6" />
              </svg>
            </button>
          </ShellGroupCard>

          <!-- 全无应用才给整体空态；全待审批时审批卡已占位，列表不重复解释 -->
          <p
            v-if="appRows.length === 0 && apps.length === 0"
            class="rounded-[12px] border border-dashed border-[var(--mobile-border-hover)] px-3 py-6 text-center shell-type-caption text-[var(--mobile-text-secondary)]"
          >
            {{ t('mobile.plugin.noPermissions') }}
          </p>
        </div>
      </template>

      <!-- ==================== 视图：按权限（反查） ==================== -->
      <template v-else>
        <template v-if="domainRows.length > 0">
          <!-- 能力域分组（顺序 = SHELL_PERMISSION_GROUPS；未归类词进「其他」）
               域内权限词一行：词名 + code + 持有应用徽章流。徽章比长列表紧凑：
               一眼扫到「谁持有什么、授予还是默认」，点徽章直达应用详情 -->
          <div v-for="row in domainRows" :key="row.id">
            <ShellSection
              :title="t(row.titleKey)"
              :note="t('shell.apps.permissionCount', { count: row.items.length })"
              class="shell-reveal"
            />
            <div class="px-4 pb-1">
              <ShellGroupCard>
                <div v-for="word in row.items" :key="word.key" class="px-3 py-2.5">
                  <div class="flex items-center gap-2 min-w-0">
                    <span class="flex-1 min-w-0 shell-type-caption font-medium text-[var(--mobile-text-primary)] truncate">
                      {{ permissionTitle(word.key) }}
                    </span>
                    <code class="flex-shrink-0 text-xs font-medium text-[var(--mobile-text-secondary)]">{{ word.key }}</code>
                  </div>
                  <div class="mt-2 flex flex-wrap gap-1.5">
                    <button
                      v-for="g in word.apps"
                      :key="g.app.id"
                      type="button"
                      class="flex items-center gap-1.5 rounded-full border px-2.5 min-h-[var(--mobile-touch-target-min)] min-w-0 transition-colors duration-200 active:opacity-70"
                      :class="grantChipClass(g)"
                      :aria-label="grantChipLabel(g)"
                      @click="nav.openDetail(g.app.id)"
                    >
                      <ShellAppIcon :app="g.app" :size="20" />
                      <span class="shell-type-micro text-[var(--mobile-text-primary)] truncate">{{ g.app.name }}</span>
                      <!-- 状态尾缀：点（实心/空心）+ 文字双通道，非纯色传态 -->
                      <span class="flex items-center gap-1 flex-shrink-0 shell-type-micro" :style="{ color: g.state.color }">
                        <span
                          class="w-1.5 h-1.5 rounded-full flex-shrink-0"
                          :style="
                            g.state.filled
                              ? { background: 'currentColor' }
                              : { border: '1.5px solid currentColor', background: 'transparent' }
                          "
                          aria-hidden="true"
                        />
                        {{ g.state.label }}
                      </span>
                    </button>
                  </div>
                </div>
              </ShellGroupCard>
            </div>
          </div>
        </template>

        <div v-else class="px-4 pt-6">
          <p class="rounded-[12px] border border-dashed border-[var(--mobile-border-hover)] px-3 py-6 text-center shell-type-caption text-[var(--mobile-text-secondary)]">
            {{ t('mobile.plugin.noPermissions') }}
          </p>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 权限总览
 *
 * 与详情页的「按应用看权限」共用同一份权限授予真源，只是投影方向相反。
 * 不新增任何权限状态——反查视图如果自己维护一份状态，两处迟早不一致。
 *
 * 2026-10-15 重设计（消除跨组重复）：原页面按权限词分组，同一应用会在多个
 * 权限组下反复出现（实测 BedCode 3 次 / File Transfer 5 次），页面充满同名
 * 重复行。改为双视图：
 *   · 按应用（默认）= 审计主视图，一行一应用，只给摘要行；勾选/审计去详情页
 *   · 按权限 = 反查「哪些应用持有某权限」，行尾带授予状态（点 + 文字双通道）
 * 两者由同一份 apps.permissions 派生，视图切换不产生新状态。
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useToast } from '../../composables/useToast'
import { useShellApps } from '../../composables/useShellApps'
import { useShellNavigation } from '../../composables/useShellNavigation'
import { permissionTitleKey, SHELL_PERMISSION_GROUPS } from '../../permissions'
import type { ShellApp } from '../../types'
import ShellAppIcon from '../ShellAppIcon.vue'
import ShellChip from '../ShellChip.vue'
import ShellGroupCard from '../ShellGroupCard.vue'
import ShellSection from '../ShellSection.vue'

/** 能力域分组（顺序 = SHELL_PERMISSION_GROUPS，未归类词进「其他」） */
interface DomainRow {
  id: string
  titleKey: string
  items: PermissionWordRow[]
}

/** 一个权限词的持有应用（带授予状态，用于徽章展示） */
interface PermissionWordRow {
  key: string
  apps: GrantApp[]
}

/** 应用 × 该权限词的授予状态 */
interface GrantApp {
  app: ShellApp
  state: GrantState
}

/** 授予状态展示（点 + 文案，不依赖单一颜色通道） */
interface GrantState {
  label: string
  color: string
  /** true = 实心点（已授予），false = 空心点（默认 / 未授予） */
  filled: boolean
  /** 默认授予且不可关闭（storage 等底座能力，徽章用灰底而非绿底区分） */
  locked: boolean
}

const { t } = useI18n()
const toast = useToast()
const nav = useShellNavigation()
const { apps, approveApp, launch } = useShellApps()

/** 当前视图：按应用（审计主视图，默认）｜按权限（反查） */
const view = ref<'apps' | 'groups'>('apps')

/** 待审批应用：审批入口的置顶分区（批准后状态变化，卡片自然消失） */
const pendingApps = computed(() => apps.value.filter((a) => a.state === 'approval'))

/**
 * 应用权限清单（按应用视图）
 *
 * 名称序而非 sortedApps 的运行优先：审计面要求顺序稳定，运行状态变化不应
 * 让整页重新排。待审批应用由上方审批卡承担展示位，列表不重复出现。
 */
const appRows = computed<ShellApp[]>(() =>
  [...apps.value]
    .filter((a) => a.state !== 'approval')
    .sort((a, b) => a.name.localeCompare(b.name, 'zh-Hans-CN')),
)

/** 行尾摘要：授予/默认计数与详情页同一口径（granted/locked 双 filter） */
function permissionSummary(app: ShellApp): string {
  const granted = app.permissions.filter((p) => p.granted).length
  const locked = app.permissions.filter((p) => p.locked).length
  if (granted === 0) return t('shell.permissions.appNone')
  return locked > 0
    ? t('shell.permissions.appSummaryLocked', { granted, locked })
    : t('shell.permissions.appSummary', { granted })
}

/** 某应用对某权限词的授予状态（徽章尾缀与状态点） */
function grantStateOf(app: ShellApp, key: string): GrantState {
  const grant = app.permissions.find((p) => p.key === key)
  if (!grant || !grant.granted) {
    return {
      label: t('shell.permissions.ungranted'),
      color: 'var(--mobile-text-disabled)',
      filled: false,
      locked: false,
    }
  }
  if (grant.locked) {
    return {
      label: t('shell.permissions.locked'),
      color: 'var(--mobile-text-secondary)',
      filled: false,
      locked: true,
    }
  }
  return {
    label: t('shell.permissions.granted'),
    color: 'var(--mobile-success)',
    filled: true,
    locked: false,
  }
}

/** 徽章容器样式：授予=绿底实描边 / 默认=灰底 / 未授予=透明虚线（形态区分，不只靠颜色） */
function grantChipClass(g: GrantApp): string {
  if (g.state.filled) return 'border-[var(--mobile-success-connected-border)] bg-[var(--mobile-success-muted)]'
  if (g.state.locked) return 'border-[var(--mobile-border)] bg-[var(--mobile-bg-elevated)]'
  return 'border-dashed border-[var(--mobile-border-hover)] bg-transparent'
}

/** 徽章可访问名：应用名 + 授予状态（读屏用户看不到点与颜色） */
function grantChipLabel(g: GrantApp): string {
  return `${g.app.name} · ${g.state.label}`
}

/**
 * 批准并启动
 *
 * 批准失败（后端拒绝）时不继续启动——半途而废的「已批准」假象比直接报错更危险。
 */
async function approveAndStart(appId: string): Promise<void> {
  const approved = await approveApp(appId)
  if (!approved) {
    toast.error(t('shell.permissions.approveFailed'))
    return
  }
  const started = await launch(appId)
  if (started) nav.openApp(appId)
}

/** 权限标题：复用既有本地化文案，未知权限回退原始权限词 */
function permissionTitle(key: string): string {
  const i18nKey = permissionTitleKey(key)
  return i18nKey ? t(i18nKey) : key
}

/**
 * 能力域 → 权限词 → 持有应用（按权限视图）
 *
 * 聚合自同一份 apps.permissions；域顺序与词库分组一致，未归类的词进「其他」
 * 底部兜底（新权限不该因为没归类就消失）。应用的授予状态在聚合时快照，
 * 徽章直接读，避免在模板里反复 find。
 */
const domainRows = computed<DomainRow[]>(() => {
  const byKey = new Map<string, GrantApp[]>()
  for (const app of apps.value) {
    for (const grant of app.permissions) {
      const list = byKey.get(grant.key) ?? []
      list.push({
        app,
        state: grantStateOf(app, grant.key),
      })
      byKey.set(grant.key, list)
    }
  }

  const byDomain = new Map<string, DomainRow>(
    SHELL_PERMISSION_GROUPS.map((g) => [g.id, { id: g.id, titleKey: g.titleKey, items: [] }]),
  )
  const rest: PermissionWordRow[] = []
  for (const [key, grants] of byKey) {
    const group = SHELL_PERMISSION_GROUPS.find((g) => g.keys.includes(key))
    const row: PermissionWordRow = {
      key,
      apps: grants.sort((a, b) => a.app.name.localeCompare(b.app.name, 'zh-Hans-CN')),
    }
    if (group) byDomain.get(group.id)?.items.push(row)
    else rest.push(row)
  }

  const nonEmpty = [...byDomain.values()].filter((g) => g.items.length > 0)
  if (rest.length > 0) {
    nonEmpty.push({ id: 'other', titleKey: 'shell.permission.group.other', items: rest })
  }
  return nonEmpty
})
</script>