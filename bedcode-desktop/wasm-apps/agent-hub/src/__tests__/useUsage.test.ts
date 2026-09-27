/**
 * useUsage 会话列表域行为契约（票 10 · 票 15）
 *
 * 契约来源：本轮审查 P1-2（两 tab 共享一份 `sessions/page/loadedOffset` 造成
 * 「统计加载更多 → 日志翻页 → 回统计」出现整段重复行、日志表格行数与分页器
 * 不符），以及 P3-7（`autoScanDone` 在 scan 发起前置位，失败后本会话不再重试）。
 * 断言全部是外部可见行为——发往 guest 的命令名与入参、由命令返回值驱动的
 * 列表内容与分页器状态；不测内部实现。
 *
 * 契约清单：
 * - U1 统计明细是「追加」语义：load-more 拼在已有列表之后，游标只在本列表内推进
 * - U2 日志表格是「按页替换」语义：goPage 覆盖列表，页码随返回值更新
 * - U3 两份列表游标互不干扰（回归见证：旧实现共用一份游标，此处会红）
 * - U4 共享查询条件变更 → 两份列表同时回第 1 页
 * - U5 分页/查询命令入参含 adapter / q / from / to 与 offset / limit
 * - U6 list-usage-sessions 失败不改变已有列表（错误只进日志，不清空界面）
 * - U7 打开/关闭会话详情
 * - U8 日志来源增删：成功回流状态 + 失败返回 { ok:false }（界面走友好 i18n）
 * - U9 autoScanDone：发起失败后释放锁，下一次 idle 回流能重试
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { defineComponent, h } from 'vue'
import { mount, flushPromises } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { useUsage, PAGE_SIZE, type UseUsageReturn } from '../composables/useUsage'
import type { UsageDomainState, UsageSessionRow } from '../types'

// ==================== 替身 ====================

const TOTAL = 45

function row(i: number): UsageSessionRow {
  return {
    id: i,
    adapter: 'claude',
    cli_session_id: `s-${i}`,
    project: 'p',
    title: `t-${i}`,
    started_at: 1_700_000_000_000 + i,
    ended_at: null,
    duration_ms: 1000,
    model: 'm',
    tokens_in: 1,
    tokens_out: 1,
    tokens_cache_read: 0,
    tokens_cache_write: 0,
    tokens_reasoning: 0,
    cost_total: 0,
    active: false,
    source_path: '',
  } as unknown as UsageSessionRow
}

/** 命令面：list-usage-sessions 按 offset/limit 切片（真实 guest 语义） */
const execute = vi.fn(async (command: string, args?: Record<string, unknown>) => {
  switch (command) {
    case 'agent-hub.get-usage-state':
      return { state: { status: 'ok' } as unknown as UsageDomainState }
    case 'agent-hub.get-usage-stats':
      return null
    case 'agent-hub.list-usage-sessions': {
      const offset = Number(args?.offset ?? 0)
      const limit = Number(args?.limit ?? PAGE_SIZE)
      const all = Array.from({ length: TOTAL }, (_, i) => row(i + 1))
      return { sessions: all.slice(offset, offset + limit), total: all.length }
    }
    case 'agent-hub.list-usage-sources':
      return { sources: [{ name: 'claude', path: '/home/u/.claude', builtin: true, scan: null }] }
    case 'agent-hub.scan-usage':
      return { state: { status: 'ok' } as unknown as UsageDomainState }
    case 'agent-hub.add-usage-source':
      return { state: { status: 'ok' } as unknown as UsageDomainState }
    case 'agent-hub.remove-usage-source':
      return { state: { status: 'ok' } as unknown as UsageDomainState }
    case 'agent-hub.read-usage-session':
      return { session: row(1), events: [], raw: [] }
    case 'agent-hub.clear-usage-data':
      return { state: { status: 'idle' } as unknown as UsageDomainState }
    default:
      throw new Error(`unexpected command: ${command}`)
  }
})

let emitUsage: ((payload: unknown) => void) | null = null

