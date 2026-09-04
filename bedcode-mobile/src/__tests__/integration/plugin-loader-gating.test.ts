/**
 * 插件启动加载门禁测试（spec §3.5 / issue 02）
 *
 * 被测行为（loader.ts:80-114 实际实现）：
 * - isEnabled（持久化意图）= false → 跳过（意图优先，避免重启丢扩展点）
 * - Activating 中间态 → 跳过（等最终态轮询兜底）
 * - pluginType: 'rust' → 跳过（后端自管）
 * - 其余状态（Activated / Degraded / Loaded / Error / NeedsApproval / Deactivated）
 *   + isEnabled=true → 放行调 loadFrontend（vitest 内动态 import 必失败 → 走
 *   markError 失败恢复路径作为「已尝试加载」探针）
 * - Degraded 在放行前 console.warn 标注降级原因（loader.ts:109-111）
 *
 * 与桌面 issue 02 plugin-loader-gating.test.ts 行为差异（spec §3.5 移动端裁决）：
 * 桌面端严格 2 态门禁（Activated/Degraded 放行，其余跳过），移动端为「意图优先」
 * 模式——已启用但未就绪的插件（Loaded/Error 等）仍尝试挂载前端模块以保留
 * 扩展点可见性，待后端就绪后由轮询补载。
 *
 * 测试 seam：mock @tauri-apps/api/core 边界（loader 走 invoke 命令字符串拼装）。
 * 「尝试加载前端模块」的可观测结果 = 动态 import 在 vitest 内必然失败（asset://
 * 协议无服务器可解析）→ loader 失败恢复路径调用 plugin_mark_error。
 * 因此断言：mark_error 恰好出现在门禁放行的插件上，跳过的插件零调用；
 * Degraded 的降级原因经 console.warn 上报（含插件 id 与原始错误串）。
 *
 * 注：移动端 issue 04 诊断命令（spec §3.6 P2）未实施，无 plugin_frontend_load_report
 * → 不写「诊断失败不阻塞」case（issue 04 P2 范围外可拆票）
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { pluginLoader } from '@/plugin/loader'
import {
  makePluginInfo,
  makeDegradedPluginInfo,
  makeActivatingPluginInfo,
} from '@/__tests__/fixtures'

// ==================== mock Tauri 边界 ====================

const mockInvoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
  // asset protocol 路径转换：loader 用它构造插件入口 URL（测试环境无 asset 服务器，
  // 返回合法协议串即可让 loader 走到动态 import 步骤并稳定失败）
  convertFileSrc: (p: string) => `asset://localhost/${p}`,
}))

/** 取某插件的 plugin_mark_error 调用次数 */
function markErrorCount(pluginId: string): number {
  return mockInvoke.mock.calls.filter(
    ([c, a]) => c === 'plugin_mark_error' && (a as { pluginId?: string })?.pluginId === pluginId,
  ).length
}

let consoleWarnSpy: ReturnType<typeof vi.spyOn>
let consoleLogSpy: ReturnType<typeof vi.spyOn>
let consoleErrorSpy: ReturnType<typeof vi.spyOn>

beforeEach(() => {
  vi.clearAllMocks()
  // 默认 mock 行为：所有命令静默返回，plugin_is_enabled=true
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'plugin_list_loaded':
        return Promise.resolve([])
      case 'plugin_is_enabled':
        return Promise.resolve(true)
      case 'plugin_mark_error':
        return Promise.resolve(undefined)
      default:
        return Promise.resolve(undefined)
    }
  })
  // 静音预期内的 log/warn/error（动态 import 失败、降级 warn 等）
  consoleWarnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
  consoleLogSpy = vi.spyOn(console, 'log').mockImplementation(() => {})
  consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {})
})

afterEach(async () => {
  // loader 单例跨用例清理：尝试对所有测试用插件 deactivate（idempotent，失败 ignore）
  for (const id of [
    'com.bedcode.gate-ok',
    'com.bedcode.gate-degraded',
    'com.bedcode.gate-activating',
    'com.bedcode.gate-loaded',
    'com.bedcode.gate-error',
    'com.bedcode.gate-approval',
    'com.bedcode.gate-off',
    'com.bedcode.gate-rust',
  ]) {
    await pluginLoader.deactivate(id).catch(() => {})
  }
  consoleWarnSpy.mockRestore()
  consoleLogSpy.mockRestore()
  consoleErrorSpy.mockRestore()
})

describe('pluginLoader.loadAll 启动加载门禁', () => {
  it('意图优先模式：6 态放行 + Activating/rust 跳过，Degraded 标注降级原因', async () => {
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
            error: DEGRADED_REASON,
          }),
          makeActivatingPluginInfo({ id: 'com.bedcode.gate-activating', name: 'Gate Activating' }),
          makePluginInfo({
            id: 'com.bedcode.gate-loaded',
            name: 'Gate Loaded',
            state: { state: 'Loaded' },
          }),
          makePluginInfo({
            id: 'com.bedcode.gate-error',
            name: 'Gate Error',
            state: { state: 'Error', error: 'boom' },
          }),
          makePluginInfo({
            id: 'com.bedcode.gate-approval',
            name: 'Gate Approval',
            state: { state: 'NeedsApproval' },
          }),
          makePluginInfo({
            id: 'com.bedcode.gate-off',
            name: 'Gate Off',
            state: { state: 'Deactivated' },
          }),
          // rust 类型：后端自管，跳过（loader.ts:82-86）
          makePluginInfo({
            id: 'com.bedcode.gate-rust',
            name: 'Gate Rust',
            pluginType: 'rust',
            state: { state: 'Activated' },
          }),
        ])
      }
      if (cmd === 'plugin_is_enabled') {
        return Promise.resolve(true)
      }
      return Promise.resolve(undefined)
    })

    await pluginLoader.loadAll()

    // 失败恢复是 async：等几个 tick 让 markError 微任务跑完
    await new Promise((r) => setTimeout(r, 0))
    await new Promise((r) => setTimeout(r, 0))

    // 意图优先模式：6 状态（Activated/Degraded/Loaded/Error/NeedsApproval/Deactivated）
    // 全部放行调 loadFrontend → vitest 内动态 import 必失败 → 各 1 次 markError
    expect(markErrorCount('com.bedcode.gate-ok')).toBe(1)
    expect(markErrorCount('com.bedcode.gate-degraded')).toBe(1)
    expect(markErrorCount('com.bedcode.gate-loaded')).toBe(1)
    expect(markErrorCount('com.bedcode.gate-error')).toBe(1)
    expect(markErrorCount('com.bedcode.gate-approval')).toBe(1)
    expect(markErrorCount('com.bedcode.gate-off')).toBe(1)
    // 跳过：Activating 中间态 + pluginType: 'rust'
    expect(markErrorCount('com.bedcode.gate-activating')).toBe(0)
    expect(markErrorCount('com.bedcode.gate-rust')).toBe(0)

    // Degraded 的降级原因经 console.warn 标注（含插件 id 与原始错误串）
    const warns = consoleWarnSpy.mock.calls.map((args) => String(args[0]))
    const degradedWarn = warns.find(
      (w) => w.includes('com.bedcode.gate-degraded') && w.toLowerCase().includes('degraded'),
    )
    expect(degradedWarn).toBeTruthy()
    expect(degradedWarn!).toContain(DEGRADED_REASON)
  })
})
