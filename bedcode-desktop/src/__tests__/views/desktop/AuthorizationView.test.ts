/**
 * AuthorizationView 渲染契约（设置页「应用授权」二级页 · 票 01）
 *
 * 被测行为（外部可见输出）：
 * - 按风险排序列出全部已安装 wasm 应用（始终允许置顶，spec §9.1）——顺序断言用
 *   与字母序**不同**的名字，顺带杀死「没排序」「按名称排序」两个变异
 * - 每类资源分别展示档位徽标 + 记录条数（allow / deny 都计数）
 * - 空读模型渲染空态而不是空白页
 * - 读模型失败走统一错误消费层（ADR 0030）：友好 toast、页面无原始错误原文
 * - 宿主面凭证绑定：读模型命令带宿主凭证（授权记录不得被插件面枚举）
 * - 首次挂载只拉一次（KeepAlive 的 onActivated 不得造成双拉）
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

/** 读模型替身（形状与宿主 `plugin_auth_overview` 输出一致） */
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

/** 后端可编程返回（每个用例重设） */
let overviewResult: PluginAuthOverview[] | Error

/** 管理命令（撤销 / 移除 deny）的可编程返回：默认成功返回 1 条 */
let manageError: Error | null

function installInvokeMock(): void {
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === 'plugin_frontend_loader_session') return Promise.resolve('loader-session')
    if (cmd === 'plugin_auth_overview') {
      if (overviewResult instanceof Error) return Promise.reject(overviewResult)
      return Promise.resolve(overviewResult)
    }
    if (cmd === 'plugin_auth_revoke' || cmd === 'plugin_auth_remove_record') {
      if (manageError) return Promise.reject(manageError)
      return Promise.resolve(1)
    }
    if (cmd === 'plugin_auth_set_strategy') {
      if (manageError) return Promise.reject(manageError)
      return Promise.resolve(undefined)
    }
    return Promise.resolve(undefined)
  })
}

function countInvokes(cmd: string): number {
  return mockInvoke.mock.calls.filter((call) => call[0] === cmd).length
}

async function mountView(): Promise<VueWrapper> {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/settings', name: 'settings', component: { template: '<div />' } },
      {
        path: '/settings/authorization',
        name: 'settings-authorization',
        component: AuthorizationView,
      },
    ],
  })
  // 先落地到本页再挂载：memory history 的初始位置为空，会告警「No match found」
  await router.push({ name: 'settings-authorization' })
  await router.isReady()
  const wrapper = mount(AuthorizationView, {
    global: { plugins: [createPinia(), i18n, router] },
  })
  await flushPromises()
  return wrapper
}

/** 去掉全部空白：元素文本串联没有分隔符，断言「资源 → 档位 → 条数」时对两侧同样压缩 */
function compact(s: string): string {
  return s.replace(/\s+/g, '')
}

/** 页面可见文本（压缩空白后用于顺序断言） */
function pageText(wrapper: VueWrapper): string {
  return compact(wrapper.text())
}

function rowTexts(wrapper: VueWrapper): string[] {
  return wrapper.findAll('[data-testid="auth-app-row"]').map((row) => compact(row.text()))
}

/** 按应用名定位其行文本（压缩空白） */
function rowOf(wrapper: VueWrapper, name: string): string {
  const row = wrapper
    .findAll('[data-testid="auth-app-row"]')
    .find((candidate) => candidate.text().includes(name))
  return row ? compact(row.text()) : ''
}

