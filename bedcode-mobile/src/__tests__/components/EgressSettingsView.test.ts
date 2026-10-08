/**
 * EgressSettingsView 组件测试（票 20：egress 三档策略设置页 + 授权记录管理）
 *
 * 行为契约（来源 = EgressSettingsView.vue + egress.rs 命令面）：
 * C-001  load 成功：egress_list_records → 记录渲染（target/path/plugin/来源徽章）
 * C-002  load 成功：记录 pluginId 去重 → 每插件 egress_get_strategy → 三档按钮 + 当前档高亮
 * C-003  插件展示名剥离 `plugin:` 前缀；宿主记录显示 host
 * C-004  空记录 → 空态文案（不渲染策略组）
 * C-005  loading 中 → loading 文案，不渲染策略组
 * C-006  load 失败 → logger.error + 空列表 + loading=false（不抛）
 * C-007  setStrategy 成功 → invoke(egress_set_strategy) + 按钮高亮切换
 * C-008  setStrategy 失败 → logger.error + 档位保持不变
 * C-009  撤销全部：确认 → egress_revoke_grants + 记录清空
 * C-010  单条撤销：确认 → egress_revoke_record(host, pluginId) + 仅移除该条
 * C-011  同 host 多来源记录各占一行（recordKey 含 source）
 * C-012  路径粒度：非 "/" 前缀显示路径；"/" 显示「全部路径」
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { nextTick } from 'vue'

// ==================== mock Tauri 边界 ====================

const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

// i18n：最小消息表（settings.egress.* + common.button.*）+ {param} 插值
const messages: Record<string, string> = {
  'settings.egress.title': '网络访问授权',
  'settings.egress.strategySection': '访问策略',
  'settings.egress.strategyHint': '决定插件请求访问未经授权的地址时，是否询问你。',
  'settings.egress.loading': '加载中…',
  'settings.egress.empty': '暂无授权记录',
  'settings.egress.alwaysAsk': '总是询问',
  'settings.egress.alwaysAskDesc': '每次访问都询问（跳过已记忆的授权）',
  'settings.egress.defaultStrategy': '默认',
  'settings.egress.defaultStrategyDesc': '已记忆的地址直接放行，其余询问',
  'settings.egress.alwaysAllow': '始终允许',
  'settings.egress.alwaysAllowDesc': '不询问直接放行（授权记录标记为「未经确认」）',
  'settings.egress.recordsSection': '授权记录',
  'settings.egress.allPaths': '全部路径',
  'settings.egress.sourceUser': '已确认',
  'settings.egress.sourceAlwaysAllow': '未经确认',
  'settings.egress.sourceUserDeny': '已拒绝',
  'settings.egress.revokeOne': '撤销',
  'settings.egress.revokeAll': '撤销全部授权',
  'settings.egress.revokeHint': '撤销后，之前放行的外部地址需要重新授权。',
  'settings.egress.revokeConfirmTitle': '撤销全部授权？',
  'settings.egress.revokeConfirmMessage': '此操作将清除所有已记忆的外部地址授权（含「不再询问」记录）。',
  'settings.egress.revokeOneConfirmTitle': '撤销此条授权？',
  'settings.egress.revokeOneConfirmMessage': '撤销后，此地址需要重新授权才能访问。',
  'common.button.confirm': '确认',
  'common.button.cancel': '取消',
}
const tFn = (key: string, params?: Record<string, unknown>) => {
  let s = messages[key] ?? key
  if (params) {
    for (const [k, v] of Object.entries(params)) s = s.replaceAll(`{${k}}`, String(v))
  }
  return s
}
vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: tFn }),
}))

vi.mock('@/utils/frontendLogger', () => ({
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), log: vi.fn() },
}))

import EgressSettingsView from '@/views/settings/EgressSettingsView.vue'
import { logger } from '@/utils/frontendLogger'

// ==================== stub 子组件 ====================

const SettingsSubPageStub = { template: '<div class="subpage-stub"><slot /></div>' }
/** ConfirmDialog stub：透传 props/emit，测试经 $emit('confirm') 驱动确认流 */
const ConfirmDialogStub = {
  name: 'ConfirmDialog',
  props: { modelValue: Boolean },
  emits: ['update:modelValue', 'confirm', 'cancel'],
  template: '<div class="confirm-stub" />',
}

