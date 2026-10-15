/**
 * FileTransferView 浏览加载闸门测试（活跃对端就绪才加载共享根）
 *
 * 背景（真机缺陷）：浏览加载此前挂在 `tasks.peerOnline`（= 发现到任意可传输设备，
 * 未连接、甚至只有快照恢复条目时也为真）上，且挂载时无条件 `fs.loadRoots()` →
 * 无活跃对端时 `file-transfer.list-remote` 必失败（插件侧 resolve_node 报
 * no active peer），被宿主包成信封后前端只见误导性的 `Command not found: ...`。
 *
 * 对齐桌面口径（desktop `watch(peer.value.id)` 驱动 loadRoots/reset）：
 * 闸门 = 活跃对端 id 且已连接；watch 源用 id 而非布尔（兜底切换要对新对端重载）。
 *
 * | 契约 | 场景 | 预期 |
 * |---|---|---|
 * | C-L1 | 挂载时无活跃对端 | 不发 list-remote（此前挂载无条件调用是缺陷） |
 * | C-L2 | 仅发现（mdns-found，未连接） | 不发 list-remote |
 * | C-L3 | 连接建立（connection-changed connected） | 以 {path:'',dirId:''} 加载共享根 |
 * | C-L4 | 活跃对端兜底切换 A→B | 对新对端重载（布尔闸门会漏掉这次切换） |
 * | C-L5 | 活跃对端断开且无候选 | 浏览态清空（entries 空、无错误残留、闸门回落） |
 * | C-L6 | 连接事件先于发现到达（乱序） | 发现补齐后兜底激活并加载，不漏 |
 * | C-L7 | BrowseTab 下拉刷新闸门 | 未就绪整轮手势惰性；就绪照常发 refresh 且 loading 回落复位 |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { nextTick } from 'vue'
import { mount } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import FileTransferView from '../../../../wasm-apps/file-transfer/src/components/FileTransferView.vue'
import BrowseTab from '../../../../wasm-apps/file-transfer/src/components/BrowseTab.vue'
import TabBar from '../../../../wasm-apps/file-transfer/src/components/TabBar.vue'

type EventHandler = (payload: any) => void

const NODE_A = 'a'.repeat(64)
const NODE_B = 'b'.repeat(64)
const SHARED_ROOT = { id: 'root-1', name: '下载' }

/** mdns:found 载荷形状（引擎透传 wire，与 usePeerDevices.test 同款） */
function makeFound(nodeId: string, name: string) {
  return {
    instanceName: `bedcode-peer-${nodeId.slice(0, 8)}._bedcode-peer._tcp.local.`,
    addresses: ['192.168.1.10'],
    port: 47821,
    txtRecords: { id: nodeId, name, ver: '1', cap: '1' },
  }
}

function makeContext() {
  const calls: Array<{ id: string; args: any }> = []
  const handlers = new Map<string, EventHandler[]>()
  const responders = new Map<string, (args: any) => unknown>()

  const context = {
    commands: {
      async execute(id: string, args?: any) {
        calls.push({ id, args })
        const respond = responders.get(id)
        if (!respond) throw new Error(`no responder for ${id}`)
        return respond(args)
      },
      register: vi.fn(),
    },
    events: {
      on(event: string, handler: EventHandler) {
        const list = handlers.get(event) ?? []
        list.push(handler)
        handlers.set(event, list)
        return {
          dispose: () =>
            handlers.set(
              event,
              (handlers.get(event) ?? []).filter((h) => h !== handler),
            ),
        }
      },
    },
    i18n: { t: (key: string) => key },
    dialogs: { showToast: vi.fn(), showDialog: vi.fn(), showConfirm: vi.fn(), showPrompt: vi.fn() },
    ui: { onBackPressed: () => ({ dispose: vi.fn() }), openPage: vi.fn() },
    system: { revealInDir: vi.fn(), revealReceivedFileLocation: vi.fn() },
    logger: { info: vi.fn(), debug: vi.fn(), warn: vi.fn(), error: vi.fn() },
  } as unknown as PluginContext

  function emit(event: string, payload: unknown): void {
    for (const handler of handlers.get(event) ?? []) handler(payload)
  }

  function onCommand(id: string, respond: (args: any) => unknown): void {
    responders.set(id, respond)
  }

  const flush = () => new Promise((r) => setTimeout(r, 0))

  return { context, calls, emit, onCommand, flush }
}