describe('AuthorizationView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockInvoke.mockReset()
    mockedToast.error.mockClear()
    mockedToast.success.mockClear()
    overviewResult = []
    manageError = null
    installInvokeMock()
  })

  it('按风险排序渲染应用：始终允许置顶，默认次之，总是询问排末', async () => {
    overviewResult = [
      // 名称字母序：Alpha < Mid < Zeta；风险序要求 Zeta 在前、Mid 在后
      app({
        pluginId: 'com.bedcode.alpha',
        name: 'Alpha Terminal',
        strategies: [
          { resource: 'fs', strategy: 'default' },
          { resource: 'network', strategy: 'default' },
        ],
      }),
      app({
        pluginId: 'com.bedcode.zeta',
        name: 'Zeta Agent',
        strategies: [
          { resource: 'fs', strategy: 'default' },
          { resource: 'network', strategy: 'always_allow' },
        ],
      }),
      app({
        pluginId: 'com.bedcode.mid',
        name: 'Mid Chat',
        strategies: [
          { resource: 'fs', strategy: 'always_ask' },
          { resource: 'network', strategy: 'always_ask' },
        ],
      }),
    ]

    const wrapper = await mountView()
    const rows = rowTexts(wrapper)

    expect(rows).toHaveLength(3)
    const flat = pageText(wrapper)
    const zeta = flat.indexOf('ZetaAgent')
    const alpha = flat.indexOf('AlphaTerminal')
    const mid = flat.indexOf('MidChat')
    expect([zeta, alpha, mid].every((idx) => idx >= 0)).toBe(true)
    expect(zeta).toBeLessThan(alpha)
    expect(alpha).toBeLessThan(mid)
    // 风险序与字母序相反：顺带证明不是「按名称排序」
    expect(zeta).toBeLessThan(mid)
    // 每行都必须带档位徽标与记录条数（不是只渲染应用名）
    expect(rows.every((row) => row.includes('条记录'))).toBe(true)
  })

  it('每类资源分别展示档位与记录条数（allow 与 deny 都计入）', async () => {
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        strategies: [
          { resource: 'fs', strategy: 'always_ask' },
          { resource: 'network', strategy: 'default' },
        ],
        records: [
          {
            id: 1,
            pluginId: 'com.bedcode.test',
            resource: 'fs',
            target: '/home/u/data',
            effect: 'allow',
            ops: ['read'],
            prefixMatch: false,
            source: 'user',
            createdAt: 1,
          },
          {
            id: 2,
            pluginId: 'com.bedcode.test',
            resource: 'fs',
            target: '/home/u/secret',
            effect: 'deny',
            ops: [],
            prefixMatch: false,
            source: 'user_deny',
            createdAt: 2,
          },
          {
            id: 3,
            pluginId: 'com.bedcode.test',
            resource: 'network',
            target: 'https://api.github.com:443',
            effect: 'allow',
            ops: [],
            prefixMatch: false,
            source: 'user',
            createdAt: 3,
          },
        ],
      }),
    ]

    const wrapper = await mountView()
    const row = rowOf(wrapper, 'Test App')

    expect(row).toContain(compact('文件 总是询问 2 条记录'))
    expect(row).toContain(compact('网络 默认 1 条记录'))
  })

  it('空读模型渲染空态（不是空白页，也不报错）', async () => {
    overviewResult = []

    const wrapper = await mountView()

    expect(rowTexts(wrapper)).toEqual([])
    expect(wrapper.text()).toContain('暂无可管理的应用')
    expect(mockedToast.error).not.toHaveBeenCalled()
  })

  it('读模型失败：toast 只出友好文案，页面不渲染原始错误', async () => {
    overviewResult = new Error('sqlite: no such table plugin_auth_records')

    const wrapper = await mountView()

    expect(mockedToast.error).toHaveBeenCalledTimes(1)
    const message = String(mockedToast.error.mock.calls[0][0])
    expect(message).toBe(i18n.global.t('errors.host.internal'))
    expect(message).not.toContain('sqlite')
    expect(rowTexts(wrapper)).toEqual([])
    expect(wrapper.text()).not.toContain('plugin_auth_records')
  })

  it('读模型命令带宿主凭证（授权记录不得被插件面枚举），且首次挂载只拉一次', async () => {
    overviewResult = [app({ pluginId: 'com.bedcode.test', name: 'Test App' })]

    await mountView()

    expect(countInvokes('plugin_auth_overview')).toBe(1)
    const call = mockInvoke.mock.calls.find((c) => c[0] === 'plugin_auth_overview')
    expect(call?.[1]).toEqual({ pluginId: null, credential: 'loader-session' })
  })
})