// ==================== 工具 ====================

/** Rust egress.rs AuthRecord 形状（`#[serde(rename_all = "camelCase")]`，mock 按 wire 真实形状） */
function makeRecord(overrides: Partial<{
  pluginId: string
  effect: string
  target: string
  pathPrefix: string | null
  source: string
  createdAt: number
}> = {}) {
  return {
    pluginId: 'plugin:com.bedcode.ai-chatbox',
    effect: 'allow',
    target: 'api.openai.com',
    pathPrefix: null,
    source: 'user',
    createdAt: 1_700_000_000,
    ...overrides,
  }
}

/** 默认 mock 面：1 条记录 + 1 个插件档位 */
function mockLoad(records = [makeRecord()], strategies: Record<string, string> = {}) {
  mockInvoke.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    if (cmd === 'egress_list_records') return Promise.resolve(records)
    if (cmd === 'egress_get_strategy') {
      const pluginId = (args as { pluginId: string })?.pluginId ?? ''
      return Promise.resolve(strategies[pluginId] ?? 'default')
    }
    return Promise.resolve(undefined)
  })
}

function mountView() {
  return mount(EgressSettingsView, {
    global: {
      stubs: {
        SettingsSubPage: SettingsSubPageStub,
        ConfirmDialog: ConfirmDialogStub,
      },
      mocks: { $t: tFn },
    },
  })
}

/** 找到指定文案的按钮 */
function findButton(wrapper: ReturnType<typeof mountView>, text: string) {
  const btn = wrapper.findAll('button').find((b) => b.text()?.includes(text))
  return btn
}

/** 策略组内指定档位按钮（高亮态 class 判定） */
function tierButton(wrapper: ReturnType<typeof mountView>, tierText: string) {
  return wrapper.findAll('button').find((b) => b.text()?.trim() === tierText)
}