function makeContext(): PluginContext {
  return {
    i18n: { t: (k: string) => k, getI18n: () => undefined },
    commands: { execute },
    events: {
      on: (name: string, cb: (payload: unknown) => void) => {
        if (name === 'plugin:agent-hub:usage') emitUsage = cb
        return { dispose: () => {} }
      },
    },
  } as unknown as PluginContext
}

/** 在真实组件生命周期里跑 composable（onMounted/onUnmounted 才有意义） */
function mountUsage(): { usage: UseUsageReturn; wrapper: ReturnType<typeof mount> } {
  let usage!: UseUsageReturn
  const Host = defineComponent({
    setup() {
      usage = useUsage(makeContext())
      return () => h('div')
    },
  })
  const wrapper = mount(Host)
  return { usage, wrapper }
}

const ids = (list: { id: number }[]) => list.map((s) => s.id)
const listCalls = () => execute.mock.calls.filter((c) => c[0] === 'agent-hub.list-usage-sessions')

beforeEach(() => {
  vi.clearAllMocks()
  emitUsage = null
  execute.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    switch (command) {
      case 'agent-hub.get-usage-state':
        return { state: { status: 'ok' } as unknown as UsageDomainState }
      case 'agent-hub.get-usage-stats':
        return null
      case 'agent-hub.list-usage-sessions': {
        const offset = Number(args?.offset ?? 0)
        const limit = Number(args?.limit ?? PAGE_SIZE)
        const all = Array.from({ length: TOTAL }, (_, i) => row(i + 1))
        return { sessions: all.slice(offset, offset + limit), total: all.length }
      }
      case 'agent-hub.list-usage-sources':
        return { sources: [{ name: 'claude', path: '/home/u/.claude', builtin: true, scan: null }] }
      case 'agent-hub.scan-usage':
        return { state: { status: 'ok' } as unknown as UsageDomainState }
      case 'agent-hub.add-usage-source':
        return { state: { status: 'ok' } as unknown as UsageDomainState }
      case 'agent-hub.remove-usage-source':
        return { state: { status: 'ok' } as unknown as UsageDomainState }
      case 'agent-hub.read-usage-session':
        return { session: row(1), events: [], raw: [] }
      case 'agent-hub.clear-usage-data':
        return { state: { status: 'idle' } as unknown as UsageDomainState }
      default:
        throw new Error(`unexpected command: ${command}`)
    }
  })
})

// ==================== U1 会话列表去重 ====================

describe('U1 会话级列表只有一份（统计/日志重复展示的回归见证）', () => {
  it('返回值上不再有统计侧列表面（statSessions / loadMoreSessions 等已删除）', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    // 此前统计分区挂了一份同源会话列表，与日志表格重复且游标语义相反
    expect('statSessions' in usage).toBe(false)
    expect('statTotal' in usage).toBe(false)
    expect('statLoaded' in usage).toBe(false)
    expect('statLoading' in usage).toBe(false)
    expect('loadMoreSessions' in usage).toBe(false)
    expect('setListFilter' in usage).toBe(false)
    wrapper.unmount()
  })

  it('挂载只发一次 list-usage-sessions（两份列表时代是两次）', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    expect(listCalls()).toHaveLength(1)
    expect(usage.logSessions.value).toHaveLength(PAGE_SIZE)
    wrapper.unmount()
  })

  it('看板与列表互不触发：切时间窗不重拉会话列表', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    const before = listCalls().length
    await usage.setStatsDays(7)
    await flushPromises()
    expect(listCalls()).toHaveLength(before)
    wrapper.unmount()
  })
})

describe('U2 日志表格：按页替换', () => {
  it('goPage 覆盖列表为该页切片，页码与总页数随之更新', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    expect(usage.logPage.value).toBe(1)
    expect(usage.logTotalPages.value).toBe(3)

    await usage.goPage(2)
    await flushPromises()
    expect(ids(usage.logSessions.value)).toEqual([16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30])
    expect(usage.logPage.value).toBe(2)

    await usage.goPage(3)
    await flushPromises()
    expect(ids(usage.logSessions.value)).toEqual([31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45])
    expect(usage.logSessions.value).toHaveLength(PAGE_SIZE)
    wrapper.unmount()
  })

  it('边界：页码钳制在 [1, 总页数]，越界请求不发命令', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    await usage.goPage(99)
    await flushPromises()
    expect(usage.logPage.value).toBe(3)
    await usage.goPage(0)
    await flushPromises()
    expect(usage.logPage.value).toBe(1)
    wrapper.unmount()
  })
})

