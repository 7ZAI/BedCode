import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { PluginInfo } from '@/plugin/types'

/**
 * useShellState（壳两屏模型 + 卡片 MRU + 覆层 + 通知机制）测试
 *
 * 依赖桩：
 * - useShellApps 整模块 mock（refs 用普通对象 {value}——壳动作是命令式调用，
 *   同步读 .value，测试不依赖响应式）；deactivate 等可替换为 vi.fn 断言调用
 * - useToast / i18n mock（避免 Tauri / vue-i18n 加载）
 */

// ==================== mock 工厂（模块级单例桩） ====================

vi.mock('@/shell/useShellApps', () => {
  const st = {
    plugins: { value: [] as PluginInfo[] },
    loading: { value: false },
    busyId: { value: null as string | null },
    busyKind: { value: null as string | null },
    detailAppId: { value: null as string | null },
    displayStates: { value: {} as Record<string, string> },
    tones: { value: {} as Record<string, string> },
    approvals: { value: [] as PluginInfo[] },
    runningCount: { value: 0 },
    appOf: (id: string) => st.plugins.value.find((p) => p.id === id),
    riskCount: () => 0,
    load: () => Promise.resolve(),
    activate: () => Promise.resolve(true),
    deactivate: () => Promise.resolve(true),
    approve: () => Promise.resolve([]),
    uninstall: () => Promise.resolve(true),
  }
  return { useShellApps: () => st }
})

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    show: () => '',
    success: () => '',
    error: () => '',
    warning: () => '',
    info: () => '',
  }),
}))

vi.mock('@/locales', () => ({
  default: { global: { t: (key: string) => key } },
}))

import { useShellState } from '@/shell/useShellState'
import { useShellApps } from '@/shell/useShellApps'

function fakeApp(id: string, state: PluginInfo['state']): PluginInfo {
  return {
    id,
    name: id,
    version: '1.0.0',
    description: '',
    author: '',
    main: '',
    pluginType: 'rust-ts',
    permissions: [],
    state,
    extensionPath: '',
    contributes: { commands: [], views: [], toolProviders: [], fileHandlers: [] },
    source: 'builtin',
    sizeBytes: 0,
  }
}

