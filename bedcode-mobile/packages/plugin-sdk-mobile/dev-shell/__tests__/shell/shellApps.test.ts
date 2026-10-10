/**
 * dev-shell 壳应用视图模型 行为契约测试
 * -----------------------------------------------------------------------------
 * 契约来源：dev-shell/src/shell/composables/useShellApps.ts
 *
 * 覆盖：运行面解析优先级（自带 surface → 数据源延迟解析 → 空）、
 *       launch/stop 的状态回写与失败落态、未知应用不启动、
 *       逐项权限授予的「不支持即失败」、统计与排序口径。
 *
 * 依赖边界：数据源是唯一替身（跨进程边界的替身）；被测逻辑（状态机 / 排序 / 统计）
 * 一律跑真实现。
 */
import { describe, it, expect, afterEach, vi } from 'vitest'
import type { Disposable } from '../../src/shell/types'
import { defineComponent, h, type Component } from 'vue'
import { getShellRegistry } from '../../src/shell/registry'
import { useShellApps } from '../../src/shell/composables/useShellApps'
import type { ShellApp, ShellAppSource } from '../../src/shell/types'

function comp(name: string): Component {
  return defineComponent({ name, setup: () => () => h('div', name) })
}

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

/** 可控数据源：清单可改，launch/stop 可指定成功或抛错 */
function makeSource(id: string, initial: ShellApp[] = []) {
  const state = { apps: initial, launchImpl: async (_appId: string) => {}, stopImpl: async (_appId: string) => {} }
  const src: ShellAppSource = {
    id,
    list: async () => state.apps,
    launch: async (appId) => state.launchImpl(appId),
    stop: async (appId) => state.stopImpl(appId),
  }
  return { src, state }
}

const registry = getShellRegistry()
const apps = useShellApps()

let sourceId: string
let sourceDisposable: Disposable | null = null
let counter = 0
function nextSourceId(): string {
  counter += 1
  sourceId = `view-src-${counter}`
  return sourceId
}

/** 登记数据源并记下句柄（afterEach 要连同 clearSource 一起撤销） */
function addSource(src: ShellAppSource): void {
  sourceDisposable = registry.registerSource(src)
}

afterEach(() => {
  // clearSource 只清应用，源本身要经 dispose 摘除——否则它会在下一次 refresh
  // 里把应用重新写回来（这正是「来源是唯一真源」的表现）
  registry.clearSource(sourceId)
  sourceDisposable?.dispose()
  sourceDisposable = null
})

describe('运行面解析优先级', () => {
  it('should_preferOwnSurface_when_appRegisteredIt', () => {
    const id = nextSourceId()
    const own = comp('OwnSurface')
    const { src } = makeSource(id, [app('a1')])
    addSource({ ...src, resolveSurface: () => comp('FromSource') })
    registry.upsertApps(id, [app('a1')])
    registry.registerSurface('a1', { component: own })

    expect(apps.resolveSurface('a1')).toBe(own)
  })

  it('should_fallbackToSourceResolution_when_appHasNoSurface', () => {
    const id = nextSourceId()
    const fromSource = comp('FromSource')
    const { src } = makeSource(id, [app('a1')])
    addSource({ ...src, resolveSurface: () => fromSource })
    registry.upsertApps(id, [app('a1')])

    expect(apps.resolveSurface('a1')).toBe(fromSource)
  })

  it('should_returnUndefined_when_noSurfaceAnywhere', () => {
    const id = nextSourceId()
    const { src } = makeSource(id, [app('a1')])
    addSource(src)
    registry.upsertApps(id, [app('a1')])

    // 空态由运行屏渲染；解析层不做「找不到就回退另一种形态」
    expect(apps.resolveSurface('a1')).toBeUndefined()
  })

  it('should_returnUndefined_when_appUnknown', () => {
    nextSourceId()
    expect(apps.resolveSurface('ghost')).toBeUndefined()
  })
})

