/**
 * 应用运行期 store 作用域单元测试（行为契约）
 *
 * 被测：src/plugin/app-run-scope.ts —— 宿主把插件 store 绑到「该插件当前运行期」的
 * Pinia 上，停止应用（dispose）即作废本次运行期的全部 store，再次启动是全新 store。
 *
 * 契约来源：`.scratch/2026-10-14-mobile-app-run-store-scope/spec.md` §3（C-01..C-14）。
 * 用替身：无（真实 pinia 2.x）。logger 只做断言用 spy。
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { logger } from '@/utils/frontendLogger'
import {
  _resetAppRunScopesForTest,
  disposeAppRunScope,
  loadPluginModuleScoped,
  scopedDefineStore,
} from '@/plugin/app-run-scope'

/** 计数 store 形状（测试用最小契约） */
interface CounterStore {
  n: number
  $dispose: () => void
}

type UseCounterStore = (() => CounterStore) & { $id?: string }

/** 在装载窗口内定义 store（复刻插件模块顶层 defineStore 的调用位置） */
async function defineCounterIn(pluginId: string, storeId: string): Promise<UseCounterStore> {
  const useStore = await loadPluginModuleScoped(pluginId, async () =>
    scopedDefineStore(storeId, () => ({ n: 0 })),
  )
  return useStore as UseCounterStore
}

beforeEach(() => {
  // 作用域表是模块级单例（生产语义：一个插件同时只有一个运行期作用域），
  // 测试间必须复位，否则上一个用例遗留的作用域会污染计数与隔离断言
  _resetAppRunScopesForTest()
  // 宿主 Pinia：窗口外回退路径需要 activePinia 才能建 store
  setActivePinia(createPinia())
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('app-run-scope：运行期 store 作用域', () => {
  it('should_reuseStoreWithinRun_andResetToInitialState_afterDispose', async () => {
    const useStore = await defineCounterIn('plugin-a', 'counter')
    expect(useStore.$id).toBe('counter') // 与 Pinia defineStore 返回形状对齐（C-01 副作用）

    const first = useStore()
    first.n = 5
    expect(useStore()).toBe(first) // 同一运行期内复用同一实例

    disposeAppRunScope('plugin-a')

    const second = useStore()
    expect(second).not.toBe(first) // 旧实例已从 Pinia 摘除
    expect(second.n).toBe(0) // 回到 setup 初始 state
    expect(useStore()).toBe(second)
  })

  it('should_startFreshRun_afterStopThenRestart', async () => {
    const useStore = await defineCounterIn('plugin-restart', 'counter')
    useStore().n = 7

    // 停止（含重复停止：幂等，不抛）
    disposeAppRunScope('plugin-restart')
    expect(() => disposeAppRunScope('plugin-restart')).not.toThrow()

    // 再次「启动」：同一个 useStore 单例（模块缓存语义），新运行期 = 新作用域
    expect(useStore().n).toBe(0)
  })

  it('should_isolateStores_betweenPlugins_evenWithSameStoreId', async () => {
    const useA = await defineCounterIn('plugin-a', 'same-id')
    const useB = await defineCounterIn('plugin-b', 'same-id')
    const a = useA()
    const b = useB()
    a.n = 3
    b.n = 5

    disposeAppRunScope('plugin-a')

    expect(useA()).not.toBe(a) // A 已作废
    expect(useA().n).toBe(0)
    expect(useB()).toBe(b) // B 不受 A 的停止影响
    expect(useB().n).toBe(5)
  })

  it('should_serializeModuleLoads_soLaterLoadStartsAfterEarlierEnds', async () => {
    const events: string[] = []
    let releaseFirst!: () => void
    const firstGate = new Promise<void>((resolve) => {
      releaseFirst = resolve
    })

    const first = loadPluginModuleScoped('load-a', async () => {
      events.push('a:start')
      await firstGate
      events.push('a:end')
    })
    const second = loadPluginModuleScoped('load-b', async () => {
      events.push('b:start')
    })

    await new Promise((resolve) => setTimeout(resolve, 0))
    // 反例：未串行化时 b:start 会插在 a:end 之前（装载窗口互相覆盖）
    expect(events).toEqual(['a:start'])

    releaseFirst()
    await Promise.all([first, second])
    expect(events).toEqual(['a:start', 'a:end', 'b:start'])
  })

  it('should_bindStoreToOwnPlugin_whenLoadsAreIssuedConcurrently', async () => {
    const [useA, useB] = await Promise.all([
      defineCounterIn('conc-a', 'conc'),
      defineCounterIn('conc-b', 'conc'),
    ])

    useA().n = 11

    // 串绑（两个定义落到同一作用域）时 B 会读到 A 写的 11
    expect(useB().n).toBe(0)
    disposeAppRunScope('conc-b')
    expect(useA().n).toBe(11) // 反过来：停 B 不影响 A
  })

  it('should_fallBackToHostPiniaWithWarn_whenDefineStoreOutsideLoadWindow', () => {
    const warn = vi.spyOn(logger, 'warn').mockImplementation(() => {})

    const useHostStore = scopedDefineStore('hostless', () => ({ n: 0 })) as UseCounterStore

    expect(warn).toHaveBeenCalledTimes(1)
    expect(String(warn.mock.calls[0]?.[0])).toContain('hostless')

    const store = useHostStore()
    store.n = 4
    disposeAppRunScope('never-a-plugin')

    // 不在任何插件作用域 → 宿主 Pinia 生命周期，应用停止不作用于它
    expect(useHostStore()).toBe(store)
    expect(useHostStore().n).toBe(4)
  })

  it('should_disposeRemainingStores_andLogError_whenOneDisposeThrows', async () => {
    const useA = await defineCounterIn('plugin-multi', 'multi-a')
    const useB = await defineCounterIn('plugin-multi', 'multi-b')
    const a = useA()
    const b = useB()
    a.$dispose = () => {
      throw new Error('dispose boom')
    }
    const error = vi.spyOn(logger, 'error').mockImplementation(() => {})

    disposeAppRunScope('plugin-multi')

    expect(error).toHaveBeenCalledTimes(1)
    expect(useA()).not.toBe(a) // 抛错的 store 本身也被摘册
    expect(useB()).not.toBe(b) // 后续 store 照常作废（不被中断）
    expect(useB().n).toBe(0)
  })

  it('should_leaveOtherPluginsUntouched_whenDisposingUnknownPlugin', async () => {
    const useStore = await defineCounterIn('plugin-live', 'counter')
    const store = useStore()
    store.n = 8

    expect(() => disposeAppRunScope('no-such-plugin')).not.toThrow()

    expect(useStore()).toBe(store)
    expect(useStore().n).toBe(8)
  })

  it('should_supportOptionsStyleDefineStore', async () => {
    const useOptions = (await loadPluginModuleScoped('plugin-options', async () =>
      scopedDefineStore({ id: 'options-store', state: () => ({ n: 0 }) }),
    )) as UseCounterStore

    useOptions().n = 9

    disposeAppRunScope('plugin-options')

    expect(useOptions().n).toBe(0)
  })
})