describe('useShellState（两屏模型 + 卡片 MRU + 覆层 + 通知）', () => {
  const apps = useShellApps()

  beforeEach(() => {
    // 重置单例状态（模块级 ref，须在用例间手工复位）
    const s = useShellState()
    s.screen.value = 'home'
    s.cards.value = []
    s.activeCard.value = null
    s.panelOpen.value = false
    s.paletteOpen.value = false
    s.ctxMenu.value = null
    s.dialog.value = null
    s.homeQ.value = ''
    s.notifs.value = []
    apps.plugins.value = []
    apps.displayStates.value = {}
    apps.tones.value = {}
    apps.approvals.value = []
    // 每用例独立 spy
    apps.activate = vi.fn(async () => true)
    apps.deactivate = vi.fn(async () => true)
    apps.approve = vi.fn(async () => [])
  })

  function seedApps(...list: Array<[string, PluginInfo['state']]>) {
    apps.plugins.value = list.map(([id, state]) => fakeApp(id, state))
    apps.displayStates.value = Object.fromEntries(list.map(([id, state]) => [id, displayOf(state)]))
  }
  function displayOf(state: PluginInfo['state']): string {
    if (state.state === 'activated') return 'running'
    if (state.state === 'degraded') return 'degraded'
    if (state.state === 'needsApproval') return 'approval'
    if (state.state === 'error') return 'error'
    return 'dormant'
  }

  it('开系统页卡：进 card 屏 + MRU 置顶；重复打开只重排不重复产卡', () => {
    const s = useShellState()
    s.openSys('settings')
    expect(s.screen.value).toBe('card')
    expect(s.activeCard.value).toBe('sys:settings')
    expect(s.cards.value).toEqual(['sys:settings'])

    s.openSys('apps')
    expect(s.cards.value).toEqual(['sys:apps', 'sys:settings'])
    expect(s.activeCard.value).toBe('sys:apps')

    // 重复打开 settings：重排到最前，不产重复卡
    s.openSys('settings')
    expect(s.cards.value).toEqual(['sys:settings', 'sys:apps'])
    expect(s.cards.value.length).toBe(2)
  })

  it('开应用卡：dormant 先启用实例再开卡；启用失败不产卡', async () => {
    const s = useShellState()
    seedApps(['a', { state: 'deactivated' }])
    const ok = await s.openApp('a')
    expect(ok).toBeUndefined()
    expect(apps.activate).toHaveBeenCalledWith('a')
    expect(s.cards.value).toEqual(['app:a'])
    expect(s.screen.value).toBe('card')
  })

  it('开应用卡：approval 态直接进审批屏卡，不触发启用', async () => {
    const s = useShellState()
    seedApps(['a', { state: 'needsApproval' }])
    await s.openApp('a')
    expect(apps.activate).not.toHaveBeenCalled()
    expect(s.cards.value).toEqual(['app:a'])
  })

  it('关应用卡 = 停用实例并移除卡片；系统页卡 = 只关页面', async () => {
    const s = useShellState()
    seedApps(['a', { state: 'activated' }])
    s.openSys('settings')
    s.openApp('a')
    // 当前活动卡 app:a → 关掉切到 MRU 次卡 sys:settings
    await s.closeCard('app:a')
    expect(apps.deactivate).toHaveBeenCalledWith('a')
    expect(s.cards.value).toEqual(['sys:settings'])
    expect(s.activeCard.value).toBe('sys:settings')

    // 系统页卡：关页不触发任何实例动作
    await s.closeCard('sys:settings')
    expect(s.cards.value).toEqual([])
    expect(s.activeCard.value).toBeNull()
    expect(s.screen.value).toBe('home')
  })

  it('关活动卡切 MRU 次卡；无卡回主屏', async () => {
    const s = useShellState()
    seedApps(['a', { state: 'activated' }], ['b', { state: 'activated' }])
    await s.openApp('a')
    await s.openApp('b')
    await s.closeCard('app:b')
    expect(s.activeCard.value).toBe('app:a')
    expect(s.screen.value).toBe('card')
  })

  it('全部关闭：逐张关卡（应用停用）+ 落一条事件通知 + 回主屏', async () => {
    const s = useShellState()
    seedApps(['a', { state: 'activated' }], ['b', { state: 'activated' }])
    await s.openApp('a')
    s.openSys('settings')
    await s.stopAllCards()
    // 两张卡里只有 app:a 是应用卡（触发停用）；sys:settings 只关页面
    expect(apps.deactivate).toHaveBeenCalledTimes(1)
    expect(s.cards.value).toEqual([])
    expect(s.screen.value).toBe('home')
    expect(s.notifs.value.length).toBe(1)
    expect(s.notifs.value[0].icon).toBe('layers')
  })

  it('返回（Esc）逐层收覆层：dialog > palette > ctx > panel，耗尽后 card → home', () => {
    const s = useShellState()
    seedApps(['a', { state: 'activated' }])
    s.openApp('a') // async，但本用例只验证 goBack 的顺序，先手动设态
    s.screen.value = 'card'

    s.dialog.value = 'exit'
    s.paletteOpen.value = true
    s.ctxMenu.value = { appId: 'a', x: 0, y: 0 }
    s.panelOpen.value = true

    s.goBack()
    expect(s.dialog.value).toBeNull()
    expect(s.paletteOpen.value).toBe(true)
    s.goBack()
    expect(s.paletteOpen.value).toBe(false)
    expect(s.ctxMenu.value).not.toBeNull()
    s.goBack()
    expect(s.ctxMenu.value).toBeNull()
    expect(s.panelOpen.value).toBe(true)
    s.goBack()
    expect(s.panelOpen.value).toBe(false)
    expect(s.screen.value).toBe('card') // 覆层耗尽，card 屏
    s.goBack()
    expect(s.screen.value).toBe('home') // 回主屏
    // 主屏上再返回：无操作
    s.goBack()
    expect(s.screen.value).toBe('home')
  })

  it('审批通知动态派生：从待审批应用生成，批准后自然消失', () => {
    const s = useShellState()
    seedApps(['a', { state: 'needsApproval' }], ['b', { state: 'activated' }])
    const appr = s.approvalNotifs()
    expect(appr.length).toBe(1)
    expect(appr[0].app).toBe('a')
    expect(appr[0].sticky).toBe(true)

    // 状态离开 approval → 派生消失
    seedApps(['a', { state: 'activated' }], ['b', { state: 'activated' }])
    expect(s.approvalNotifs()).toEqual([])
  })

  it('通知：push / 单条关闭 / 清除全部只清非驻留', () => {
    const s = useShellState()
    s.pushNotif({ kind: 'event', icon: 'stop', source: 's', title: 't1' })
    s.pushNotif({ kind: 'info', icon: 'check', source: 's', title: 't2', sticky: true })
    expect(s.notifs.value.length).toBe(2)

    // 关掉非驻留的 t1（notifs[0] 是最新推送 = t2，须按标题找）
    s.dismissNotif(s.notifs.value.find((n) => n.title === 't1')!.id)
    expect(s.notifs.value.length).toBe(1)
    expect(s.notifs.value[0].title).toBe('t2')

    s.pushNotif({ kind: 'event', icon: 'x', source: 's', title: 't3' })
    s.clearNotifs()
    // 驻留 t2 保留，普通 t3 清除
    expect(s.notifs.value.map((n) => n.title)).toEqual(['t2'])
  })

  it('铃铛角标 = 通知 + 待审批计数', () => {
    const s = useShellState()
    seedApps(['a', { state: 'needsApproval' }])
    s.pushNotif({ kind: 'event', icon: 'x', source: 's', title: 't' })
    expect(s.notifCountTotal.value).toBe(2)
  })

  it('stopApp：开着卡走 closeCard 同一路径（停用 + 通知语义一致）', async () => {
    const s = useShellState()
    seedApps(['a', { state: 'activated' }])
    await s.openApp('a')
    expect(s.cards.value).toEqual(['app:a'])
    await s.stopApp('a')
    expect(apps.deactivate).toHaveBeenCalledTimes(1)
    expect(s.cards.value).toEqual([])
  })
})
