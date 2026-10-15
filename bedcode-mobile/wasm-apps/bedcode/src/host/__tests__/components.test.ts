/**
 * 宿主页两个区块组件 挂载契约测试
 * （票 2026-10-09 首版；2026-10-15 按「旧宿主形态还原」重写契约）
 *
 * 被测：`DevicesSection.vue` / `SessionsSection.vue`（经 `pluginContext` provide 驱动）。
 * 形态背景：两页已按旧宿主 `Views/DevicesView.vue` + `Views/SessionsView.vue` 还原——
 * 页头 + 底部双 CTA + 弹出式认证（配对码弹窗 / 生物弹窗）+ 发现面板 + 二次确认弹窗。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-C1 | 连接页底部 CTA | 未连接 | 二维码连接 + 手动连接按钮 + 页头雷达按钮在场 |
 * | C-C2 | 连接历史区块 | 有历史 | 名称 / 地址 / 相对时间 / 清除入口在场；单项 × 触发移除 |
 * | C-C3 | 已认证设备卡 | isPaired | 名称 + 地址 + 断开 + 会话入口在场 |
 * | C-C7 | 已连未认证 | status=connected、paired=false | 自动进入认证流程（请求配对命令 + 配对弹窗） |
 * | C-C8 | 手动连接 | 点手动连接 → 弹层提交 | connectDevice 收到默认端口补齐的目标 |
 * | C-C4 | 会话行 | 已认证且有会话 | 会话名 + 状态徽章 + 停止按钮在场 |
 * | C-C5 | 会话未认证分支 | 未认证 | 空态 + 去连接入口（emit open-connection），不渲染会话 |
 * | C-C6 | 打开终端动作 | 点会话行 | emit open-terminal + prewarm（只建连，不发 subscribe） |
 * | C-C9 | 终端上限 | 运行中会话数达 maxOpenTerminals | 拒绝启动（不调命令）+ toast |
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { nextTick } from 'vue'
import { mount } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import DevicesSection from '../components/DevicesSection.vue'
import SessionsSection from '../components/SessionsSection.vue'
import { installMockMobileApi, uninstallMockMobileApi, type MockMobileApi } from '../../terminal/__tests__/testKit'

// 扫码面板内嵌 html5-qrcode（浏览器库）：挂载测试不触碰相机，仅替身掉模块加载
vi.mock('html5-qrcode', () => ({ Html5Qrcode: class {} }))

/** 最小插件上下文（i18n 回显 key；commands 一律受理） */
function makePluginContext() {
  const listeners = new Map<string, Set<(payload?: unknown) => void>>()
  return {
    id: 'com.bedcode.terminal-session',
    commands: { execute: vi.fn(async () => ({})) },
    events: {
      on: (event: string, handler: (payload?: unknown) => void) => {
        let set = listeners.get(event)
        if (!set) {
          set = new Set()
          listeners.set(event, set)
        }
        set.add(handler)
        return { dispose: () => set!.delete(handler) }
      },
    },
    i18n: { t: (key: string) => key },
    dialogs: { showToast: vi.fn(), showConfirm: vi.fn(async () => true) },
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
  } as unknown as PluginContext
}

let api: MockMobileApi
let ctx: PluginContext

/** 挂载组件（provide pluginContext：与 PluginViewHost 生产路径同款） */
function mountSection(component: unknown) {
  return mount(component as never, {
    global: { provide: { pluginContext: ctx } },
  })
}

/** 等异步编排（宿主投影 + 命令面）落定 */
async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) await Promise.resolve()
  await nextTick()
}

beforeEach(() => {
  api = installMockMobileApi({})
  ctx = makePluginContext()
})

afterEach(() => {
  uninstallMockMobileApi()
  document.body.innerHTML = ''
})