describe('启动与停止', () => {
  it('should_reflectSourceTruthAfterRefresh_when_launchSucceeds', async () => {
    const id = nextSourceId()
    const { src, state } = makeSource(id, [app('a1', { state: 'stopped' })])
    const launchSpy = vi.fn(async () => {
      // 真实数据源在启动后会把清单刷成运行态
      state.apps = [app('a1', { state: 'running' })]
    })
    state.launchImpl = launchSpy
    addSource(src)
    registry.upsertApps(id, state.apps)

    const ok = await apps.launch('a1')

    expect(ok).toBe(true)
    expect(launchSpy).toHaveBeenCalledWith('a1')
    expect(registry.getApp('a1')?.state).toBe('running')
    expect(apps.lastError.value).toBeNull()
  })

  it('should_overwriteOptimisticState_when_sourceStillReportsStopped', async () => {
    const id = nextSourceId()
    const { src } = makeSource(id, [app('a1', { state: 'stopped' })])
    addSource(src)
    registry.upsertApps(id, [app('a1', { state: 'stopped' })])

    // 启动过程先把界面乐观置为运行中，随后以数据源为准——数据源没变就不能一直显示「运行中」
    await apps.launch('a1')

    expect(registry.getApp('a1')?.state).toBe('stopped')
  })

  it('should_markErrorWithMessage_when_launchThrows', async () => {
    const id = nextSourceId()
    const { src, state } = makeSource(id, [app('a1')])
    state.launchImpl = async () => {
      throw new Error('boom')
    }
    addSource(src)
    registry.upsertApps(id, [app('a1')])

    const ok = await apps.launch('a1')

    expect(ok).toBe(false)
    expect(registry.getApp('a1')?.state).toBe('error')
    expect(registry.getApp('a1')?.error).toBe('boom')
    expect(apps.lastError.value).toBe('boom')
  })

  it('should_notCallSource_when_appUnknown', async () => {
    const id = nextSourceId()
    const { src, state } = makeSource(id, [])
    const launchSpy = vi.fn(async () => {})
    state.launchImpl = launchSpy
    addSource(src)

    const ok = await apps.launch('ghost')

    expect(ok).toBe(false)
    expect(launchSpy).not.toHaveBeenCalled()
    expect(apps.lastError.value).toBe('unknown app: ghost')
  })

  it('should_skipLaunch_when_appAlreadyRunning', async () => {
    const id = nextSourceId()
    const { src, state } = makeSource(id, [])
    const launchSpy = vi.fn(async () => {})
    state.launchImpl = launchSpy
    addSource(src)
    registry.upsertApps(id, [app('a1', { state: 'running' })])

    const ok = await apps.launch('a1')

    expect(ok).toBe(true)
    expect(launchSpy).not.toHaveBeenCalled()
  })

  it('should_reflectSourceTruthAfterRefresh_when_stopSucceeds', async () => {
    const id = nextSourceId()
    const { src, state } = makeSource(id, [app('a1', { state: 'running' })])
    const stopSpy = vi.fn(async () => {
      state.apps = [app('a1', { state: 'disabled' })]
    })
    state.stopImpl = stopSpy
    addSource(src)
    registry.upsertApps(id, state.apps)

    const ok = await apps.stop('a1')

    expect(ok).toBe(true)
    expect(stopSpy).toHaveBeenCalledWith('a1')
    // 停止后的终态由数据源给（dev-shell 数据源映射为「已停用」），不由壳臆造
    expect(registry.getApp('a1')?.state).toBe('disabled')
  })

  it('should_markError_when_stopThrows', async () => {
    const id = nextSourceId()
    const { src, state } = makeSource(id, [app('a1')])
    state.stopImpl = async () => {
      throw new Error('stop failed')
    }
    addSource(src)
    registry.upsertApps(id, [app('a1', { state: 'running' })])

    const ok = await apps.stop('a1')

    expect(ok).toBe(false)
    expect(registry.getApp('a1')?.state).toBe('error')
    expect(registry.getApp('a1')?.error).toBe('stop failed')
  })
})