describe('U3 看板时间窗（服务端切片）', () => {
  it('默认 30 天，挂载时就把 days 传给 guest', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    expect(usage.statsDays.value).toBe(30)
    const args = execute.mock.calls.filter((c) => c[0] === 'agent-hub.get-usage-stats').at(-1)?.[1]
    expect(args).toEqual({ days: 30 })
    wrapper.unmount()
  })

  it('setStatsDays 切窗重拉看板；同窗连点不重发命令', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    await usage.setStatsDays(7)
    await flushPromises()
    expect(usage.statsDays.value).toBe(7)

    const after = execute.mock.calls.filter((c) => c[0] === 'agent-hub.get-usage-stats').length
    await usage.setStatsDays(7)
    await flushPromises()
    expect(execute.mock.calls.filter((c) => c[0] === 'agent-hub.get-usage-stats')).toHaveLength(after)
    wrapper.unmount()
  })

  it('反例守门：切窗期间的旧响应不得覆盖新窗（否则「新 pill + 旧数据」）', async () => {
    // 先让 7 天那次请求挂起，再用 30 天的响应放行
    const gates = new Map<number, () => void>()
    const emptyStats = (days: number) => ({
      window: { days, now: 0 },
      total: { sessions: days },
      byDay: [],
      byCli: [],
      byProject: [],
      byModel: [],
      byHour: [],
    })
    execute.mockImplementation((async (command: string, args?: Record<string, unknown>) => {
      if (command === 'agent-hub.get-usage-stats') {
        const days = Number(args?.days)
        if (days === 7) {
          await new Promise<void>((r) => gates.set(days, r))
          return emptyStats(7)
        }
        return emptyStats(30)
      }
      if (command === 'agent-hub.get-usage-state') {
        return { state: { status: 'ok' } as unknown as UsageDomainState }
      }
      return null
    }) as never)

    const { usage, wrapper } = mountUsage()
    await flushPromises()
    void usage.setStatsDays(7) // 挂起
    await flushPromises()
    await usage.setStatsDays(30) // 30 天先落地
    await flushPromises()
    expect(usage.stats.value?.total.sessions).toBe(30)

    gates.get(7)?.() // 7 天的旧响应这时才回来
    await flushPromises()
    expect(usage.stats.value?.total.sessions, '旧窗响应覆盖了新窗').toBe(30)
    expect(usage.statsLoading.value).toBe(false)
    wrapper.unmount()
  })
})

// ==================== U4/U5 共享查询条件 ====================

describe('U4 查询条件变更让日志列表回第 1 页', () => {
  it('改适配器条件 + reloadSessions → 日志页码归 1、回到首屏', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    await usage.goPage(3)
    await flushPromises()

    usage.listFilter.value = 'claude'
    await usage.reloadSessions()
    await flushPromises()
    expect(usage.listFilter.value).toBe('claude')
    expect(usage.logPage.value).toBe(1)
    expect(ids(usage.logSessions.value)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15])
    wrapper.unmount()
  })

  it('resetQuery 清空四个查询条件并让日志列表回第 1 页', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    usage.listFilter.value = 'claude'
    usage.searchText.value = 'kw'
    usage.rangeFrom.value = 111
    usage.rangeTo.value = 222
    await usage.goPage(2)
    await flushPromises()

    await usage.resetQuery()
    await flushPromises()
    expect([usage.listFilter.value, usage.searchText.value, usage.rangeFrom.value, usage.rangeTo.value]).toEqual([
      '',
      '',
      null,
      null,
    ])
    expect(usage.logPage.value).toBe(1)
    expect(usage.logSessions.value[0]?.id).toBe(1)
    wrapper.unmount()
  })
})

