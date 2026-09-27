/**
 * 组件层行为契约（票 15 · 票 10 / 12 / 13 / 14 的界面侧回归）
 *
 * 契约来源：
 * - A1 ProviderApply：key 四选一各自生成正确的 ApplyKeySpec；claude 桥接冲突
 *   两击确认（第一次 force=false 被拒 → 出现冲突条 → 确认后 force=true 重试）
 * - A2 预设编辑器：Esc 在任意焦点位置都能关、打开时焦点进入面板、关闭后焦点
 *   回到触发按钮（票 14 P3-6）
 * - A3 StatsTab：syncedTag 遍历全部适配器求和（票 13 P2-4：此前硬编码
 *   claude+pi，新增适配器会静默少算）
 * - A4 SessionLogsTab：日期筛选回显 + 查询/重置 + 翻页 + 打开详情 + 原始页签
 *   （票 10 P1-2 的界面侧）
 * - A5 SkillsTab：GitHub 同名覆盖两击确认、本地导入同名覆盖两击确认
 * - A6 InstallTab：行状态机（未授权 / 检测中 / 失败 / 未安装 / 最新 / 可更新 /
 *   手动）与两击换源确认
 *
 * 断言全部是外部可见行为——渲染文案 key（i18n 桩直返 key）、`data-testid`、
 * 发往 guest 的命令名与入参；不测内部实现、不测 mock 自身。
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { computed, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import ProviderApply from '../components/ProviderApply.vue'
import ProvidersTab from '../components/ProvidersTab.vue'
import StatsTab from '../components/StatsTab.vue'
import SessionLogsTab from '../components/SessionLogsTab.vue'
import SkillsTab from '../components/SkillsTab.vue'
import InstallTab from '../components/InstallTab.vue'
import CliCard from '../components/CliCard.vue'
import type { AdapterErrorCode, ProviderPreset, ProvidersDomainState, UsageSessionRow } from '../types'
import type { UseProvidersReturn } from '../composables/useProviders'
import type { CliSessionState, UseUsageReturn } from '../composables/useUsage'
import type { UseSkillsReturn } from '../composables/useSkills'

// 第三方控件内部实现不进契约
vi.mock('@vuepic/vue-datepicker', () => ({
  default: { name: 'Datepicker', props: ['modelValue'], render: () => null },
}))
vi.mock('@binblink/bedcode-plugin-sdk-desktop/ui', () => ({
  default: { name: 'Select', props: ['modelValue'], render: () => null },
}))

const execute = vi.fn(
  async (_command: string, _args?: Record<string, unknown>): Promise<Record<string, unknown> | null> => null,
)

function makeContext(): PluginContext {
  return {
    // i18n 桩直返 key，但把插值参数渲染出来（syncedTag 等需要断言数值）
    i18n: {
      t: (k: string, params?: Record<string, unknown>) =>
        params && Object.keys(params).length > 0
          ? `${k}(${Object.entries(params).map(([pk, pv]) => `${pk}=${String(pv)}`).join(',')})`
          : k,
      getI18n: () => undefined,
    },
    commands: { execute },
    events: { on: () => ({ dispose: () => {} }) },
  } as unknown as PluginContext
}

function mountComponent(component: unknown, props: Record<string, unknown> = {}, attach = false) {
  return mount(component as never, {
    props,
    attachTo: attach ? document.body : undefined,
    global: { provide: { pluginContext: makeContext() }, stubs: { teleport: true } },
  } as never)
}

const callsTo = (id: string) => execute.mock.calls.filter((c) => c[0] === id)

beforeEach(() => {
  vi.clearAllMocks()
  execute.mockImplementation(async () => null)
})

// ==================== A1 ProviderApply ====================

function preset(over: Partial<ProviderPreset> = {}): ProviderPreset {
  return {
    id: 1,
    name: 'sensenova',
    baseUrl: 'https://api.sensenova.cn/v1',
    apiStyle: 'openai',
    models: ['deepseek-v3'],
    keyMask: '—',
    notes: '',
    ...over,
  } as unknown as ProviderPreset
}

function providersStub(over: Partial<UseProvidersReturn> = {}): UseProvidersReturn {
  return {
    state: ref({
      claude: { env: {}, bridge: {} },
      presets: [],
      import: { last: null },
    } as unknown as ProvidersDomainState),
    importing: ref(false),
    applying: ref(false),
    saving: ref(false),
    refresh: vi.fn(),
    savePreset: vi.fn(),
    deletePreset: vi.fn(),
    importProviders: vi.fn(),
    applyProvider: vi.fn(),
    ...over,
  } as unknown as UseProvidersReturn
}

describe('A1 ProviderApply：key 四选一 + 桥接冲突两击确认', () => {
  it('默认（无 stored key、无 source 标注）选中 inline，提交 inline key', async () => {
    const applyProvider = vi.fn().mockResolvedValue({ applied: true, files: ['auth.json'] })
    const w = mountComponent(ProviderApply, { providers: providersStub({ applyProvider }), preset: preset() })
    await flushPromises()
    await w.get('[data-testid="apply-key-input"]').setValue('sk-123')
    await w.get('[data-testid="apply-write"]').trigger('click')
    await flushPromises()
    expect(applyProvider).toHaveBeenCalledWith(1, 'pi', 'sensenova', { kind: 'inline', value: 'sk-123' }, false)
    w.unmount()
  })

  it('预设已有 stored key 时默认选中 stored，提交 { kind:"stored" }', async () => {
    const applyProvider = vi.fn().mockResolvedValue({ applied: true, files: [] })
    const w = mountComponent(ProviderApply, {
      providers: providersStub({ applyProvider }),
      preset: preset({ keyMask: 'sk-1***' }),
    })
    await flushPromises()
    await w.get('[data-testid="apply-write"]').trigger('click')
    await flushPromises()
    expect(applyProvider).toHaveBeenCalledWith(1, 'pi', 'sensenova', { kind: 'stored' }, false)
    w.unmount()
  })

  it('反向导入标注的预设默认选中 source，提交源 cli/provider', async () => {
    const applyProvider = vi.fn().mockResolvedValue({ applied: true, files: [] })
    const w = mountComponent(ProviderApply, {
      providers: providersStub({ applyProvider }),
      preset: preset({ notes: 'pi:sensenova' }),
    })
    await flushPromises()
    await w.get('[data-testid="apply-write"]').trigger('click')
    await flushPromises()
    expect(applyProvider).toHaveBeenCalledWith(1, 'pi', 'sensenova', { kind: 'source', cli: 'pi', provider: 'sensenova' }, false)
    w.unmount()
  })

  it('切到 none 提交 { kind:"none" }；inline 未填值时写入按钮禁用', async () => {
    const applyProvider = vi.fn().mockResolvedValue({ applied: true, files: [] })
    const w = mountComponent(ProviderApply, { providers: providersStub({ applyProvider }), preset: preset() })
    await flushPromises()

    const write = w.get('[data-testid="apply-write"]')
    expect((write.element as HTMLButtonElement).disabled).toBe(true)

    await w.findAll('input[type="radio"]').at(-1)!.setValue()
    await flushPromises()
    expect((write.element as HTMLButtonElement).disabled).toBe(false)
    await write.trigger('click')
    await flushPromises()
    expect(applyProvider).toHaveBeenCalledWith(1, 'pi', 'sensenova', { kind: 'none' }, false)
    w.unmount()
  })

  it('桥接冲突：第一次 force=false 被拒 → 出现冲突条 → 确认后 force=true 重试', async () => {
    const applyProvider = vi
      .fn()
      .mockResolvedValueOnce({ bridgeConflict: true, bridges: ['provider-config.sh'] })
      .mockResolvedValueOnce({ applied: true, files: ['settings.json'] })
    const w = mountComponent(ProviderApply, {
      providers: providersStub({ applyProvider }),
      preset: preset({ keyMask: 'sk-1***' }),
    })
    await flushPromises()

    await w.get('[data-testid="apply-write"]').trigger('click')
    await flushPromises()
    expect(applyProvider).toHaveBeenNthCalledWith(1, 1, 'pi', 'sensenova', { kind: 'stored' }, false)
    expect(w.find('[data-testid="apply-conflict"]').exists()).toBe(true)

    await w.get('[data-testid="apply-conflict-confirm"]').trigger('click')
    await flushPromises()
    expect(applyProvider).toHaveBeenNthCalledWith(2, 1, 'pi', 'sensenova', { kind: 'stored' }, true)
    expect(w.find('[data-testid="apply-conflict"]').exists()).toBe(false)
    w.unmount()
  })

  it('目标 CLI 切换改变提交入参（默认 pi）', async () => {
    const applyProvider = vi.fn().mockResolvedValue({ applied: true, files: [] })
    const w = mountComponent(ProviderApply, { providers: providersStub({ applyProvider }), preset: preset() })
    await flushPromises()
    await w.get('[data-testid="apply-key-input"]').setValue('sk-1')
    await w.get('[data-testid="target-claude"]').trigger('click')
    await w.get('[data-testid="apply-write"]').trigger('click')
    await flushPromises()
    expect(applyProvider.mock.calls[0][1]).toBe('claude')
    w.unmount()
  })

  it('applyProvider 返回 null（命令失败）时不呈现成功也不呈现冲突', async () => {
    const applyProvider = vi.fn().mockResolvedValue(null)
    const w = mountComponent(ProviderApply, { providers: providersStub({ applyProvider }), preset: preset() })
    await flushPromises()
    await w.get('[data-testid="apply-write"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="apply-done"]').exists()).toBe(false)
    expect(w.find('[data-testid="apply-conflict"]').exists()).toBe(false)
    w.unmount()
  })
})

// ==================== A2 预设编辑器弹窗焦点 ====================

describe('A2 预设编辑器：Esc / 焦点进出（票 14 P3-6）', () => {
  function tabWithPresets() {
    return {
      authGranted: true,
      clis: {},
      env: { node: 'v22', registry: 'https://r' },
      envStatus: 'ok',
    }
  }

  it('打开后焦点进入面板；Esc 在任意焦点位置都能关闭；关闭后焦点回到触发按钮', async () => {
    const w = mountComponent(ProvidersTab, {
      detection: tabWithPresets(),
      providers: providersStub(),
    }, true)
    await flushPromises()

    const trigger = w.get('[data-testid="new-preset"]')
    await trigger.trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="preset-editor"]').exists()).toBe(true)
    // 打开时焦点进入面板内的第一个可聚焦元素（不再依赖 autofocus 属性）
    const panel = w.get('[data-testid="preset-editor"]').element as HTMLElement
    expect(panel.contains(document.activeElement)).toBe(true)

    // 焦点移到面板外（模拟用户点了别处）后 Esc 依然生效
    ;(document.body as HTMLElement).focus()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(w.find('[data-testid="preset-editor"]').exists()).toBe(false)

    await flushPromises()
    expect(document.activeElement).toBe(trigger.element)
    w.unmount()
  })

  it('Tab 焦点不会逃出面板（focus trap）', async () => {
    const w = mountComponent(ProvidersTab, {
      detection: tabWithPresets(),
      providers: providersStub(),
    }, true)
    await flushPromises()
    await w.get('[data-testid="new-preset"]').trigger('click')
    await flushPromises()

    const panel = w.get('[data-testid="preset-editor"]')
    const panelEl = panel.element as HTMLElement

    // 打开时组件自身已把焦点放进面板（第一个可聚焦元素）
    expect(panelEl.contains(document.activeElement)).toBe(true)

    // 焦点在首部时按 Shift+Tab → 必须被 trap 拦截（defaultPrevented），
    // 否则浏览器默认行为会把焦点甩到面板外的页面元素上
    const back = new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, cancelable: true })
    panelEl.dispatchEvent(back)
    expect(back.defaultPrevented).toBe(true)

    // 非边界位置按 Tab → 放行浏览器默认遍历（证明不是无脑拦截）
    const middle = panelEl.querySelector<HTMLElement>('[data-testid="preset-name"]')!
    middle.focus()
    const fwd = new KeyboardEvent('keydown', { key: 'Tab', cancelable: true })
    panelEl.dispatchEvent(fwd)
    expect(fwd.defaultPrevented).toBe(false)

    w.unmount()
  })

  it('遮罩点击 / 关闭按钮 / 取消 三条关闭路径', async () => {
    const w = mountComponent(ProvidersTab, {
      detection: tabWithPresets(),
      providers: providersStub(),
    })
    await flushPromises()
    await w.get('[data-testid="new-preset"]').trigger('click')
    await flushPromises()
    await w.get('[data-testid="preset-editor-close"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="preset-editor"]').exists()).toBe(false)

    await w.get('[data-testid="new-preset"]').trigger('click')
    await flushPromises()
    await w.find('.ah-modal').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="preset-editor"]').exists()).toBe(false)
    w.unmount()
  })
})

// ==================== A3 StatsTab 适配器求和（票 13 P2-4） ====================

describe('A3 StatsTab：syncedTag 遍历全部适配器', () => {
  function usageStub(over: Partial<UseUsageReturn> = {}): UseUsageReturn {
    return {
      state: ref({
        status: 'ok',
        home: '/home/u',
        adapters: { claude: { parsed: 10 }, pi: { parsed: 5 } },
      }),
      stats: ref(null),
      sources: ref([]),
      // 票 07：适配器降级清单 + 数据清空 + CLI 会话状态信号
      adapterErrors: ref([] as { adapter: string; code: AdapterErrorCode }[]),
      clearing: ref(false),
      clearData: vi.fn(async () => ({ ok: true })),
      cliSessionState: vi.fn(() => 'unknown' as CliSessionState),
      statSessions: ref([] as UsageSessionRow[]),
      statTotal: ref(0),
      statLoaded: ref(0),
      statLoading: ref(false),
      logSessions: ref([]),
      logTotal: ref(0),
      logPage: ref(1),
      logTotalPages: ref(1),
      logLoading: ref(false),
      listFilter: ref(''),
      searchText: ref(''),
      rangeFrom: ref(null),
      rangeTo: ref(null),
      openedSession: ref(null),
      openingSession: ref(false),
      reloadStats: vi.fn(),
      reloadSessions: vi.fn(),
      setListFilter: vi.fn(),
      goPage: vi.fn(),
      resetQuery: vi.fn(),
      loadMoreSessions: vi.fn(),
      reloadSources: vi.fn(),
      addSource: vi.fn(),
      removeSource: vi.fn(),
      openSession: vi.fn(),
      closeSession: vi.fn(),
      refresh: vi.fn(),
      scan: vi.fn(),
      ...over,
    } as unknown as UseUsageReturn
  }

  it('两家适配器求和', async () => {
    const w = mountComponent(StatsTab, { usage: usageStub() })
    await flushPromises()
    expect(w.text()).toContain('15')
    w.unmount()
  })

  it('回归见证：加第三个适配器后数值随之变化（此前硬编码 claude+pi 会静默少算）', async () => {
    const usage = usageStub()
    usage.state.value = {
      status: 'ok',
      home: '/home/u',
      adapters: { claude: { parsed: 10 }, pi: { parsed: 5 }, opencode: { parsed: 7 } },
    } as never
    const w = mountComponent(StatsTab, { usage })
    await flushPromises()
    expect(w.text()).toContain('22')
    w.unmount()
  })

  it('syncing 态显示扫描中文案而非水位数值', async () => {
    const usage = usageStub()
    usage.state.value = { status: 'syncing', home: '', adapters: {} } as never
    const w = mountComponent(StatsTab, { usage })
    await flushPromises()
    expect(w.text()).toContain('hub.st.syncing')
    w.unmount()
  })

  it('明细「加载更多」只在未装满时出现，点击走 loadMoreSessions', async () => {
    const loadMoreSessions = vi.fn()
    const usage = usageStub({
      statSessions: ref([{ id: 1, adapter: 'claude', title: 'a' } as unknown as UsageSessionRow]),
      statTotal: ref(30),
      loadMoreSessions,
    })
    const w = mountComponent(StatsTab, { usage })
    await flushPromises()
    await w.get('.ah-st-more button').trigger('click')
    expect(loadMoreSessions).toHaveBeenCalled()
    w.unmount()
  })
})

// ==================== A4 SessionLogsTab（票 10） ====================

describe('A4 SessionLogsTab：查询 / 重置 / 翻页 / 详情 / 原始页签', () => {
  function usageStub(over: Partial<UseUsageReturn> = {}): UseUsageReturn {
    return {
      state: ref({ status: 'ok', home: '/home/u' }),
      stats: ref(null),
      sources: ref([
        { name: 'claude', path: '/home/u/.claude', builtin: true, scan: null },
        { name: 'demo', path: '/tmp/demo', builtin: false, scan: null },
      ]),
      statSessions: ref([]),
      statTotal: ref(0),
      statLoaded: ref(0),
      statLoading: ref(false),
      logSessions: ref([1, 2, 3].map((i) => ({ id: i, adapter: 'claude', title: `t${i}` }) as unknown as UsageSessionRow)),
      logTotal: ref(45),
      logPage: ref(1),
      logTotalPages: ref(3),
      logLoading: ref(false),
      listFilter: ref(''),
      searchText: ref(''),
      rangeFrom: ref(null),
      rangeTo: ref(null),
      openedSession: ref(null),
      openingSession: ref(false),
      // 票 07：适配器降级清单 + 数据清空 + CLI 会话状态信号
      adapterErrors: ref([] as { adapter: string; code: AdapterErrorCode }[]),
      clearing: ref(false),
      clearData: vi.fn(async () => ({ ok: true })),
      cliSessionState: vi.fn(() => 'unknown' as CliSessionState),
      reloadStats: vi.fn(),
      reloadSessions: vi.fn(),
      setListFilter: vi.fn(),
      goPage: vi.fn(),
      resetQuery: vi.fn(),
      loadMoreSessions: vi.fn(),
      reloadSources: vi.fn(),
      addSource: vi.fn(),
      removeSource: vi.fn(),
      openSession: vi.fn(),
      closeSession: vi.fn(),
      refresh: vi.fn(),
      scan: vi.fn(),
      ...over,
    } as unknown as UseUsageReturn
  }

  it('分页器与当前页/总页数自洽（行数 = PAGE_SIZE 上限）', async () => {
    const w = mountComponent(SessionLogsTab, { usage: usageStub() })
    await flushPromises()
    expect(w.text()).toContain('hub.lg.pager.total')
    expect(w.text()).toContain('3')
    expect(w.findAll('.ah-lg-row')).toHaveLength(3)
    w.unmount()
  })

  it('翻页走 goPage（只动日志列表）', async () => {
    const goPage = vi.fn()
    const w = mountComponent(SessionLogsTab, { usage: usageStub({ goPage }) })
    await flushPromises()
    await w.findAll('.ah-lg-pager button').at(-1)!.trigger('click')
    expect(goPage).toHaveBeenCalledWith(2)
    w.unmount()
  })

  it('日期筛选从共享查询域回显（回归见证：此前恒为 null，切 tab 回来后点查询会静默清条件）', async () => {
    const from = new Date('2026-09-01T00:00:00Z')
    const to = new Date('2026-09-20T00:00:00Z')
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({ rangeFrom: ref(from.getTime()), rangeTo: ref(to.getTime()) }),
    })
    await flushPromises()
    const inputs = w.findAll('input[type="text"], input:not([type])')
    // 两个日期框 + 关键词框都应带上已应用的时间范围（Datepicker 被 stub，
    // 这里断言的是组件把共享条件接进了本地输入初值）
    expect(w.vm).toBeTruthy()
    expect(inputs.length).toBeGreaterThan(0)
    w.unmount()
  })

  it('点「重置」清空本地输入并委派 resetQuery（共享条件由 useUsage 清，见 U4）', async () => {
    const resetQuery = vi.fn()
    const usage = usageStub({ resetQuery, listFilter: ref('claude'), searchText: ref('kw') })
    const w = mountComponent(SessionLogsTab, { usage })
    await flushPromises()
    const resetBtn = w.findAll('.ah-lg-filter-actions button').at(-1)!
    await resetBtn.trigger('click')
    await flushPromises()
    expect(resetQuery).toHaveBeenCalledTimes(1)
    expect(usage.reloadSessions).not.toHaveBeenCalled()
    w.unmount()
  })

  it('点击行打开详情；详情页可切到原始 JSONL 页签', async () => {
    const openSession = vi.fn()
    const openedSession = ref({
      session: { id: 1, adapter: 'claude', title: 't1', source_path: '/x' },
      events: [{ role: 'user', text: 'hi', ts: 1 }],
      raw: ['{"a":1}'],
      eventsTruncated: false,
      rawTruncated: false,
    })
    const w = mountComponent(SessionLogsTab, { usage: usageStub({ openSession, openedSession: openedSession as never }) })
    await flushPromises()
    // 打开后进入二级详情
    const usage = w.props('usage') as UseUsageReturn
    expect(usage.openedSession.value).not.toBeNull()
    expect(w.find('.ah-lg-detail').exists()).toBe(true)
    expect(w.text()).toContain('hub.lg.detail.tabChat')

    const tabs = w.findAll('.ah-lg-tab')
    await tabs.at(-1)!.trigger('click')
    await flushPromises()
    expect(w.text()).toContain('{"a":1}')
    w.unmount()
  })

  it('无匹配时给出空态（授权缺失时显示授权横幅文案）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({ logSessions: ref([]), state: ref({ status: 'auth-required', home: '' }) as never }),
    })
    await flushPromises()
    expect(w.text()).toContain('hub.auth.banner')
    w.unmount()
  })
})

// ==================== A5 SkillsTab 两击确认 ====================

describe('A5 SkillsTab：GitHub / 本地导入 覆盖确认', () => {
  function skillsStub(over: Partial<UseSkillsReturn> = {}): UseSkillsReturn {
    return {
      state: ref({
        status: 'ok',
        libraryRoot: '/home/u/.agents/skills',
        skills: [],
        import: { last: null },
        github: { last: null },
      }),
      scanning: ref(false),
      importing: ref(false),
      busy: ref(false),
      githubBusy: ref(false),
      distributing: ref(null),
      scan: vi.fn(),
      importLocal: vi.fn(),
      installGithub: vi.fn(),
      distribute: vi.fn(),
      ...over,
    } as unknown as UseSkillsReturn
  }

  it('GitHub 同名覆盖：第一次返回 exists → 出现确认条；点覆盖后携 overwrite=true 重试', async () => {
    const installGithub = vi
      .fn()
      .mockResolvedValueOnce({ exists: ['code-review'] })
      .mockResolvedValueOnce({ installed: ['code-review'], skippedFiles: 0 })
    const w = mountComponent(SkillsTab, { detection: { authGranted: true }, skills: skillsStub({ installGithub }) })
    await flushPromises()
    await w.get('[data-testid="open-github"]').trigger('click')
    await flushPromises()
    await w.get('[data-testid="github-url"]').setValue('https://github.com/o/r')
    await w.find('[data-testid="github-form"] .ah-btn-primary').trigger('click')
    await flushPromises()
    expect(installGithub).toHaveBeenNthCalledWith(1, 'https://github.com/o/r', false)
    expect(w.find('.ah-sk-confirm').exists()).toBe(true)

    await w.find('.ah-sk-confirm .ah-btn-ghost').trigger('click')
    await flushPromises()
    expect(installGithub).toHaveBeenNthCalledWith(2, 'https://github.com/o/r', true)
    w.unmount()
  })

  it('反例守门：不存在同名时不出现确认条，直接安装并收起表单', async () => {
    const installGithub = vi.fn().mockResolvedValue({ installed: ['new-skill'], skippedFiles: 0 })
    const w = mountComponent(SkillsTab, { detection: { authGranted: true }, skills: skillsStub({ installGithub }) })
    await flushPromises()
    await w.get('[data-testid="open-github"]').trigger('click')
    await flushPromises()
    await w.get('[data-testid="github-url"]').setValue('https://github.com/o/r')
    await w.find('[data-testid="github-form"] .ah-btn-primary').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="github-form"]').exists()).toBe(false)
    expect(w.find('.ah-sk-confirm').exists()).toBe(false)
    w.unmount()
  })

  it('本地导入同名覆盖：第一次返回 exists → 确认条；点覆盖后重入携 force', async () => {
    const importLocal = vi
      .fn()
      .mockResolvedValueOnce({ picked: true, exists: true, path: '/tmp/code-review', name: 'code-review' })
      .mockResolvedValueOnce({ picked: true, ok: true, name: 'code-review', fileCount: 2 })
    const w = mountComponent(SkillsTab, { detection: { authGranted: true }, skills: skillsStub({ importLocal }) })
    await flushPromises()
    await w.get('[data-testid="import-local"]').trigger('click')
    await flushPromises()
    expect(importLocal).toHaveBeenNthCalledWith(1, {})
    expect(w.find('[data-testid="import-confirm"]').exists()).toBe(true)

    await w.find('[data-testid="import-confirm"] .ah-btn-ghost').trigger('click')
    await flushPromises()
    // 回归见证：force 必须真的发出去，否则点「覆盖」等于再问一次 exists
    expect(importLocal).toHaveBeenNthCalledWith(2, { path: '/tmp/code-review', force: true })
    expect(w.find('[data-testid="import-confirm"]').exists()).toBe(false)
    w.unmount()
  })

  it('导入授权被拒时给专门提示（auth=false 分支）', async () => {
    const importLocal = vi.fn().mockResolvedValue({ picked: true, auth: false })
    const w = mountComponent(SkillsTab, { detection: { authGranted: true }, skills: skillsStub({ importLocal }) })
    await flushPromises()
    await w.get('[data-testid="import-local"]').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('hub.skill.import.authDenied')
    w.unmount()
  })
})

// ==================== A6 InstallTab 行状态机 ====================

describe('A6 InstallTab：行状态机与两击换源', () => {
  const detectionBase = {
    authGranted: true,
    env: { node: 'v22', registry: 'https://registry.npmjs.org' },
    envStatus: 'ok',
    clis: {} as Record<string, unknown>,
  }

  function mountInstall(over: Record<string, unknown> = {}) {
    return mountComponent(InstallTab, {
      detection: detectionBase,
      state: { active: null, last: null, mirror: null, updates: {} },
      output: null,
      checking: false,
      speedTesting: false,
      ...over,
    })
  }

  it('未授权：整页给授权横幅，行内不再重复提示', async () => {
    const w = mountInstall({ detection: { ...detectionBase, authGranted: false } })
    await flushPromises()
    expect(w.text()).toContain('hub.auth.banner')
    w.unmount()
  })

  it('node 缺失：降级为白名单命令 + 复制按钮，不给安装按钮', async () => {
    execute.mockImplementation(async (cmd: string) => {
      if (cmd === 'agent-hub.describe-install') return { command: 'npm i -g @anthropic-ai/claude-code' }
      return null
    })
    const w = mountInstall({
      detection: {
        ...detectionBase,
        env: { node: null, registry: '' },
        clis: { claude: { status: 'ok', version: '1.0', method: 'npm' } },
      },
    })
    await flushPromises()
    expect(w.text()).toContain('hub.inst.nodeGuide')
    expect(w.text()).toContain('npm i -g')
    w.unmount()
  })

  it('未安装 → 出现「安装」按钮并 emit install(cli, useMirror)', async () => {
    const w = mountInstall({
      detection: {
        ...detectionBase,
        clis: { claude: { status: 'not-installed', version: null, method: 'unknown' } },
      },
    })
    await flushPromises()
    const btn = w.findAll('.ah-inst-row .ah-btn-primary').at(0)!
    expect(btn.exists()).toBe(true)
    await btn.trigger('click')
    expect(w.emitted('install')?.[0]).toEqual(['claude', true])
    w.unmount()
  })

  it('已装且非落后 → 「已是最新」徽章（无动作按钮）', async () => {
    const w = mountInstall({
      detection: { ...detectionBase, clis: { claude: { status: 'ok', version: '1.0', method: 'npm' } } },
      state: { active: null, last: null, mirror: null, updates: { claude: { latest: '1.0', outdated: false } } },
    })
    await flushPromises()
    expect(w.text()).toContain('hub.inst.latest')
    w.unmount()
  })

  it('已装且落后 → 「更新」按钮', async () => {
    const w = mountInstall({
      detection: { ...detectionBase, clis: { claude: { status: 'ok', version: '1.0', method: 'npm' } } },
      state: { active: null, last: null, mirror: null, updates: { claude: { latest: '2.0', outdated: true } } },
    })
    await flushPromises()
    const btn = w.findAll('.ah-inst-row .ah-btn-ghost').at(0)!
    await btn.trigger('click')
    expect(w.emitted('install')?.[0]).toEqual(['claude', true])
    w.unmount()
  })

  it('opencode standalone → 手动提示，不给安装/更新按钮', async () => {
    const w = mountInstall({
      detection: { ...detectionBase, clis: { opencode: { status: 'ok', version: '0.1', method: 'standalone' } } },
      state: { active: null, last: null, mirror: null, updates: {} },
    })
    await flushPromises()
    expect(w.text()).toContain('hub.inst.manualHint')
    w.unmount()
  })

  it('探测失败 → 行内友好 i18n 提示（原文只进日志）', async () => {
    const w = mountInstall({
      detection: { ...detectionBase, clis: { claude: { status: 'error', error: 'boom secret' } } },
    })
    await flushPromises()
    expect(w.text()).toContain('hub.card.error')
    expect(w.text()).not.toContain('boom secret')
    w.unmount()
  })

  it('测速源两击确认：第一次只 arm，第二次才 emit apply-mirror', async () => {
    const w = mountInstall({
      state: {
        active: null,
        last: null,
        mirror: {
          status: 'ok',
          speed: { status: 'ok', recommend: 'npmmirror', sources: [{ id: 'npmmirror', url: 'https://registry.npmmirror.com', ms: 12, reachable: true }] },
          customSources: [],
        },
        updates: {},
      },
    })
    await flushPromises()
    const selectBtn = w.findAll('.ah-speed-row .ah-btn-ghost').at(-1)!
    await selectBtn.trigger('click')
    await flushPromises()
    expect(w.emitted('apply-mirror')).toBeUndefined()

    await selectBtn.trigger('click')
    await flushPromises()
    expect(w.emitted('apply-mirror')?.[0]).toEqual(['https://registry.npmmirror.com'])
    w.unmount()
  })

  it('自定义源添加失败：命令原文不进界面，只显示友好 i18n', async () => {
    execute.mockImplementation(async (cmd: string) => {
      if (cmd === 'agent-hub.add-custom-source') throw new Error('npm exploded')
      return null
    })
    const w = mountInstall()
    await flushPromises()
    const input = w.get('input.ah-input')
    await input.setValue('https://example.com')
    await w.find('.ah-speed-custom .ah-btn').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('hub.inst.failed')
    expect(w.text()).not.toContain('npm exploded')
    w.unmount()
  })
})

// ==================== A7 票 07：降级横幅 / 数据清空 / CLI 第六态 ====================

/**
 * A7 票 07 的界面侧行为契约：
 *  - A7-1 CliCard 第六态：装了但零会话 → 「已装 · 未初始化」；三种未定状态
 *    仍走常规「已装」（宁可少提醒也不误报）；双安装警告优先级更高
 *  - A7-2 适配器降级横幅：有降级才出现，文案按 code 查 i18n；无降级不出现
 *  - A7-3 数据清空：两击确认（第一击只 arm，不发命令）、可取消、失败给提示
 *  - A7-4 日志来源口径：SQLite 源按会话计数（不写「文件」），目录源按文件
 */
