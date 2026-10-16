/**
 * dev-shell 宿主壳注册表 行为契约测试
 * -----------------------------------------------------------------------------
 * 契约来源：dev-shell/src/shell/registry.ts（与宿主 src/shell/registry.ts 同构）
 *
 * 覆盖：清单全量覆盖与回收、增量贡献注册/摘除/合并、order 排序、
 *       组件免响应式代理（markRaw）、数据源枚举。
 *
 * 注册表是全局单例：每个用例用独立源 id，并在 afterEach 清源，避免相互污染。
 */
import { describe, it, expect, afterEach } from 'vitest'
import { defineComponent, h, isReactive, type Component } from 'vue'
import { getShellRegistry } from '../../src/shell/registry'
import type { ShellApp, ShellAppSource } from '../../src/shell/types'

/** 测试用组件（带可识别 name，便于断言拿到的是哪一个） */
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

/** 最小数据源 */
function source(id: string, apps: ShellApp[]): ShellAppSource {
  return {
    id,
    list: async () => apps,
    launch: async () => {},
    stop: async () => {},
  }
}

const registry = getShellRegistry()
let sourceId: string
let counter = 0

/** 每个用例一个独立源 id */
function nextSourceId(): string {
  counter += 1
  sourceId = `test-src-${counter}`
  return sourceId
}

afterEach(() => {
  registry.clearSource(sourceId)
})

describe('清单写入与回收', () => {
  it('should_exposeApps_when_upsertedFromSource', () => {
    const id = nextSourceId()
    registry.upsertApps(id, [app('a1'), app('a2')])

    expect(registry.appsRef.value.map((a) => a.id)).toEqual(['a1', 'a2'])
    expect(registry.getApp('a1')?.name).toBe('a1')
  })

  it('should_recycleOnlyOwnApps_when_sameSourceOmitsOne', () => {
    const id = nextSourceId()
    registry.upsertApps(id, [app('a1'), app('a2')])

    // 第二次清单不再包含 a2 → 视为已卸载
    registry.upsertApps(id, [app('a1')])

    expect(registry.getApp('a2')).toBeUndefined()
    expect(registry.getApp('a1')).toBeDefined()
  })

  it('should_keepOtherSourceApps_when_clearingOneSource', () => {
    const id = nextSourceId()
    const other = `${id}-other`
    registry.upsertApps(id, [app('mine')])
    registry.upsertApps(other, [app('theirs')])
    sourceId = other // afterEach 只清这一个，另一个留给下一用例不共享

    registry.clearSource(id)

    expect(registry.getApp('mine')).toBeUndefined()
    expect(registry.getApp('theirs')).toBeDefined()
    registry.clearSource(other)
  })

  it('should_patchState_when_patchAppCalled', () => {
    const id = nextSourceId()
    registry.upsertApps(id, [app('a1')])

    registry.patchApp('a1', { state: 'running' })

    expect(registry.getApp('a1')?.state).toBe('running')
  })

  it('should_ignorePatch_when_appUnknown', () => {
    const id = nextSourceId()
    registry.upsertApps(id, [])

    // 未注册应用不应凭空出现
    registry.patchApp('ghost', { state: 'running' })

    expect(registry.getApp('ghost')).toBeUndefined()
  })
})

describe('贡献注册与摘除', () => {
  it('should_exposeSurface_when_registered', () => {
    const id = nextSourceId()
    const surface = comp('Surface')
    registry.upsertApps(id, [app('a1')])

    registry.registerSurface('a1', { component: surface })

    expect(registry.getApp('a1')?.contributions.surface?.component).toBe(surface)
  })

  it('should_removeSurfaceOnly_after_dispose', () => {
    const id = nextSourceId()
    registry.upsertApps(id, [app('a1')])
    const d1 = registry.registerSurface('a1', { component: comp('S1') })
    registry.registerSurface('a1', { component: comp('S2') })

    // 旧注册的 dispose 不得摘掉后注册的那一个
    d1.dispose()
    expect(registry.getApp('a1')?.contributions.surface?.component).not.toBeUndefined()

    const d2 = registry.registerSurface('a1', { component: comp('S3') })
    d2.dispose()
    expect(registry.getApp('a1')?.contributions.surface).toBeUndefined()
  })

  it('should_sortSlotsByOrder_when_multipleRegistered', () => {
    const id = nextSourceId()
    registry.upsertApps(id, [app('a1')])

    registry.registerSlot('a1', { id: 'late', component: comp('Late') })
    registry.registerSlot('a1', { id: 'early', component: comp('Early'), order: 1 })

    expect(registry.getApp('a1')?.contributions.slots?.map((s) => s.id)).toEqual([
      'early',
      'late',
    ])
  })

  it('should_mergeManifestAndRuntimeContributions_when_bothProvided', () => {
    const id = nextSourceId()
    const fromManifest = comp('FromManifest')
    registry.upsertApps(id, [
      app('a1', {
        contributions: { slots: [{ id: 's1', component: fromManifest }] },
      }),
    ])

    registry.registerSlot('a1', { id: 's2', component: comp('FromRuntime') })

    expect(registry.getApp('a1')?.contributions.slots?.map((s) => s.id)).toEqual(['s1', 's2'])
  })

  it('should_letRuntimeOverrideManifest_when_sameSlotId', () => {
    const id = nextSourceId()
    registry.upsertApps(id, [
      app('a1', { contributions: { slots: [{ id: 'dup', component: comp('Old') }] } }),
    ])

    registry.registerSlot('a1', { id: 'dup', component: comp('New') })

    const slots = registry.getApp('a1')?.contributions.slots ?? []
    expect(slots).toHaveLength(1)
    expect(slots[0].component).not.toBeUndefined()
  })

  it('should_exposeCapsuleAndSettingsEntries_when_registered', () => {
    const id = nextSourceId()
    registry.upsertApps(id, [app('a1')])

    registry.registerCapsuleItem('a1', { id: 'c1', label: 'Capsule' })
    registry.registerSettingsEntry('a1', { id: 'e1', label: 'Entry' })

    const view = registry.getApp('a1')
    expect(view?.contributions.capsuleItems?.map((c) => c.id)).toEqual(['c1'])
    expect(view?.contributions.settingsEntries?.map((e) => e.id)).toEqual(['e1'])
  })

  it('should_keepComponentsNonReactive_when_registered', () => {
    const id = nextSourceId()
    registry.upsertApps(id, [app('a1')])
    const surface = comp('Surface')

    registry.registerSurface('a1', { component: surface })

    // markRaw 缺失会让组件被包进 reactive 代理，破坏组件身份与性能
    expect(isReactive(registry.getApp('a1')?.contributions.surface?.component)).toBe(false)
  })
})

describe('数据源', () => {
  it('should_exposeSourceAndDropIt_when_disposed', () => {
    const id = nextSourceId()
    const d = registry.registerSource(source(id, [app('a1')]))

    expect(registry.listSources().map((s) => s.id)).toContain(id)
    d.dispose()
    expect(registry.listSources().map((s) => s.id)).not.toContain(id)
  })

  it('should_resolveSourceOfApp_when_appBelongsToIt', () => {
    const id = nextSourceId()
    const src = source(id, [app('a1')])
    registry.registerSource(src)
    registry.upsertApps(id, [app('a1')])

    expect(registry.getSourceOf('a1')).toBe(src)
    expect(registry.getSourceOf('ghost')).toBeUndefined()
  })
})