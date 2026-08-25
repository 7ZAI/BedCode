/**
 * 插件启动加载门禁测试（spec §3.6 / issue 02）+ 前端加载诊断上报（issue 04）
 *
 * 被测行为：pluginLoader.loadAll 按后端返回的插件状态决定是否加载前端模块——
 * - Activated：放行（现状）
 * - Degraded：也放行（实例在运行、扩展点已注册），且 console.warn 标注降级原因
 * - 其余状态（Activating 中间态 / Loaded / Error / NeedsApproval / Deactivated）：
 *   跳过；rust 类型（后端自管）与 isolated sandbox 同样跳过
 *
 * issue 04：放行插件的导入失败路径经 plugin_frontend_load_report 上报一次
 * （宿主内部诊断通道，仅写 tracing 不入状态机）；跳过的插件零上报；
 * 诊断命令自身失败被吞掉，不阻塞加载流程。
 *
 * 测试 seam（与 plugin-flow.test.ts 同模式）：只 mock @tauri-apps/api 边界。
 * 「尝试加载前端模块」的可观测结果 = 动态 import 在 vitest 内必然失败（asset://
 * 协议无服务器可解析）→ loader 失败恢复路径调用 plugin_mark_error。
 * 因此断言：mark_error 恰好出现在门禁放行的插件上，跳过的插件零调用；
 * Degraded 的降级原因经 console.warn 上报（含插件 id 与原始错误串）。
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { pluginLoader } from '@/plugin/loader'
import {
  makePluginInfo,
  makeDegradedPluginInfo,
  makeActivatingPluginInfo,
} from '@/__tests__/fixtures/index'

// ==================== mock Tauri 边界 ====================

const mockInvoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
  // asset protocol 路径转换：loader 用它构造插件入口 URL（测试环境无 asset 服务器，
  // 返回合法协议串即可让 loader 走到动态 import 步骤并稳定失败）
  convertFileSrc: (p: string) => `asset://localhost/${p}`,
}))

function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'plugin_mark_error':
        return Promise.resolve(undefined)
      default:
        return Promise.resolve(undefined)
    }
  })
}

/** 取某插件的 plugin_mark_error 调用次数 */
function markErrorCount(pluginId: string): number {
  return mockInvoke.mock.calls.filter(
    ([c, args]) => c === 'plugin_mark_error' && args?.pluginId === pluginId,
  ).length
}

/** 取某插件的 plugin_frontend_load_report 上报参数列表（issue 04 诊断通道） */
function frontendReports(pluginId: string): Array<Record<string, unknown>> {
  return mockInvoke.mock.calls
    .filter(([c, args]) => c === 'plugin_frontend_load_report' && args?.pluginId === pluginId)
    .map(([, args]) => args as Record<string, unknown>)
}

let consoleWarnSpy: ReturnType<typeof vi.spyOn>
let consoleLogSpy: ReturnType<typeof vi.spyOn>
let consoleErrorSpy: ReturnType<typeof vi.spyOn>

beforeEach(() => {
  vi.clearAllMocks()
  installInvokeMock()
  // 静音预期内的 log/warn/error（动态 import 失败、降级 warn 等）
  consoleWarnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
  consoleLogSpy = vi.spyOn(console, 'log').mockImplementation(() => {})
  consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
})

afterEach(() => {
  // loader 单例跨用例清理：导入失败的插件不会入 map，兜底清一次
  pluginLoader.deactivate('com.bedcode.gate-ok').catch(() => {})
  pluginLoader.deactivate('com.bedcode.gate-degraded').catch(() => {})
  consoleWarnSpy.mockRestore()
  consoleLogSpy.mockRestore()
  consoleErrorSpy.mockRestore()
})