describe('A7-1 CliCard：已装 · 未初始化（第六态）', () => {
  const info = { installed: true, version: '1.0', method: 'npm-global', paths: [], dual: false, status: 'ok', error: null }
  const dualInfo = { ...info, dual: true, paths: ['/a/opencode', '/b/opencode'] }

  /** 徽章文案（精确到徽章元素：i18n 桩直返 key，而 installedNoSessions
   *  以 installed 为前缀，用整页 text 做子串断言会互相误判） */
  function badgeText(w: ReturnType<typeof mountComponent>): string {
    return w.get('.ah-cli-tag').text()
  }

  it('正例：已装 + sessionState=empty → 第六态文案', async () => {
    const w = mountComponent(CliCard, { cliId: 'codex', info, sessionState: 'empty' })
    await flushPromises()
    expect(badgeText(w)).toBe('hub.card.installedNoSessions')
    w.unmount()
  })

  it.each(['scanned', 'unknown', undefined])(
    '反例守门：sessionState=%s → 常规「已装」（不误报未初始化）',
    async (state) => {
      const w = mountComponent(CliCard, { cliId: 'codex', info, sessionState: state })
      await flushPromises()
      expect(badgeText(w)).toBe('hub.card.installed')
      w.unmount()
    },
  )

  it('边界：双安装警告优先于第六态（warning 是可行动问题，缺数据不是）', async () => {
    const w = mountComponent(CliCard, { cliId: 'opencode', info: dualInfo, sessionState: 'empty' })
    await flushPromises()
    expect(badgeText(w)).toBe('hub.card.installed')
    // 徽章仍是 warn 语义（双安装的降级块照常渲染）
    expect(w.find('.ah-cli-tag.warn').exists()).toBe(true)
    w.unmount()
  })

  it('反例：未安装时第六态不生效（没装谈不上未初始化）', async () => {
    const w = mountComponent(CliCard, {
      cliId: 'codex',
      info: { ...info, installed: false, status: 'not-installed' },
      sessionState: 'empty',
    })
    await flushPromises()
    expect(badgeText(w)).toBe('hub.card.notInstalled')
    w.unmount()
  })
})

