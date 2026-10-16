import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import FsAuthDialog from '@/components/FsAuthDialog.vue'
import LoadingOverlay from '@/components/LoadingOverlay.vue'

// mock Tauri 事件通道：捕获 plugin:fs-auth-request 处理器，测试中直接投递授权请求
const tauriEvent = vi.hoisted(() => ({
  lastFsAuthHandler: null as ((e: { payload: unknown }) => void) | null,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_event: string, handler: (e: { payload: unknown }) => void) => {
    tauriEvent.lastFsAuthHandler = handler
    return () => {}
  }),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => undefined),
}))

/** 测试用最小 i18n 实例：仅含 fs 授权弹窗相关 key */
function createTestI18n() {
  return createI18n({
    legacy: false,
    locale: 'zh-CN',
    fallbackLocale: 'zh-CN',
    messages: {
      'zh-CN': {
        desktop: {
          plugin: {
            fsAuthTitle: '文件访问授权',
            fsAuthRequest: '{plugin} 请求{operation}以下路径',
            fsAuthPickerRequest: '你刚在系统选择框中选中以下路径，{plugin} 请求{operation}',
            // 票 02：目录范围与「记住」都要带本次请求的操作集（读 / 写 / 读写）
            fsAuthGrantScopeDir: '按所在目录授权（{operation}）',
            fsAuthPath: '路径',
            fsAuthPaths: '{count} 个路径',
            fsAuthRemember: '记住此{operation}授权',
            fsAuthDeny: '拒绝',
            fsAuthAllow: '允许',
            // 票 03：三态应答——「总是询问」档不提供「记住」，允许只承诺这一次
            fsAuthAllowOnce: '允许本次',
            fsAuthDenyAlways: '以后都拒绝',
            fsAuthWrite: '写入',
            fsAuthRead: '读取',
            fsAuthReadWrite: '读写',
          },
        },
      },
    },
  })
}

const stubs = { Teleport: false, Transition: false }

function mountDialog() {
  return mount(FsAuthDialog, {
    global: { plugins: [createTestI18n()], stubs },
    attachTo: document.body,
  })
}

function mountMask() {
  return mount(LoadingOverlay, {
    props: { visible: true },
    global: { stubs },
    attachTo: document.body,
  })
}

/** 投递一次批量授权请求（模拟 agent-hub 激活期 fs_request_auth） */
async function emitFsAuthRequest() {
  await emitFsAuthPayload({
    requestId: 'req-1',
    pluginId: 'com.bedcode.agent-hub',
    paths: ['/home/u/.codex', '/home/u/.pi'],
    path: '/home/u/.codex',
    operation: 'read',
  })
}

/**
 * 投递一次授权请求（系统选择器结果门：origin=picker / grantScope=directory）
 *
 * 形状与宿主 `fs_auth::request_user_auth_batch` 发出的事件逐字对齐：
 * grantScope 决定「记住」按文件还是按目录落账，origin 决定弹窗文案。
 */
async function emitFsAuthPayload(payload: Record<string, unknown>) {
  await flushPromises()
  tauriEvent.lastFsAuthHandler?.({ payload })
  await flushPromises()
}

/** 授权弹窗里的正文文案节点（不含路径列表/按钮，便于精确断言） */
function dialogBody(): string {
  return document.querySelector('.max-w-sm')?.textContent ?? ''
}

/** 点弹窗上文本含 `label` 的按钮并等回调落地（找不到即失败——不静默跳过） */
async function clickButton(label: string) {
  const buttons = Array.from(document.querySelectorAll('.max-w-sm button'))
  const button = buttons.find((b) => b.textContent?.includes(label))
  expect(button, `弹窗上没有按钮: ${label}`).toBeTruthy()
  await button!.click()
  await flushPromises()
}

/** 点弹窗上的「允许」并等回调落地 */
async function clickAllow() {
  await clickButton('允许')
}

/** 弹窗上的「记住」勾选框（「总是询问」档不应存在） */
function rememberCheckbox(): HTMLInputElement | null {
  return document.querySelector<HTMLInputElement>('.max-w-sm input[type="checkbox"]')
}

/** 最近一次 plugin_fs_auth_respond 的 decision（弹窗应答的对外事实） */
async function lastDecision(): Promise<string | undefined> {
  const invoke = vi.mocked((await import('@tauri-apps/api/core')).invoke)
  const call = invoke.mock.calls.filter((c) => c[0] === 'plugin_fs_auth_respond').at(-1)
  return (call?.[1] as { decision?: string } | undefined)?.decision
}

/** 从 overlay 根元素的 Tailwind z 工具类解析数值（z-50 → 50、z-[9999] → 9999） */
function zIndexOf(root: Element): number {
  const token = root.className
    .split(/\s+/)
    .find((c) => /^z-(\d+|\[\d+\])$/.test(c))
  if (!token) throw new Error(`overlay root has no z utility class: ${root.className}`)
  return Number(token.replace(/^z-/, '').replaceAll(/[\[\]]/g, ''))
}