describe('C-C1/C-C2/C-C3/C-C7/C-C8 连接页形态与流程契约', () => {
  it('should_renderBottomCtasAndRadar_when_notConnected', () => {
    const wrapper = mountSection(DevicesSection)

    expect(wrapper.text()).toContain('hub.qrConnect')
    expect(wrapper.text()).toContain('hub.manualConnect')
    expect(wrapper.find('button[title="hub.discoverDevices"]').exists()).toBe(true)
    wrapper.unmount()
  })

  it('should_renderHistoryRowWithTimeAndRemoveEntry_when_historyFilled', async () => {
    api.connectionHistory.value = [
      { address: '10.0.0.9:8765', name: 'desk-9', lastConnected: new Date().toISOString() },
    ]
    api.removeFromConnectionHistory = vi.fn(async () => {})
    const wrapper = mountSection(DevicesSection)

    expect(wrapper.text()).toContain('desk-9')
    expect(wrapper.text()).toContain('10.0.0.9:8765')
    // 相对时间（刚刚）：旧实现 formatLastConnected 的落点
    expect(wrapper.text()).toContain('hub.timeJustNow')

    await wrapper.find('button[aria-label="hub.remove"]').trigger('click')
    expect(api.removeFromConnectionHistory).toHaveBeenCalledWith('10.0.0.9:8765')
    wrapper.unmount()
  })

  it('should_renderDeviceCardWithDisconnectAndSessionsEntry_when_authenticated', () => {
    api.isConnected.value = true
    api.isPaired.value = true
    api.connectionStatus.value = 'paired'
    api.currentDevice.value = { address: '10.0.0.9', port: 8765, name: 'desk-9' }
    const wrapper = mountSection(DevicesSection)

    expect(wrapper.text()).toContain('desk-9')
    expect(wrapper.text()).toContain('10.0.0.9:8765')
    expect(wrapper.text()).toContain('hub.disconnect')
    expect(wrapper.text()).toContain('hub.sessions')
    wrapper.unmount()
  })

  it('should_enterAuthFlowWithPairingDialog_when_connectedButNotPaired', async () => {
    // 回归：传输层连上不算可用（受保护端点 401），必须给出认证入口。
    // 旧实现由连接流程主动弹认证弹窗；本页对「挂载时已连未认证」（重连恢复）也走同一路径。
    api.isConnected.value = true
    api.isPaired.value = false
    api.connectionStatus.value = 'connected'
    api.currentDevice.value = { address: '10.0.0.9', port: 8765, name: 'desk-9' }

    const wrapper = mountSection(DevicesSection)
    await settle()

    expect(ctx.commands.execute).toHaveBeenCalledWith('terminal-session.request-pairing')
    // 配对弹窗 Teleport 到 body
    expect(document.body.textContent).toContain('hub.pairingCodeTitle')
    wrapper.unmount()
  })

  it('should_connectWithDefaultPort_when_manualAddressHasNoPort', async () => {
    const connectDevice = vi.fn(async () => {})
    api.connectDevice = connectDevice
    api.mobileSettings.value = { defaultPort: 9000 }

    const wrapper = mountSection(DevicesSection)
    const manualBtn = wrapper.findAll('button').find((b) => b.text().includes('hub.manualConnect'))
    expect(manualBtn, '未找到手动连接按钮').toBeTruthy()
    await manualBtn!.trigger('click')
    await nextTick()

    // BottomSheet 也是 Teleport 到 body 的组件：经组件实例拿输入框
    const input = document.querySelector('input[placeholder="hub.addressPlaceholder"]') as HTMLInputElement
    expect(input, '未找到手动地址输入框').toBeTruthy()
    input.value = '10.0.0.5'
    input.dispatchEvent(new Event('input'))
    await nextTick()

    const confirmBtn = Array.from(document.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('hub.actionConfirm'),
    ) as HTMLButtonElement
    confirmBtn.click()
    await settle()

    expect(connectDevice).toHaveBeenCalledWith({ address: '10.0.0.5', port: 9000, name: '10.0.0.5' })
    // 手动连接必须走认证流程（不允许静默用存量凭据）
    expect(ctx.commands.execute).toHaveBeenCalledWith('terminal-session.request-pairing')
    wrapper.unmount()
  })
})

