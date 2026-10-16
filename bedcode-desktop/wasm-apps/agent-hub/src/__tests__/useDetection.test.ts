/**
 * useDetection 探测编排行为契约（票 15）
 *
 * 契约来源：useDetection 文件头声明的三个不变量——
 * ① 乱序免疫：guest 推送带单调递增 seq，只接受最新全量
 * ② 超时兜底：detecting 超过 25s 未收敛 → 强制复位并拉 storage 权威态
 * ③ 错误可见：detect 命令失败 → 复位 detecting，不卡死
 * 以及 refresh 从未探测过（state=null）时自动触发首轮探测。
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { defineComponent, h } from 'vue'
import { mount, flushPromises } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { useDetection } from '../composables/useDetection'
import type { AgentHubState } from '../types'

/** 命令面替身：按命令 id 返回最小载荷并记录调用（宽松返回类型便于逐测试换 impl） */
const execute = vi.fn(
  async (command: string, _args?: Record<string, unknown>): Promise<Record<string, unknown> | null> => {
    switch (command) {
      case 'agent-hub.get-state':
        return { state: null }
      case 'agent-hub.detect':
      case 'agent-hub.request-auth':
        return null
      default:
        throw new Error(`unexpected command: ${command}`)
    }
  },
)

let emitDetection: ((payload: unknown) => void) | null = null

function makeContext(): PluginContext {
  return {
    i18n: { t: (k: string) => k, getI18n: () => undefined },
    commands: { execute },
    events: {
      on: (name: string, cb: (payload: unknown) => void) => {
        if (name === 'plugin:agent-hub:detection') emitDetection = cb
        return { dispose: () => {} }
      },
    },
  } as unknown as PluginContext
}

type Detection = ReturnType<typeof useDetection>

/**
 * 挂载并触发一次 refresh()——useDetection 本身不在 onMounted 里自取数，
 * 由宿主组件（AgentHubView）在 onMounted 调 refresh()，这里镜像同一时序。
 */
function mountDetection(autoRefresh = true): { d: Detection; wrapper: ReturnType<typeof mount> } {
  let d!: Detection
  const Host = defineComponent({
    setup() {
      d = useDetection(makeContext())
      if (autoRefresh) void d.refresh()
      return () => h('div')
    },
  })
  const wrapper = mount(Host)
  return { d, wrapper }
}

/** 构造一份最小可渲染状态；envStatus 决定是否仍在 detecting */
function state(seq: number, envStatus: string, claudeStatus = 'ok'): AgentHubState {
  return {
    seq,
    authGranted: true,
    home: '/home/u',
    env: { os: 'linux', node: 'v22', npm: '10', pnpm: '12', registry: 'https://r' },
    envStatus,
    clis: { claude: { status: claudeStatus, version: '1', method: 'npm', paths: [], error: null, dual: false, updatedAt: null } },
  } as unknown as AgentHubState
}

beforeEach(() => {
  vi.clearAllMocks()
  emitDetection = null
  vi.useFakeTimers()
  execute.mockImplementation(async (command: string): Promise<Record<string, unknown> | null> => {
    if (command === 'agent-hub.get-state') return { state: null }
    if (command === 'agent-hub.detect' || command === 'agent-hub.request-auth') return null
    throw new Error(`unexpected command: ${command}`)
  })
})

afterEach(() => {
  vi.useRealTimers()
})

describe('D1 挂载与首轮探测', () => {
  it('state 为 null 时自动发起一次探测（spec §3 首轮自动探测）', async () => {
    const { wrapper } = mountDetection()
    await flushPromises()
    expect(execute).toHaveBeenCalledWith('agent-hub.detect', {})
    wrapper.unmount()
  })

  it('反例：已有持久化状态时不重复探测', async () => {
    execute.mockImplementation(async (command: string): Promise<Record<string, unknown> | null> => {
      if (command === 'agent-hub.get-state') return { state: state(1, 'ok') }
      return null
    })
    const { d, wrapper } = mountDetection()
    await flushPromises()
    expect(execute).not.toHaveBeenCalledWith('agent-hub.detect', {})
    expect(d.detecting.value).toBe(false)
    expect(d.state.value?.env?.node).toBe('v22')
    wrapper.unmount()
  })
})

describe('D2 乱序免疫（seq 过滤）', () => {
  it('旧 seq 事件不得覆盖新 seq 的最终态', async () => {
    const { d, wrapper } = mountDetection()
    await flushPromises()

    emitDetection?.(state(5, 'detecting', 'detecting'))
    await flushPromises()
    expect(d.detecting.value).toBe(true)

    emitDetection?.(state(9, 'ok'))
    await flushPromises()
    expect(d.detecting.value).toBe(false)
    expect(d.state.value?.env?.node).toBe('v22')

    // 乱序抵达的旧中间态必须被丢弃
    emitDetection?.(state(7, 'detecting', 'detecting'))
    await flushPromises()
    expect(d.detecting.value).toBe(false)
    expect(d.state.value?.env?.node).toBe('v22')
    wrapper.unmount()
  })

  it('同 seq 允许覆盖（guest 单调递增下同 seq 即最终态）', async () => {
    const { d, wrapper } = mountDetection()
    await flushPromises()
    emitDetection?.(state(3, 'detecting', 'detecting'))
    await flushPromises()
    expect(d.detecting.value).toBe(true)
    emitDetection?.(state(3, 'ok'))
    await flushPromises()
    expect(d.detecting.value).toBe(false)
    wrapper.unmount()
  })

  it('seq=0（无 seq 的事件）按到达顺序接受', async () => {
    const { d, wrapper } = mountDetection()
    await flushPromises()
    emitDetection?.(state(0, 'detecting', 'detecting'))
    await flushPromises()
    expect(d.detecting.value).toBe(true)
    emitDetection?.(state(0, 'ok'))
    await flushPromises()
    expect(d.detecting.value).toBe(false)
    wrapper.unmount()
  })
})