describe('FileTransferView 浏览加载闸门', () => {
  let env: ReturnType<typeof makeContext>

  beforeEach(() => {
    vi.clearAllMocks()
    env = makeContext()
    // 挂载期全量命令面应答（传输/设备/设置快照均为空，只关注 list-remote）
    env.onCommand('file-transfer.get-device-snapshot', () => ({ devices: [] }))
    env.onCommand('file-transfer.save-device-snapshot', () => ({}))
    env.onCommand('file-transfer.set-active-peer', () => ({}))
    env.onCommand('file-transfer.remember-peer-endpoint', () => ({}))
    env.onCommand('file-transfer.refresh-devices', () => ({}))
    env.onCommand('file-transfer.get-settings', () => ({}))
    env.onCommand('file-transfer.list-tasks', () => [])
    env.onCommand('file-transfer.list-batches', () => [])
    env.onCommand('file-transfer.list-receiving', () => [])
    env.onCommand('file-transfer.list-history', () => [])
    env.onCommand('file-transfer.list-remote', () => ({ roots: [SHARED_ROOT] }))
  })

  function mountView() {
    return mount(FileTransferView, { global: { provide: { pluginContext: env.context } } })
  }

  function listRemoteCalls() {
    return env.calls.filter((c) => c.id === 'file-transfer.list-remote')
  }

  /** 切到浏览 tab（Transition 默认被 VTU stub，emit 后一次 nextTick 即可见） */
  async function openBrowseTab(wrapper: ReturnType<typeof mountView>): Promise<void> {
    wrapper.findComponent(TabBar).vm.$emit('update:modelValue', 'browse')
    await nextTick()
  }

  it('C-L1 挂载时无活跃对端不发 list-remote', async () => {
    const wrapper = mountView()
    await env.flush()

    expect(listRemoteCalls()).toHaveLength(0)

    wrapper.unmount()
  })

  it('C-L2 仅发现未连接不发 list-remote（旧 watch 挂 tasks.peerOnline 会误发）', async () => {
    const wrapper = mountView()
    await env.flush()

    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, '设备-a'))
    await env.flush()

    expect(listRemoteCalls()).toHaveLength(0)

    wrapper.unmount()
  })

  it('C-L3 连接建立后按根清单层加载共享根', async () => {
    const wrapper = mountView()
    await env.flush()

    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, '设备-a'))
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: true })
    await env.flush()

    const calls = listRemoteCalls()
    expect(calls).toHaveLength(1)
    expect(calls[0]!.args).toEqual({ path: '', dirId: '' })

    wrapper.unmount()
  })

  it('C-L4 活跃对端兜底切换 A→B 时对新对端重载', async () => {
    const wrapper = mountView()
    await env.flush()

    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, '设备-a'))
    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_B, '设备-b'))
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: true })
    await env.flush()
    expect(listRemoteCalls()).toHaveLength(1)

    // B 上线但活跃仍是 A：不重复加载
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_B, connected: true })
    await env.flush()
    expect(listRemoteCalls()).toHaveLength(1)

    // A 断开 → 兜底切到 B → 必须按新对端重载（布尔闸门在此不会触发）
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: false })
    await env.flush()
    expect(listRemoteCalls()).toHaveLength(2)

    wrapper.unmount()
  })

  it('C-L5 活跃对端断开且无候选时清空浏览态', async () => {
    const wrapper = mountView()
    await env.flush()

    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, '设备-a'))
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: true })
    await env.flush()
    await openBrowseTab(wrapper)
    expect(wrapper.findComponent(BrowseTab).props('entries')).toHaveLength(1)
    expect(wrapper.findComponent(BrowseTab).props('peerOnline')).toBe(true)

    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: false })
    await env.flush()
    await nextTick()

    const browse = wrapper.findComponent(BrowseTab)
    expect(browse.props('entries')).toEqual([])
    expect(browse.props('error')).toBeNull()
    expect(browse.props('peerOnline')).toBe(false)

    wrapper.unmount()
  })

  it('C-L6 连接事件先于发现到达时，发现补齐后加载（乱序不漏）', async () => {
    const wrapper = mountView()
    await env.flush()

    // 连接先到：自建缓存还没有条目，兜底激活无从下手
    env.emit('plugin:file-transfer:connection-changed', { nodeId: NODE_A, connected: true })
    await env.flush()
    expect(listRemoteCalls()).toHaveLength(0)

    // 发现补齐：handleFound 触发兜底激活 → 闸门打开 → 加载
    env.emit('plugin:file-transfer:mdns-found', makeFound(NODE_A, '设备-a'))
    await env.flush()
    expect(listRemoteCalls()).toHaveLength(1)

    wrapper.unmount()
  })
})