/** 按(FsAuthDialog)与启停遮罩(LoadingOverlay)的可辨识子元素从 body 中区分两者根节点 */
function findOverlayRoots() {
  const roots = Array.from(document.body.querySelectorAll('.fixed.inset-0'))
  const maskRoot = roots.find((el) => el.querySelector('.loading-overlay-orb'))
  const authRoot = roots.find((el) => el.querySelector('.max-w-sm'))
  if (!maskRoot || !authRoot) {
    throw new Error(`overlay roots not found: mask=${!!maskRoot} auth=${!!authRoot}`)
  }
  return { maskRoot, authRoot }
}

describe('FsAuthDialog Component', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    tauriEvent.lastFsAuthHandler = null
  })

  it('should render dialog with granted paths after fs-auth-request event', async () => {
    const wrapper = mountDialog()
    await emitFsAuthRequest()

    const dialog = document.querySelector('.max-w-sm')
    expect(dialog).toBeTruthy()
    expect(dialog?.textContent).toContain('文件访问授权')
    expect(dialog?.textContent).toContain('com.bedcode.agent-hub')
    // 多路径批量请求：逐条展示
    expect(dialog?.textContent).toContain('/home/u/.codex')
    expect(dialog?.textContent).toContain('/home/u/.pi')
    wrapper.unmount()
  })

  it('should allow and respond via plugin_fs_auth_respond', async () => {
    const wrapper = mountDialog()
    await emitFsAuthRequest()

    const buttons = Array.from(document.querySelectorAll('.max-w-sm button'))
    const allowBtn = buttons.find((b) => b.textContent?.includes('允许'))
    expect(allowBtn).toBeTruthy()
    await allowBtn!.click()
    await flushPromises()

    expect(vi.mocked(await import('@tauri-apps/api/core')).invoke).toHaveBeenCalledWith(
      'plugin_fs_auth_respond',
      { requestId: 'req-1', decision: 'allow_remember' },
    )
    // 响应后弹窗关闭（Transition 离场在 happy-dom 依赖兜底定时器，轮询等待）
    await vi.waitFor(() => {
      expect(document.querySelector('.max-w-sm')).toBeNull()
    })
    wrapper.unmount()
  })

  it('授权弹窗层级必须高于启停 LoadingOverlay（回归：遮罩不得盖住授权弹窗）', async () => {
    // 挂载顺序复刻真实时序：FsAuthDialog 随 App 启动先上屏，启停遮罩在进入
    // 插件管理页时后上屏——同层级时后者靠 DOM 顺序覆盖前者（用户报告的症状）
    const dialogWrapper = mountDialog()
    const maskWrapper = mountMask()
    // ADR 0007：插件 activate 期间 fs_request_auth 弹授权，此刻遮罩已在显示
    await emitFsAuthRequest()

    const { authRoot, maskRoot } = findOverlayRoots()
    const authZ = zIndexOf(authRoot)
    const maskZ = zIndexOf(maskRoot)
    expect(authZ).toBeGreaterThan(maskZ)

    maskWrapper.unmount()
    dialogWrapper.unmount()
  })

  // ==================== 系统选择器结果门（origin=picker） ====================

  it('选择器来源的请求用「你刚选中」文案，并说明授权按目录落账', async () => {
    const wrapper = mountDialog()
    await emitFsAuthPayload({
      requestId: 'req-pick-1',
      pluginId: 'com.bedcode.file-transfer',
      paths: ['/home/u/Downloads/report.pdf'],
      path: '/home/u/Downloads/report.pdf',
      operation: 'read',
      origin: 'picker',
      grantScope: 'directory',
    })

    const body = dialogBody()
    expect(body).toContain('你刚在系统选择框中选中以下路径')
    expect(body).toContain('com.bedcode.file-transfer')
    // 授权范围必须让用户看得见：否则「勾了记住」的真实含义只有宿主知道
    expect(body).toContain('按所在目录授权')
    wrapper.unmount()
  })

  it('选择器来源点「允许」照常回调，且默认勾选「记住」（同目录后续免问的前提）', async () => {
    const wrapper = mountDialog()
    await emitFsAuthPayload({
      requestId: 'req-pick-2',
      pluginId: 'com.bedcode.file-transfer',
      paths: ['/home/u/Downloads/report.pdf'],
      path: '/home/u/Downloads/report.pdf',
      operation: 'read',
      origin: 'picker',
      grantScope: 'directory',
    })

    await clickAllow()
    expect(vi.mocked(await import('@tauri-apps/api/core')).invoke).toHaveBeenCalledWith(
      'plugin_fs_auth_respond',
      { requestId: 'req-pick-2', decision: 'allow_remember' },
    )
    wrapper.unmount()
  })

  it('插件直请（旧事件，无 origin）不得被说成「你刚选中」，也不显示目录范围说明', async () => {
    const wrapper = mountDialog()
    await emitFsAuthRequest()

    const body = dialogBody()
    expect(body).toContain('请求读取以下路径')
    expect(body).not.toContain('你刚在系统选择框中选中以下路径')
    expect(body).not.toContain('按所在目录授权')
    wrapper.unmount()
  })

  it('exact 粒度（插件直请路径）不显示目录授权说明', async () => {
    const wrapper = mountDialog()
    await emitFsAuthPayload({
      requestId: 'req-exact',
      pluginId: 'com.bedcode.agent-hub',
      paths: ['/home/u/data'],
      path: '/home/u/data',
      operation: 'write',
      origin: 'fs',
      grantScope: 'exact',
    })

    const body = dialogBody()
    expect(body).toContain('写入')
    expect(body).not.toContain('按所在目录授权')
    wrapper.unmount()
  })

  // ==================== 操作集文案（票 02：读 / 写 / 读写） ====================

  it('读写请求显示「读写」，且「记住」与目录范围文案都带本次操作集', async () => {
    const wrapper = mountDialog()
    await emitFsAuthPayload({
      requestId: 'req-rw',
      pluginId: 'com.bedcode.file-transfer',
      paths: ['/home/u/Downloads'],
      path: '/home/u/Downloads',
      operation: 'read+write',
      origin: 'picker',
      grantScope: 'directory',
    })

    const body = dialogBody()
    expect(body).toContain('读写')
    // 票 02：授权按操作拆分，「记住」记的是这次的操作集——文案必须说清
    expect(body).toContain('记住此读写授权')
    expect(body).toContain('按所在目录授权（读写）')
    wrapper.unmount()
  })

  it('只读请求的「记住」文案带「读取」（不得暗示连写一起记住）', async () => {
    const wrapper = mountDialog()
    await emitFsAuthRequest()

    const body = dialogBody()
    expect(body).toContain('请求读取以下路径')
    expect(body).toContain('记住此读取授权')
    expect(body).not.toContain('记住此读写授权')
    wrapper.unmount()
  })
})

