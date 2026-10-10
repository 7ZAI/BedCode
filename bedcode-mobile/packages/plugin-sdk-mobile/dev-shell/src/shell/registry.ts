/**
 * Dev Shell 宿主壳注册表
 * -----------------------------------------------------------------------------
 * 结构与宿主 `bedcode-mobile/src/shell/registry.ts` 同构（只做注册/撤销/查询，
 * 不做 UI 决策、不感知数据来源）。
 *
 * 与宿主的唯一差异是数据源：宿主的数据源是插件系统（真后端），dev-shell 的数据源
 * 是 `adapters/devAppSource.ts`（把被调试插件记录投影成应用）。壳内组件对两者无感——
 * 这正是这套分层要保证的事：换真源只换数据源实现。
 *
 * 同构纪律：改这里之前先看宿主那份，两边结构应保持一致；字段级漂移由
 * `__tests__/shell/contractDrift.test.ts` 兜底。
 */

import { ref, markRaw, type Ref } from 'vue'
import type {
  Disposable,
  ShellApp,
  ShellAppSource,
  ShellCapsuleItem,
  ShellSettingsEntry,
  ShellSlotContribution,
  ShellSurfaceContribution,
} from './types'

/** 单个应用的增量贡献集合（键为贡献项 id，重复注册即覆盖） */
interface ContributionStore {
  slots: Map<string, ShellSlotContribution>
  capsuleItems: Map<string, ShellCapsuleItem>
  settingsEntries: Map<string, ShellSettingsEntry>
  surface?: ShellSurfaceContribution
}

/** 应用记录归属：哪个数据源写入的，清理时按源整体回收 */
interface AppRecord {
  sourceId: string
  app: ShellApp
}

function emptyContributions(): ContributionStore {
  return { slots: new Map(), capsuleItems: new Map(), settingsEntries: new Map() }
}

/** 宿主壳注册表 */
class ShellRegistryClass {
  private readonly apps = new Map<string, AppRecord>()
  private readonly sources = new Map<string, ShellAppSource>()
  private readonly contributions = new Map<string, ContributionStore>()

  /** 响应式应用清单（供壳内组件消费） */
  readonly appsRef: Ref<ShellApp[]> = ref([])

  // ==================== 数据源 ====================

  /** 注册应用数据源；重复注册同一 id 覆盖旧源 */
  registerSource(source: ShellAppSource): Disposable {
    this.sources.set(source.id, source)
    return {
      dispose: () => {
        this.sources.delete(source.id)
      },
    }
  }

  /** 已注册的数据源（按注册顺序） */
  listSources(): ShellAppSource[] {
    return [...this.sources.values()]
  }

  // ==================== 应用清单 ====================

  /**
   * 全量覆盖某数据源的应用清单
   *
   * 清单自带的 contributions 会并入增量贡献表（同 id 覆盖），因此「清单下发」与
   * 「运行时注册」两条路径最终合并为同一份视图。
   */
  upsertApps(sourceId: string, apps: ShellApp[]): void {
    for (const app of apps) {
      this.apps.set(app.id, { sourceId, app })
      this.mergeContributions(app.id, app.contributions)
    }
    // 该数据源本次未返回的应用视为已卸载：回收其记录与贡献
    const alive = new Set(apps.map((a) => a.id))
    for (const [id, rec] of [...this.apps.entries()]) {
      if (rec.sourceId === sourceId && !alive.has(id)) this.apps.delete(id)
    }
    this.publish()
  }

  /** 单个应用的状态补丁（启动/停止/失败后回写，避免整表重拉） */
  patchApp(appId: string, patch: Partial<ShellApp>): void {
    const rec = this.apps.get(appId)
    if (!rec) return
    rec.app = { ...rec.app, ...patch }
    this.publish()
  }

  /** 获取应用（含合并后的贡献） */
  getApp(appId: string): ShellApp | undefined {
    const rec = this.apps.get(appId)
    if (!rec) return undefined
    return this.toView(rec.app)
  }