describe('D3 超时兜底（25s）', () => {
  it('detecting 持续 25s 未收敛 → 复位并拉 storage 权威态', async () => {
    // 兜底 refresh 必须拉到一份已收敛态（真实场景：guest 早已写完 storage，
    // 只是最终态事件丢失）。此处返回 ok 态以隔离「兜底后重新探测」分支。
    execute.mockImplementation(async (command: string): Promise<Record<string, unknown> | null> => {
      if (command === 'agent-hub.get-state') return { state: state(1, 'ok') }
      if (command === 'agent-hub.detect') return null
      throw new Error(`unexpected command: ${command}`)
    })
    const { d, wrapper } = mountDetection()
    await flushPromises()
    emitDetection?.(state(1, 'detecting', 'detecting'))
    await flushPromises()
    expect(d.detecting.value).toBe(true)
    execute.mockClear()

    await vi.advanceTimersByTimeAsync(24_000)
    expect(d.detecting.value).toBe(true)
    expect(execute).not.toHaveBeenCalledWith('agent-hub.get-state', {})

    await vi.advanceTimersByTimeAsync(1_000)
    expect(d.detecting.value).toBe(false)
    expect(execute).toHaveBeenCalledWith('agent-hub.get-state', {})
    expect(d.state.value?.env?.node).toBe('v22')
    wrapper.unmount()
  })

  it('反例：事件在 25s 内收敛则不触发兜底 refresh', async () => {
    execute.mockImplementation(async (command: string): Promise<Record<string, unknown> | null> => {
      if (command === 'agent-hub.get-state') return { state: state(1, 'ok') }
      return null
    })
    const { d, wrapper } = mountDetection()
    await flushPromises()
    emitDetection?.(state(1, 'detecting', 'detecting'))
    await flushPromises()
    execute.mockClear()

    await vi.advanceTimersByTimeAsync(10_000)
    emitDetection?.(state(2, 'ok'))
    await flushPromises()
    await vi.advanceTimersByTimeAsync(30_000)

    expect(d.detecting.value).toBe(false)
    expect(execute).not.toHaveBeenCalledWith('agent-hub.get-state', {})
    wrapper.unmount()
  })

  it('兜底只在 detecting 期间挂表：初始非 detecting 时 30s 后不刷新', async () => {
    execute.mockImplementation(async (command: string): Promise<Record<string, unknown> | null> => {
      if (command === 'agent-hub.get-state') return { state: state(1, 'ok') }
      return null
    })
    const { wrapper } = mountDetection()
    await flushPromises()
    execute.mockClear()
    await vi.advanceTimersByTimeAsync(60_000)
    expect(execute).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('卸载时清理定时器（不再触发 refresh）', async () => {
    execute.mockImplementation(async (command: string): Promise<Record<string, unknown> | null> => {
      if (command === 'agent-hub.get-state') return { state: state(1, 'ok') }
      return null
    })
    const { d, wrapper } = mountDetection()
    await flushPromises()
    emitDetection?.(state(1, 'detecting', 'detecting'))
    await flushPromises()
    wrapper.unmount()
    execute.mockClear()
    await vi.advanceTimersByTimeAsync(60_000)
    expect(d.detecting.value).toBe(true) // 状态不再被更新 = 定时器已清理
    expect(execute).not.toHaveBeenCalled()
  })
})

describe('D4 探测失败不卡死', () => {
  it('detect 命令抛错 → detecting 复位', async () => {
    execute.mockImplementation(async (command: string): Promise<Record<string, unknown> | null> => {
      if (command === 'agent-hub.get-state') return { state: state(1, 'ok') }
      if (command === 'agent-hub.detect') throw new Error('boom')
      return null
    })
    const { d, wrapper } = mountDetection()
    await flushPromises()
    await d.detect()
    await flushPromises()
    expect(d.detecting.value).toBe(false)
    wrapper.unmount()
  })

  it('重复 detect 在 detecting 期间被忽略（不重复发命令）', async () => {
    execute.mockImplementation(async (command: string): Promise<Record<string, unknown> | null> => {
      if (command === 'agent-hub.get-state') return { state: state(1, 'ok') }
      return null
    })
    const { d, wrapper } = mountDetection()
    await flushPromises()
    emitDetection?.(state(1, 'detecting', 'detecting'))
    await flushPromises()
    execute.mockClear()
    await d.detect()
    await d.detect()
    expect(execute).not.toHaveBeenCalledWith('agent-hub.detect', {})
    wrapper.unmount()
  })

  it('requestAuth 只发命令，失败不影响 detecting', async () => {
    execute.mockImplementation(async (command: string): Promise<Record<string, unknown> | null> => {
      if (command === 'agent-hub.get-state') return { state: state(1, 'ok') }
      if (command === 'agent-hub.request-auth') throw new Error('denied')
      return null
    })
    const { d, wrapper } = mountDetection()
    await flushPromises()
    await d.requestAuth()
    await flushPromises()
    expect(d.detecting.value).toBe(false)
    expect(execute).toHaveBeenCalledWith('agent-hub.request-auth', {})
    wrapper.unmount()
  })
})