describe('EgressSettingsView', () => {
  let wrapper: ReturnType<typeof mountView>

  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    wrapper?.unmount()
  })

  it('C-001 load 成功渲染授权记录：target + 路径 + 插件 + 来源徽章文案', async () => {
    mockLoad([
      makeRecord({
        pluginId: 'plugin:com.bedcode.ai-chatbox',
        target: 'api.openai.com',
        pathPrefix: '/v1',
        source: 'user',
      }),
    ])
    wrapper = mountView()
    await flushPromises()

    const text = wrapper.text()
    expect(text).toContain('api.openai.com')
    expect(text).toContain('/v1')
    expect(text).toContain('com.bedcode.ai-chatbox') // plugin: 前缀剥离
    expect(text).toContain('已确认') // source=user → sourceUser
    expect(mockInvoke).toHaveBeenCalledWith('egress_list_records')
  })

  it('C-002 记录 pluginId 去重拉取档位，三档按钮渲染且当前档高亮', async () => {
    mockLoad(
      [
        makeRecord({ target: 'a.example.com' }),
        makeRecord({ target: 'b.example.com' }),
        makeRecord({ pluginId: 'plugin:com.bedcode.file-transfer', target: 'c.example.com' }),
      ],
      { 'plugin:com.bedcode.ai-chatbox': 'always_allow' },
    )
    wrapper = mountView()
    await flushPromises()

    // 每插件一次 get_strategy（去重后两个插件各一次）
    expect(mockInvoke).toHaveBeenCalledWith('egress_get_strategy', { pluginId: 'plugin:com.bedcode.ai-chatbox' })
    expect(mockInvoke).toHaveBeenCalledWith('egress_get_strategy', { pluginId: 'plugin:com.bedcode.file-transfer' })

    // 三档按钮在场
    for (const tier of ['总是询问', '默认', '始终允许']) {
      expect(tierButton(wrapper, tier)).toBeTruthy()
    }
    // 当前档（always_allow）按钮带高亮 class，其余不带
    const allowBtn = tierButton(wrapper, '始终允许')!
    expect(allowBtn.classes()).toContain('bg-[var(--mobile-accent)]')
    const askBtn = tierButton(wrapper, '总是询问')!
    expect(askBtn.classes()).not.toContain('bg-[var(--mobile-accent)]')
    // 档位描述文案（始终允许描述）
    expect(wrapper.text()).toContain('不询问直接放行（授权记录标记为「未经确认」）')
  })

  it('C-003 宿主全局记录显示 host（非 plugin: 前缀原样展示）', async () => {
    mockLoad([makeRecord({ pluginId: 'host', target: 'github.com' })])
    wrapper = mountView()
    await flushPromises()
    expect(wrapper.text()).toContain('github.com')
    expect(wrapper.text()).toContain('host')
  })

  it('C-004 空记录 → 空态文案，不渲染策略组', async () => {
    mockLoad([])
    wrapper = mountView()
    await flushPromises()
    expect(wrapper.text()).toContain('暂无授权记录')
    // 无插件 → 策略组空态（不是三档按钮）
    expect(tierButton(wrapper, '始终允许')).toBeUndefined()
    expect(wrapper.text()).toContain('暂无授权记录')
  })

  it('C-005 loading 中显示 loading 文案，不渲染策略组与记录', async () => {
    mockLoad([makeRecord()])
    // 挂载瞬间（invoke 未 resolve）即 loading
    let resolveRecords!: (v: unknown[]) => void
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'egress_list_records') return new Promise((r) => { resolveRecords = r })
      return Promise.resolve('default')
    })
    wrapper = mountView()
    expect(wrapper.text()).toContain('加载中…')
    expect(tierButton(wrapper, '始终允许')).toBeUndefined()
    resolveRecords([makeRecord()])
    await flushPromises()
    expect(wrapper.text()).not.toContain('加载中…')
  })

  it('C-006 load 失败 → logger.error + 空列表 + loading=false（不抛）', async () => {
    mockInvoke.mockRejectedValueOnce(new Error('egress_list_records failed'))
    wrapper = mountView()
    await flushPromises()
    expect(logger.error).toHaveBeenCalled()
    expect(wrapper.text()).toContain('暂无授权记录')
    expect(wrapper.text()).not.toContain('加载中…')
  })

  it('C-007 setStrategy 成功 → invoke(egress_set_strategy) + 按钮高亮切换', async () => {
    mockLoad([makeRecord()], { 'plugin:com.bedcode.ai-chatbox': 'default' })
    wrapper = mountView()
    await flushPromises()

    const allowBtn = tierButton(wrapper, '始终允许')!
    expect(allowBtn.classes()).not.toContain('bg-[var(--mobile-accent)]')
    await allowBtn.trigger('click')
    await flushPromises()

    expect(mockInvoke).toHaveBeenCalledWith('egress_set_strategy', {
      pluginId: 'plugin:com.bedcode.ai-chatbox',
      strategy: 'always_allow',
    })
    // 高亮切换 + 描述文案更新
    expect(tierButton(wrapper, '始终允许')!.classes()).toContain('bg-[var(--mobile-accent)]')
    expect(tierButton(wrapper, '默认')!.classes()).not.toContain('bg-[var(--mobile-accent)]')
    expect(wrapper.text()).toContain('不询问直接放行（授权记录标记为「未经确认」）')
  })

  it('C-008 setStrategy 失败 → logger.error + 档位保持不变', async () => {
    mockLoad([makeRecord()], { 'plugin:com.bedcode.ai-chatbox': 'default' })
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'egress_list_records') return Promise.resolve([makeRecord()])
      if (cmd === 'egress_get_strategy') return Promise.resolve('default')
      if (cmd === 'egress_set_strategy') return Promise.reject(new Error('invalid strategy'))
      return Promise.resolve(undefined)
    })
    wrapper = mountView()
    await flushPromises()

    await tierButton(wrapper, '始终允许')!.trigger('click')
    await flushPromises()

    expect(logger.error).toHaveBeenCalled()
    // 档位保持默认（未切换高亮）
    expect(tierButton(wrapper, '默认')!.classes()).toContain('bg-[var(--mobile-accent)]')
    expect(tierButton(wrapper, '始终允许')!.classes()).not.toContain('bg-[var(--mobile-accent)]')
  })

  it('C-009 撤销全部：确认 → egress_revoke_grants + 记录清空', async () => {
    mockLoad([makeRecord()])
    wrapper = mountView()
    await flushPromises()

    await findButton(wrapper, '撤销全部授权')!.trigger('click')
    await nextTick()
    const dialog = wrapper.findComponent(ConfirmDialogStub)
    expect(dialog.exists()).toBe(true)

    dialog.vm.$emit('confirm')
    await flushPromises()

    expect(mockInvoke).toHaveBeenCalledWith('egress_revoke_grants')
    expect(wrapper.text()).toContain('暂无授权记录')
  })

  it('C-010 单条撤销：确认 → egress_revoke_record(host, pluginId) + 仅移除该条', async () => {
    mockLoad([
      makeRecord({ target: 'a.example.com' }),
      makeRecord({ pluginId: 'plugin:com.bedcode.file-transfer', target: 'b.example.com' }),
    ])
    wrapper = mountView()
    await flushPromises()

    // 目标：第一条记录的「撤销」按钮（两条记录各有一个）
    const revokeBtns = wrapper.findAll('button').filter((b) => b.text()?.trim() === '撤销')
    expect(revokeBtns.length).toBe(2)
    await revokeBtns[0].trigger('click')
    await nextTick()
    // 两个 ConfirmDialog（撤销全部 + 单条）——定位第二个（单条撤销）
    wrapper.findAllComponents(ConfirmDialogStub)[1].vm.$emit('confirm')
    await flushPromises()

    expect(mockInvoke).toHaveBeenCalledWith('egress_revoke_record', {
      host: 'a.example.com',
      pluginId: 'plugin:com.bedcode.ai-chatbox',
    })
    // 仅移除该条，另一条保留
    expect(wrapper.text()).not.toContain('a.example.com')
    expect(wrapper.text()).toContain('b.example.com')
  })

  it('C-011 同 host 多来源记录各占一行（recordKey 含 source）', async () => {
    mockLoad([
      makeRecord({ target: 'api.openai.com', source: 'user' }),
      makeRecord({ target: 'api.openai.com', source: 'always_allow' }),
    ])
    wrapper = mountView()
    await flushPromises()

    const text = wrapper.text()
    // 两种来源徽章文案各自渲染（同 target 两行）
    expect(text).toContain('已确认')
    expect(text).toContain('未经确认')
    const revokeBtns = wrapper.findAll('button').filter((b) => b.text()?.trim() === '撤销')
    expect(revokeBtns.length).toBe(2)
  })

  it('C-012 路径粒度：非 "/" 前缀显示路径；"/" 显示「全部路径」', async () => {
    mockLoad([
      makeRecord({ target: 'a.example.com', pathPrefix: '/v1' }),
      makeRecord({ target: 'b.example.com', pathPrefix: '/' }),
    ])
    wrapper = mountView()
    await flushPromises()

    const text = wrapper.text()
    expect(text).toContain('/v1')
    expect(text).toContain('全部路径')
  })
})
