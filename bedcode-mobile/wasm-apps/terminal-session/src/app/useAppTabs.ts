/**
 * 应用页签状态机（票 2026-10-10：全量 UI 下沉 —— app 域页签容器）
 *
 * 纯状态 + 纯函数，不触 DOM、不触组件，便于单测钉行为契约。
 *
 * 排序口径沿用旧宿主底部导航：内置插槽 order = 连接 0 / 会话 100 / 工具箱 200 /
 * 设置 300（`e92cc40a3^:src/components/MobileNav.vue`），页面索引按排序后位置生成。
 * 保留 order 而非直接用数组下标，是为了让后续插件页签能像旧宿主那样插在中间值上。
 */
import { computed, ref, type ComputedRef, type Ref } from 'vue'

export type AppTab = 'connection' | 'sessions' | 'toolbox' | 'settings'

export interface AppTabDef {
  id: AppTab
  /** 文案键（域内相对键，app.nav.*） */
  labelKey: string
  /** 全局排序值（内置插槽 0/100/200/300） */
  order: number
}

/** 全部内置页签定义（按 order 升序即视觉顺序） */
export const APP_TABS: readonly AppTabDef[] = [
  { id: 'connection', labelKey: 'app.nav.connection', order: 0 },
  { id: 'sessions', labelKey: 'app.nav.sessions', order: 100 },
  { id: 'toolbox', labelKey: 'app.nav.toolbox', order: 200 },
  { id: 'settings', labelKey: 'app.nav.settings', order: 300 },
]

/** 可用页签的默认首项（可用集非空时的缺省落点） */
export const DEFAULT_TAB: AppTab = 'connection'

export interface AppTabs {
  /** 实际呈现的页签（已按 order 排序、过滤掉尚无内容的页签） */
  tabs: ComputedRef<AppTabDef[]>
  /** 当前页签 */
  active: Ref<AppTab>
  /** 当前页签在 tabs 中的序号（-1 = 当前页签不可用，理论上不应出现） */
  activeIndex: ComputedRef<number>
  /** 切到指定页签；页签不可用时不切换（返回是否切换成功） */
  select(id: AppTab): boolean
  /** 按方向步进（横滑翻页）；越界返回 false */
  step(dir: 'left' | 'right'): boolean
}

/**
 * 建页签状态机
 *
 * @param available 已具备内容的页签；未列出的内置页签不呈现
 *                 （避免导航项点了落到空屏——设置域落地前先不显示设置项）
 */
export function useAppTabs(available: readonly AppTab[]): AppTabs {
  const availableSet = new Set<AppTab>(available)
  const tabs = computed<AppTabDef[]>(() =>
    APP_TABS.filter((tab) => availableSet.has(tab.id)).sort((a, b) => a.order - b.order),
  )

  const active = ref<AppTab>(availableSet.has(DEFAULT_TAB) ? DEFAULT_TAB : (available[0] as AppTab))
  const activeIndex = computed(() => tabs.value.findIndex((tab) => tab.id === active.value))

  function select(id: AppTab): boolean {
    if (!availableSet.has(id)) return false
    active.value = id
    return true
  }

  function step(dir: 'left' | 'right'): boolean {
    const next = activeIndex.value + (dir === 'left' ? 1 : -1)
    if (next < 0 || next >= tabs.value.length) return false
    active.value = tabs.value[next].id
    return true
  }

  return { tabs, active, activeIndex, select, step }
}