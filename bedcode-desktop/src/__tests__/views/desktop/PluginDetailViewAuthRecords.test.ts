/**
 * PluginDetailView 授权记录区块渲染契约（票 07）
 *
 * 被测行为（外部可见输出）：
 * - 权限区块标题改为「申请的权限」（manifest 静态声明位），授权记录区块并列展示
 * - 授权记录四个分区：用户已授权 / 免询问自动放行（带「未经确认」标记）/ 内置免询问 / 硬拒绝
 * - 读模型命令带应用 id 与宿主凭证（与设置页共用同一命令，spec §9.3）
 * - 撤销 allow 记录调 `plugin_auth_revoke`；移除 deny 调 `plugin_auth_remove_record`
 * - 撤销内置免询问（home 形态）调 `plugin_auth_revoke` 且 target 为 `~/` 前缀
 * - 读模型失败走统一错误消费层（ADR 0030）：友好 toast，页面无原始错误原文
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import i18n from '@/locales'
import PluginDetailView from '@/views/PluginDetailView.vue'
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

/** 详情页依赖的插件信息（plugin_list_loaded 的返回项） */
let pluginList: Array<Record<string, unknown>>
/** 授权读模型（plugin_auth_overview 的返回项） */
let overviewResult: PluginAuthOverview[] | Error

/** 宿主命令的可编程返回 */
let manageError: Error | null

function installInvokeMock(): void {
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === 'plugin_frontend_loader_session') return Promise.resolve('loader-session')
    if (cmd === 'plugin_list_loaded') return Promise.resolve(pluginList)
    if (cmd === 'plugin_auth_overview') {
      if (overviewResult instanceof Error) return Promise.reject(overviewResult)
      return Promise.resolve(overviewResult)
    }
    if (cmd === 'plugin_auth_revoke' || cmd === 'plugin_auth_remove_record') {
      if (manageError) return Promise.reject(manageError)
      return Promise.resolve(1)
    }
    return Promise.resolve(undefined)
  })
}

function countInvokes(cmd: string): number {
  return mockInvoke.mock.calls.filter((call) => call[0] === cmd).length
}

/** 详情页依赖的插件信息（最小可渲染形状） */
function makePlugin(): Record<string, unknown> {
  return {
    id: 'com.bedcode.test',
    name: 'Test App',
    version: '1.0.0',
    description: 'desc',
    author: 'author',
    main: 'index.html',
    pluginType: 'rust-ts',
    permissions: ['fs:read'],
    state: { state: 'Activated' },
    extensionPath: '/path/to/app',
    contributes: { contributes: [] },
    source: 'wasm',
    sizeBytes: 1024,
  }
}

/** 读模型替身（形状与宿主 plugin_auth_overview 输出一致） */
function app(overrides: Partial<PluginAuthOverview> & { pluginId: string }): PluginAuthOverview {
  return {
    name: overrides.pluginId,
    strategies: [
      { resource: 'fs', strategy: 'default' },
      { resource: 'network', strategy: 'default' },
    ],
    records: [],
    firstPartyDirs: [],
    ...overrides,
  }
}

/** 一条授权记录 */
function record(overrides: Partial<AuthRecord> & { id: number }): AuthRecord {
  return {
    pluginId: 'com.bedcode.test',
    resource: 'fs',
    target: '/home/u/data',
    effect: 'allow',
    ops: ['read'],
    prefixMatch: false,
    source: 'user',
    createdAt: 1,
    ...overrides,
  }
}

async function mountView(): Promise<VueWrapper> {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/plugins', name: 'plugins', component: { template: '<div />' } },
      {
        path: '/plugins/:id',
        name: 'plugin-detail',
        component: PluginDetailView,
      },
    ],
  })
  await router.push({ name: 'plugin-detail', params: { id: 'com.bedcode.test' } })
  await router.isReady()
  const wrapper = mount(PluginDetailView, {
    global: { plugins: [createPinia(), i18n, router] },
  })
  await flushPromises()
  await flushPromises()
  return wrapper
}

/** 去掉全部空白：元素文本串联没有分隔符 */
function compact(s: string): string {
  return s.replace(/\s+/g, '')
}

/** 展开「授权记录」折叠区（区内唯一的折叠按钮；「申请的权限」在其前） */
async function openAuthRecords(wrapper: VueWrapper): Promise<void> {
  const buttons = wrapper.findAll('button')
  const authToggle = buttons.find((b) => b.text().includes('授权记录'))
  expect(authToggle).toBeTruthy()
  await authToggle!.trigger('click')
}

