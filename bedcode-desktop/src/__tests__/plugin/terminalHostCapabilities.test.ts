/**
 * 宿主终端能力注入：背景图 URL 端口契约（2026-09-26 修复）
 *
 * 背景：`/static/terminal-bg` 挂在宿主主服务器上（端口由 supervisor 持有，用户可改），
 * 而插件侧 `getServerPort()` 是同步接口。旧实现硬编码 8080，与实际端口（默认 8765+，
 * 本机 8767）不符 → 背景图预加载必然失败（`Failed to resolve background image URL`）。
 *
 * 行为契约（来源：`provideTerminalHostCapabilities` 的端口实现 + 插件侧
 * `useTerminalSettingsSync.resolveBgImageUrl` 的调用时序）：
 * - C1 端口 = 宿主主服务器实际端口（预取 `get_server_status`），不再是硬编码值
 * - C2 预取失败回落 8080 且不抛（终端视图不因探测失败而崩）
 * - C3 背景图保存前先刷新端口（插件侧紧随的设置 watch 会**同步**解析背景图 URL）
 * - C4 与背景图无关的保存不触发端口刷新（不引入无谓 IPC 与耦合）
 *
 * 不测注入对象内部实现：断言的是可观测边界（getter 返回值 + invoke 调用序列）。
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { defineComponent, h, inject } from 'vue'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import {
  TERMINAL_HOST_CAPABILITIES_KEY,
  type TerminalSettingsAccessor,
} from '@/plugin/terminal-host-capabilities-contract'
import { provideTerminalHostCapabilities } from '@/plugin/terminal-host-capabilities'

const invokeMock = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(async () => null) }))

/** 宿主服务器实际端口（取自 supervisor 状态；与旧硬编码 8080 有区分度） */
const ACTUAL_PORT = 8767
const FALLBACK_PORT = 8080

type InjectedCapabilities = { settings: TerminalSettingsAccessor }

let serverStatusOutcome: { port: number } | Error = { port: ACTUAL_PORT }

beforeEach(() => {
  setActivePinia(createPinia())
  invokeMock.mockReset()
  serverStatusOutcome = { port: ACTUAL_PORT }
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === 'get_server_status') {
      if (serverStatusOutcome instanceof Error) throw serverStatusOutcome
      return serverStatusOutcome
    }
    return undefined
  })
})

/** 挂载调用方（PluginWindowHostView 等效），返回注入给插件视图的能力对象 */
function mountProvider(): InjectedCapabilities {
  const holder: { caps: InjectedCapabilities | null } = { caps: null }
  const Probe = defineComponent({
    setup() {
      holder.caps = inject<InjectedCapabilities>(TERMINAL_HOST_CAPABILITIES_KEY) ?? null
      return () => h('div')
    },
  })
  mount(
    defineComponent({
      setup() {
        provideTerminalHostCapabilities()
        return () => h(Probe)
      },
    }),
  )
  expect(holder.caps, 'provider 必须注入 terminalHostCapabilities').toBeTruthy()
  return holder.caps as InjectedCapabilities
}

/** 取 mock 调用序列中的命令名 */
function invokedCommands(): string[] {
  return invokeMock.mock.calls.map(([cmd]) => cmd as string)
}

describe('宿主终端能力注入：背景图端口', () => {
  it('C1 端口取自宿主服务器状态（get_server_status），不是硬编码 8080', async () => {
    const { settings } = mountProvider()
    await flushPromises()

    expect(settings.getServerPort()).toBe(ACTUAL_PORT)
    expect(invokedCommands().filter((c) => c === 'get_server_status')).toHaveLength(1)
  })

  it('C2 预取失败回落 8080 且不抛错（终端视图不因探测失败而崩）', async () => {
    serverStatusOutcome = new Error('ipc unavailable')
    const { settings } = mountProvider()
    await flushPromises()

    expect(settings.getServerPort()).toBe(FALLBACK_PORT)
  })

  it('C3 背景图保存前先刷新端口（插件侧紧随的设置 watch 同步解析 URL）', async () => {
    const { settings } = mountProvider()
    await flushPromises()
    invokeMock.mockClear()
    serverStatusOutcome = { port: 9000 }

    settings.save({ bgImage: 'wall.png' })
    await flushPromises()

    const cmds = invokedCommands()
    const refreshAt = cmds.indexOf('get_server_status')
    const saveAt = cmds.indexOf('save_app_settings')
    expect(refreshAt, '背景图保存前必须刷新端口').toBeGreaterThanOrEqual(0)
    expect(saveAt, '设置必须落盘').toBeGreaterThan(-1)
    expect(saveAt, '端口刷新必须先于设置落盘').toBeGreaterThan(refreshAt)
    expect(settings.getServerPort()).toBe(9000)
  })

  it('C4 与背景图无关的保存不触发端口刷新（不引入无谓 IPC）', async () => {
    const { settings } = mountProvider()
    await flushPromises()
    invokeMock.mockClear()

    settings.save({ fontSize: 14 })
    await flushPromises()

    expect(invokedCommands()).toEqual(['save_app_settings'])
  })
})