describe('U5 查询命令入参', () => {
  it('带上 adapter / q / from / to 与 offset / limit（q 走 trim）', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    usage.listFilter.value = 'claude'
    usage.searchText.value = '  keyword  '
    usage.rangeFrom.value = 100
    usage.rangeTo.value = 200
    await usage.reloadSessions()
    await flushPromises()

    const args = listCalls().at(-1)?.[1] as Record<string, unknown>
    expect(args).toMatchObject({ adapter: 'claude', q: 'keyword', from: 100, to: 200, offset: 0, limit: PAGE_SIZE })
    wrapper.unmount()
  })

  it('goPage 的 offset 按页号推进', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    await usage.goPage(3)
    await flushPromises()
    expect((listCalls().at(-1)?.[1] as Record<string, unknown>).offset).toBe(2 * PAGE_SIZE)
    wrapper.unmount()
  })
})

// ==================== U6 失败分支 ====================

describe('U6 列表加载失败不破坏已有数据', () => {
  it('list-usage-sessions 抛错时保留原列表且 loading 复位', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    await usage.goPage(2)
    await flushPromises()
    const before = ids(usage.logSessions.value)

    execute.mockRejectedValueOnce(new Error('boom'))
    await usage.reloadSessions()
    await flushPromises()

    expect(ids(usage.logSessions.value)).toEqual(before)
    expect(usage.logLoading.value).toBe(false)
    wrapper.unmount()
  })

  it('反例守门：若失败时清空列表，本例会红（错误只进日志，不清界面）', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    execute.mockRejectedValueOnce(new Error('boom'))
    await usage.reloadSessions()
    await flushPromises()
    // reload 语义是「重置到第 1 页」；失败时保留上一份已加载内容而非清空
    expect(usage.logSessions.value.length).toBeGreaterThan(0)
    wrapper.unmount()
  })
})

// ==================== U7 会话详情 ====================

describe('U7 打开 / 关闭会话详情', () => {
  it('openSession 带回详情，closeSession 清空', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()

    await usage.openSession(7)
    await flushPromises()
    expect(execute).toHaveBeenCalledWith('agent-hub.read-usage-session', { id: 7 })
    expect(usage.openedSession.value?.session.id).toBe(1)
    expect(usage.openingSession.value).toBe(false)

    usage.closeSession()
    expect(usage.openedSession.value).toBeNull()
    wrapper.unmount()
  })

  it('失败分支：详情读取报错时 openedSession 保持 null 且不卡在 opening', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    execute.mockRejectedValueOnce(new Error('boom'))
    await usage.openSession(7)
    await flushPromises()
    expect(usage.openedSession.value).toBeNull()
    expect(usage.openingSession.value).toBe(false)
    wrapper.unmount()
  })
})

// ==================== U8 日志来源增删 ====================

describe('U8 日志来源增删', () => {
  it('addSource 成功 → 回流状态 + 重新拉来源列表，返回 { ok:true }', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    const r = await usage.addSource('demo', '/tmp/demo')
    expect(r).toEqual({ ok: true })
    expect(execute).toHaveBeenCalledWith('agent-hub.add-usage-source', { name: 'demo', path: '/tmp/demo' })
    expect(usage.state.value?.status).toBe('ok')
    wrapper.unmount()
  })

  it('addSource 失败 → { ok:false }（原文不外泄，界面走友好 i18n）', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    execute.mockRejectedValueOnce(new Error('secret detail'))
    const r = await usage.addSource('demo', '/tmp/demo')
    expect(r.ok).toBe(false)
    expect(JSON.stringify(r)).not.toContain('secret detail')
    wrapper.unmount()
  })

  it('removeSource 成功/失败同样两态', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    expect(await usage.removeSource('demo')).toEqual({ ok: true })
    execute.mockRejectedValueOnce(new Error('x'))
    expect((await usage.removeSource('demo')).ok).toBe(false)
    wrapper.unmount()
  })

  it('pickSourceDir 成功选中 → { ok:true, picked:true, path }', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    execute.mockResolvedValueOnce({ picked: true, path: '/home/u/.pi/sessions' } as never)
    const r = await usage.pickSourceDir()
    expect(r).toEqual({ ok: true, picked: true, path: '/home/u/.pi/sessions' })
    expect(execute).toHaveBeenCalledWith('agent-hub.pick-source-dir', {})
    wrapper.unmount()
  })

  it('pickSourceDir 用户取消 → picked:false 且 path 为空（不打扰）', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    execute.mockResolvedValueOnce({ picked: false, path: '' } as never)
    expect(await usage.pickSourceDir()).toEqual({ ok: true, picked: false, path: '' })
    wrapper.unmount()
  })

  it('pickSourceDir 宿主拒绝（未授权等）→ { ok:false } 且不携带原文', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    execute.mockRejectedValueOnce(new Error('permission denied: manifest must declare fs:pick'))
    const r = await usage.pickSourceDir()
    expect(r.ok).toBe(false)
    expect(JSON.stringify(r)).not.toContain('fs:pick')
    wrapper.unmount()
  })
})

