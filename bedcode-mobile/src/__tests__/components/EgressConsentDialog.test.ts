/**
 * EgressConsentDialog 组件测试（ticket 10：Egress L3 授权弹窗前端侧）
 *
 * 覆盖：
 * 1. 监听 egress_consent_request → 懒触发渲染（域名/路径/来源展示）
 * 2. 允许 → egress_consent_resolve(allow=true, persist=勾选)
 * 3. 拒绝 → egress_consent_resolve(allow=false)
 * 4. 背板点击 = 拒绝
 * 5. FIFO 队列：并发请求先入队，当前结算后再弹下一个
 * 6. 30s 超时自动收起（与 Rust CONSENT_TIMEOUT 同值，fail-closed）
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { nextTick } from 'vue'

// ==================== mock Tauri 边界 ====================

const mockInvoke = vi.fn()
const eventHandlers: Record<string, Array<(payload: unknown) => void>> = {}
const mockListen = vi.fn((event: string, handler: (payload: unknown) => void) => {
  if (!eventHandlers[event]) eventHandlers[event] = []
  eventHandlers[event].push(handler)
  return Promise.resolve(() => {
    eventHandlers[event] = (eventHandlers[event] || []).filter((h) => h !== handler)
  })
})

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: any[]) => mockListen(...args),
}))

// i18n：最小消息表 + {param} 插值（断言描述文本真实渲染，含插件来源插值）
const messages: Record<string, string> = {
  'mobile.egress.title': '外网访问授权',
  'mobile.egress.requestByHost': '应用请求访问以下外部地址',
  'mobile.egress.requestByPlugin': '插件 {plugin} 请求访问以下外部地址',
  'mobile.egress.requestByUnknown': '来源 {source} 请求访问以下外部地址',
  'mobile.egress.targetUrl': '访问地址',
  'mobile.egress.remember': '不再询问（记住此域名）',
  'mobile.egress.allow': '允许',
  'mobile.egress.deny': '拒绝',
}
vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, unknown>) => {
      let s = messages[key] ?? key
      if (params) {
        for (const [k, v] of Object.entries(params)) s = s.replaceAll(`{${k}}`, String(v))
      }
      return s
    },
  }),
}))

vi.mock('@/utils/frontendLogger', () => ({
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), log: vi.fn() },
}))

import EgressConsentDialog from '@/components/EgressConsentDialog.vue'

// ==================== 工具 ====================

/** 构造 egress_consent_request 事件 payload（Rust ConsentRequest emit 形状） */
function makePayload(overrides: Partial<{
  request_id: string
  url: string
  host: string
  path: string
  source: string
}> = {}): {
  request_id: string
  url: string
  host: string
  path: string
  source: string
} {
  return {
    request_id: 'req-1',
    url: 'https://custom.example.com/api/v1',
    host: 'custom.example.com',
    path: '/api/v1',
    source: 'host',
    ...overrides,
  }
}

/** 推送一个授权请求（经组件注册的监听器），并等待渲染 */
async function emitConsent(payload = makePayload()) {
  for (const h of eventHandlers['egress_consent_request'] || []) h({ payload })
  await nextTick()
}

/** 挂载组件（Transition stub 掉使挂载/卸载即时；内容经 Teleport 落 body） */
function mountDialog() {
  return mount(EgressConsentDialog, {
    global: {
      stubs: { Transition: { template: '<slot />' } },
    },
  })
}

/** body 内当前弹窗面板（Teleport 目标） */
function panel(): HTMLElement | null {
  return document.body.querySelector('.modal-panel')
}

/** 点击弹窗内指定文案的按钮 */
async function clickButton(text: string) {
  const btn = [...document.body.querySelectorAll('button')].find((b) => b.textContent?.trim() === text)
  expect(btn).toBeTruthy()
  btn!.click()
  await nextTick()
}