describe('逐项权限授予', () => {
  it('should_returnFalseWithoutMutation_when_sourceDoesNotSupport', async () => {
    const id = nextSourceId()
    const { src } = makeSource(id, [app('a1', { permissions: [{ key: 'fs:read', granted: false }] })])
    addSource(src)
    registry.upsertApps(id, [
      app('a1', { permissions: [{ key: 'fs:read', granted: false }] }),
    ])

    expect(apps.supportsPermissionControl('a1')).toBe(false)
    const ok = await apps.setPermissionGrant('a1', 'fs:read', true)

    expect(ok).toBe(false)
    // 不做「本地写一下就假装生效」
    expect(registry.getApp('a1')?.permissions[0].granted).toBe(false)
  })

  it('should_patchGrant_when_sourceApplies', async () => {
    const id = nextSourceId()
    const { src } = makeSource(id, [app('a1')])
    addSource({
      ...src,
      setPermissionGrant: async (_appId, _key, granted) => granted,
    })
    registry.upsertApps(id, [
      app('a1', { permissions: [{ key: 'fs:read', granted: false }] }),
    ])

    const ok = await apps.setPermissionGrant('a1', 'fs:read', true)

    expect(ok).toBe(true)
    expect(registry.getApp('a1')?.permissions[0].granted).toBe(true)
  })

  it('should_returnFalse_when_sourceRejectsGrant', async () => {
    const id = nextSourceId()
    const { src } = makeSource(id, [app('a1')])
    addSource({ ...src, setPermissionGrant: async () => false })
    registry.upsertApps(id, [
      app('a1', { permissions: [{ key: 'fs:read', granted: true }] }),
    ])

    const ok = await apps.setPermissionGrant('a1', 'fs:read', false)

    expect(ok).toBe(false)
    expect(registry.getApp('a1')?.permissions[0].granted).toBe(true)
  })
})

describe('统计与排序', () => {
  it('should_reportTotalBytesUndefined_when_anyAppUnmeasured', async () => {
    const id = nextSourceId()
    const { src } = makeSource(id, [
      app('a1', { sizeBytes: 100 }),
      app('a2'),
    ])
    addSource(src)
    await apps.refresh()

    // 缺失值不得当 0 累加成假数字
    expect(apps.stats.value.totalBytes).toBeUndefined()
  })

  it('should_sumTotalBytes_when_allMeasured', async () => {
    const id = nextSourceId()
    const { src } = makeSource(id, [app('a1', { sizeBytes: 100 }), app('a2', { sizeBytes: 50 })])
    addSource(src)
    await apps.refresh()

    expect(apps.stats.value.totalBytes).toBe(150)
    expect(apps.stats.value.installed).toBe(2)
  })

  it('should_putRunningAppsFirst_when_sorted', async () => {
    const id = nextSourceId()
    const { src } = makeSource(id, [
      app('a-stopped', { name: 'A', state: 'stopped' }),
      app('b-running', { name: 'B', state: 'running' }),
    ])
    addSource(src)
    await apps.refresh()

    expect(apps.sortedApps.value.map((a) => a.id)).toEqual(['b-running', 'a-stopped'])
    expect(apps.runningApps.value.map((a) => a.id)).toEqual(['b-running'])
  })

  it('should_keepOtherSourcesOnFailure_when_oneSourceThrows', async () => {
    const good = `view-src-${counter + 1}`
    const bad = `view-src-${counter + 2}`
    counter += 2
    addSource(makeSource(good, [app('from-good')]).src)
    addSource({
      ...makeSource(bad, []).src,
      list: async () => {
        throw new Error('source down')
      },
    })

    await apps.refresh()

    // 一个源挂掉不该让整个首页空白
    expect(registry.getApp('from-good')).toBeDefined()
    expect(apps.lastError.value).toBe('source down')
  })
})