describe('pluginLoader.loadAll 启动加载门禁', () => {
  it('Activated/Degraded 放行前端模块加载，其余状态跳过', async () => {
    const DEGRADED_REASON = 'startup init failed: db migration error'

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'plugin_list_loaded') {
        return Promise.resolve([
          makePluginInfo({
            id: 'com.bedcode.gate-ok',
            name: 'Gate Ok',
            state: { state: 'Activated' },
          }),
          makeDegradedPluginInfo({
            id: 'com.bedcode.gate-degraded',
            name: 'Gate Degraded',
            state: { state: 'Degraded', error: DEGRADED_REASON },
          }),
          makeActivatingPluginInfo({ id: 'com.bedcode.gate-activating' }),
          makePluginInfo({ id: 'com.bedcode.gate-loaded', state: { state: 'Loaded' } }),
          makePluginInfo({
            id: 'com.bedcode.gate-error',
            state: { state: 'Error', error: 'boom' },
          }),
          makePluginInfo({ id: 'com.bedcode.gate-approval', state: { state: 'NeedsApproval' } }),
          makePluginInfo({ id: 'com.bedcode.gate-off', state: { state: 'Deactivated' } }),
          // rust 类型：后端自管，即使 Activated 也跳过
          makePluginInfo({
            id: 'com.bedcode.gate-rust',
            pluginType: 'rust',
            state: { state: 'Activated' },
          }),
          // isolated sandbox 不支持，跳过
          makePluginInfo({
            id: 'com.bedcode.gate-isolated',
            sandbox: 'isolated',
            state: { state: 'Activated' },
          }),
        ])
      }
      return Promise.resolve(undefined)
    })

    await pluginLoader.loadAll()

    // 门禁放行：仅 Activated 与 Degraded 尝试加载前端模块
    // （vitest 内动态 import 必然失败 → 走 mark_error 失败恢复路径，恰好作为「已尝试」探针）
    expect(markErrorCount('com.bedcode.gate-ok')).toBe(1)
    expect(markErrorCount('com.bedcode.gate-degraded')).toBe(1)
    // 其余状态一律跳过
    expect(markErrorCount('com.bedcode.gate-activating')).toBe(0)
    expect(markErrorCount('com.bedcode.gate-loaded')).toBe(0)
    expect(markErrorCount('com.bedcode.gate-error')).toBe(0)
    expect(markErrorCount('com.bedcode.gate-approval')).toBe(0)
    expect(markErrorCount('com.bedcode.gate-off')).toBe(0)
    expect(markErrorCount('com.bedcode.gate-rust')).toBe(0)
    expect(markErrorCount('com.bedcode.gate-isolated')).toBe(0)

    // Degraded 的降级原因经 console.warn 标注（含插件 id 与原始错误串）
    const warns = consoleWarnSpy.mock.calls.map((args) => String(args[0]))
    const degradedWarn = warns.find(
      (w) => w.includes('DEGRADED') && w.includes('com.bedcode.gate-degraded'),
    )
    expect(degradedWarn).toBeTruthy()
    expect(degradedWarn!).toContain(DEGRADED_REASON)

    // 诊断上报（issue 04）：放行的插件在导入失败路径各上报一次（vitest 内动态
    // import 必然失败 → stage=import, ok=false）；跳过的插件零上报
    const okReports = frontendReports('com.bedcode.gate-ok')
    expect(okReports).toHaveLength(1)
    expect(okReports[0]).toMatchObject({ stage: 'import', ok: false })
    expect(typeof okReports[0].detail).toBe('string')

    expect(frontendReports('com.bedcode.gate-degraded')).toHaveLength(1)
    for (const skipped of [
      'com.bedcode.gate-activating',
      'com.bedcode.gate-loaded',
      'com.bedcode.gate-error',
      'com.bedcode.gate-approval',
      'com.bedcode.gate-off',
      'com.bedcode.gate-rust',
      'com.bedcode.gate-isolated',
    ]) {
      expect(frontendReports(skipped)).toHaveLength(0)
    }
  })

  it('诊断命令失败不阻塞加载流程（issue 04）：mark_error 恢复路径照常执行', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'plugin_list_loaded') {
        return Promise.resolve([
          makePluginInfo({ id: 'com.bedcode.diag-fail', name: 'Diag Fail' }),
        ])
      }
      if (cmd === 'plugin_frontend_load_report') {
        return Promise.reject(new Error('bridge unavailable'))
      }
      return Promise.resolve(undefined)
    })

    // 诊断 invoke 被吞掉：loadAll 不应向外抛 rejection
    await expect(pluginLoader.loadAll()).resolves.toBeUndefined()

    // 加载流程未被阻塞：失败恢复路径（mark_error）仍执行，且诊断仍尝试过一次
    expect(markErrorCount('com.bedcode.diag-fail')).toBe(1)
    expect(frontendReports('com.bedcode.diag-fail')).toHaveLength(1)
  })
})