describe('C-L7 BrowseTab 下拉刷新闸门', () => {
  function makeProps(overrides: Record<string, unknown> = {}) {
    return {
      entries: [],
      loading: false,
      error: null,
      notice: null,
      crumbs: [],
      currentRoot: null,
      selected: new Set<string>(),
      selectedCount: 0,
      peerOnline: false,
      t: (key: string) => key,
      ...overrides,
    } as any
  }

  /** 构造可派发的触摸事件（happy-dom 无 TouchEvent 构造器） */
  function touchEvent(type: string, y: number): Event {
    const event = new Event(type, { bubbles: true, cancelable: true })
    Object.defineProperty(event, 'touches', { value: [{ clientX: 0, clientY: y }] })
    return event
  }

  function pullGesture(el: Element, dy: number): void {
    el.dispatchEvent(touchEvent('touchstart', 100))
    el.dispatchEvent(touchEvent('touchmove', 100 + dy))
    el.dispatchEvent(touchEvent('touchend', 100 + dy))
  }

  it('C-L7a 未就绪时整轮手势惰性：不发 refresh、下拉指示器不产生位移', async () => {
    const wrapper = mount(BrowseTab, { props: makeProps({ peerOnline: false }) })

    pullGesture(wrapper.find('.overflow-y-auto').element, 200)
    await nextTick()

    expect(wrapper.emitted('refresh')).toBeUndefined()
    // 闸门在 touchstart 生效：位移恒为 0，不会留下「刷新中」spinner
    expect(wrapper.find('.fv2-pull').attributes('style')).toContain('height: 0px')

    wrapper.unmount()
  })

  it('C-L7b 已就绪时下拉发出 refresh；loading 回落复位下拉态', async () => {
    const wrapper = mount(BrowseTab, { props: makeProps({ peerOnline: true }) })
    const el = wrapper.find('.overflow-y-auto').element

    el.dispatchEvent(touchEvent('touchstart', 100))
    el.dispatchEvent(touchEvent('touchmove', 300)) // dy=200 → 位移 90px ≥ 56px 触发阈值
    await nextTick()
    expect(wrapper.find('.fv2-pull').attributes('style')).toContain('height: 90px')

    el.dispatchEvent(touchEvent('touchend', 300))
    expect(wrapper.emitted('refresh')).toHaveLength(1)

    // fs.refresh 完成（loading true→false）→ 下拉态复位（BrowseTab 既有机制）
    await wrapper.setProps({ loading: true })
    await wrapper.setProps({ loading: false })
    await nextTick()
    expect(wrapper.find('.fv2-pull').attributes('style')).toContain('height: 0px')

    wrapper.unmount()
  })

  it('C-L7c 未达触发阈值的下拉不发 refresh', async () => {
    const wrapper = mount(BrowseTab, { props: makeProps({ peerOnline: true }) })

    pullGesture(wrapper.find('.overflow-y-auto').element, 50) // 位移 22.5px < 56px
    await nextTick()

    expect(wrapper.emitted('refresh')).toBeUndefined()

    wrapper.unmount()
  })
})