// ==================== 三态应答与档位（票 03） ====================

describe('FsAuthDialog 三态应答与档位（票 03）', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    tauriEvent.lastFsAuthHandler = null
  })

  /** 「总是询问」档的授权请求（宿主 `request_user_auth` 发出的形状：带 strategy） */
  async function emitAlwaysAsk() {
    await emitFsAuthPayload({
      requestId: 'req-ask',
      pluginId: 'com.bedcode.test',
      paths: ['/home/u/data'],
      path: '/home/u/data',
      operation: 'read',
      origin: 'fs',
      grantScope: 'exact',
      strategy: 'always_ask',
    })
  }

  it('「总是询问」档不出现「记住」，允许按钮只承诺这一次', async () => {
    const wrapper = mountDialog()
    await emitAlwaysAsk()

    expect(rememberCheckbox(), '总是询问档跳过记录，不得提供「记住」').toBeNull()
    const labels = Array.from(document.querySelectorAll('.max-w-sm button')).map((b) =>
      b.textContent?.trim(),
    )
    expect(labels).toEqual(['拒绝', '以后都拒绝', '允许本次'])
    wrapper.unmount()
  })

  it('「允许本次」= allow_once（一次性放行，不落授权记录）', async () => {
    const wrapper = mountDialog()
    await emitAlwaysAsk()

    await clickButton('允许本次')
    expect(await lastDecision()).toBe('allow_once')
    wrapper.unmount()
  })

  it('「以后都拒绝」= deny_always（落 deny 记录由宿主执行）', async () => {
    const wrapper = mountDialog()
    await emitAlwaysAsk()

    await clickButton('以后都拒绝')
    expect(await lastDecision()).toBe('deny_always')
    wrapper.unmount()
  })

  it('「拒绝」= deny（不落账，下次访问仍会询问）', async () => {
    const wrapper = mountDialog()
    await emitFsAuthRequest()

    await clickButton('拒绝')
    expect(await lastDecision()).toBe('deny')
    wrapper.unmount()
  })

  it('「默认」档保留「记住」：勾选=allow_remember，取消勾选=allow_once', async () => {
    const wrapper = mountDialog()
    await emitFsAuthRequest()
    expect(rememberCheckbox(), '默认档必须提供「记住」').not.toBeNull()

    const box = rememberCheckbox()!
    box.checked = false
    box.dispatchEvent(new Event('change'))
    await flushPromises()
    await clickButton('允许')
    expect(await lastDecision()).toBe('allow_once')

    // 再弹一次并保持默认勾选 → 记住（两态确实不同，不是恒真断言）
    await emitFsAuthRequest()
    await clickAllow()
    expect(await lastDecision()).toBe('allow_remember')
    wrapper.unmount()
  })
})