describe('EgressConsentDialog', () => {
  let wrapper: ReturnType<typeof mountDialog>

  beforeEach(() => {
    vi.clearAllMocks()
    wrapper = mountDialog()
  })

  afterEach(() => {
    wrapper.unmount()
    document.body.innerHTML = ''
    vi.useRealTimers()
  })

  it('挂载时注册 egress_consent_request 监听', async () => {
    await flushPromises()
    expect(mockListen).toHaveBeenCalledWith('egress_consent_request', expect.any(Function))
  })

  it('收到请求时懒触发渲染：域名 + 路径 + 来源（宿主）', async () => {
    await flushPromises()
    await emitConsent()
    const text = panel()!.textContent || ''
    expect(text).toContain('外网访问授权')
    expect(text).toContain('应用请求访问以下外部地址')
    expect(text).toContain('custom.example.com')
    expect(text).toContain('/api/v1')
  })

  it('插件来源展示插件 id（source=plugin:<id> 插值）', async () => {
    await flushPromises()
    await emitConsent(makePayload({ source: 'plugin:com.bedcode.ai-chatbox' }))
    const text = panel()!.textContent || ''
    expect(text).toContain('插件 com.bedcode.ai-chatbox 请求访问以下外部地址')
  })

  it('允许 → egress_consent_resolve(allow=true, persist=false)', async () => {
    await flushPromises()
    await emitConsent()
    await clickButton('允许')
    expect(mockInvoke).toHaveBeenCalledWith('egress_consent_resolve', {
      requestId: 'req-1',
      allow: true,
      persist: false,
    })
    expect(panel()).toBeNull()
  })

  it('勾选「不再询问」后允许 → persist=true（持久授权）', async () => {
    await flushPromises()
    await emitConsent()
    const checkbox = panel()!.querySelector('input[type="checkbox"]') as HTMLInputElement
    checkbox.click()
    await nextTick()
    await clickButton('允许')
    expect(mockInvoke).toHaveBeenCalledWith('egress_consent_resolve', {
      requestId: 'req-1',
      allow: true,
      persist: true,
    })
  })

  it('拒绝 → egress_consent_resolve(allow=false, persist=false)', async () => {
    await flushPromises()
    await emitConsent()
    await clickButton('拒绝')
    expect(mockInvoke).toHaveBeenCalledWith('egress_consent_resolve', {
      requestId: 'req-1',
      allow: false,
      persist: false,
    })
    expect(panel()).toBeNull()
  })

  it('点击背板 = 拒绝（fail-closed 默认动作）', async () => {
    await flushPromises()
    await emitConsent()
    const backdrop = document.body.querySelector('.fixed.inset-0') as HTMLElement
    backdrop.click()
    await nextTick()
    expect(mockInvoke).toHaveBeenCalledWith('egress_consent_resolve', {
      requestId: 'req-1',
      allow: false,
      persist: false,
    })
  })

  it('FIFO 队列：并发请求先入队，当前结算后再弹下一个', async () => {
    await flushPromises()
    await emitConsent(makePayload({ request_id: 'req-1', host: 'first.example.com', path: '/a' }))
    await emitConsent(makePayload({ request_id: 'req-2', host: 'second.example.com', path: '/b' }))
    // 只展示第一个
    expect(panel()!.textContent).toContain('first.example.com')
    expect(panel()!.textContent).not.toContain('second.example.com')
    // 结算第一个（拒绝）
    await clickButton('拒绝')
    expect(mockInvoke).toHaveBeenCalledWith('egress_consent_resolve', {
      requestId: 'req-1',
      allow: false,
      persist: false,
    })
    // 队列推进 → 展示第二个
    expect(panel()!.textContent).toContain('second.example.com')
    await clickButton('允许')
    expect(mockInvoke).toHaveBeenCalledWith('egress_consent_resolve', {
      requestId: 'req-2',
      allow: true,
      persist: false,
    })
  })

  it('30s 超时自动收起（Rust 侧同值超时已按拒绝结算，不 invoke 回执）', async () => {
    vi.useFakeTimers()
    await flushPromises()
    await emitConsent()
    expect(panel()).not.toBeNull()
    await vi.advanceTimersByTimeAsync(30_000)
    await nextTick()
    expect(panel()).toBeNull()
    // 超时由 Rust 结算，前端不 invoke（request_id 已不存在）
    expect(mockInvoke).not.toHaveBeenCalled()
  })

  it('重复 request_id 事件去重（防御重发，不产生悬挂事务）', async () => {
    await flushPromises()
    await emitConsent(makePayload({ request_id: 'req-1', host: 'dup.example.com' }))
    await emitConsent(makePayload({ request_id: 'req-1', host: 'dup.example.com' }))
    expect(panel()!.textContent).toContain('dup.example.com')
    await clickButton('拒绝')
    expect(mockInvoke).toHaveBeenCalledTimes(1)
    expect(panel()).toBeNull()
  })
})
