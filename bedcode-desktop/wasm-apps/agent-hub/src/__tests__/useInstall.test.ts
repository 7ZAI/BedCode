/**
 * useInstall 卸载编排行为契约（本次新增）
 *
 * 契约来源：
 * - uninstall(cli)：只发 `agent-hub.uninstall` 命令（recipe 白名单与并发
 *   互斥都在 guest 端仲裁，前端不做业务判断）；成功返回 true，guest 拒绝
 *   （并发 run / 探测未就绪等）返回 false 且不抛——概览卡片据此显示友好文案
 * - install(cli, mirror)：既有安装面不被卸载改动（回归见证）
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { defineComponent, h } from 'vue'
import { mount } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { useInstall } from '../composables/useInstall'

const execute = vi.fn(
  async (_command: string, _args?: Record<string, unknown>): Promise<Record<string, unknown> | null> => null,
)

function makeContext(): PluginContext {
  return {
    i18n: { t: (k: string) => k, getI18n: () => undefined },
    commands: { execute },
    events: { on: () => ({ dispose: () => {} }) },
  } as unknown as PluginContext
}

/** 挂载宿主触发 setup（composable 内部不自动发命令，需手动调用） */
function mountInstall(): ReturnType<typeof useInstall> {
  let s!: ReturnType<typeof useInstall>
  const Host = defineComponent({
    setup() {
      s = useInstall(makeContext())
      return () => h('div')
    },
  })
  mount(Host)
  return s
}

beforeEach(() => {
  vi.clearAllMocks()
  execute.mockImplementation(async () => null)
})

afterEach(() => {
  vi.useRealTimers()
})

describe('U1 uninstall 命令分发', () => {
  it('正例：成功 → 调用 agent-hub.uninstall 并返回 true', async () => {
    const s = mountInstall()
    const ok = await s.uninstall('codex')
    expect(ok).toBe(true)
    expect(execute).toHaveBeenCalledWith('agent-hub.uninstall', { cli: 'codex' })
  })

  it('反例：guest 拒绝（如并发 run）→ 返回 false 且不抛', async () => {
    execute.mockImplementation(async (cmd: string) => {
      if (cmd === 'agent-hub.uninstall') throw new Error('another run is active')
      return null
    })
    const s = mountInstall()
    const ok = await s.uninstall('pi')
    expect(ok).toBe(false)
    expect(execute).toHaveBeenCalledWith('agent-hub.uninstall', { cli: 'pi' })
  })

  it('反例：命令原文不进返回值（错误只进日志，调用方拿 false 显示友好文案）', async () => {
    execute.mockImplementation(async () => {
      throw new Error('npm exploded secret')
    })
    const s = mountInstall()
    const ok = await s.uninstall('claude')
    expect(ok).toBe(false)
    // 返回值不携带技术详情
    expect(s.state.value).toBeNull()
  })

  it('回归：install 仍走 agent-hub.install（卸载不改变既有安装面）', async () => {
    const s = mountInstall()
    await s.install('claude', true)
    expect(execute).toHaveBeenCalledWith('agent-hub.install', { cli: 'claude', mirror: true })
  })
})