  /** 清理单个应用（记录 + 贡献） */
  clearApp(appId: string): void {
    this.apps.delete(appId)
    this.contributions.delete(appId)
    this.publish()
  }

  /** 清理某数据源写入的全部应用 */
  clearSource(sourceId: string): void {
    for (const [id, rec] of [...this.apps.entries()]) {
      if (rec.sourceId === sourceId) {
        this.apps.delete(id)
        this.contributions.delete(id)
      }
    }
    this.publish()
  }

  /** 应用所属数据源（按来源解析延迟能力，如运行面组件） */
  getSourceOf(appId: string): ShellAppSource | undefined {
    const rec = this.apps.get(appId)
    if (!rec) return undefined
    return this.sources.get(rec.sourceId)
  }

  // ==================== 界面贡献 ====================

  /** 注册首页快捷卡片 */
  registerSlot(appId: string, slot: ShellSlotContribution): Disposable {
    const store = this.storeFor(appId)
    store.slots.set(slot.id, { ...slot, component: markRaw(slot.component) })
    this.publish()
    return { dispose: () => this.drop(store.slots, slot.id) }
  }

  /** 注册应用运行面 */
  registerSurface(appId: string, surface: ShellSurfaceContribution): Disposable {
    const store = this.storeFor(appId)
    store.surface = { ...surface, component: markRaw(surface.component) }
    this.publish()
    return {
      dispose: () => {
        if (store.surface?.component === surface.component) store.surface = undefined
        this.publish()
      },
    }
  }

  /** 注册胶囊菜单附加项 */
  registerCapsuleItem(appId: string, item: ShellCapsuleItem): Disposable {
    const store = this.storeFor(appId)
    store.capsuleItems.set(item.id, item)
    this.publish()
    return { dispose: () => this.drop(store.capsuleItems, item.id) }
  }

  /** 注册平台设置入口 */
  registerSettingsEntry(appId: string, entry: ShellSettingsEntry): Disposable {
    const store = this.storeFor(appId)
    store.settingsEntries.set(entry.id, entry)
    this.publish()
    return { dispose: () => this.drop(store.settingsEntries, entry.id) }
  }

  // ==================== 内部 ====================

  private storeFor(appId: string): ContributionStore {
    let store = this.contributions.get(appId)
    if (!store) {
      store = emptyContributions()
      this.contributions.set(appId, store)
    }
    return store
  }

  private mergeContributions(appId: string, c: ShellApp['contributions']): void {
    if (!c) return
    const store = this.storeFor(appId)
    for (const slot of c.slots ?? []) {
      store.slots.set(slot.id, { ...slot, component: markRaw(slot.component) })
    }
    for (const item of c.capsuleItems ?? []) store.capsuleItems.set(item.id, item)
    for (const entry of c.settingsEntries ?? []) store.settingsEntries.set(entry.id, entry)
    if (c.surface) {
      store.surface = { ...c.surface, component: markRaw(c.surface.component) }
    }
  }

  private drop(map: Map<string, unknown>, key: string): void {
    map.delete(key)
    this.publish()
  }

  /** 把存储模型投影为「含合并贡献」的视图模型 */
  private toView(app: ShellApp): ShellApp {
    const store = this.contributions.get(app.id)
    if (!store) return { ...app, contributions: {} }
    return {
      ...app,
      contributions: {
        slots: [...store.slots.values()].sort(byOrder),
        surface: store.surface,
        capsuleItems: [...store.capsuleItems.values()].sort(byOrder),
        settingsEntries: [...store.settingsEntries.values()].sort(byOrder),
      },
    }
  }

  /** 重算响应式清单 */
  private publish(): void {
    this.appsRef.value = [...this.apps.values()].map((rec) => this.toView(rec.app))
  }
}

function byOrder<T extends { order?: number }>(a: T, b: T): number {
  return (a.order ?? 100) - (b.order ?? 100)
}

/** 全局单例 */
const registry = new ShellRegistryClass()

/** 获取宿主壳注册表 */
export function getShellRegistry(): ShellRegistryClass {
  return registry
}

export type { ShellRegistryClass }