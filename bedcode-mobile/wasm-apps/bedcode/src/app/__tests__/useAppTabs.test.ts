/**
 * 应用页签状态机 行为契约测试
 * （票 2026-10-10：全量 UI 下沉 —— app 域页签容器）
 *
 * 被测：`src/app/useAppTabs.ts`（useAppTabs）。
 * 纯状态 + 纯函数，不触 DOM / 不触组件，故本测试不挂载组件。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-AT1 | tabs computed 过滤+排序分支 | 只呈现 available 内页签，按 order 升序 | 顺序 connection→sessions→tasks |
 * | C-AT2 | active 初值三元分支 | available 含 connection 时缺省它，否则取首项 | 见用例 |
 * | C-AT3 | select 可用分支 | 切到可用页签 | 返回 true，active 更新 |
 * | C-AT4 | select 不可用分支（反例） | 切到未开放页签 | 返回 false，active **不变** |
 * | C-AT5 | activeIndex computed | 当前页签在 tabs 中的序号 | 与 tabs 顺序一致 |
 *
 * 横滑「一步去哪一页」的取邻与越界拒绝已随入场过渡一起下沉到 `../panelDock`
 * （neighborTabId），本文件只留状态机自身的契约。
 */
import { describe, it, expect } from 'vitest'
import { APP_TABS, useAppTabs, type AppTab } from '../useAppTabs'

const ALL: AppTab[] = ['connection', 'sessions', 'tasks', 'settings']

describe('C-AT1 页签集合：过滤 + 排序', () => {
  it('should_exposeAllTabsInOrder_when_allAvailable', () => {
    const tabs = useAppTabs(ALL)

    expect(tabs.tabs.value.map((t) => t.id)).toEqual([
      'connection',
      'sessions',
      'tasks',
      'settings',
    ])
  })

  it('should_exposeSubsetInOrder_when_someTabsNotYetAvailable', () => {
    // 设置域落地前的真实形态：settings 未开放，不应出现在导航里（避免点了落到空屏）
    const tabs = useAppTabs(['connection', 'sessions', 'tasks'])

    expect(tabs.tabs.value.map((t) => t.id)).toEqual(['connection', 'sessions', 'tasks'])
    expect(tabs.tabs.value.some((t) => t.id === 'settings')).toBe(false)
  })

  it('should_sortByOrderNotByInputOrder_when_inputOrderShuffled', () => {
    const tabs = useAppTabs(['settings', 'tasks', 'connection'])

    expect(tabs.tabs.value.map((t) => t.id)).toEqual(['connection', 'tasks', 'settings'])
  })

  it('should_attachLabelKeysToEveryTab_when_resolved', () => {
    const tabs = useAppTabs(ALL)

    expect(tabs.tabs.value.map((t) => t.labelKey)).toEqual([
      'app.nav.connection',
      'app.nav.sessions',
      'app.nav.tasks',
      'app.nav.settings',
    ])
  })
})

describe('C-AT2 初始页签', () => {
  it('should_defaultToConnection_when_connectionAvailable', () => {
    const tabs = useAppTabs(ALL)

    expect(tabs.active.value).toBe('connection')
  })

  it('should_fallbackToFirstAvailable_when_connectionNotAvailable', () => {
    const tabs = useAppTabs(['sessions', 'tasks'])

    expect(tabs.active.value).toBe('sessions')
  })
})

describe('C-AT3/C-AT4 select 正反例', () => {
  it('should_switchActive_when_selectingAvailableTab', () => {
    const tabs = useAppTabs(ALL)

    const switched = tabs.select('tasks')

    expect(switched).toBe(true)
    expect(tabs.active.value).toBe('tasks')
  })

  it('should_rejectAndKeepActive_when_selectingUnavailableTab', () => {
    // 反例：未开放的页签必须被拒且不改变状态，否则导航会切到空屏
    const tabs = useAppTabs(['connection', 'sessions'])
    tabs.select('sessions')

    const switched = tabs.select('settings')

    expect(switched).toBe(false)
    expect(tabs.active.value).toBe('sessions')
  })
})

describe('C-AT5 当前序号', () => {
  it('should_reportIndexWithinRenderedTabs_when_activeSet', () => {
    const tabs = useAppTabs(['connection', 'sessions', 'tasks'])

    expect(tabs.activeIndex.value).toBe(0)
    tabs.select('tasks')
    expect(tabs.activeIndex.value).toBe(2)
  })
})

describe('内置页签定义表', () => {
  it('should_keepLegacySlotOrders_when_comparedWithOldHostNav', () => {
    // 旧宿主底部导航的内置插槽约定（e92cc40a3^:src/components/MobileNav.vue）：
    // 连接=0 / 会话=100 / 任务=200 / 设置=300，供后续插件页签插中间值
    expect(APP_TABS.map((t) => [t.id, t.order])).toEqual([
      ['connection', 0],
      ['sessions', 100],
      ['tasks', 200],
      ['settings', 300],
    ])
  })
})