/**
 * NetworkAuthDialog 渲染契约（授权策略增强 · 票 05）
 *
 * 被测行为（外部可见输出）：
 * - 事件 `plugin:network-auth-request` 到达后弹出，展示应用名 + **归一化 origin**；
 * - 三个按钮映射到宿主固定的三态决定（allow_once / deny / deny_always），
 *   且都经 `plugin_network_auth_respond` 回传并带宿主凭证（AGENTS §8：授权应答
 *   不得由插件面代答）；
 * - 授权范围说清「同意后此地址不再询问」（否则用户只能靠猜「允许」意味着什么）；
 * - 应答失败（宿主拒绝代答 / 询问已超时失效）不再抛错：用户已做过决定，
 *   宿主侧按拒绝收场；
 * - 层级必须高于插件启停遮罩（与 FsAuthDialog 同一回归：遮罩盖住授权框时，
 *   用户看不见 → 30s 超时 → 请求被拒）。
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import NetworkAuthDialog from '@/components/NetworkAuthDialog.vue'
import LoadingOverlay from '@/components/LoadingOverlay.vue'

// mock Tauri 事件通道：捕获 plugin:network-auth-request 处理器，测试直接投递询问
const tauriEvent = vi.hoisted(() => ({
  lastNetworkAuthHandler: null as ((e: { payload: unknown }) => void) | null,
}))

const mockInvoke = vi.hoisted(() => vi.fn())

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_event: string, handler: (e: { payload: unknown }) => void) => {
    tauriEvent.lastNetworkAuthHandler = handler
    return () => {}
  }),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

/** 测试用最小 i18n 实例：仅含网络授权弹窗相关 key */
function createTestI18n() {
  return createI18n({
    legacy: false,
    locale: 'zh-CN',
    fallbackLocale: 'zh-CN',
    messages: {
      'zh-CN': {
        desktop: {
          plugin: {
            netAuthTitle: '网络访问授权',
            netAuthRequest: '应用 {plugin} 请求访问一个网络地址',
            netAuthOrigin: '地址',
            netAuthScope: '同意后不再询问',
            netAuthScopeOnce: '只放行这一批请求',
            netAuthAllow: '允许',
            netAuthDeny: '拒绝',
            netAuthDenyAlways: '以后都拒绝',
          },
        },
      },
    },
  })
}

const stubs = { Teleport: false, Transition: false }

function mountDialog(): VueWrapper {
  return mount(NetworkAuthDialog, {
    global: { plugins: [createTestI18n()], stubs },
    attachTo: document.body,
  })
}

function mountMask(): VueWrapper {
  return mount(LoadingOverlay, {
    props: { visible: true },
    global: { stubs },
    attachTo: document.body,
  })
}

/** 投递一次出站授权询问（形状与宿主 `network_auth::prompt` 的事件载荷逐字对齐） */
async function emitRequest(overrides: Record<string, unknown> = {}): Promise<void> {
  await flushPromises()
  tauriEvent.lastNetworkAuthHandler?.({
    payload: {
      requestId: 'req-net-1',
      pluginId: 'com.bedcode.agent-hub',
      origin: 'https://api.github.com:443',
      ...overrides,
    },
  })
  await flushPromises()
}

/** 按可见文案点按钮（**全等匹配**：「拒绝」是「以后都拒绝」的子串，模糊匹配会点错） */
async function clickButton(label: string): Promise<void> {
  const button = Array.from(document.querySelectorAll('.max-w-sm button')).find(
    (b) => b.textContent?.trim() === label,
  )
  expect(button, `找不到按钮「${label}」`).toBeTruthy()
  await button!.click()
  await flushPromises()
}

/** 该弹窗发出的应答调用（取最后一次 plugin_network_auth_respond） */
function respondCalls(): unknown[][] {
  return mockInvoke.mock.calls.filter((call) => call[0] === 'plugin_network_auth_respond')
}

/** 从 overlay 根元素的 Tailwind z 工具类解析数值（z-50 → 50、z-[9999] → 9999） */
function zIndexOf(root: Element): number {
  const token = root.className
    .split(/\s+/)
    .find((c) => /^z-(\d+|\[\d+\])$/.test(c))
  if (!token) throw new Error(`overlay root has no z utility class: ${root.className}`)
  return Number(token.replace(/^z-/, '').replaceAll(/[\[\]]/g, ''))
}