describe('AuthorizationView 授权记录展开与逐条管理（票 02）', () => {
  function fsRecord(overrides: Partial<AuthRecord> & { id: number }): AuthRecord {
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

  /** 展开行的开关（行内唯一的 aria-expanded 按钮） */
  function toggleOf(wrapper: VueWrapper): ReturnType<VueWrapper['find']> {
    return wrapper.find('[data-testid="auth-app-row"] button[aria-expanded]')
  }

  function recordRows(wrapper: VueWrapper): string[] {
    return wrapper.findAll('[data-testid="auth-record-row"]').map((row) => compact(row.text()))
  }

  beforeEach(() => {
    setActivePinia(createPinia())
    mockInvoke.mockReset()
    mockedToast.error.mockClear()
    mockedToast.success.mockClear()
    manageError = null
    installInvokeMock()
  })

  it('默认收起；展开后列出文件目录记录（目标 + 效果 + 操作集 + 来源）', async () => {
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        records: [
          fsRecord({ id: 1, target: '/home/u/data', ops: ['read', 'write'], source: 'user' }),
          fsRecord({
            id: 2,
            target: '/home/u/secret',
            effect: 'deny',
            ops: [],
            source: 'user_deny',
          }),
        ],
      }),
    ]
    const wrapper = await mountView()

    expect(recordRows(wrapper)).toEqual([])
    expect(toggleOf(wrapper).attributes('aria-expanded')).toBe('false')

    await toggleOf(wrapper).trigger('click')

    expect(toggleOf(wrapper).attributes('aria-expanded')).toBe('true')
    const rows = recordRows(wrapper)
    expect(rows).toHaveLength(2)
    expect(rows[0]).toContain('/home/u/data')
    expect(rows[0]).toContain(compact('已授权 读写 用户确认 取消授权'))
    expect(rows[1]).toContain('/home/u/secret')
    expect(rows[1]).toContain(compact('硬拒绝 用户拒绝 移除拒绝'))
  })

  it('「取消授权」调宿主命令（目标 / 资源 / 宿主凭证）并刷新读模型', async () => {
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        records: [fsRecord({ id: 1, target: '/home/u/data', ops: ['read'] })],
      }),
    ]
    const wrapper = await mountView()
    await toggleOf(wrapper).trigger('click')

    // 撤销后的真源状态：allow 行换成 deny 行（后端 §8.4 的语义）
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        records: [
          fsRecord({ id: 9, target: '/home/u/data', effect: 'deny', ops: [], source: 'user_deny' }),
        ],
      }),
    ]
    const revokeBtn = wrapper
      .findAll('[data-testid="auth-record-row"] button')
      .find((btn) => btn.text().includes('取消授权'))
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
    expect(countInvokes('plugin_auth_overview')).toBe(2)
    expect(mockedToast.success).toHaveBeenCalledTimes(1)
    // 刷新后的可见事实：该目录变成硬拒绝行（可再自行移除）
    expect(recordRows(wrapper)).toEqual([
      compact('/home/u/data 硬拒绝 用户拒绝 移除拒绝'),
    ])
  })

  it('「移除拒绝」调宿主命令且不带 revoke（两种意图走两条命令）', async () => {
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'App',
        records: [
          fsRecord({ id: 9, target: '/home/u/data', effect: 'deny', ops: [], source: 'user_deny' }),
        ],
      }),
    ]
    const wrapper = await mountView()
    await toggleOf(wrapper).trigger('click')

    const removeBtn = wrapper
      .findAll('[data-testid="auth-record-row"] button')
      .find((btn) => btn.text().includes('移除拒绝'))
    await removeBtn!.trigger('click')
    await flushPromises()

    const call = mockInvoke.mock.calls.find((c) => c[0] === 'plugin_auth_remove_record')
    expect(call?.[1]).toEqual({
      pluginId: 'com.bedcode.test',
      resource: 'fs',
      target: '/home/u/data',
      credential: 'loader-session',
    })
    expect(countInvokes('plugin_auth_revoke')).toBe(0)
  })

  it('管理命令失败走统一错误消费层：友好 toast，页面不渲染原始错误', async () => {
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'App',
        records: [fsRecord({ id: 1, target: '/home/u/data' })],
      }),
    ]
    manageError = new Error('sqlite: database is locked')
    const wrapper = await mountView()
    await toggleOf(wrapper).trigger('click')

    const revokeBtn = wrapper
      .findAll('[data-testid="auth-record-row"] button')
      .find((btn) => btn.text().includes('取消授权'))
    await revokeBtn!.trigger('click')
    await flushPromises()

    expect(mockedToast.error).toHaveBeenCalledTimes(1)
    const message = String(mockedToast.error.mock.calls[0][0])
    expect(message).toBe(i18n.global.t('errors.host.internal'))
    expect(message).not.toContain('sqlite')
    expect(wrapper.text()).not.toContain('database is locked')
    // 失败不刷新、不弹成功提示（避免「看起来成功了」）
    expect(countInvokes('plugin_auth_overview')).toBe(1)
    expect(mockedToast.success).not.toHaveBeenCalled()
  })

  it('免询问自动放行记录带「未经确认」标记（票 04 / spec §9.4：与用户确认记录视觉区分）', async () => {
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        records: [
          fsRecord({ id: 1, target: '/home/u/auto', source: 'always_allow' }),
          fsRecord({ id: 2, target: '/home/u/user', source: 'user' }),
        ],
      }),
    ]
    const wrapper = await mountView()
    await toggleOf(wrapper).trigger('click')

    const rows = recordRows(wrapper)
    expect(rows).toHaveLength(2)
    expect(rows[0]).toContain('/home/u/auto')
    expect(rows[0]).toContain(compact('未经确认'))
    expect(rows[0]).toContain(compact('免询问自动放行'))
    expect(rows[1]).toContain('/home/u/user')
    expect(rows[1]).not.toContain('未经确认')
  })
})