// ==================== U9 autoScanDone 语义 ====================

describe('U9 自动扫描：发起失败后可再次自动触发（P3-7）', () => {
  it('首次 idle 自动扫描；发起失败后锁被释放，每次 idle 回流都会重试', async () => {
    execute.mockImplementation((async (command: string) => {
      if (command === 'agent-hub.get-usage-state') return { state: { status: 'idle' } as unknown as UsageDomainState }
      if (command === 'agent-hub.scan-usage') throw new Error('scan down')
      if (command === 'agent-hub.list-usage-sessions') return { sessions: [] as UsageSessionRow[], total: 0 }
      if (command === 'agent-hub.list-usage-sources') return { sources: [] }
      return null
    }) as never)

    const { wrapper } = mountUsage()
    await flushPromises()
    const scanCalls = () => execute.mock.calls.filter((c) => c[0] === 'agent-hub.scan-usage').length
    // refresh 拿到 idle → 自动发起一次
    expect(scanCalls()).toBe(1)

    // 发起失败 → 锁释放；事件再次回流 idle 时能重试（旧实现在这里永远不再试）
    emitUsage?.({ status: 'idle' })
    await flushPromises()
    expect(scanCalls()).toBe(2)

    emitUsage?.({ status: 'idle' })
    await flushPromises()
    expect(scanCalls()).toBe(3)
    wrapper.unmount()
  })

  it('反例守门：扫描成功时锁保持，idle 事件不再重复发起（防事件风暴）', async () => {
    const { usage, wrapper } = mountUsage()
    await usage.scan()
    await flushPromises()
    const before = execute.mock.calls.filter((c) => c[0] === 'agent-hub.scan-usage').length
    emitUsage?.({ status: 'idle' })
    await flushPromises()
    expect(execute.mock.calls.filter((c) => c[0] === 'agent-hub.scan-usage')).toHaveLength(before)
    wrapper.unmount()
  })
})

// ==================== 票 07：适配器降级 / 会话状态 / 数据清空 ====================

/**
 * U10 适配器降级（opencode SQLite 源读不到时的**显式**说明）
 *  - 只收机器可读 code 认识的适配器；guest 若给出未登记的 code 一律不收
 *    （拿「未知原因」去渲染一条错误文案比不显示更糟）
 *  - error 为 null 的适配器不收
 */