describe('NetworkAuthDialog 组件（票 05）', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    tauriEvent.lastNetworkAuthHandler = null
    mockInvoke.mockReset()
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'plugin_frontend_loader_session') return Promise.resolve('loader-session')
      return Promise.resolve(undefined)
    })
  })

  it('事件到达后展示应用与归一化 origin，并说清授权范围', async () => {
    const wrapper = mountDialog()
    await emitRequest()

    const dialog = document.querySelector('.max-w-sm')
    expect(dialog).toBeTruthy()
    expect(dialog?.textContent).toContain('网络访问授权')
    expect(dialog?.textContent).toContain('com.bedcode.agent-hub')
    expect(dialog?.textContent).toContain('https://api.github.com:443')
    // 同意 = 此地址此后免询问：范围说不清时用户只能猜
    expect(dialog?.textContent).toContain('同意后不再询问')
    // 三个决定都要在（宿主只认这三态）
    const labels = Array.from(document.querySelectorAll('.max-w-sm button')).map((b) => b.textContent)
    expect(labels).toEqual(expect.arrayContaining(['以后都拒绝', '拒绝', '允许']))
    wrapper.unmount()
  })

  it('未收到事件时不渲染任何弹窗（不空挂一层遮罩）', () => {
    const wrapper = mountDialog()
    expect(document.querySelector('.max-w-sm')).toBeNull()
    wrapper.unmount()
  })

  it('「总是询问」档（remembers=false）换一句说明：不说「以后不再询问」', async () => {
    const wrapper = mountDialog()
    await emitRequest({ remembers: false })

    const dialog = document.querySelector('.max-w-sm')
    expect(dialog?.textContent).toContain('只放行这一批请求')
    expect(
      dialog?.textContent,
      '该档下不落记录，说「以后不再询问」就是在骗用户（弹窗解释与实际行为必须同源）',
    ).not.toContain('同意后不再询问')
    wrapper.unmount()
  })

  it('载荷缺 remembers（宿主早于该字段）时按旧文案渲染，不凭空改行为', async () => {
    const wrapper = mountDialog()
    await emitRequest()
    const dialog = document.querySelector('.max-w-sm')
    expect(dialog?.textContent).toContain('同意后不再询问')
    wrapper.unmount()
  })

  it('「允许」回传 allow_once 并带宿主凭证，弹窗随即关闭', async () => {
    const wrapper = mountDialog()
    await emitRequest()

    await clickButton('允许')

    expect(respondCalls()).toHaveLength(1)
    expect(respondCalls()[0][1]).toEqual({
      requestId: 'req-net-1',
      decision: 'allow_once',
      credential: 'loader-session',
    })
    await vi.waitFor(() => {
      expect(document.querySelector('.max-w-sm')).toBeNull()
    })
    wrapper.unmount()
  })

  it('「拒绝」与「以后都拒绝」是两个决定（前者不落账、后者落 deny）', async () => {
    const wrapper = mountDialog()

    await emitRequest()
    await clickButton('拒绝')
    expect(respondCalls()[0][1]).toMatchObject({
      requestId: 'req-net-1',
      decision: 'deny',
    })

    await emitRequest({ requestId: 'req-net-2' })
    await clickButton('以后都拒绝')
    expect(respondCalls()[1][1]).toMatchObject({
      requestId: 'req-net-2',
      decision: 'deny_always',
    })

    wrapper.unmount()
  })

  it('应答失败不抛错也不二次弹窗（用户已做过决定，宿主侧按拒绝收场）', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'plugin_frontend_loader_session') return Promise.resolve('loader-session')
      if (cmd === 'plugin_network_auth_respond') {
        return Promise.reject(new Error('network authorization must be answered by the host frontend'))
      }
      return Promise.resolve(undefined)
    })
    const wrapper = mountDialog()
    await emitRequest()

    await clickButton('允许')

    expect(respondCalls()).toHaveLength(1)
    await vi.waitFor(() => {
      expect(document.querySelector('.max-w-sm')).toBeNull()
    })
    // 失败不重开弹窗：再弹一次会让用户以为还能改决定
    await flushPromises()
    expect(document.querySelector('.max-w-sm')).toBeNull()
    wrapper.unmount()
  })

  it('授权弹窗层级必须高于启停 LoadingOverlay（回归：遮罩不得盖住授权弹窗）', async () => {
    // 挂载顺序复刻真实时序：NetworkAuthDialog 随 App 启动先上屏，启停遮罩后上屏
    const dialogWrapper = mountDialog()
    const maskWrapper = mountMask()
    await emitRequest()

    const roots = Array.from(document.body.querySelectorAll('.fixed.inset-0'))
    const maskRoot = roots.find((el) => el.querySelector('.loading-overlay-orb'))
    const authRoot = roots.find((el) => el.querySelector('.max-w-sm'))
    expect(maskRoot).toBeTruthy()
    expect(authRoot).toBeTruthy()
    expect(zIndexOf(authRoot!)).toBeGreaterThan(zIndexOf(maskRoot!))

    maskWrapper.unmount()
    dialogWrapper.unmount()
  })
})