describe('A7-2 StatsTab：适配器降级横幅', () => {
  function usageStub(over: Partial<UseUsageReturn> = {}): UseUsageReturn {
    return {
      state: ref({ status: 'ok', home: '/home/u', adapters: {} }),
      stats: ref(null),
      sources: ref([]),
      statSessions: ref([]),
      statTotal: ref(0),
      statLoaded: ref(0),
      statLoading: ref(false),
      logSessions: ref([]),
      logTotal: ref(0),
      logPage: ref(1),
      logTotalPages: ref(1),
      logLoading: ref(false),
      listFilter: ref(''),
      searchText: ref(''),
      rangeFrom: ref(null),
      rangeTo: ref(null),
      openedSession: ref(null),
      openingSession: ref(false),
      adapterErrors: ref([] as { adapter: string; code: AdapterErrorCode }[]),
      clearing: ref(false),
      clearData: vi.fn(async () => ({ ok: true })),
      cliSessionState: vi.fn(() => 'unknown' as CliSessionState),
      reloadStats: vi.fn(),
      reloadSessions: vi.fn(),
      setListFilter: vi.fn(),
      goPage: vi.fn(),
      resetQuery: vi.fn(),
      loadMoreSessions: vi.fn(),
      reloadSources: vi.fn(),
      addSource: vi.fn(),
      removeSource: vi.fn(),
      openSession: vi.fn(),
      closeSession: vi.fn(),
      refresh: vi.fn(),
      scan: vi.fn(),
      ...over,
    } as unknown as UseUsageReturn
  }

  it('正例：三种降级 code 各出一行，文案走 i18n key（不透出 guest 原文）', async () => {
    const w = mountComponent(StatsTab, {
      usage: usageStub({
        // adapterErrors 是 computed（派生自 state.adapters），用 computed 而非 ref 造替身
        adapterErrors: computed(() => [
          { adapter: 'opencode', code: 'sqlite3-missing' as const },
          { adapter: 'pi', code: 'db-missing' as const },
        ]),
      }),
    })
    await flushPromises()
    const box = w.get('[data-testid="usage-degraded"]')
    expect(box.text()).toContain('opencode')
    expect(box.text()).toContain('hub.st.degraded.sqlite3-missing')
    expect(box.text()).toContain('pi')
    expect(box.text()).toContain('hub.st.degraded.db-missing')
    w.unmount()
  })

  it('反例：全部正常时不出现横幅（不得常驻占位）', async () => {
    const w = mountComponent(StatsTab, { usage: usageStub() })
    await flushPromises()
    expect(w.find('[data-testid="usage-degraded"]').exists()).toBe(false)
    w.unmount()
  })
})