// ==================== 策略档位控件（票 03 / 04） ====================

describe('AuthorizationView 策略档位控件（票 03 / 04）', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockInvoke.mockReset()
    mockedToast.error.mockClear()
    mockedToast.success.mockClear()
    manageError = null
    installInvokeMock()
    // 确认弹窗 Teleport 到 body：清掉上一个用例留下的节点，避免跨用例串台
    document.body.innerHTML = ''
  })

  /** 挂在 body 上的二次确认文案（Teleport 后不在 wrapper 里） */
  function confirmText(): string {
    return document.body.querySelector('[data-testid="auth-strategy-confirm"]')?.textContent ?? ''
  }

  function confirmOkButton(): HTMLElement | null {
    return document.body.querySelector('[data-testid="auth-strategy-confirm-ok"]')
  }

  function confirmCancelButton(): HTMLElement | null {
    return document.body.querySelector('[data-testid="auth-strategy-confirm-cancel"]')
  }

  /** 展开首个应用的行（策略控件在展开面板里，与记录清单同级） */
  async function expandFirstRow(wrapper: VueWrapper): Promise<void> {
    await wrapper.find('[data-testid="auth-app-row"] button[aria-expanded]').trigger('click')
  }

  /** 某资源的全部档位按钮（含禁用的「始终允许」） */
  function tiersOf(wrapper: VueWrapper, resource: string) {
    return wrapper.findAll(`[data-testid="auth-strategy-option"][data-resource="${resource}"]`)
  }

  function tierButton(wrapper: VueWrapper, resource: string, tier: string) {
    const button = tiersOf(wrapper, resource).find((b) => b.attributes('data-tier') === tier)
    expect(button, `missing ${resource}/${tier} button`).toBeTruthy()
    return button!
  }

  it('三档都渲染且都可选（票 04 起「始终允许」不再禁用），title 是各自档位的说明', async () => {
    overviewResult = [app({ pluginId: 'com.bedcode.test', name: 'Test App' })]
    const wrapper = await mountView()
    await expandFirstRow(wrapper)

    for (const resource of ['fs', 'network']) {
      expect(tiersOf(wrapper, resource)).toHaveLength(3)
      for (const tier of ['always_ask', 'default', 'always_allow']) {
        expect(
          tierButton(wrapper, resource, tier).attributes('disabled'),
          `${resource}/${tier} 不应禁用（三档都已接线）`,
        ).toBeUndefined()
        expect(tierButton(wrapper, resource, tier).attributes('title')).toBe(
          i18n.global.t(`settings.authorization.strategyControl.hint.${tier}`),
        )
      }
    }
  })

  it('点「总是询问」写档位（应用 / 资源 / 宿主凭证）并刷新读模型', async () => {
    overviewResult = [app({ pluginId: 'com.bedcode.test', name: 'Test App' })]
    const wrapper = await mountView()
    await expandFirstRow(wrapper)

    // 后端真源更新后的读模型：fs 档位变「总是询问」（页面必须跟真源走，不乐观更新）
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        strategies: [
          { resource: 'fs', strategy: 'always_ask' },
          { resource: 'network', strategy: 'default' },
        ],
      }),
    ]
    await tierButton(wrapper, 'fs', 'always_ask').trigger('click')
    await flushPromises()

    expect(
      mockInvoke.mock.calls.find((c) => c[0] === 'plugin_auth_set_strategy')?.[1],
    ).toEqual({
      pluginId: 'com.bedcode.test',
      resource: 'fs',
      strategy: 'always_ask',
      credential: 'loader-session',
    })
    expect(mockedToast.success).toHaveBeenCalledTimes(1)
    expect(countInvokes('plugin_auth_overview')).toBe(2)
    expect(rowOf(wrapper, 'Test App')).toContain(compact('文件 总是询问'))
    // 选中态落在被点的那一档上（否则用户看不出当前档位）
    expect(tierButton(wrapper, 'fs', 'always_ask').classes()).toContain(
      'border-[var(--color-primary)]',
    )
  })

  it('切「始终允许」先弹二次确认：确认前不写库，确认后才发命令并刷新', async () => {
    overviewResult = [app({ pluginId: 'com.bedcode.test', name: 'Test App' })]
    const wrapper = await mountView()
    await expandFirstRow(wrapper)

    await tierButton(wrapper, 'fs', 'always_allow').trigger('click')
    await flushPromises()

    // 二次确认（票 04）：风险最高的一档，写库前必须让用户看到语义边界
    expect(countInvokes('plugin_auth_set_strategy')).toBe(0)
    const text = compact(confirmText())
    expect(text).toContain(compact('Test App'))
    expect(text).toContain('没记录过的目标会直接放行')
    expect(text).toContain('切档不会一次性授予全部权限')

    // 后端真源更新后的读模型：fs 档位变「始终允许」
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        strategies: [
          { resource: 'fs', strategy: 'always_allow' },
          { resource: 'network', strategy: 'default' },
        ],
      }),
    ]
    confirmOkButton()!.click()
    await flushPromises()

    expect(
      mockInvoke.mock.calls.find((c) => c[0] === 'plugin_auth_set_strategy')?.[1],
    ).toEqual({
      pluginId: 'com.bedcode.test',
      resource: 'fs',
      strategy: 'always_allow',
      credential: 'loader-session',
    })
    expect(mockedToast.success).toHaveBeenCalledTimes(1)
    expect(countInvokes('plugin_auth_overview')).toBe(2)
    expect(rowOf(wrapper, 'Test App')).toContain(compact('文件 始终允许'))
    expect(confirmText(), '确认后弹窗必须关闭').toBe('')
  })

  it('二次确认取消：不写库也不刷新（用户还没做决定）', async () => {
    overviewResult = [app({ pluginId: 'com.bedcode.test', name: 'Test App' })]
    const wrapper = await mountView()
    await expandFirstRow(wrapper)

    await tierButton(wrapper, 'fs', 'always_allow').trigger('click')
    await flushPromises()
    confirmCancelButton()!.click()
    await flushPromises()

    expect(countInvokes('plugin_auth_set_strategy')).toBe(0)
    expect(countInvokes('plugin_auth_overview')).toBe(1)
    expect(mockedToast.success).not.toHaveBeenCalled()
    expect(confirmText()).toBe('')
  })

  it('已是「始终允许」时点击：不弹确认也不写库', async () => {
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        strategies: [
          { resource: 'fs', strategy: 'always_allow' },
          { resource: 'network', strategy: 'default' },
        ],
      }),
    ]
    const wrapper = await mountView()
    await expandFirstRow(wrapper)

    await tierButton(wrapper, 'fs', 'always_allow').trigger('click')
    await flushPromises()

    expect(countInvokes('plugin_auth_set_strategy')).toBe(0)
    expect(countInvokes('plugin_auth_overview')).toBe(1)
    expect(confirmText()).toBe('')
  })

  it('已是当前档位时不重复写库（点击不产生任何请求）', async () => {
    overviewResult = [app({ pluginId: 'com.bedcode.test', name: 'Test App' })]
    const wrapper = await mountView()
    await expandFirstRow(wrapper)

    await tierButton(wrapper, 'network', 'default').trigger('click')
    await flushPromises()

    expect(countInvokes('plugin_auth_set_strategy')).toBe(0)
    expect(countInvokes('plugin_auth_overview')).toBe(1)
  })

  it('设置失败走统一错误消费层：友好 toast，不刷新也不弹成功提示', async () => {
    overviewResult = [app({ pluginId: 'com.bedcode.test', name: 'Test App' })]
    const wrapper = await mountView()
    await expandFirstRow(wrapper)

    manageError = new Error('sqlite: database is locked')
    await tierButton(wrapper, 'fs', 'always_ask').trigger('click')
    await flushPromises()

    expect(mockedToast.error).toHaveBeenCalledTimes(1)
    const message = String(mockedToast.error.mock.calls[0][0])
    expect(message).toBe(i18n.global.t('errors.host.internal'))
    expect(message).not.toContain('sqlite')
    expect(wrapper.text()).not.toContain('database is locked')
    expect(countInvokes('plugin_auth_overview')).toBe(1)
    expect(mockedToast.success).not.toHaveBeenCalled()
  })
})