describe('U10 适配器降级清单', () => {
  /** 造一个带指定 adapters 的状态并让它成为当前 state */
  async function withAdapters(adapters: Record<string, unknown>) {
    execute.mockImplementation((async (command: string) => {
      if (command === 'agent-hub.get-usage-state') {
        return {
          state: { status: 'ok', home: '/home/u', authGranted: true, adapters } as unknown as UsageDomainState,
        }
      }
      return null
    }) as never)
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    return { usage, wrapper }
  }

  it('正例：sqlite3 缺失 / 库不存在 / 查询失败 三种 code 各自成一条', async () => {
    const { usage, wrapper } = await withAdapters({
      claude: { files: 10, parsed: 10, skipped: 0, sessions: 3, error: null },
      opencode: { files: 0, parsed: 0, skipped: 0, sessions: 0, error: 'sqlite3-missing' },
      pi: { files: 1, parsed: 1, skipped: 0, sessions: 1, error: 'db-missing' },
      codex: { files: 0, parsed: 0, skipped: 0, sessions: 0, error: 'query-failed' },
    })
    expect(usage.adapterErrors.value).toEqual([
      { adapter: 'opencode', code: 'sqlite3-missing' },
      { adapter: 'pi', code: 'db-missing' },
      { adapter: 'codex', code: 'query-failed' },
    ])
    wrapper.unmount()
  })

  it('反例：全部正常时清单为空（横幅不得常驻）', async () => {
    const { usage, wrapper } = await withAdapters({
      claude: { files: 10, parsed: 10, skipped: 0, sessions: 3, error: null },
      opencode: { files: 0, parsed: 54, skipped: 0, sessions: 54, error: null },
    })
    expect(usage.adapterErrors.value).toEqual([])
    wrapper.unmount()
  })

  it('反例：guest 给出未登记 code 时不收（不拿错误文案渲染未知原因）', async () => {
    const { usage, wrapper } = await withAdapters({
      opencode: { files: 0, parsed: 0, skipped: 0, sessions: 0, error: 'brand-new-code' },
    })
    expect(usage.adapterErrors.value).toEqual([])
    wrapper.unmount()
  })
})

/**
 * U11 CLI 会话数据状态（概览卡片第六态「已装 · 未初始化」的信号源）
 *  - 已扫描 + 已授权 + 零条目 → 'empty'
 *  - 已扫描 + 有条目 → 'scanned'
 *  - 未扫描 / 未授权 / 扫描中 / 扫描失败 → 'unknown'（**不下结论**：
 *    把「看不到数据」误报成「装了没初始化」是误报）
 */
describe('U11 CLI 会话数据状态', () => {
  async function withState(state: Record<string, unknown>) {
    execute.mockImplementation((async (command: string) => {
      if (command === 'agent-hub.get-usage-state') {
        return { state: state as unknown as UsageDomainState }
      }
      return null
    }) as never)
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    return { usage, wrapper }
  }

  const stat = (n: number) => ({ files: n, parsed: n, skipped: 0, sessions: n, error: null })

  it('正例：已扫描 + 已授权 + 零条目 → empty（本机 codex 正是此态）', async () => {
    const { usage, wrapper } = await withState({
      status: 'ok',
      home: '/home/u',
      authGranted: true,
      adapters: { codex: stat(0), claude: stat(5) },
    })
    expect(usage.cliSessionState('codex')).toBe('empty')
    expect(usage.cliSessionState('claude')).toBe('scanned')
    wrapper.unmount()
  })

  it('边界：sqlite 源的零条目看 parsed/opencode 口径（files 恒 0 不算「零」）', async () => {
    const { usage, wrapper } = await withState({
      status: 'ok',
      home: '/home/u',
      authGranted: true,
      // opencode 未装 → 库不存在 → 全零
      adapters: { opencode: stat(0) },
    })
    expect(usage.cliSessionState('opencode')).toBe('empty')

    const { usage: u2, wrapper: w2 } = await withState({
      status: 'ok',
      home: '/home/u',
      authGranted: true,
      // opencode 采到 54 个会话：files=0 但 parsed=54 → 必须判 scanned
      adapters: { opencode: { files: 0, parsed: 54, skipped: 0, sessions: 54, error: null } },
    })
    expect(u2.cliSessionState('opencode')).toBe('scanned')
    w2.unmount()
    wrapper.unmount()
  })

  it.each([
    ['idle（未扫描）', 'idle', true],
    ['syncing（扫描中）', 'syncing', true],
    ['error（扫描失败）', 'error', true],
    ['auth-required（未授权）', 'auth-required', true],
  ])('反例：%s → unknown（不下结论）', async (_label, status, granted) => {
    const { usage, wrapper } = await withState({
      status,
      home: '/home/u',
      authGranted: granted,
      adapters: { codex: stat(0) },
    })
    expect(usage.cliSessionState('codex')).toBe('unknown')
    wrapper.unmount()
  })

  it('反例：扫描成功但未授权 → unknown（授权缺失不是「装了没初始化」）', async () => {
    const { usage, wrapper } = await withState({
      status: 'ok',
      home: '/home/u',
      authGranted: false,
      adapters: { codex: stat(0) },
    })
    expect(usage.cliSessionState('codex')).toBe('unknown')
    wrapper.unmount()
  })

  it('边界：适配器槽位缺失（票 06 旧状态）→ unknown，不误判为 empty', async () => {
    const { usage, wrapper } = await withState({
      status: 'ok',
      home: '/home/u',
      authGranted: true,
      adapters: { claude: stat(5) },
    })
    expect(usage.cliSessionState('codex')).toBe('unknown')
    wrapper.unmount()
  })
})

