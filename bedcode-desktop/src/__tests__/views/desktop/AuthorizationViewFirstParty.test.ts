/**
 * 内置免询问（第一方）项的两处展示与撤销（授权策略增强 · 票 08）
 *
 * spec §7 裁定：第一方免询问层的优先级高于策略档位，**前提是它必须可见**——
 * 不显式化，它在新管理界面里就是一张「用户看不见的特权」：看 agent-hub 的目录清单
 * 会以为它只能碰自己授权过的地方。
 *
 * 被测行为（外部可见输出）：
 * - 设置页展开行出现「内置免询问」分区，列出 home / project-segment 两种形态；
 * - home 形态可撤销 ⇒ 调 `plugin_auth_revoke` 且 **target 是 `~/` 前缀形态**
 *   （前端拿不到 `$HOME`，传绝对路径或裸值都落不成宿主要的形状）；
 * - project-segment 形态**不渲染撤销按钮**（项目根由用户每次选，落不成可复用记录）；
 * - 无第一方项时显示空态（是事实，不是「没数据」）；
 * - 撤销走统一错误消费层：友好 toast、页面不渲染原始错误。
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createRouter, createMemoryHistory } from 'vue-router'
import i18n from '@/locales'
import AuthorizationView from '@/views/AuthorizationView.vue'
import { toast } from 'vue-sonner'
import type { FirstPartyDirEntry, PluginAuthOverview } from '@/utils/authPolicy'

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

/** 第一方免询问项（形态与宿主 `fs_auth::FirstPartyDirEntry` 的 wire 一致） */
function firstParty(kind: string, value: string): FirstPartyDirEntry {
  return { pluginId: 'com.bedcode.test', kind, value }
}

/** 读模型替身：一个应用 + 若干第一方项 */
function appWith(firstPartyDirs: FirstPartyDirEntry[]): PluginAuthOverview {
  return {
    pluginId: 'com.bedcode.test',
    name: 'Test App',
    strategies: [
      { resource: 'fs', strategy: 'default' },
      { resource: 'network', strategy: 'default' },
    ],
    records: [],
    firstPartyDirs,
  }
}

let overviewResult: PluginAuthOverview[] | Error
let revokeCalls: Record<string, unknown>[]
let revokeError: Error | null

function installInvokeMock(): void {
  mockInvoke.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    if (cmd === 'plugin_frontend_loader_session') return Promise.resolve('loader-session')
    if (cmd === 'plugin_auth_overview') {
      if (overviewResult instanceof Error) return Promise.reject(overviewResult)
      return Promise.resolve(overviewResult)
    }
    if (cmd === 'plugin_auth_revoke') {
      revokeCalls.push(args)
      return revokeError ? Promise.reject(revokeError) : Promise.resolve(1)
    }
    return Promise.resolve(undefined)
  })
}

async function mountExpanded(): Promise<VueWrapper> {
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
  await wrapper.find('[data-testid="auth-app-row"] button[aria-expanded]').trigger('click')
  await flushPromises()
  return wrapper
}

/** 去掉全部空白（元素文本串联没有分隔符） */
function compact(s: string): string {
  return s.replace(/\s+/g, '')
}

describe('AuthorizationView 内置免询问分区（票 08）', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockInvoke.mockReset()
    mockedToast.success.mockClear()
    mockedToast.error.mockClear()
    revokeCalls = []
    revokeError = null
    overviewResult = [appWith([firstParty('home', '.agents'), firstParty('project-segment', '.claude')])]
    installInvokeMock()
  })

  it('展开后列出全部第一方项（~/ 前缀与 <project>/ 段两种展示形态）', async () => {
    const wrapper = await mountExpanded()
    const section = wrapper.find('[data-testid="auth-first-party-section"]')
    expect(section.exists(), '设置页必须展示内置免询问分区').toBe(true)
    expect(compact(section.text())).toContain(
      compact(i18n.global.t('settings.authorization.sections.firstParty')),
    )

    const rows = section.findAll('[data-testid="first-party-row"]')
    expect(rows).toHaveLength(2)
    const texts = rows.map((row) => compact(row.text()))
    expect(texts[0]).toContain(compact('~/.agents'))
    expect(texts[0]).toContain(compact(i18n.global.t('settings.authorization.sections.firstPartyHome')))
    expect(texts[1]).toContain(compact('<project>/.claude'))
    expect(texts[1]).toContain(
      compact(i18n.global.t('settings.authorization.sections.firstPartySegment')),
    )
  })

  it('home 形态可撤销：调 plugin_auth_revoke 且 target 是 ~/ 前缀形态', async () => {
    const wrapper = await mountExpanded()
    const rows = wrapper.findAll('[data-testid="first-party-row"]')
    await rows[0].find('[data-testid="first-party-revoke"]').trigger('click')
    await flushPromises()

    expect(revokeCalls).toHaveLength(1)
    expect(revokeCalls[0]).toEqual({
      pluginId: 'com.bedcode.test',
      resource: 'fs',
      // 前端拿不到 $HOME：必须传 ~/ 前缀让宿主展开（传字面量会静默撤销无效）
      target: '~/.agents',
      credential: 'loader-session',
    })
    expect(mockedToast.success).toHaveBeenCalledTimes(1)
  })

  it('project-segment 形态不渲染撤销按钮（项目根每次由用户选，落不成可复用记录）', async () => {
    const wrapper = await mountExpanded()
    const rows = wrapper.findAll('[data-testid="first-party-row"]')
    expect(
      rows[1].find('[data-testid="first-party-revoke"]').exists(),
      '段名形态不可撤销：没有可落账的具体目标',
    ).toBe(false)
  })

  it('没有第一方项时显示空态（是事实，不是「没数据」）', async () => {
    overviewResult = [appWith([])]
    const wrapper = await mountExpanded()
    const section = wrapper.find('[data-testid="auth-first-party-section"]')
    expect(compact(section.text())).toContain(
      compact(i18n.global.t('settings.authorization.sections.firstPartyEmpty')),
    )
    expect(section.findAll('[data-testid="first-party-row"]')).toHaveLength(0)
  })

  it('撤销失败走统一错误消费层：友好 toast，页面不渲染原始错误', async () => {
    revokeError = new Error('sqlite: database is locked')
    const wrapper = await mountExpanded()
    const row = wrapper.findAll('[data-testid="first-party-row"]')[0]
    await row.find('[data-testid="first-party-revoke"]').trigger('click')
    await flushPromises()

    expect(mockedToast.error).toHaveBeenCalledTimes(1)
    const message = String(mockedToast.error.mock.calls[0][0])
    expect(message).toBe(i18n.global.t('errors.host.internal'))
    expect(wrapper.text()).not.toContain('database is locked')
    expect(mockedToast.success).not.toHaveBeenCalled()
  })
})
