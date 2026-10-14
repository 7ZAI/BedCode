/**
 * 宿主页两个区块组件 挂载契约测试
 * （票 2026-10-09；补自：模板曾引用未声明的绑定 —— 运行时 Vue 会警告且渲染为空，
 *   而 composable 单测覆盖不到模板绑定，故这里按「真实挂载 + 断言渲染」补齐）
 *
 * 被测：`DevicesSection.vue` / `SessionsSection.vue`（经 `pluginContext` provide 驱动）。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-C1 | DevicesSection 模板 | 扫码入口 / mDNS 切换 / 手动地址输入在场 | 三个控件渲染 |
 * | C-C2 | `connectionHistory` 绑定 | 有历史时渲染历史区（漏声明绑定则整块不渲染） | 历史条目地址在场 |
 * | C-C3 | `isAuthenticated` + `biometric` 绑定 | 已认证且设备支持生物 → 生物登录按钮在场 | 按钮文案在场 |
 * | C-C4 | SessionsSection `sessions` 绑定 | 已认证且有会话 → 会话名 + 「打开终端」 | 文案在场 |
 * | C-C5 | SessionsSection 未认证分支 | 未认证 → 空态提示（不渲染会话列表） | 空态文案在场 + 无终端按钮 |
 * | C-C6 | 打开终端动作 | 点会话 → emit open-terminal（并预热订阅命令） | emit 载荷 + 命令调用 |
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import DevicesSection from '../components/DevicesSection.vue'
import SessionsSection from '../components/SessionsSection.vue'
import { installMockMobileApi, uninstallMockMobileApi, type MockMobileApi } from '../../terminal/__tests__/testKit'

// 扫码面板内嵌 html5-qrcode（浏览器库）：挂载测试不触碰相机，仅替身掉模块加载
vi.mock('html5-qrcode', () => ({ Html5Qrcode: class {} }))

/** 最小插件上下文（i18n 回显 key；其余面仅供组件 setup 读取） */
function makePluginContext(): PluginContext {
  const listeners = new Map<string, Set<(payload?: unknown) => void>>()
  return {
    id: 'com.bedcode.terminal-session',
    commands: {
      execute: vi.fn(async () => ({})),
    },
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
    dialogs: { showToast: vi.fn() },
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

beforeEach(() => {
  api = installMockMobileApi({})
  ctx = makePluginContext()
})

afterEach(() => {
  uninstallMockMobileApi()
})

describe('C-C1/C-C2/C-C3 DevicesSection 渲染契约', () => {
  it('should_renderScanMdnsAndManualControls_when_mounted', () => {
    const wrapper = mountSection(DevicesSection)

    expect(wrapper.text()).toContain('hub.qrConnect')
    expect(wrapper.text()).toContain('hub.scan')
    expect(wrapper.find('input').exists()).toBe(true)
    wrapper.unmount()
  })

  it('should_renderHistoryEntry_when_connectionHistoryFilled', () => {
    api.connectionHistory.value = [{ address: '10.0.0.9:8765', name: 'desk-9' }]
    const wrapper = mountSection(DevicesSection)

    expect(wrapper.text()).toContain('desk-9')
    expect(wrapper.text()).toContain('10.0.0.9:8765')
    wrapper.unmount()
  })

  it('should_renderBiometricEntry_when_authenticatedAndDeviceSupports', async () => {
    api.isConnected.value = true
    api.getBiometricKeyStatus = async () => ({ deviceSupported: true, deviceReason: 0, hasKey: false })
    const wrapper = mountSection(DevicesSection)

    // 生物状态经 onMounted 异步探测，等一轮微任务
    await Promise.resolve()
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('hub.biometricLogin')
    expect(wrapper.text()).toContain('hub.pairBiometric')
    wrapper.unmount()
  })
})

describe('C-C4/C-C5/C-C6 SessionsSection 渲染与动作契约', () => {
  it('should_renderSessionRow_when_authenticatedWithSessions', () => {
    api.isConnected.value = true
    api.activeSessions.value = [{ id: 's-1', name: 'build', status: 'running' }]
    const wrapper = mountSection(SessionsSection)

    expect(wrapper.text()).toContain('build')
    expect(wrapper.text()).toContain('hub.openTerminal')
    expect(wrapper.text()).toContain('hub.statusRunning')
    wrapper.unmount()
  })

  it('should_renderEmptyStateAndHideTerminalAction_when_notAuthenticated', () => {
    api.isConnected.value = false
    api.activeSessions.value = [{ id: 's-1', name: 'build', status: 'running' }]
    const wrapper = mountSection(SessionsSection)

    expect(wrapper.text()).toContain('hub.notConnected')
    expect(wrapper.text()).not.toContain('hub.openTerminal')
    expect(wrapper.text()).not.toContain('build')
    wrapper.unmount()
  })

  it('should_emitOpenTerminalAndPreSubscribe_when_sessionTapped', async () => {
    api.isConnected.value = true
    api.activeSessions.value = [{ id: 's-7', name: 'logs', status: 'running' }]
    const wrapper = mountSection(SessionsSection)

    // 会话行按钮（按会话名定位，避开页顶「刷新」按钮）
    const rowButton = wrapper.findAll('button').find((b) => b.text().includes('logs'))
    expect(rowButton, '未找到会话行按钮').toBeTruthy()
    await rowButton!.trigger('click')

    const emitted = wrapper.emitted('open-terminal')
    expect(emitted?.[0]).toEqual(['s-7', 'logs'])
    expect(ctx.commands.execute).toHaveBeenCalledWith('terminal-session.subscribe', { sessionId: 's-7' })
    wrapper.unmount()
  })
})