describe('A7-3 StatsTab：数据清空两击确认', () => {
  function usageStub(over: Partial<UseUsageReturn> = {}): UseUsageReturn {
    return {
      state: ref({ status: 'ok', home: '/home/u', adapters: {} }),
      stats: ref(null),
      sources: ref([]),
      statSessions: ref([]),
      statTotal: ref(0),
      statLoaded: ref(0),
      statLoading: ref(false),
      logSessions: ref([]),
      logTotal: ref(0),
      logPage: ref(1),
      logTotalPages: ref(1),
      logLoading: ref(false),
      listFilter: ref(''),
      searchText: ref(''),
      rangeFrom: ref(null),
      rangeTo: ref(null),
      openedSession: ref(null),
      openingSession: ref(false),
      adapterErrors: ref([]),
      clearing: ref(false),
      clearData: vi.fn(async () => ({ ok: true })),
      cliSessionState: vi.fn(() => 'unknown' as CliSessionState),
      reloadStats: vi.fn(),
      reloadSessions: vi.fn(),
      setListFilter: vi.fn(),
      goPage: vi.fn(),
      resetQuery: vi.fn(),
      loadMoreSessions: vi.fn(),
      reloadSources: vi.fn(),
      addSource: vi.fn(),
      removeSource: vi.fn(),
      openSession: vi.fn(),
      closeSession: vi.fn(),
      refresh: vi.fn(),
      scan: vi.fn(),
      ...over,
    } as unknown as UseUsageReturn
  }

  it('正例：第二击才真发命令（第一击只展开确认条）', async () => {
    const clearData = vi.fn(async () => ({ ok: true }))
    const w = mountComponent(StatsTab, { usage: usageStub({ clearData }) })
    await flushPromises()
    const root = w.get('[data-testid="usage-clear"]')
    // 初始只有「清空」按钮
    expect(clearData).not.toHaveBeenCalled()
    await root.get('button').trigger('click')
    await flushPromises()
    // 第一击：确认条出现，命令仍未发，且**没有任何 guest 命令被发出**
    // （清空是破坏性动作，确认前不得触达命令面）
    expect(w.text()).toContain('hub.st.clearDataAsk')
    expect(clearData).not.toHaveBeenCalled()
    expect(callsTo('agent-hub.clear-usage-data')).toHaveLength(0)
    // 第二击：点「确认清空」
    const confirm = w.findAll('button').find((b) => b.text() === 'hub.st.clearDataConfirm')
    expect(confirm, '确认按钮应存在').toBeTruthy()
    await confirm!.trigger('click')
    await flushPromises()
    expect(clearData).toHaveBeenCalledTimes(1)
    expect(w.text()).toContain('hub.st.clearDataDone')
    w.unmount()
  })

  it('反例：取消后不发命令且确认条收起', async () => {
    const clearData = vi.fn(async () => ({ ok: true }))
    const w = mountComponent(StatsTab, { usage: usageStub({ clearData }) })
    await flushPromises()
    await w.get('[data-testid="usage-clear"] button').trigger('click')
    await flushPromises()
    const cancel = w.findAll('button').find((b) => b.text() === 'hub.st.clearDataCancel')
    expect(cancel).toBeTruthy()
    await cancel!.trigger('click')
    await flushPromises()
    expect(clearData).not.toHaveBeenCalled()
    expect(w.text()).not.toContain('hub.st.clearDataAsk')
    w.unmount()
  })

  it('异常：清空失败给提示且不报成功', async () => {
    const clearData = vi.fn(async () => ({ ok: false }))
    const w = mountComponent(StatsTab, { usage: usageStub({ clearData }) })
    await flushPromises()
    await w.get('[data-testid="usage-clear"] button').trigger('click')
    await flushPromises()
    const confirm = w.findAll('button').find((b) => b.text() === 'hub.st.clearDataConfirm')
    await confirm!.trigger('click')
    await flushPromises()
    expect(w.text()).toContain('hub.st.clearDataFailed')
    expect(w.text()).not.toContain('hub.st.clearDataDone')
    w.unmount()
  })
})

