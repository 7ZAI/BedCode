/**
 * AuthorizationView 网络分区（授权策略增强 · 票 05）
 *
 * 票 02 的文件分区用例在 `AuthorizationView.test.ts`；本文件只锁**网络 origin**
 * 那一半的行为，避免与文件侧用例互相覆盖：
 * - 展开行后网络分区独立成块，列出归一化 origin（`https://api.github.com:443`）；
 * - 网络记录**不渲染操作集徽标**（`ops` 恒空，渲染出来是「空操作」的死信息）；
 * - 撤销网络记录调 `plugin_auth_revoke` 且 `resource='network'`（写错资源会把
 *   文件记录删掉——那种 bug 只有断言参数才抓得到）；
 * - 硬拒绝行显示「移除拒绝」而不是「取消授权」（两种意图走两条命令）；
 * - 网络分区无记录时显示网络专属空态文案（不是文件那一句）。
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import i18n from '@/locales'
import AuthorizationView from '@/views/AuthorizationView.vue'
import { toast } from 'vue-sonner'
import type { AuthRecord, PluginAuthOverview } from '@/utils/authPolicy'

const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
  convertFileSrc: (p: string) => `asset://localhost/${p}`,
}))

vi.mock('vue-sonner', () => ({
  toast: {
    success: vi.fn(() => 'mock-id'),
    error: vi.fn(() => 'mock-id-error'),
    warning: vi.fn(() => 'mock-id'),
    info: vi.fn(() => 'mock-id'),
  },
}))

const mockedToast = vi.mocked(toast)

/** 一条网络授权记录（`ops` 恒空、target = 归一化 origin，与宿主写面一致） */
function netRecord(overrides: Partial<AuthRecord> & { id: number; target: string }): AuthRecord {
  return {
    pluginId: 'com.bedcode.test',
    resource: 'network',
    effect: 'allow',
    ops: [],
    prefixMatch: false,
    source: 'user',
    createdAt: 1,
    ...overrides,
  }
}

/** 读模型替身：一个应用 + 若干网络记录 */
function appWithNetwork(records: AuthRecord[]): PluginAuthOverview {
  return {
    pluginId: 'com.bedcode.test',
    name: 'Test App',
    strategies: [
      { resource: 'fs', strategy: 'default' },
      { resource: 'network', strategy: 'default' },
    ],
    records,
    firstPartyDirs: [],
  }
}

let overviewResult: PluginAuthOverview[] | Error
let revokeCalls: unknown[][]
let removeCalls: unknown[][]

function installInvokeMock(): void {
  mockInvoke.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    if (cmd === 'plugin_frontend_loader_session') return Promise.resolve('loader-session')
    if (cmd === 'plugin_auth_overview') {
      if (overviewResult instanceof Error) return Promise.reject(overviewResult)
      return Promise.resolve(overviewResult)
    }
    if (cmd === 'plugin_auth_revoke') {
      revokeCalls.push(args)
      return Promise.resolve(1)
    }
    if (cmd === 'plugin_auth_remove_record') {
      removeCalls.push(args)
      return Promise.resolve(1)
    }
    return Promise.resolve(undefined)
  })
}

async function mountView(): Promise<VueWrapper> {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/settings', name: 'settings', component: { template: '<div />' } },
      { path: '/settings/authorization', name: 'settings-authorization', component: AuthorizationView },
    ],
  })
  await router.push({ name: 'settings-authorization' })
  await router.isReady()
  const wrapper = mount(AuthorizationView, {
    global: { plugins: [createPinia(), i18n, router] },
  })
  await flushPromises()
  return wrapper
}

/** 展开首个应用行（网络分区在展开块里） */
async function expandFirstRow(wrapper: VueWrapper): Promise<void> {
  await wrapper.find('button[aria-expanded]').trigger('click')
  await flushPromises()
}

/** 去掉全部空白（元素文本串联没有分隔符） */
function compact(s: string): string {
  return s.replace(/\s+/g, '')
}