describe('PluginDetailView 授权记录区块（票 07）', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockInvoke.mockReset()
    mockedToast.error.mockClear()
    mockedToast.success.mockClear()
    pluginList = [makePlugin()]
    overviewResult = [app({ pluginId: 'com.bedcode.test', name: 'Test App' })]
    manageError = null
    installInvokeMock()
  })

  it('权限区块标题为「申请的权限」，授权记录区块与之并列', async () => {
    const wrapper = await mountView()

    const text = wrapper.text()
    // 「申请的权限」出现在折叠区（权限区块标题），不再是「权限」
    expect(text).toContain('申请的权限')
    expect(text).toContain('授权记录')
  })

  it('四个分区渲染：用户已授权 / 免询问自动放行 / 内置免询问 / 硬拒绝', async () => {
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        records: [
          record({ id: 1, target: '/home/u/data', ops: ['read'], source: 'user' }),
          record({
            id: 2,
            target: '/home/u/auto',
            ops: ['read', 'write'],
            source: 'always_allow',
          }),
          record({ id: 3, target: '/home/u/secret', effect: 'deny', source: 'user_deny' }),
          record({
            id: 4,
            resource: 'network',
            target: 'https://api.github.com:443',
            ops: [],
            source: 'user',
          }),
        ],
        firstPartyDirs: [
          { pluginId: 'com.bedcode.test', kind: 'home', value: '.agents' },
          { pluginId: 'com.bedcode.test', kind: 'project-segment', value: '.claude' },
        ],
      }),
    ]

    const wrapper = await mountView()
    await openAuthRecords(wrapper)

    const text = compact(wrapper.text())
    expect(text).toContain('用户已授权')
    expect(text).toContain('免询问自动放行')
    expect(text).toContain('内置免询问')
    expect(text).toContain('硬拒绝')
    // 四分区内容：文件与网络记录都在对应分区内
    expect(text).toContain(compact('/home/u/data'))
    expect(text).toContain(compact('/home/u/auto'))
    expect(text).toContain(compact('/home/u/secret'))
    expect(text).toContain(compact('https://api.github.com:443'))
    // 内置免询问项：home 形态显示 ~/ 前缀，项目段显示 <project>/ 前缀
    expect(text).toContain(compact('~/.agents'))
    expect(text).toContain(compact('<project>/.claude'))
  })

  it('免询问自动放行记录带「未经确认」标记，用户已授权记录不带', async () => {
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        records: [
          record({ id: 1, target: '/home/u/auto', source: 'always_allow' }),
          record({ id: 2, target: '/home/u/data', source: 'user' }),
        ],
      }),
    ]

    const wrapper = await mountView()
    await openAuthRecords(wrapper)

    const rows = wrapper.findAll('[data-testid="auth-record-row"]')
    expect(rows).toHaveLength(2)
    const autoRow = rows.find((row) => row.text().includes('/home/u/auto'))
    const userRow = rows.find((row) => row.text().includes('/home/u/data'))
    expect(autoRow?.text()).toContain('未经确认')
    expect(userRow?.text()).not.toContain('未经确认')
  })

  it('撤销授权记录调 plugin_auth_revoke（资源 / 目标 / 宿主凭证）并刷新读模型', async () => {
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        records: [record({ id: 1, target: '/home/u/data', ops: ['read'], source: 'user' })],
      }),
    ]

    const wrapper = await mountView()
    await openAuthRecords(wrapper)

    const revokeBtn = wrapper
      .findAll('[data-testid="auth-record-row"] button')
      .find((b) => b.text().includes('取消授权'))
    expect(revokeBtn).toBeTruthy()
    await revokeBtn!.trigger('click')
    await flushPromises()

    const call = mockInvoke.mock.calls.find((c) => c[0] === 'plugin_auth_revoke')
    expect(call?.[1]).toEqual({
      pluginId: 'com.bedcode.test',
      resource: 'fs',
      target: '/home/u/data',
      credential: 'loader-session',
    })
    expect(mockedToast.success).toHaveBeenCalledTimes(1)
  })

  it('移除 deny 记录调 plugin_auth_remove_record 且不触发 revoke', async () => {
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        records: [
          record({ id: 9, target: '/home/u/secret', effect: 'deny', ops: [], source: 'user_deny' }),
        ],
      }),
    ]

    const wrapper = await mountView()
    await openAuthRecords(wrapper)

    const removeBtn = wrapper
      .findAll('[data-testid="auth-record-row"] button')
      .find((b) => b.text().includes('移除拒绝'))
    expect(removeBtn).toBeTruthy()
    await removeBtn!.trigger('click')
    await flushPromises()

    const call = mockInvoke.mock.calls.find((c) => c[0] === 'plugin_auth_remove_record')
    expect(call?.[1]).toEqual({
      pluginId: 'com.bedcode.test',
      resource: 'fs',
      target: '/home/u/secret',
      credential: 'loader-session',
    })
    expect(countInvokes('plugin_auth_revoke')).toBe(0)
  })

  it('撤销内置免询问（home 形态）调 plugin_auth_revoke 且 target 为 ~/ 前缀', async () => {
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        firstPartyDirs: [{ pluginId: 'com.bedcode.test', kind: 'home', value: '.agents' }],
      }),
    ]

    const wrapper = await mountView()
    await openAuthRecords(wrapper)

    const firstPartyRevoke = wrapper
      .findAll('[data-testid="first-party-row"] button')
      .find((b) => b.text().includes('取消授权'))
    expect(firstPartyRevoke).toBeTruthy()
    await firstPartyRevoke!.trigger('click')
    await flushPromises()

    const call = mockInvoke.mock.calls.find((c) => c[0] === 'plugin_auth_revoke')
    expect(call?.[1]).toEqual({
      pluginId: 'com.bedcode.test',
      resource: 'fs',
      target: '~/.agents',
      credential: 'loader-session',
    })
  })

  it('读模型失败走统一错误消费层：友好 toast，页面无原始错误原文', async () => {
    overviewResult = new Error('sqlite: no such table plugin_auth_records')

    const wrapper = await mountView()
    await openAuthRecords(wrapper)

    expect(mockedToast.error).toHaveBeenCalledTimes(1)
    const message = String(mockedToast.error.mock.calls[0][0])
    expect(message).toBe(i18n.global.t('errors.host.internal'))
    expect(message).not.toContain('sqlite')
    expect(wrapper.text()).not.toContain('plugin_auth_records')
  })

  it('读模型命令带应用 id 与宿主凭证，且只拉一次（首挂载）', async () => {
    await mountView()

    expect(countInvokes('plugin_auth_overview')).toBe(1)
    const call = mockInvoke.mock.calls.find((c) => c[0] === 'plugin_auth_overview')
    expect(call?.[1]).toEqual({
      pluginId: 'com.bedcode.test',
      credential: 'loader-session',
    })
  })
})
