/**
 * useInstall 卸载编排行为契约（本次新增）
 *
 * 契约来源：
 * - uninstall(cli)：只发 `agent-hub.uninstall` 命令（recipe 白名单与并发
 *   互斥都在 guest 端仲裁，前端不做业务判断）；成功返回 `{ ok: true }`，
 *   guest 拒绝（并发 run / 探测未就绪等）返回 `{ ok: false, error }` 且不抛——
 *   error 为友好 i18n 文案（ADR 0030：guest 业务码优先，原文只进日志），
 *   概览卡片据此展示
 * - install(cli, mirror)：既有安装面不被卸载改动（回归见证）
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { defineComponent, h } from 'vue'
import { mount } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { LastRun } from '../types'
import { failedUninstallFromLast, useInstall } from '../composables/useInstall'

const execute = vi.fn(
  async (_command: string, _args?: Record<string, unknown>): Promise<Record<string, unknown> | null> => null,
)

function makeContext(): PluginContext {
  return {
    id: 'com.bedcode.agent-hub',
    // mock i18n：context.i18n.t 直返 key（既有断言兼容）；getI18n 返回注册表桩——
    // `com.bedcode.agent-hub.<短key>` 解析为短 key（i18n 桩直返 key 约定，模拟
    // registerMessages 扁平注册后的命中），非前缀 key 原样返回 = 未注册语义
    i18n: {
      t: (k: string) => k,
      getI18n: () => ({
        global: {
          t: (key: string, params?: Record<string, unknown>) => {
            const prefix = 'com.bedcode.agent-hub.'
            const short = key.startsWith(prefix) ? key.slice(prefix.length) : key
            if (params && Object.keys(params).length > 0) {
              return `${short}(${Object.entries(params)
                .map(([pk, pv]) => `${pk}=${String(pv)}`)
                .join(',')})`
            }
            return short
          },
        },
      }),
    },
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
  it('正例：成功 → 调用 agent-hub.uninstall 并返回 { ok:true }', async () => {
    const s = mountInstall()
    const ok = await s.uninstall('codex')
    expect(ok).toEqual({ ok: true })
    expect(execute).toHaveBeenCalledWith('agent-hub.uninstall', { cli: 'codex' })
  })

  it('反例：guest 拒绝（如并发 run）→ { ok:false, error:友好文案 } 且不抛', async () => {
    execute.mockImplementation(async (cmd: string) => {
      if (cmd === 'agent-hub.uninstall') throw new Error('another run is active')
      return null
    })
    const s = mountInstall()
    const ok = await s.uninstall('pi')
    expect(ok.ok).toBe(false)
    expect(ok.error).toBe('hub.card.uninstallFailed')
    expect(execute).toHaveBeenCalledWith('agent-hub.uninstall', { cli: 'pi' })
  })

  it('反例：命令原文不进返回值（错误只进日志，error 为友好 i18n 文案）', async () => {
    execute.mockImplementation(async () => {
      throw new Error('npm exploded secret')
    })
    const s = mountInstall()
    const ok = await s.uninstall('claude')
    expect(ok.ok).toBe(false)
    expect(JSON.stringify(ok)).not.toContain('npm exploded secret')
    expect(s.state.value).toBeNull()
  })

  it('回归：install 仍走 agent-hub.install（卸载不改变既有安装面）', async () => {
    const s = mountInstall()
    await s.install('claude', true)
    expect(execute).toHaveBeenCalledWith('agent-hub.install', { cli: 'claude', mirror: true })
  })
})

describe('U2 failedUninstallFromLast 终态失败归因（概览卡片失败上屏）', () => {
  /** 构造 last 终态夹具（关键字段显式，其余取既有默认语义） */
  function last(partial: Partial<LastRun>): LastRun {
    return {
      cli: 'codex',
      action: 'uninstall',
      command: 'npm uninstall -g @openai/codex',
      ok: false,
      cancelled: false,
      exitCode: 127,
      timedOut: false,
      error: 'exit=Some(127) timed_out=false',
      output: '/bin/bash: 行 1: npm: 未找到命令\n',
      finishedAt: 1790647390916,
      ...partial,
    }
  }

  it('正例：进程级卸载失败（exit≠0）→ 归因该 CLI', () => {
    expect(failedUninstallFromLast(last({}))).toBe('codex')
  })

  it('反例：卸载成功 → null（不得误报）', () => {
    expect(failedUninstallFromLast(last({ ok: true, exitCode: 0, error: null }))).toBeNull()
  })

  it('反例：用户取消 → null（不得误报为失败）', () => {
    expect(failedUninstallFromLast(last({ cancelled: true }))).toBeNull()
  })

  it('反例：安装动作的失败 → null（信号只归卸载）', () => {
    expect(failedUninstallFromLast(last({ action: 'install' }))).toBeNull()
    expect(failedUninstallFromLast(last({ action: 'update' }))).toBeNull()
  })

  it('边界：无 last 终态 → null', () => {
    expect(failedUninstallFromLast(null)).toBeNull()
  })
})