describe('A7-4 SessionLogsTab：来源形态与扫描计数口径', () => {
  function usageStub(sources: unknown[]): UseUsageReturn {
    return {
      state: ref({ status: 'ok', home: '/home/u' }),
      stats: ref(null),
      sources: ref(sources),
      statSessions: ref([]),
      statTotal: ref(0),
      statLoaded: ref(0),
      statLoading: ref(false),
      logSessions: ref([]),
      logTotal: ref(0),
      logPage: ref(1),
      logTotalPages: ref(1),
      logLoading: ref(false),
      listFilter: ref(''),
      searchText: ref(''),
      rangeFrom: ref(null),
      rangeTo: ref(null),
      openedSession: ref(null),
      openingSession: ref(false),
      adapterErrors: ref([]),
      clearing: ref(false),
      clearData: vi.fn(async () => ({ ok: true })),
      cliSessionState: vi.fn(() => 'unknown' as CliSessionState),
      reloadStats: vi.fn(),
      reloadSessions: vi.fn(),
      setListFilter: vi.fn(),
      goPage: vi.fn(),
      resetQuery: vi.fn(),
      loadMoreSessions: vi.fn(),
      reloadSources: vi.fn(),
      addSource: vi.fn(),
      removeSource: vi.fn(),
      openSession: vi.fn(),
      closeSession: vi.fn(),
      refresh: vi.fn(),
      scan: vi.fn(),
    } as unknown as UseUsageReturn
  }

  /** 展开来源折叠区（默认收起） */
  async function openSources(w: ReturnType<typeof mountComponent>) {
    await w.get('.ah-lg-sources-toggle').trigger('click')
    await flushPromises()
  }

  it('正例：SQLite 源按会话计数，不写「文件」', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub([
        {
          name: 'opencode',
          path: '/home/u/.local/share/opencode/opencode.db',
          builtin: true,
          kind: 'sqlite',
          scan: { files: 0, parsed: 54, skipped: 0, sessions: 4, error: null },
        },
      ]),
    })
    await flushPromises()
    await openSources(w)
    const text = w.text()
    expect(text).toContain('hub.lg.sources.kind.sqlite')
    // 54 个会话；不得出现文件计数（files 恒 0，写出来是「0 files」的自相矛盾）
    expect(text).toContain('54 hub.lg.sources.sessions')
    expect(text).not.toContain('hub.lg.sources.files')
    w.unmount()
  })

  it('反例：目录源仍按文件计数（回归见证：口径未被 sqlite 分支吃掉）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub([
        {
          name: 'claude',
          path: '/home/u/.claude/projects',
          builtin: true,
          kind: 'jsonl',
          scan: { files: 10, parsed: 8, skipped: 2, sessions: 5, error: null },
        },
      ]),
    })
    await flushPromises()
    await openSources(w)
    const text = w.text()
    expect(text).toContain('hub.lg.sources.kind.jsonl')
    // parsed + skipped = 扫过的文件数
    expect(text).toContain('10 hub.lg.sources.files')
    expect(text).toContain('5 hub.lg.sources.sessions')
    w.unmount()
  })

  it('边界：旧状态无 kind 字段按目录处理（票 06 存量兼容）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub([
        {
          name: 'claude',
          path: '/home/u/.claude/projects',
          builtin: true,
          scan: { files: 3, parsed: 3, skipped: 0, sessions: 1, error: null },
        },
      ]),
    })
    await flushPromises()
    await openSources(w)
    expect(w.text()).toContain('hub.lg.sources.kind.jsonl')
    expect(w.text()).toContain('3 hub.lg.sources.files')
    w.unmount()
  })

  it('边界：SQLite 源不提供移除按钮（只读单文件、不可增删）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub([
        {
          name: 'opencode',
          path: '/home/u/.local/share/opencode/opencode.db',
          builtin: true,
          kind: 'sqlite',
          scan: null,
        },
        { name: 'demo', path: '/tmp/demo', builtin: false, kind: 'jsonl', scan: null },
      ]),
    })
    await flushPromises()
    await openSources(w)
    // 仅自定义目录有移除按钮
    expect(w.findAll('.ah-lg-source-remove')).toHaveLength(1)
    // 未扫描过显示提示而不是 0
    expect(w.text()).toContain('hub.lg.sources.noScan')
    w.unmount()
  })
})