describe('AuthorizationView 网络分区（票 05）', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockInvoke.mockReset()
    mockedToast.success.mockClear()
    revokeCalls = []
    removeCalls = []
    overviewResult = []
    installInvokeMock()
  })

  it('展开后网络分区独立成块并列出归一化 origin', async () => {
    overviewResult = [
      appWithNetwork([
        netRecord({ id: 1, target: 'https://api.github.com:443', createdAt: 2 }),
        netRecord({
          id: 2,
          target: 'https://api.openai.com:443',
          effect: 'deny',
          source: 'user_deny',
          createdAt: 1,
        }),
      ]),
    ]

    const wrapper = await mountView()
    await expandFirstRow(wrapper)

    const text = compact(wrapper.text())
    // 两个 origin 都在，且分属「已授权 / 硬拒绝」两种效果
    expect(text).toContain(compact('https://api.github.com:443'))
    expect(text).toContain(compact('https://api.openai.com:443'))
    expect(text).toContain(compact('网络地址记录'))
    expect(text).toContain(compact('已授权'))
    expect(text).toContain(compact('硬拒绝'))
    // 已授权在前、硬拒绝在后（收敛信息后置）
    expect(text.indexOf('api.github.com')).toBeLessThan(text.indexOf('api.openai.com'))
    wrapper.unmount()
  })

  it('网络记录不渲染操作集徽标（ops 恒空，渲染出来是死信息）', async () => {
    overviewResult = [
      appWithNetwork([netRecord({ id: 1, target: 'https://api.github.com:443' })]),
    ]

    const wrapper = await mountView()
    await expandFirstRow(wrapper)

    const row = wrapper.findAll('[data-testid="auth-record-row"]').find((r) =>
      r.text().includes('api.github.com'),
    )
    expect(row).toBeTruthy()
    const rowText = row!.text()
    expect(rowText).toContain('用户确认')
    // 文件侧的「读 / 写 / 读写」徽标不得出现在网络行上
    for (const opLabel of ['读写', '读', '写']) {
      expect(compact(rowText)).not.toContain(compact(opLabel))
    }
    wrapper.unmount()
  })

  it('撤销网络记录调 plugin_auth_revoke 且 resource=network', async () => {
    overviewResult = [appWithNetwork([netRecord({ id: 1, target: 'https://api.github.com:443' })])]

    const wrapper = await mountView()
    await expandFirstRow(wrapper)

    const row = wrapper.findAll('[data-testid="auth-record-row"]').find((r) =>
      r.text().includes('api.github.com'),
    )!
    const revokeBtn = row.findAll('button').find((b) => b.text().includes('取消授权'))!
    await revokeBtn.trigger('click')
    await flushPromises()

    expect(revokeCalls).toHaveLength(1)
    expect(revokeCalls[0]).toMatchObject({
      pluginId: 'com.bedcode.test',
      resource: 'network',
      target: 'https://api.github.com:443',
      credential: 'loader-session',
    })
    expect(mockedToast.success).toHaveBeenCalledTimes(1)
    expect(String(mockedToast.success.mock.calls[0][0])).toBe(
      i18n.global.t('settings.authorization.records.networkRevoked'),
    )
    wrapper.unmount()
  })

  it('硬拒绝行走「移除拒绝」命令（两种意图不混用）', async () => {
    overviewResult = [
      appWithNetwork([
        netRecord({
          id: 2,
          target: 'https://api.openai.com:443',
          effect: 'deny',
          source: 'user_deny',
        }),
      ]),
    ]

    const wrapper = await mountView()
    await expandFirstRow(wrapper)

    const row = wrapper.findAll('[data-testid="auth-record-row"]').find((r) =>
      r.text().includes('api.openai.com'),
    )!
    const removeBtn = row.findAll('button').find((b) => b.text().includes('移除拒绝'))!
    expect(removeBtn.text()).not.toContain('取消授权')
    await removeBtn.trigger('click')
    await flushPromises()

    expect(removeCalls).toHaveLength(1)
    expect(removeCalls[0]).toMatchObject({
      pluginId: 'com.bedcode.test',
      resource: 'network',
      target: 'https://api.openai.com:443',
    })
    expect(revokeCalls).toHaveLength(0)
    wrapper.unmount()
  })

  it('网络分区无记录时显示网络专属空态（不串用文件那一句文案）', async () => {
    overviewResult = [appWithNetwork([])]

    const wrapper = await mountView()
    await expandFirstRow(wrapper)

    const text = compact(wrapper.text())
    expect(text).toContain(compact('该应用还没有网络授权记录'))
    expect(text).toContain(compact('该应用还没有文件授权记录'))
    expect(wrapper.findAll('[data-testid="auth-record-row"]')).toHaveLength(0)
    wrapper.unmount()
  })
})
