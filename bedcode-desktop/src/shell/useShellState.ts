/**
 * 设备壳状态（模块级单例 composable）— 两屏模型 + 卡片 MRU + 覆层 + 通知机制
 *
 * 模型（spec-c §1）：只有 home / card 两屏，无屏幕栈。
 * - 开卡：点主屏图标 → 有卡直接激活（MRU 置顶），无卡产卡；重复打开只重排不重复产卡
 * - 关卡通配语义：应用卡 ×= 停用实例（→ dormant），系统页卡 ×= 只关页面；
 *   关活动卡切 MRU 次卡，无卡回主屏
 * - 返回（Esc / 工具栏 ←）= 逐层收覆层（dialog > palette > ctx > panel），耗尽后 card → home
 *
 * 通知（spec-c §7）：宿主只承载机制面；审批通知从待审批应用动态派生，批准后自然消失。
 */
import { computed, ref } from 'vue'
import { useToast } from '@/composables/useToast'
import i18n from '@/locales'
import {
  SYS_PAGES,
  appCardKey,
  approvalApps,
  cardId,
  cardKind,
  mruActivate,
  mruRemove,
  notifCount,
  sysCardKey,
} from './state'
import { useShellApps } from './useShellApps'
import type { CardKey, HostNotif, ShellCtxMenu, ShellDialogKind, ShellScreen } from './types'

const toast = useToast()
const t = i18n.global.t

// ==================== 两屏 + 卡片 ====================

const screen = ref<ShellScreen>('home')
/** MRU 卡片清单（最新在前） */
const cards = ref<CardKey[]>([])
/** 当前全屏卡片（screen === 'card' 时非空） */
const activeCard = ref<CardKey | null>(null)

// ==================== 覆层（不产卡；Esc 逐层收） ====================

const panelOpen = ref(false)
const paletteOpen = ref(false)
const ctxMenu = ref<ShellCtxMenu | null>(null)
const dialog = ref<ShellDialogKind | null>(null)
/** 弹窗关联的应用 id（审批 / 策略二次确认 / 卸载） */
const dialogAppId = ref<string | null>(null)
/** 策略二次确认的资源 */
const dialogResource = ref<string | null>(null)
/** 启动屏 */
const boot = ref(true)
/** 主屏 type-to-search 查询串 */
const homeQ = ref('')

// ==================== 通知（机制面） ====================

const notifs = ref<HostNotif[]>([])
let notifSeq = 100

function pushNotif(
  n: Omit<HostNotif, 'id' | 'mins' | 'sticky'> & { mins?: number; sticky?: boolean },
): void {
  const { mins = 0, sticky = false, ...rest } = n
  notifs.value = [{ id: 'n' + ++notifSeq, mins, sticky, ...rest }, ...notifs.value]
}

function dismissNotif(id: string): void {
  notifs.value = notifs.value.filter((n) => n.id !== id)
}

/** 清除全部：驻留项（sticky）与审批派生项不动，只清可清除的普通通知 */
function clearNotifs(): void {
  notifs.value = notifs.value.filter((n) => n.sticky)
}

/** 审批通知：从待审批应用动态派生（无独立生命周期，批准后自然消失） */
function approvalNotifs(): HostNotif[] {
  return approvalApps(useShellApps().plugins.value).map((a) => ({
    id: `appr-${a.id}`,
    kind: 'event',
    sticky: true,
    icon: 'warn',
    source: a.name,
    app: a.id,
    title: t('shell.notif.approval.title'),
    body: t('shell.notif.approval.body', {
      perms: a.permissions.length,
      risks: a.permissions.filter(
        (p) =>
          p === 'process:run' ||
          p === 'pty:spawn' ||
          p === 'terminal:input' ||
          p === 'database:main',
      ).length,
    }),
    mins: 0,
    actions: [{ id: 'approve', label: t('shell.notif.approval.go') }],
  }))
}

/** 铃铛角标 = 全部条目计数（通知 + 待审批） */
const notifCountTotal = computed(() =>
  notifCount(notifs.value, approvalApps(useShellApps().plugins.value)),
)

// ==================== 动作 ====================

/** 开应用卡：dormant 即点即启；approval 进审批屏卡；已在跑/降级直接激活卡 */
async function openApp(pluginId: string): Promise<void> {
  const apps = useShellApps()
  const app = apps.appOf(pluginId)
  if (!app) return
  const key = appCardKey(pluginId)

  // 未启动（dormant）：先启用实例再开卡；启用失败不产卡
  if (apps.displayStates.value[pluginId] === 'dormant') {
    const ok = await apps.activate(pluginId)
    if (!ok) return
  }

  // 审批态开卡 = 进审批屏卡（卡内提供「批准并启动」）
  if (apps.displayStates.value[pluginId] === 'approval') {
    activateCard(key)
    return
  }

  activateCard(key)
  // 审批态刚批准（approveApp 后 state 已是 running）→ 已开卡，无需重复动作
}

/** 开系统页卡 */
function openSys(pageId: string): void {
  activateCard(sysCardKey(pageId))
}

/** 激活卡片（MRU 置顶 + 切 card 屏） */
function activateCard(key: CardKey): void {
  cards.value = mruActivate(cards.value, key)
  activeCard.value = key
  screen.value = 'card'
  panelOpen.value = false
  ctxMenu.value = null
}

/**
 * 关卡片：应用卡 = 停用实例（→ dormant，卡片移除）；系统页卡 = 只关页面。
 * 关的是活动卡 → 切 MRU 次卡；无卡 → 回主屏。silent 供「全部关闭」批量调用。
 */
