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
            fsAuthPath: '路径',
            fsAuthPaths: '{count} 个路径',
            fsAuthRemember: '记住此授权',
            fsAuthDeny: '拒绝',
            fsAuthAllow: '允许',
            fsAuthWrite: '写入',
            fsAuthRead: '读取',
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
  await flushPromises()
  tauriEvent.lastFsAuthHandler?.({
    payload: {
      requestId: 'req-1',
      pluginId: 'com.bedcode.agent-hub',
      paths: ['/home/u/.codex', '/home/u/.pi'],
      path: '/home/u/.codex',
      operation: 'read',
    },
  })
  await flushPromises()
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
      { requestId: 'req-1', allowed: true, remember: true },
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
})
