/**
 * 宿主壳注册表 行为契约测试
 *
 * 覆盖：清单写入与回收、增量贡献注册与摘除、两条贡献路径合并、
 *       组件免响应式代理（markRaw）、清理语义。
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { defineComponent, h, isReactive, type Component } from 'vue'
import { getShellRegistry } from '@/shell/registry'
import type { ShellApp, ShellAppSource } from '@/shell/types'

/** 测试用组件（每个都带可识别的 name，便于断言拿到的是哪一个） */
function comp(name: string): Component {
  return defineComponent({ name, setup: () => () => h('div', name) })
}

/** 最小应用描述 */
function app(id: string, patch: Partial<ShellApp> = {}): ShellApp {
  return {
    id,
    name: id,
    version: '1.0.0',
    state: 'stopped',
    permissions: [],
    contributions: {},
    ...patch,
  }
}

/** 最小数据源（list 返回给定清单，launch/stop 记录调用） */
function source(id: string, apps: ShellApp[]): ShellAppSource {
  return {
    id,
    list: async () => apps,
    launch: async () => {},
    stop: async () => {},
  }
}

const registry = getShellRegistry()
/** 每个用例用独立源 id，避免相互污染（注册表是全局单例） */
let sourceId: string
let counter = 0

beforeEach(() => {
  counter += 1
  sourceId = `src-${counter}`
})

afterEach(() => {
  registry.clearSource(sourceId)
})

describe('ShellRegistry 应用清单', () => {
  it('should_exposeAppWithMergedContributions_when_upsertedFromSource', async () => {
    registry.upsertApps(sourceId, [app('a1')])

    const list = registry.appsRef.value
    expect(list).toHaveLength(1)
    expect(list[0].id).toBe('a1')
    expect(list[0].contributions.slots).toEqual([])
  })

  it('should_dropAppsMissingFromLatestList_when_sourceResyncs', async () => {
    registry.upsertApps(sourceId, [app('a1'), app('a2')])
    expect(registry.appsRef.value.map((a) => a.id)).toEqual(['a1', 'a2'])

    // 第二次只返回 a2：a1 视为已卸载，必须回收，否则首页会留残桩
    registry.upsertApps(sourceId, [app('a2')])
    expect(registry.appsRef.value.map((a) => a.id)).toEqual(['a2'])
  })

  it('should_keepOtherSourceApps_when_oneSourceResyncs', async () => {
    const other = 'src-other'
    registry.upsertApps(other, [app('b1')])
    registry.upsertApps(sourceId, [app('a1')])

    registry.upsertApps(sourceId, [])
    expect(registry.appsRef.value.map((a) => a.id)).toEqual(['b1'])
    registry.clearSource(other)
  })

  it('should_patchOnlyGivenFields_when_patchAppCalled', async () => {
    registry.upsertApps(sourceId, [app('a1', { name: 'origin' })])
    registry.patchApp('a1', { state: 'running' })

    const patched = registry.getApp('a1')
    expect(patched?.state).toBe('running')
    // 未给字段保持原值（补丁是浅合并，不是替换）
    expect(patched?.name).toBe('origin')
  })

  it('should_returnSourceOfApp_when_appWrittenByThatSource', async () => {
    const src = source(sourceId, [])
    const disposable = registry.registerSource(src)
    registry.upsertApps(sourceId, [app('a1')])

    expect(registry.getSourceOf('a1')?.id).toBe(sourceId)
    expect(registry.getSourceOf('missing')).toBeUndefined()
    disposable.dispose()
  })
})

describe('ShellRegistry 界面贡献', () => {
  it('should_mergeIncrementalAndManifestContributions_when_bothProvided', async () => {
    registry.upsertApps(sourceId, [
      app('a1', { contributions: { slots: [{ id: 'from-manifest', component: comp('ManifestSlot') }] } }),
    ])
    registry.registerSlot('a1', { id: 'from-runtime', component: comp('RuntimeSlot'), order: 5 })

    const slots = registry.getApp('a1')?.contributions.slots ?? []
    expect(slots.map((s) => s.id)).toEqual(['from-runtime', 'from-manifest'])
  })

  it('should_sortContributionsByOrder_when_orderGiven', async () => {
    registry.upsertApps(sourceId, [app('a1')])
    registry.registerSlot('a1', { id: 'late', component: comp('S1'), order: 200 })
    registry.registerSlot('a1', { id: 'early', component: comp('S2'), order: 10 })
    registry.registerSlot('a1', { id: 'default', component: comp('S3') })

    // 未给 order 的按默认 100 参与排序，落在 10 与 200 之间
    expect(registry.getApp('a1')?.contributions.slots?.map((s) => s.id)).toEqual([
      'early',
      'default',
      'late',
    ])
  })

  it('should_removeContribution_when_disposableDisposed', async () => {
    registry.upsertApps(sourceId, [app('a1')])
    const slot = registry.registerSlot('a1', { id: 's1', component: comp('S1') })
    expect(registry.getApp('a1')?.contributions.slots).toHaveLength(1)

    slot.dispose()
    expect(registry.getApp('a1')?.contributions.slots).toHaveLength(0)
  })

  it('should_removeAllContributionsAndRecord_when_clearAppCalled', async () => {
    registry.upsertApps(sourceId, [app('a1')])
    registry.registerSurface('a1', { component: comp('Surface') })
    registry.registerCapsuleItem('a1', { id: 'c1', label: 'x' })
    registry.registerSettingsEntry('a1', { id: 'e1', label: 'y' })
    expect(registry.getApp('a1')?.contributions.surface).toBeDefined()

    registry.clearApp('a1')
    expect(registry.getApp('a1')).toBeUndefined()
  })

  it('should_clearOnlyTargetSource_when_clearSourceCalled', async () => {
    registry.upsertApps(sourceId, [app('a1')])
    registry.upsertApps('src-keep', [app('b1')])

    registry.clearSource(sourceId)
    expect(registry.appsRef.value.map((a) => a.id)).toEqual(['b1'])
    registry.clearSource('src-keep')
  })

  it('should_notWrapComponentInReactiveProxy_when_componentRegistered', async () => {
    registry.upsertApps(sourceId, [app('a1')])
    registry.registerSlot('a1', { id: 's1', component: comp('S1') })

    // 组件进响应式数组会被 Vue 深代理，渲染时报 "Component was made a reactive object"
    const slot = registry.appsRef.value[0].contributions.slots?.[0]
    expect(isReactive(slot?.component)).toBe(false)
    // 应用模型本身是响应式的（状态变化要驱动 UI），只有组件被豁免
    expect(isReactive(registry.appsRef.value[0])).toBe(true)
  })
})