async function closeCard(key: CardKey, silent = false): Promise<void> {
  const apps = useShellApps()
  cards.value = mruRemove(cards.value, key)

  if (cardKind(key) === 'app') {
    const pluginId = cardId(key)
    const app = apps.appOf(pluginId)
    const st = apps.displayStates.value[pluginId]
    // 审批态 / 已停用不产停用动作；在跑实例才真正停用并落事件通知
    if (app && (st === 'running' || st === 'degraded')) {
      const ok = await apps.deactivate(pluginId)
      if (!silent && ok) {
        pushNotif({
          kind: 'event',
          icon: 'stop',
          source: app.name,
          app: pluginId,
          title: t('shell.notif.stopped.title'),
          body: t('shell.notif.stopped.body'),
          actions: [{ id: 'open', label: t('shell.notif.reopen') }],
        })
        toast.success(t('shell.notif.stopped.toast', { name: app.name }))
      }
    }
  }

  if (activeCard.value === key) {
    if (cards.value.length) {
      activeCard.value = cards.value[0]
    } else {
      goHome()
      return
    }
  }
}

/** 回主屏 */
function goHome(): void {
  screen.value = 'home'
  activeCard.value = null
  panelOpen.value = false
  paletteOpen.value = false
  ctxMenu.value = null
  homeQ.value = ''
}

/** 返回：覆层按 z 序先收（dialog/palette > ctx > panel），耗尽后 card → home */
function goBack(): void {
  if (dialog.value) {
    dialog.value = null
    return
  }
  if (paletteOpen.value) {
    paletteOpen.value = false
    return
  }
  if (ctxMenu.value) {
    ctxMenu.value = null
    return
  }
  if (panelOpen.value) {
    panelOpen.value = false
    return
  }
  if (screen.value !== 'home') goHome()
}

/** 全部关闭：一键清场（应用实例全部停用 + 系统页关闭），落一条事件通知 */
async function stopAllCards(): Promise<void> {
  const n = cards.value.length
  if (!n) return
  // 显式包箭头函数（forEach 会把下标当第二参数传入，index>0 被误当 silent=true）
  const list = [...cards.value]
  for (const k of list) await closeCard(k, true)
  pushNotif({
    kind: 'event',
    icon: 'layers',
    source: t('shell.notif.hostSource'),
    title: t('shell.notif.closeAll.title', { count: n }),
    body: t('shell.notif.closeAll.body'),
  })
  toast.success(t('shell.notif.closeAll.toast', { count: n }))
  goHome()
}

/** 卡片标题（应用卡取应用名；系统页卡取系统页名） */
function cardTitleOf(key: CardKey): string {
  if (cardKind(key) === 'app') {
    return useShellApps().appOf(cardId(key))?.name ?? cardId(key)
  }
  const sys = sysPageName(cardId(key))
  return sys ?? cardId(key)
}

// 系统页 key → i18n 名（卡片标题与当前位置用）
function sysPageName(pageId: string): string | undefined {
  const hit = SYS_PAGES.find((s) => s.id === pageId)
  return hit ? t(hit.nameKey) : undefined
}

// ==================== 应用生命周期通知助手 ====================

/** 批准并启动（审批屏卡 / 审批弹窗共用）：批准成功 → 通知 → 开卡 */
async function approveAndStart(pluginId: string): Promise<boolean> {
  const apps = useShellApps()
  const app = apps.appOf(pluginId)
  if (!app) return false
  await apps.approve(pluginId)
  // 批准成功与否以应用状态是否离开 approval 为准（零权限应用批准集为空是正常情形）
  if (apps.displayStates.value[pluginId] === 'approval') return false
  toast.success(t('shell.dialog.approval.success', { name: app.name }))
  pushNotif({
    kind: 'event',
    icon: 'check',
    source: app.name,
    app: pluginId,
    title: t('shell.notif.approved.title'),
    body: t('shell.notif.approved.body'),
    actions: [{ id: 'open', label: t('shell.notif.open') }],
  })
  await openApp(pluginId)
  return true
}

/** 停用应用（应用管理页 / 右键菜单共用；若开着卡则走 closeCard 同一路径保持通知语义一致） */
async function stopApp(pluginId: string): Promise<void> {
  const key = appCardKey(pluginId)
  if (cards.value.includes(key)) {
    await closeCard(key)
  } else {
    const apps = useShellApps()
    const app = apps.appOf(pluginId)
    const st = apps.displayStates.value[pluginId]
    if (app && st !== 'approval' && st !== 'dormant' && st !== 'error') {
      const ok = await apps.deactivate(pluginId)
      if (ok) {
        pushNotif({
          kind: 'event',
          icon: 'stop',
          source: app.name,
          app: pluginId,
          title: t('shell.notif.stopped.title'),
          body: t('shell.notif.stopped.body'),
          actions: [{ id: 'open', label: t('shell.notif.reopen') }],
        })
        toast.success(t('shell.notif.stopped.toast', { name: app.name }))
      }
    }
  }
}

export function useShellState() {
  return {
    screen,
    cards,
    activeCard,
    panelOpen,
    paletteOpen,
    ctxMenu,
    dialog,
    dialogAppId,
    dialogResource,
    boot,
    homeQ,
    notifs,
    notifCountTotal,
    pushNotif,
    dismissNotif,
    clearNotifs,
    approvalNotifs,
    openApp,
    openSys,
    activateCard,
    closeCard,
    goHome,
    goBack,
    stopAllCards,
    approveAndStart,
    stopApp,
    cardTitleOf,
  }
}