describe('C-C4/C-C5/C-C6/C-C9 会话页形态与动作契约', () => {
  it('should_renderSessionRowWithStatusAndStop_when_authenticatedWithSessions', () => {
    api.isConnected.value = true
    api.isPaired.value = true
    api.activeSessions.value = [{ id: 's-1', name: 'build', status: 'running' }]
    const wrapper = mountSection(SessionsSection)

    expect(wrapper.text()).toContain('build')
    expect(wrapper.text()).toContain('hub.statusRunning')
    expect(wrapper.find('button[title="hub.stopSession"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('hub.runningSessions')
    wrapper.unmount()
  })

  it('should_renderEmptyStateWithConnectEntry_when_notAuthenticated', async () => {
    // 传输层已连但未认证同样算「不可用」：受保护端点会 401
    api.isConnected.value = true
    api.isPaired.value = false
    api.activeSessions.value = [{ id: 's-1', name: 'build', status: 'running' }]
    const wrapper = mountSection(SessionsSection)

    expect(wrapper.text()).toContain('hub.notConnected')
    expect(wrapper.text()).not.toContain('build')

    // 会话 → 连接联动：空态的去连接入口上抛 open-connection
    const connectBtn = wrapper.findAll('button').find((b) => b.text().includes('hub.qrConnect'))
    expect(connectBtn, '未找到去连接入口').toBeTruthy()
    await connectBtn!.trigger('click')
    expect(wrapper.emitted('open-connection')).toBeTruthy()
    wrapper.unmount()
  })

  it('should_emitOpenTerminalAndPrewarmLink_when_sessionTapped', async () => {
    api.isConnected.value = true
    api.isPaired.value = true
    api.activeSessions.value = [{ id: 's-7', name: 'logs', status: 'running' }]
    const wrapper = mountSection(SessionsSection)

    const row = wrapper.findAll('.group-row').find((r) => r.text().includes('logs'))
    expect(row, '未找到会话行').toBeTruthy()
    await row!.trigger('click')

    expect(wrapper.emitted('open-terminal')?.[0]).toEqual(['s-7', 'logs'])
    expect(api.activeSessionId.value).toBe('s-7')
    // 预热只建连（不发 subscribe）：桌面端每次 subscribe 都整段重放，预热期
    // 页面通道未登记 ⇒ 走 subscribe 预热等于白传一遍大块历史
    expect(ctx.commands.execute).toHaveBeenCalledWith('terminal-session.prewarm', { sessionId: 's-7' })
    wrapper.unmount()
  })

  it('should_rejectStart_when_runningSessionsReachTerminalLimit', async () => {
    api.isConnected.value = true
    api.isPaired.value = true
    api.mobileSettings.value = { maxOpenTerminals: 1 }
    api.activeSessions.value = [{ id: 's-1', name: 'build', status: 'running' }]
    api.sessionConfigs.value = [{ id: 'c-1', name: 'cfg', environment: 'local', working_dir: '/w' }]

    const wrapper = mountSection(SessionsSection)
    const startBtn = wrapper.findAll('button').find((b) => b.text().includes('hub.start'))
    expect(startBtn, '未找到启动按钮').toBeTruthy()
    await startBtn!.trigger('click')
    await settle()

    expect(ctx.commands.execute).not.toHaveBeenCalledWith('terminal-session.start-session', expect.anything())
    expect(ctx.dialogs.showToast).toHaveBeenCalledWith('hub.maxReached', 'error')
    wrapper.unmount()
  })
})

describe('C-C10 扫码认证路径（自持 token，不走配对码兜底）', () => {
  it('should_authenticateWithQrAndSkipPairingRequest_when_scanResultConfirmed', async () => {
    // 扫码途中状态会短暂落在 connected 且未认证：兜底 watch 不得抢先请求配对码
    api.connectDevice = async () => {
      api.isConnected.value = true
      api.connectionStatus.value = 'connected'
    }
    ctx.commands.execute = vi.fn(async (id: string) =>
      id === 'terminal-session.authenticate-with-qr' ? { accepted: true } : {},
    )

    const wrapper = mountSection(DevicesSection)
    const scanBtn = wrapper.findAll('button').find((b) => b.text().includes('hub.qrConnect'))
    expect(scanBtn, '未找到二维码连接按钮').toBeTruthy()
    await scanBtn!.trigger('click')
    await nextTick()

    // 模拟取景器识别到合法载荷
    const scanPanel = wrapper.findComponent({ name: 'ScanPanel' })
    scanPanel.vm.$emit('scan-result', { host: '10.0.0.9', port: 8765, token: 'one-shot' })
    await nextTick()

    // 扫描结果卡（点击卡片即连接 + 用 token 认证）
    const resultCard = wrapper.findAll('button').find((b) => b.text().includes('10.0.0.9:8765'))
    expect(resultCard, '未找到扫描结果卡').toBeTruthy()
    await resultCard!.trigger('click')
    await settle()

    expect(ctx.commands.execute).toHaveBeenCalledWith('terminal-session.authenticate-with-qr', {
      token: 'one-shot',
    })
    expect(ctx.commands.execute).not.toHaveBeenCalledWith('terminal-session.request-pairing')
    wrapper.unmount()
  })
})

describe('C-C11 首选认证方式（设置项驱动的弹窗选择）', () => {
  it('should_openBiometricDialog_when_preferredAuthIsBiometricAndBound', async () => {
    api.mobileSettings.value = { preferredAuthMethod: 'biometric' }
    api.getBiometricKeyStatus = async () => ({ deviceSupported: true, deviceReason: 0, hasKey: true })
    api.isConnected.value = true
    api.connectionStatus.value = 'connected'
    api.currentDevice.value = { address: '10.0.0.9', port: 8765, name: 'desk-9' }

    const wrapper = mountSection(DevicesSection)
    await settle()

    expect(ctx.commands.execute).toHaveBeenCalledWith('terminal-session.authenticate-with-biometric')
    expect(ctx.commands.execute).not.toHaveBeenCalledWith('terminal-session.request-pairing')
    expect(document.body.textContent).toContain('hub.authBiometric')
    wrapper.unmount()
  })

  it('should_fallBackToPairing_when_preferredAuthIsBiometricButNotBound', async () => {
    // 首选生物但本机没绑定：旧实现在此回落配对码（不能弹一个点不动的空指纹窗）
    api.mobileSettings.value = { preferredAuthMethod: 'biometric' }
    api.getBiometricKeyStatus = async () => ({ deviceSupported: true, deviceReason: 0, hasKey: false })
    api.isConnected.value = true
    api.connectionStatus.value = 'connected'
    api.currentDevice.value = { address: '10.0.0.9', port: 8765, name: 'desk-9' }

    const wrapper = mountSection(DevicesSection)
    await settle()

    expect(ctx.commands.execute).toHaveBeenCalledWith('terminal-session.request-pairing')
    expect(ctx.commands.execute).not.toHaveBeenCalledWith('terminal-session.authenticate-with-biometric')
    wrapper.unmount()
  })
})