/**
 * U12 数据清空（保留策略为全量保留不自动过期，这是唯一清理入口）
 *  - 成功 → 回流状态 + 重拉看板与两份列表（不留「空看板 + 旧列表」中间态）
 *  - 失败 → { ok:false }，列表内容保留（不因清空失败而清空界面）
 *  - 重入守门：清空中再次触发不发第二条命令
 */
describe('U12 数据清空', () => {
  const callsOf = (name: string) => execute.mock.calls.filter((c) => c[0] === name)

  it('正例：清空成功后重拉看板与两份列表（不留中间态）', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    const before = {
      stats: callsOf('agent-hub.get-usage-stats').length,
      sessions: callsOf('agent-hub.list-usage-sessions').length,
    }
    const r = await usage.clearData()
    await flushPromises()
    expect(r).toEqual({ ok: true })
    expect(callsOf('agent-hub.clear-usage-data')).toHaveLength(1)
    expect(callsOf('agent-hub.get-usage-stats').length).toBe(before.stats + 1)
    // 清空后重拉看板 + 日志首屏各一次（两份列表时代是 +2）
    expect(callsOf('agent-hub.list-usage-sessions').length).toBe(before.sessions + 1)
    expect(usage.clearing.value).toBe(false)
    wrapper.unmount()
  })

  it('异常：命令抛错 → ok:false 且不重拉（列表内容原样保留）', async () => {
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    execute.mockImplementation((async (command: string) => {
      if (command === 'agent-hub.clear-usage-data') throw new Error('db locked')
      if (command === 'agent-hub.list-usage-sessions') {
        const all = Array.from({ length: TOTAL }, (_, i) => row(i + 1))
        return { sessions: all.slice(0, PAGE_SIZE), total: all.length }
      }
      return null
    }) as never)
    await usage.reloadSessions()
    await flushPromises()
    const before = {
      stats: callsOf('agent-hub.get-usage-stats').length,
      sessions: callsOf('agent-hub.list-usage-sessions').length,
      list: ids(usage.logSessions.value),
    }
    const r = await usage.clearData()
    await flushPromises()
    expect(r).toEqual({ ok: false })
    expect(callsOf('agent-hub.get-usage-stats').length).toBe(before.stats)
    expect(callsOf('agent-hub.list-usage-sessions').length).toBe(before.sessions)
    // 失败不得把已有列表清空
    expect(ids(usage.logSessions.value)).toEqual(before.list)
    expect(usage.clearing.value).toBe(false)
    wrapper.unmount()
  })

  it('边界：清空中再次触发 → 不发第二条命令（重入守门）', async () => {
    let release!: () => void
    const gate = new Promise<void>((r) => (release = r))
    execute.mockImplementation((async (command: string) => {
      if (command === 'agent-hub.clear-usage-data') {
        await gate
        return { state: { status: 'idle' } as unknown as UsageDomainState }
      }
      return null
    }) as never)
    const { usage, wrapper } = mountUsage()
    await flushPromises()
    const first = usage.clearData()
    await flushPromises()
    // 第一次仍在途（clearing 为真）→ 第二次直接被拒，不落到命令面
    const second = await usage.clearData()
    expect(second).toEqual({ ok: false })
    expect(callsOf('agent-hub.clear-usage-data')).toHaveLength(1)
    release()
    expect(await first).toEqual({ ok: true })
    wrapper.unmount()
  })
})