/**
 * 票 06：两类资源的策略档位**独立设置、独立展示**
 *
 * 变异判据：把策略控件改成「一个共享的选中态」（写库时用固定的 resource、
 * 或渲染时两份控件读同一个档位）⇒ 本组两条转红。
 */
describe('AuthorizationView 两类资源的档位互相独立（票 06）', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockInvoke.mockReset()
    mockedToast.error.mockClear()
    mockedToast.success.mockClear()
    manageError = null
    installInvokeMock()
    document.body.innerHTML = ''
  })

  async function mountExpanded(): Promise<VueWrapper> {
    const wrapper = await mountView()
    await wrapper.find('[data-testid="auth-app-row"] button[aria-expanded]').trigger('click')
    return wrapper
  }

  function tierButtons(wrapper: VueWrapper, resource: string) {
    return wrapper.findAll(`[data-testid="auth-strategy-option"][data-resource="${resource}"]`)
  }

  function tierButton(wrapper: VueWrapper, resource: string, tier: string) {
    const button = tierButtons(wrapper, resource).find(
      (b) => b.attributes('data-tier') === tier,
    )
    expect(button, `missing ${resource}/${tier} button`).toBeTruthy()
    return button!
  }

  it('改网络档位只写 network 资源；文件档位的选中态不受影响', async () => {
    overviewResult = [app({ pluginId: 'com.bedcode.test', name: 'Test App' })]
    const wrapper = await mountExpanded()

    // 写库后读模型只把 network 改成 always_ask（fs 仍是 default）
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        strategies: [
          { resource: 'fs', strategy: 'default' },
          { resource: 'network', strategy: 'always_ask' },
        ],
      }),
    ]
    await tierButton(wrapper, 'network', 'always_ask').trigger('click')
    await flushPromises()

    expect(
      mockInvoke.mock.calls.find((c) => c[0] === 'plugin_auth_set_strategy')?.[1],
    ).toEqual({
      pluginId: 'com.bedcode.test',
      resource: 'network',
      strategy: 'always_ask',
      credential: 'loader-session',
    })

    // 选中态：各自落在自己那一档（若两份控件共用一个选中态，fs 也会被点亮）
    expect(tierButton(wrapper, 'network', 'always_ask').classes()).toContain(
      'border-[var(--color-primary)]',
    )
    expect(tierButton(wrapper, 'fs', 'default').classes()).toContain(
      'border-[var(--color-primary)]',
    )
    expect(tierButton(wrapper, 'fs', 'always_ask').classes()).not.toContain(
      'border-[var(--color-primary)]',
    )

    // 徽标：两个资源各说各的档位（行文本含「文件 默认」「网络 总是询问」）
    const row = rowOf(wrapper, 'Test App')
    expect(row).toContain(compact('文件默认'))
    expect(row).toContain(compact('网络总是询问'))
  })

  it('文件已是某档时点网络的同档按钮：只对 network 生效（不误判成 fs 的当前档）', async () => {
    overviewResult = [
      app({
        pluginId: 'com.bedcode.test',
        name: 'Test App',
        strategies: [
          { resource: 'fs', strategy: 'always_ask' },
          { resource: 'network', strategy: 'default' },
        ],
      }),
    ]
    const wrapper = await mountExpanded()

    // 「已是当前档位」的不重复写库规则必须**逐资源**判定：fs 已是 always_ask，
    // 但 network 的 default 仍可点
    await tierButton(wrapper, 'network', 'default').trigger('click')
    await flushPromises()
    expect(countInvokes('plugin_auth_set_strategy')).toBe(0)

    await tierButton(wrapper, 'network', 'always_ask').trigger('click')
    await flushPromises()
    expect(
      mockInvoke.mock.calls.find((c) => c[0] === 'plugin_auth_set_strategy')?.[1],
    ).toMatchObject({ resource: 'network', strategy: 'always_ask' })
  })
})
