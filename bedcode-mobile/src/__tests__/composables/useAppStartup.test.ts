/**
 * useAppStartup 单测:开屏退出策略纯函数 + 启动任务注册表
 */
import { describe, it, expect, vi } from 'vitest'
import { SPLASH_CONFIG } from '@/config/splash'
import { computeSplashExitAt } from '@/composables/useAppStartup'

describe('computeSplashExitAt 开屏退出策略', () => {
  const MIN = 2800
  const MAX = 8000

  it('启动快于固定时长:补足到 minExitAt', () => {
    expect(computeSplashExitAt({ now: 1000, readyAt: 900, minExitAt: MIN, maxExitAt: MAX })).toBe(MIN)
  })

  it('启动快于固定时长但 minExitAt 已过:立即退出(返回 now)', () => {
    expect(computeSplashExitAt({ now: 3000, readyAt: 900, minExitAt: MIN, maxExitAt: MAX })).toBe(3000)
  })

  it('就绪落在 min~max 区间:就绪即退', () => {
    expect(computeSplashExitAt({ now: 1000, readyAt: 4000, minExitAt: MIN, maxExitAt: MAX })).toBe(4000)
  })

  it('就绪落在区间但时刻已过:返回 now', () => {
    expect(computeSplashExitAt({ now: 4200, readyAt: 4000, minExitAt: MIN, maxExitAt: MAX })).toBe(4200)
  })

  it('尚未就绪:由兜底时刻接管', () => {
    expect(computeSplashExitAt({ now: 1000, readyAt: null, minExitAt: MIN, maxExitAt: MAX })).toBe(MAX)
  })

  it('尚未就绪且兜底时刻已过:立即退出', () => {
    expect(computeSplashExitAt({ now: 8500, readyAt: null, minExitAt: MIN, maxExitAt: MAX })).toBe(8500)
  })

  it('就绪晚于兜底:视同兜底已过,返回 now', () => {
    expect(computeSplashExitAt({ now: 8600, readyAt: 8200, minExitAt: MIN, maxExitAt: MAX })).toBe(8600)
  })
})

describe('启动任务注册表', () => {
  // 模块含全局启动状态:resetModules + 动态 import 获取隔离实例
  async function loadModule() {
    vi.resetModules()
    return await import('@/composables/useAppStartup')
  }

  it('注册表键与 SPLASH_CONFIG.lines 一一对应(初始均为 null)', async () => {
    const m = await loadModule()
    for (const line of SPLASH_CONFIG.lines) {
      expect(m.taskCompletedAt[line.id]).toBeNull()
    }
  })

  it('未知 id 打点被忽略,不影响就绪状态', async () => {
    const m = await loadModule()
    m.completeStartupTask('nonexistent')
    expect(m.startupReady.value).toBe(false)
    expect(m.readyAt.value).toBeNull()
  })

  it('重复打点幂等:首打点时刻不被覆盖', async () => {
    const m = await loadModule()
    const id = SPLASH_CONFIG.lines[0].id
    m.completeStartupTask(id)
    const first = m.taskCompletedAt[id]
    expect(first).not.toBeNull()
    await new Promise((r) => setTimeout(r, 3))
    m.completeStartupTask(id)
    expect(m.taskCompletedAt[id]).toBe(first)
  })

  it('全部任务完成后 startupReady 为真且 readyAt 非空', async () => {
    const m = await loadModule()
    for (const line of SPLASH_CONFIG.lines) {
      m.completeStartupTask(line.id)
    }
    expect(m.startupReady.value).toBe(true)
    expect(m.readyAt.value).not.toBeNull()
  })
})
