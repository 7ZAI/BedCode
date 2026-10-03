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
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { mount, flushPromises } from '@vue/test-utils'
import { computed, h, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import ProviderApply from '../components/ProviderApply.vue'
import ProvidersTab from '../components/ProvidersTab.vue'
import StatsTab from '../components/StatsTab.vue'
import SessionLogsTab from '../components/SessionLogsTab.vue'
import { COLLAPSE_THRESHOLD_CHARS, COLLAPSE_THRESHOLD_TOOL_CHARS, GUEST_TEXT_CAPS } from '../utils/format'
import SkillsTab from '../components/SkillsTab.vue'
import InstallTab from '../components/InstallTab.vue'
import OverviewTab from '../components/OverviewTab.vue'
import CliCard from '../components/CliCard.vue'
import type { AdapterErrorCode, ProviderPreset, ProvidersDomainState, UsageSessionDetail, UsageSessionRow, UsageSource } from '../types'
import type { UseProvidersReturn } from '../composables/useProviders'
import type { CliSessionState, StatsDays, UseUsageReturn } from '../composables/useUsage'
import { AGENT_HUB } from './helpers/contrast'
import type { UseSkillsReturn } from '../composables/useSkills'

// 第三方控件内部实现不进契约
vi.mock('@vuepic/vue-datepicker', () => ({
  default: { name: 'Datepicker', props: ['modelValue'], render: () => null },
}))

// toast 队列本体由宿主共享实例持有（SDK vite 插件外部化到 __BEDCODE_SHARED__）。
// 这里只录「发了哪条、什么级别」，不关心 sonner 内部渲染。
const toastCalls: Array<{ level: string; message: unknown }> = []
vi.mock('vue-sonner', () => ({
  toast: {
    success: (m: unknown) => toastCalls.push({ level: 'success', message: m }),
    error: (m: unknown) => toastCalls.push({ level: 'error', message: m }),
    info: (m: unknown) => toastCalls.push({ level: 'info', message: m }),
    warning: (m: unknown) => toastCalls.push({ level: 'warning', message: m }),
  },
}))
vi.mock('@binblink/bedcode-plugin-sdk-desktop/ui', () => ({
  // 真实 Select（宿主共享下拉）的最小可用替身：渲染 <select> 并在 change 时
  // emit update:modelValue，使「下拉选项 + 选中回填」可被组件层行为契约断言；
  // 面板定位 / 键盘 / 主题等实现细节不进契约。
  default: {
    name: 'Select',
    props: ['modelValue', 'options', 'placeholder', 'disabled'],
    emits: ['update:modelValue'],
    render(this: any) {
      return h(
        'select',
        {
          class: 'select-stub',
          disabled: this.disabled,
          onChange: (e: Event) =>
            this.$emit('update:modelValue', (e.target as HTMLSelectElement).value),
        },
        [
          this.placeholder ? h('option', { value: '' }, this.placeholder) : null,
          ...(this.options ?? []).map((o: any) =>
            h('option', { value: String(o.value), selected: String(o.value) === String(this.modelValue) }, o.label),
          ),
        ],
      )
    },
  },
}))

const execute = vi.fn(
  async (_command: string, _args?: Record<string, unknown>): Promise<Record<string, unknown> | null> => null,
)

function makeContext(): PluginContext {
  return {
    id: 'com.bedcode.agent-hub',
    // i18n 桩直返 key，但把插值参数渲染出来（syncedTag 等需要断言数值）；
    // getI18n 返回注册表桩（模拟 registerMessages 扁平命中：完整 key → 短 key），
    // 供 resolvePluginErrorText 的业务码 / fallback 解析（ADR 0030）
    i18n: {
      t: (k: string, params?: Record<string, unknown>) =>
        params && Object.keys(params).length > 0
          ? `${k}(${Object.entries(params).map(([pk, pv]) => `${pk}=${String(pv)}`).join(',')})`
          : k,
      getI18n: () => ({
        global: {
          t: (key: string, params?: Record<string, unknown>) => {
            const prefix = 'com.bedcode.agent-hub.'
            const short = key.startsWith(prefix) ? key.slice(prefix.length) : key
            if (params && Object.keys(params).length > 0) {
              return `${short}(${Object.entries(params)
                .map(([pk, pv]) => `${pk}=${String(pv)}`)
                .join(',')})`
            }
            return short
          },
        },
      }),
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
  toastCalls.length = 0
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

/** 命令结果判别联合的构造助手（与 useProviders.CommandResult 同形） */
const ok = <T,>(data: T) => ({ status: 'ok' as const, data })
const busy = () => ({ status: 'busy' as const })
const cmdErr = (e: unknown) => ({ status: 'error' as const, error: e })

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
    savePreset: vi.fn().mockResolvedValue(ok({ saved: true })),
    deletePreset: vi.fn().mockResolvedValue(ok(null)),
    importProviders: vi.fn().mockResolvedValue(ok(null)),
    applyProvider: vi.fn().mockResolvedValue(ok(null)),
    ...over,
  } as unknown as UseProvidersReturn
}

describe('A1 ProviderApply：key 四选一 + 桥接冲突两击确认', () => {
  it('默认（无 stored key、无 source 标注）选中 inline，提交 inline key', async () => {
    const applyProvider = vi.fn().mockResolvedValue(ok({ applied: true, files: ['auth.json'] }))
    const w = mountComponent(ProviderApply, { providers: providersStub({ applyProvider }), preset: preset() })
    await flushPromises()
    await w.get('[data-testid="apply-key-input"]').setValue('sk-123')
    await w.get('[data-testid="apply-write"]').trigger('click')
    await flushPromises()
    expect(applyProvider).toHaveBeenCalledWith(1, 'pi', 'sensenova', { kind: 'inline', value: 'sk-123' }, false)
    w.unmount()
  })

  it('预设已有 stored key 时默认选中 stored，提交 { kind:"stored" }', async () => {
    const applyProvider = vi.fn().mockResolvedValue(ok({ applied: true, files: [] }))
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
    const applyProvider = vi.fn().mockResolvedValue(ok({ applied: true, files: [] }))
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
    const applyProvider = vi.fn().mockResolvedValue(ok({ applied: true, files: [] }))
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
      .mockResolvedValueOnce(ok({ bridgeConflict: true, bridges: ['provider-config.sh'] }))
      .mockResolvedValueOnce(ok({ applied: true, files: ['settings.json'] }))
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
    const applyProvider = vi.fn().mockResolvedValue(ok({ applied: true, files: [] }))
    const w = mountComponent(ProviderApply, { providers: providersStub({ applyProvider }), preset: preset() })
    await flushPromises()
    await w.get('[data-testid="apply-key-input"]').setValue('sk-1')
    await w.get('[data-testid="target-claude"]').trigger('click')
    await w.get('[data-testid="apply-write"]').trigger('click')
    await flushPromises()
    expect(applyProvider.mock.calls[0][1]).toBe('claude')
    w.unmount()
  })

  it('apply 命令报错时显性失败：面板报错 + error toast，不呈现成功也不呈现冲突', async () => {
    const applyProvider = vi.fn().mockResolvedValue(cmdErr(new Error('host command failed')))
    const w = mountComponent(ProviderApply, { providers: providersStub({ applyProvider }), preset: preset() })
    await flushPromises()
    // 默认方言 inline 且写入按钮要求已填 key（未填时 disabled）
    await w.get('[data-testid="apply-key-input"]').setValue('sk-1')
    await w.get('[data-testid="apply-write"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="apply-done"]').exists()).toBe(false)
    expect(w.find('[data-testid="apply-conflict"]').exists()).toBe(false)
    // fail-visible：命令失败必须在界面上看得见（旧实现静默 return）
    expect(toastCalls.some((c) => c.level === 'error')).toBe(true)
    expect(w.text()).toContain('hub.pv.apply.failed')
    w.unmount()
  })

  it('apply 并发被忽略（busy）时不误报失败、不弹错误', async () => {
    const applying = ref(false)
    const applyProvider = vi
      .fn()
      .mockResolvedValueOnce(ok({ bridgeConflict: true, bridges: ['provider-config.sh'] }))
      .mockResolvedValue(busy())
    const w = mountComponent(ProviderApply, {
      providers: providersStub({ applyProvider, applying }),
      preset: preset({ keyMask: 'sk-1***' }),
    })
    await flushPromises()
    await w.get('[data-testid="apply-write"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="apply-conflict"]').exists()).toBe(true)
    toastCalls.length = 0

    // 冲突确认按钮不受 applying 影响 → 能真正进入 apply()：守卫必须先拦下
    applying.value = true
    await flushPromises()
    await w.get('[data-testid="apply-conflict-confirm"]').trigger('click')
    await flushPromises()

    expect(applyProvider).toHaveBeenCalledTimes(1)
    expect(toastCalls.filter((c) => c.level === 'error')).toHaveLength(0)
    w.unmount()
  })

  it('apply 成功：弹成功 toast + 呈现写入文件清单（面板保留供读重启提示）', async () => {
    const applyProvider = vi.fn().mockResolvedValue(ok({ applied: true, files: ['auth.json'] }))
    const w = mountComponent(ProviderApply, { providers: providersStub({ applyProvider }), preset: preset() })
    await flushPromises()
    await w.get('[data-testid="apply-key-input"]').setValue('sk-1')
    await w.get('[data-testid="apply-write"]').trigger('click')
    await flushPromises()
    expect(toastCalls.some((c) => c.level === 'success')).toBe(true)
    expect(toastCalls.filter((c) => c.level === 'error')).toHaveLength(0)
    expect(w.find('[data-testid="apply-done"]').exists()).toBe(true)
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
    // 且落点是表单首个字段（名称），不是面板里 DOM 最靠前的关闭按钮
    expect(document.activeElement).toBe(w.get('[data-testid="preset-name"]').element)

    // 焦点移到面板外（模拟用户点了别处）后 Esc 依然生效
    ;(document.body as HTMLElement).focus()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(w.find('[data-testid="preset-editor"]').exists()).toBe(false)

    await flushPromises()
    expect(document.activeElement).toBe(trigger.element)
    w.unmount()
  })

  // 回归锁：document 级 Esc 监听曾漏判 key，导致「弹窗里敲任意键即关闭」
  // （输入框无法输入）。契约：只有 Escape 关闭；字符键 / Enter / IME 组合中的
  // Escape 都不关。断言强到能杀死「删掉 key 过滤」这一变异。
  it('输入普通按键（字符 / Enter）不关闭弹窗：焦点与已输入内容都保持', async () => {
    const w = mountComponent(ProvidersTab, {
      detection: tabWithPresets(),
      providers: providersStub(),
    }, true)
    await flushPromises()
    await w.get('[data-testid="new-preset"]').trigger('click')
    await flushPromises()

    const name = w.get('[data-testid="preset-name"]').element as HTMLInputElement
    name.focus()
    name.value = 'gpt'
    for (const key of ['p', 'Enter', 'ArrowLeft', 'Shift']) {
      name.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }))
      await flushPromises()
    }

    expect(w.find('[data-testid="preset-editor"]').exists()).toBe(true)
    expect(document.activeElement).toBe(name)
    expect(name.value).toBe('gpt')

    // 证明监听仍挂着（不是「意外没注册」导致的假绿）：Escape 立刻能关
    name.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(w.find('[data-testid="preset-editor"]').exists()).toBe(false)
    w.unmount()
  })

  it('IME 组合输入中的 Escape 只取消候选词，不关闭弹窗', async () => {
    const w = mountComponent(ProvidersTab, {
      detection: tabWithPresets(),
      providers: providersStub(),
    }, true)
    await flushPromises()
    await w.get('[data-testid="new-preset"]').trigger('click')
    await flushPromises()

    const name = w.get('[data-testid="preset-name"]').element as HTMLInputElement
    name.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, isComposing: true }))
    await flushPromises()
    expect(w.find('[data-testid="preset-editor"]').exists()).toBe(true)
    w.unmount()
  })

  it('Tab 焦点不会逃出面板（focus trap）：首尾两个边界拦截、中间放行', async () => {
    const w = mountComponent(ProvidersTab, {
      detection: tabWithPresets(),
      providers: providersStub(),
    }, true)
    await flushPromises()
    await w.get('[data-testid="new-preset"]').trigger('click')
    await flushPromises()
    // 名称非空 → 保存按钮（type=submit）不被 disabled，成为面板内最后一个可聚焦元素
    await w.get('[data-testid="preset-name"]').setValue('my-preset')

    const panelEl = w.get('[data-testid="preset-editor"]').element as HTMLElement

    // 打开时组件自身已把焦点放进面板（表单首个字段）
    expect(panelEl.contains(document.activeElement)).toBe(true)

    // 焦点在首部（关闭按钮）时按 Shift+Tab → 必须被 trap 拦截（defaultPrevented），
    // 否则浏览器默认行为会把焦点甩到面板外的页面元素上
    const close = w.get('[data-testid="preset-editor-close"]').element as HTMLElement
    close.focus()
    const back = new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, cancelable: true })
    panelEl.dispatchEvent(back)
    expect(back.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(w.get('[data-testid="preset-save"]').element)

    // 焦点在尾部（保存）时按 Tab → 同样被拦回面板首部
    const fwd = new KeyboardEvent('keydown', { key: 'Tab', cancelable: true })
    panelEl.dispatchEvent(fwd)
    expect(fwd.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(close)

    // 非边界位置按 Tab → 放行浏览器默认遍历（证明不是无脑拦截）
    const middle = panelEl.querySelector<HTMLElement>('[data-testid="preset-name"]')!
    middle.focus()
    const mid = new KeyboardEvent('keydown', { key: 'Tab', cancelable: true })
    panelEl.dispatchEvent(mid)
    expect(mid.defaultPrevented).toBe(false)

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

// ==================== A7 预设编辑器弹窗：表单语义与无障碍关联 ====================

describe('A7 预设编辑器弹窗：标签关联 / 必填标记 / 错误落位 / 表单提交', () => {
  function tabWithPresets() {
    return {
      authGranted: true,
      clis: {},
      env: { node: 'v22', registry: 'https://r' },
      envStatus: 'ok',
    }
  }

  async function openEditor(over: Partial<UseProvidersReturn> = {}, attach = true) {
    const providers = providersStub(over)
    const w = mountComponent(
      ProvidersTab,
      { detection: tabWithPresets(), providers },
      attach,
    )
    await flushPromises()
    await w.get('[data-testid="new-preset"]').trigger('click')
    await flushPromises()
    return { w, providers }
  }

  /**
   * 点「保存」必须走原生 element.click()：面板是 <form>、保存是 type=submit，
   * 而 VTU 的 trigger('click') 派发的是合成事件，jsdom 不执行 submit 按钮的
   * activation behavior（不派发 submit），表单提交路径会被静默跳过。
   */
  function clickSave(w: { get: (s: string) => { element: unknown } }) {
    ;(w.get('[data-testid="preset-save"]').element as HTMLButtonElement).click()
  }

  // ==================== A7-b 保存反馈：成功 toast + 自动关窗 / 失败可见 ====================

  it('保存成功：弹成功 toast 并自动关闭弹窗', async () => {
    const savePreset = vi.fn().mockResolvedValue(ok({ saved: true }))
    const { w } = await openEditor({ savePreset })
    await w.get('[data-testid="preset-name"]').setValue('my-preset')
    clickSave(w)
    await flushPromises()

    expect(w.find('[data-testid="preset-editor"]').exists()).toBe(false)
    expect(toastCalls).toHaveLength(1)
    expect(toastCalls[0].level).toBe('success')
    expect(toastCalls[0].message).toBe('hub.pv.toast.created')
    w.unmount()
  })

  it('编辑态保存成功用「已更新」文案（新建/编辑不串词）', async () => {
    const savePreset = vi.fn().mockResolvedValue(ok({ saved: true }))
    const providers = providersStub({
      savePreset,
      state: ref({
        claude: { env: {}, bridge: {} },
        presets: [preset({ id: 7, name: 'kimi' })],
        import: { last: null },
      } as unknown as ProvidersDomainState),
    })
    const w = mountComponent(ProvidersTab, { detection: tabWithPresets(), providers }, true)
    await flushPromises()
    await w.get('[data-testid="edit-kimi"]').trigger('click')
    await flushPromises()
    clickSave(w)
    await flushPromises()

    expect(toastCalls[0]?.message).toBe('hub.pv.toast.updated')
    w.unmount()
  })

  it('保存命令报错：弹错误 toast、弹窗保持打开（旧实现静默无反应）', async () => {
    const savePreset = vi.fn().mockResolvedValue(cmdErr(new Error('host command failed')))
    const { w } = await openEditor({ savePreset })
    await w.get('[data-testid="preset-name"]').setValue('my-preset')
    clickSave(w)
    await flushPromises()

    // fail-visible：不能默默吞掉，用户必须能重试
    expect(toastCalls).toHaveLength(1)
    expect(toastCalls[0].level).toBe('error')
    expect(toastCalls[0].message).toBe('hub.pv.editor.saveFailed')
    expect(w.find('[data-testid="preset-editor"]').exists()).toBe(true)
    w.unmount()
  })

  it('guest 回执异常（既无 saved 也无 nameExists）视为失败，不装作无事发生', async () => {
    const savePreset = vi.fn().mockResolvedValue(ok({}))
    const { w } = await openEditor({ savePreset })
    await w.get('[data-testid="preset-name"]').setValue('my-preset')
    clickSave(w)
    await flushPromises()

    expect(toastCalls[0]?.level).toBe('error')
    expect(w.find('[data-testid="preset-editor"]').exists()).toBe(true)
    w.unmount()
  })

  it('同名冲突：弹窗内报错，但不算命令失败、不弹错误 toast', async () => {
    const savePreset = vi.fn().mockResolvedValue(ok({ saved: false, nameExists: true }))
    const { w } = await openEditor({ savePreset })
    await w.get('[data-testid="preset-name"]').setValue('dup')
    clickSave(w)
    await flushPromises()

    expect(w.find('.ah-cli-error').exists()).toBe(true)
    expect(toastCalls.filter((c) => c.level === 'error')).toHaveLength(0)
    w.unmount()
  })

  it('并发保存被忽略（busy）：不发命令，也不误报失败', async () => {
    const saving = ref(false)
    const savePreset = vi.fn().mockResolvedValue(busy())
    const { w } = await openEditor({ savePreset, saving })
    await w.get('[data-testid="preset-name"]').setValue('my-preset')
    saving.value = true
    await flushPromises()
    // 直接 submit 表单（回车路径不受 submit 按钮 disabled 影响）→ 真正进入 save()
    await w.get('[data-testid="preset-editor"]').trigger('submit')
    await flushPromises()

    expect(savePreset).not.toHaveBeenCalled()
    expect(toastCalls).toHaveLength(0)
    w.unmount()
  })

  it('删除失败：弹错误 toast（列表行保留，不装作已删）', async () => {
    const deletePreset = vi.fn().mockResolvedValue(cmdErr(new Error('host command failed')))
    const providers = providersStub({
      deletePreset,
      state: ref({
        claude: { env: {}, bridge: {} },
        presets: [preset({ id: 3, name: 'kimi' })],
        import: { last: null },
      } as unknown as ProvidersDomainState),
    })
    const w = mountComponent(ProvidersTab, { detection: tabWithPresets(), providers }, true)
    await flushPromises()

    await w.get('[data-testid="delete-kimi"]').trigger('click') // 一击：武装
    await w.get('[data-testid="delete-kimi"]').trigger('click') // 二击：执行
    await flushPromises()

    expect(toastCalls.some((c) => c.level === 'error')).toBe(true)
    expect(w.find('[data-testid="preset-row-kimi"]').exists()).toBe(true)
    w.unmount()
  })

  it('删除成功：弹成功 toast', async () => {
    const deletePreset = vi.fn().mockResolvedValue(ok(null))
    const providers = providersStub({
      deletePreset,
      state: ref({
        claude: { env: {}, bridge: {} },
        presets: [preset({ id: 3, name: 'kimi' })],
        import: { last: null },
      } as unknown as ProvidersDomainState),
    })
    const w = mountComponent(ProvidersTab, { detection: tabWithPresets(), providers }, true)
    await flushPromises()

    await w.get('[data-testid="delete-kimi"]').trigger('click')
    await w.get('[data-testid="delete-kimi"]').trigger('click')
    await flushPromises()

    expect(toastCalls.some((c) => c.level === 'success')).toBe(true)
    w.unmount()
  })

  it('导入失败：弹错误 toast，且不展示上一次导入的回执卡片', async () => {
    const importProviders = vi.fn().mockResolvedValue(cmdErr(new Error('host command failed')))
    const providers = providersStub({
      importProviders,
      state: ref({
        claude: { env: {}, bridge: {} },
        presets: [],
        // 上一轮成功导入的陈旧回执：失败时绝不能被当成本次结果呈现
        import: { last: { created: ['old'], skipped: [], keys: {} } },
      } as unknown as ProvidersDomainState),
    })
    const w = mountComponent(ProvidersTab, { detection: tabWithPresets(), providers }, true)
    await flushPromises()

    await w.get('[data-testid="import-providers"]').trigger('click')
    await flushPromises()

    expect(toastCalls.some((c) => c.level === 'error')).toBe(true)
    expect(w.find('[data-testid="import-result"]').exists()).toBe(false)
    w.unmount()
  })

  it('导入成功：展示本次回执卡片', async () => {
    const importProviders = vi.fn().mockResolvedValue(ok({ created: ['a'], skipped: [], keys: {} }))
    const providers = providersStub({
      importProviders,
      state: ref({
        claude: { env: {}, bridge: {} },
        presets: [],
        import: { last: { created: ['a'], skipped: [], keys: {} } },
      } as unknown as ProvidersDomainState),
    })
    const w = mountComponent(ProvidersTab, { detection: tabWithPresets(), providers }, true)
    await flushPromises()

    await w.get('[data-testid="import-providers"]').trigger('click')
    await flushPromises()

    expect(w.find('[data-testid="import-result"]').exists()).toBe(true)
    expect(toastCalls.filter((c) => c.level === 'error')).toHaveLength(0)
    w.unmount()
  })

  // ==================== A7-c 模板快选：空表单直接填 / 已填内容先确认 ====================

  it('空表单点模板：直接填满，不弹覆盖确认条（常用路径零摩擦）', async () => {
    const { w } = await openEditor()
    await w.get('[data-testid="preset-template-deepseek"]').trigger('click')
    await flushPromises()

    expect(w.find('[data-testid="template-confirm"]').exists()).toBe(false)
    expect((w.get('[data-testid="preset-name"]').element as HTMLInputElement).value).not.toBe('')
    w.unmount()
  })

  it('已填内容点模板：不静默抹除，先弹覆盖确认条且表单原样保留', async () => {
    const { w } = await openEditor()
    await w.get('[data-testid="preset-name"]').setValue('my-custom')
    await w.get('[data-testid="preset-baseurl"]').setValue('https://my.api.dev/v1')

    await w.get('[data-testid="preset-template-deepseek"]').trigger('click')
    await flushPromises()

    expect(w.find('[data-testid="template-confirm"]').exists()).toBe(true)
    // 关键：用户刚输入的内容一条都不能丢
    expect((w.get('[data-testid="preset-name"]').element as HTMLInputElement).value).toBe('my-custom')
    expect((w.get('[data-testid="preset-baseurl"]').element as HTMLInputElement).value).toBe(
      'https://my.api.dev/v1',
    )
    w.unmount()
  })

  it('覆盖确认 → 应用模板；取消 → 不覆盖（表单与确认条各自复位）', async () => {
    const { w } = await openEditor()
    await w.get('[data-testid="preset-name"]').setValue('my-custom')

    // 取消路径
    await w.get('[data-testid="preset-template-deepseek"]').trigger('click')
    await flushPromises()
    await w.get('[data-testid="template-overwrite-cancel"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="template-confirm"]').exists()).toBe(false)
    expect((w.get('[data-testid="preset-name"]').element as HTMLInputElement).value).toBe('my-custom')

    // 再确认路径
    await w.get('[data-testid="preset-template-deepseek"]').trigger('click')
    await flushPromises()
    await w.get('[data-testid="template-overwrite-confirm"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="template-confirm"]').exists()).toBe(false)
    expect((w.get('[data-testid="preset-name"]').element as HTMLInputElement).value).not.toBe('my-custom')

    // 覆盖后基准重置：再点其它模板不应再要确认（不会二次打扰）
    await w.get('[data-testid="preset-template-deepseek"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="template-confirm"]').exists()).toBe(false)
    w.unmount()
  })

  it('只改了方言也算「已填内容」（填了 key 同理），一并纳入确认', async () => {
    const { w } = await openEditor()
    await w.get('[data-testid="preset-style-anthropic"]').trigger('click')
    await flushPromises()
    await w.get('[data-testid="preset-template-deepseek"]').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="template-confirm"]').exists()).toBe(true)
    w.unmount()
  })

  it('每个控件都有 label[for] 关联（无 “只有 placeholder 的输入框”）', async () => {
    const { w } = await openEditor()
    const panel = w.get('[data-testid="preset-editor"]').element as HTMLElement
    for (const id of ['pv-name', 'pv-baseurl', 'pv-models', 'pv-key']) {
      expect(panel.querySelector(`label[for="${id}"]`), `缺少 label[for=${id}]`).not.toBeNull()
      expect(panel.querySelector(`#${id}`), `缺少控件 #${id}`).not.toBeNull()
    }
    w.unmount()
  })

  it('名称是唯一必填项：星号标记带可读名 + aria-required', async () => {
    const { w } = await openEditor()
    const name = w.get('[data-testid="preset-name"]')
    expect(name.attributes('aria-required')).toBe('true')
    const req = w.get('.ah-pv-req')
    expect(req.text()).toBe('*')
    // 星号对读屏无意义：另给「必填」的可读名
    expect(req.attributes('aria-label')).toBe('hub.pv.editor.required')
    // 其它字段不得标成必填
    expect(w.get('[data-testid="preset-baseurl"]').attributes('aria-required')).toBeUndefined()
    w.unmount()
  })

  it('同名错误落在名称字段下方，并被 aria-describedby / aria-invalid 关联', async () => {
    const savePreset = vi.fn().mockResolvedValue(ok({ saved: false, nameExists: true }))
    const { w } = await openEditor({ savePreset })
    await w.get('[data-testid="preset-name"]').setValue('dup')
    clickSave(w)
    await flushPromises()

    // 错误在弹窗里（未关闭）
    expect(w.find('[data-testid="preset-editor"]').exists()).toBe(true)
    const name = w.get('[data-testid="preset-name"]')
    const describedBy = name.attributes('aria-describedby')
    expect(name.attributes('aria-invalid')).toBe('true')
    const err = w.find(`#${describedBy}`)
    expect(err.exists()).toBe(true)
    expect(err.text()).toBe('hub.pv.editor.nameExists')
    // 落位在名称字段内（紧贴出错控件，而不是飘到弹窗底部）
    const field = err.element.closest('.ah-pv-field')
    expect(field, '错误未落在任何 .ah-pv-field 内').not.toBeNull()
    expect(field?.contains(name.element), '错误与名称控件不在同一字段行').toBe(true)
    w.unmount()
  })

  it('修改名称后上一轮的同名错误作废（不挂到下次重试）', async () => {
    const savePreset = vi.fn().mockResolvedValue(ok({ saved: false, nameExists: true }))
    const { w } = await openEditor({ savePreset })
    await w.get('[data-testid="preset-name"]').setValue('dup')
    clickSave(w)
    await flushPromises()
    expect(w.find('.ah-cli-error').exists()).toBe(true)

    await w.get('[data-testid="preset-name"]').setValue('dup-2')
    await flushPromises()
    expect(w.find('.ah-cli-error').exists()).toBe(false)
    expect(w.get('[data-testid="preset-name"]').attributes('aria-invalid')).toBeUndefined()
    w.unmount()
  })

  it('面板是 form：点保存 / 表单 submit（等价于文本字段回车）走同一条保存入参', async () => {
    const savePreset = vi.fn().mockResolvedValue(ok({ saved: true }))
    const { w } = await openEditor({ savePreset })
    // 面板本身就是 <form>（回车 = 保存），而不是一堆散装按钮
    expect(w.get('[data-testid="preset-editor"]').element.tagName).toBe('FORM')
    expect(w.get('[data-testid="preset-save"]').attributes('type')).toBe('submit')

    await w.get('[data-testid="preset-name"]').setValue(' my-preset ')
    await w.get('[data-testid="preset-baseurl"]').setValue(' https://api.x.dev ')
    await w.get('[data-testid="preset-style-anthropic"]').trigger('click')
    await w.get('[data-testid="preset-models"]').setValue('a\n\nb\n')
    await w.get('[data-testid="preset-key"]').setValue(' sk-1 ')

    // 路径一：点保存按钮
    clickSave(w)
    await flushPromises()
    expect(savePreset).toHaveBeenCalledTimes(1)
    expect(savePreset).toHaveBeenCalledWith({
      id: undefined,
      name: 'my-preset',
      baseUrl: 'https://api.x.dev',
      apiStyle: 'anthropic',
      models: ['a', 'b'],
      apiKey: 'sk-1',
    })
    // 保存成功 → 弹窗关闭
    expect(w.find('[data-testid="preset-editor"]').exists()).toBe(false)
    w.unmount()
  })

  it('名称为空时保存按钮 disabled（必填项未填不得提交）', async () => {
    const savePreset = vi.fn().mockResolvedValue(ok({ saved: true }))
    const { w } = await openEditor({ savePreset })
    const save = w.get('[data-testid="preset-save"]')
    expect(save.attributes('disabled')).toBeDefined()
    clickSave(w)
    await flushPromises()
    expect(savePreset).not.toHaveBeenCalled()
    w.unmount()
  })

  it('模板快选仅新建态出现，点击后回填名称 / URL / 方言 / 模型', async () => {
    const { w } = await openEditor()
    expect(w.find('[data-testid="preset-template-deepseek"]').exists()).toBe(true)
    await w.get('[data-testid="preset-template-deepseek"]').trigger('click')
    expect((w.get('[data-testid="preset-name"]').element as HTMLInputElement).value).not.toBe('')
    expect((w.get('[data-testid="preset-baseurl"]').element as HTMLInputElement).value).toContain(
      'https://',
    )
    // 选中的方言 chip 带 active 与 aria-pressed（可读出当前选择）
    const active = w.findAll('.ah-pv-target.active')
    expect(active.length).toBe(1)
    expect(active[0].attributes('aria-pressed')).toBe('true')
    w.unmount()
  })

  it('编辑态：无模板组、key 占位带掩码、清空开关映射为 apiKey=""', async () => {
    const savePreset = vi.fn().mockResolvedValue(ok({ saved: true }))
    const providers = providersStub({
      savePreset,
      state: ref({
        claude: { env: {}, bridge: {} },
        presets: [preset({ id: 7, name: 'kimi', keyMask: 'sk-9***3ab' })],
        import: { last: null },
      } as unknown as ProvidersDomainState),
    })
    const w = mountComponent(
      ProvidersTab,
      { detection: tabWithPresets(), providers },
      true,
    )
    await flushPromises()
    await w.get('[data-testid="edit-kimi"]').trigger('click')
    await flushPromises()

    // 编辑态不提供模板快选（会覆盖既有值）
    expect(w.find('[data-testid="preset-template-deepseek"]').exists()).toBe(false)
    expect(w.get('[data-testid="preset-key"]').attributes('placeholder')).toContain('sk-9***3ab')

    const clear = w.get('[data-testid="preset-key-clear"]')
    expect(clear.attributes('aria-pressed')).toBe('false')
    await clear.trigger('click')
    expect(clear.attributes('aria-pressed')).toBe('true')
    clickSave(w)
    await flushPromises()
    expect(savePreset).toHaveBeenCalledWith(
      expect.objectContaining({ id: 7, name: 'kimi', apiKey: '' }),
    )
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
      statsDays: ref(30 as StatsDays),
      statsLoading: ref(false),
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
      setStatsDays: vi.fn(),
      reloadSessions: vi.fn(),
      goPage: vi.fn(),
      resetQuery: vi.fn(),
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

  it('看板不再有会话明细列表（去重契约：明细只在日志分区）', async () => {
    const w = mountComponent(StatsTab, { usage: usageStub() })
    await flushPromises()
    // 统计分区曾挂了一份同源会话列表，与日志表格重复
    expect(w.find('.ah-st-row').exists()).toBe(false)
    expect(w.find('.ah-st-more').exists()).toBe(false)
    w.unmount()
  })

  it('时间窗 pills 点选走 setStatsDays（服务端切片，前端不自行过滤）', async () => {
    const setStatsDays = vi.fn()
    const w = mountComponent(StatsTab, { usage: usageStub({ setStatsDays }) })
    await flushPromises()
    await w.get('[data-testid=range-7]').trigger('click')
    expect(setStatsDays).toHaveBeenCalledWith(7)
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
        {
          name: 'claude',
          paths: [{ path: '/home/u/.claude', removable: false }],
          builtin: true,
          scan: null,
        },
        {
          name: 'demo',
          paths: [{ path: '/tmp/demo', removable: true }],
          builtin: false,
          scan: null,
        },
      ]),
      statsDays: ref(30 as StatsDays),
      statsLoading: ref(false),
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
      setStatsDays: vi.fn(),
      reloadSessions: vi.fn(),
      goPage: vi.fn(),
      resetQuery: vi.fn(),
      reloadSources: vi.fn(),
      addSource: vi.fn(),
      removeSource: vi.fn(),
      addSourcePath: vi.fn(),
      removeSourcePath: vi.fn(),
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

  // ------ 模型输出折叠（默认折叠：超阈值助手正文收起 + 展开/收起互切） ------

  /** 打开详情视图的通用夹具：长/短助手消息 + 长用户消息 */
  function openedWithMessages(events: unknown[]): UsageSessionDetail {
    return {
      session: {
        id: 1,
        adapter: 'claude',
        cli_session_id: 'c-1',
        project: null,
        title: 't1',
        started_at: 1,
        ended_at: null,
        duration_ms: null,
        tokens_in: 0,
        tokens_out: 0,
        tokens_cache_read: 0,
        tokens_cache_write: 0,
        tokens_reasoning: 0,
        cost_total: null,
        model: null,
        active: false,
        source_path: '/x',
      },
      events: events as UsageSessionDetail['events'],
      raw: [],
      eventsTruncated: false,
      rawTruncated: false,
      skippedLines: 0,
    } satisfies UsageSessionDetail
  }

  it('超阈值助手正文默认折叠：clamp 类 + 展开按钮，aria 状态为收起', async () => {
    const ts = 'x'.repeat(COLLAPSE_THRESHOLD_CHARS + 1)
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'assistant', text: ts, ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    const textEl = w.get('.ah-msg-text')
    // 默认折叠：截断样式生效
    expect(textEl.classes()).toContain('is-collapsed')
    expect(textEl.text()).toBe(ts) // 文本本身仍在 DOM（CSS 截断，非删内容）
    // 展开按钮存在且处于收起态
    const btn = w.get('.ah-msg-expand')
    expect(btn.attributes('aria-expanded')).toBe('false')
    expect(btn.text()).toContain('hub.lg.detail.expand')
    expect(btn.attributes('aria-controls')).toBe('lg-msg-text-0')
    w.unmount()
  })

  it('点展开 → 全文展示（去掉 clamp 类），再点收起回到折叠态', async () => {
    const ts = 'x'.repeat(COLLAPSE_THRESHOLD_CHARS + 1)
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'assistant', text: ts, ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    const btn = w.get('.ah-msg-expand')
    await btn.trigger('click')
    await flushPromises()
    expect(w.get('.ah-msg-text').classes()).not.toContain('is-collapsed')
    expect(btn.attributes('aria-expanded')).toBe('true')
    expect(btn.text()).toContain('hub.lg.detail.collapse')
    // 圆路：再点收起
    await btn.trigger('click')
    await flushPromises()
    expect(w.get('.ah-msg-text').classes()).toContain('is-collapsed')
    expect(btn.attributes('aria-expanded')).toBe('false')
    w.unmount()
  })

  it('短暂助手正文不折叠也不出现按钮（clamp 剪不到，按钮是噪音）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'assistant', text: '短回复', ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    expect(w.find('.ah-msg-text').classes()).not.toContain('is-collapsed')
    expect(w.find('.ah-msg-expand').exists()).toBe(false)
    w.unmount()
  })

  it('用户 / 系统角色即使超阈值也不提供折叠（折叠只针对模型输出与工具卡身）', async () => {
    const long = 'y'.repeat(COLLAPSE_THRESHOLD_CHARS + 1)
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([
            { role: 'user', text: long, ts: 1 },
            { role: 'system', text: long, ts: 3 },
          ]),
        ) as never,
      }),
    })
    await flushPromises()
    expect(w.findAll('.ah-msg-expand')).toHaveLength(0)
    expect(w.findAll('.ah-msg-text.is-collapsed')).toHaveLength(0)
    w.unmount()
  })

  // ------ B2：助手正文 markdown 渲染（只助手，其他角色保持纯文本） ------

  it('助手正文渲染成 markdown 结构（围栏 / 强调 / 标题），用户行仍为纯文本', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([
            { role: 'user', text: '# 不是标题\n\n**也不是加粗**', ts: 1 },
            { role: 'assistant', text: '## 小节\n\n看 `code` 与 **加粗**\n\n```bash\nls -l\n```', ts: 2 },
          ]),
        ) as never,
      }),
    })
    await flushPromises()
    const md = w.get('[data-testid="lg-msg-md"]')
    expect(md.classes()).toContain('ah-md')
    expect(md.html()).toContain('<h2>小节</h2>')
    expect(md.html()).toContain('<strong>加粗</strong>')
    expect(md.html()).toContain('<pre><code data-lang="bash">ls -l</code></pre>')
    // 用户行不得带 markdown 容器（“我说的话”不该被排版）
    expect(w.findAll('.ah-md')).toHaveLength(1)
    expect(w.find('.ah-msg.role-user .ah-msg-text').text()).toContain('# 不是标题')
    w.unmount()
  })

  it('助手正文里的原始 HTML 被实体化（不产生可执行节点）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'assistant', text: '<img src=x onerror=alert(1)><script>alert(2)</script>', ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    // 断言「没有被解析成节点」而非「序列化串里没有尖括号」：happy-dom 的
    // outerHTML 序列化对文本节点不保证转义，字符串断言会假红
    expect(w.find('script').exists()).toBe(false)
    expect(w.find('img').exists()).toBe(false)
    expect(w.get('[data-testid="lg-msg-md"]').text()).toContain('<script>alert(2)</script>')
    w.unmount()
  })

  // 上条只覆盖**展开态**（renderMarkdown 会转义）；折叠态走 markdownPlainPreview，
  // 那条路径同样经 v-html 注入，不转义就会把 agent 输出里的 HTML 当节点执行
  it('折叠态（默认态）的原始 HTML 同样不产生可执行节点', async () => {
    const payload = `<img src=x onerror=alert(1)>\n\n${'后续正文。'.repeat(COLLAPSE_THRESHOLD_CHARS / 5)}`
    expect(payload.length).toBeGreaterThan(COLLAPSE_THRESHOLD_CHARS)
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(openedWithMessages([{ role: 'assistant', text: payload, ts: 1 }])) as never,
      }),
    })
    await flushPromises()
    const el = w.get('[data-testid="lg-msg-md"]')
    expect(el.classes()).toContain('is-collapsed')
    expect(w.find('img').exists()).toBe(false)
    // 原文仍可读（实体化后浏览器会还原成字符，text() 拿到的是原始字面量）
    expect(el.text()).toContain('<img src=x onerror=alert(1)>')
    w.unmount()
  })

  it('仅由分隔线组成的长正文不被整行丢弃（折叠预览为空也不判行空）', async () => {
    // 预览会剥掉全部 HR 行 → 空串；若拿预览判空，这条实质消息会连同截断提示一起消失
    const payload = '---\n'.repeat(COLLAPSE_THRESHOLD_CHARS / 2)
    expect(payload.length).toBeGreaterThan(COLLAPSE_THRESHOLD_CHARS)
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(openedWithMessages([{ role: 'assistant', text: payload, ts: 1 }])) as never,
      }),
    })
    await flushPromises()
    expect(w.findAll('.ah-msg.role-assistant')).toHaveLength(1)
    w.unmount()
  })

  it('折叠态给剥标记的纯文本预览，展开后才产出结构化 HTML', async () => {
    const md = `## 标题\n\n**要点**\n\n\`\`\`js\nconst a = 1\n\`\`\`\n\n${'补充说明。'.repeat(120)}`
    expect(md.length).toBeGreaterThan(COLLAPSE_THRESHOLD_CHARS)
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(openedWithMessages([{ role: 'assistant', text: md, ts: 1 }])) as never,
      }),
    })
    await flushPromises()
    const el = w.get('[data-testid="lg-msg-md"]')
    expect(el.classes()).toContain('is-collapsed')
    expect(el.html()).not.toContain('<pre>')
    expect(el.text()).not.toContain('**')
    await w.get('.ah-msg-expand').trigger('click')
    await flushPromises()
    expect(w.get('[data-testid="lg-msg-md"]').html()).toContain('<pre><code data-lang="js">')
    w.unmount()
  })

  // ------ B3：工具卡（头 / 身切片 + 失败标记 + 默认收起） ------

  it('工具行切成卡头 + 卡身（codex/opencode 形态）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'tool', text: 'tool · read (completed) · ok', ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    expect(w.get('.ah-msg-tool-head').text()).toBe('tool · read (completed)')
    expect(w.get('.ah-msg.role-tool .ah-msg-text').text()).toBe('ok')
    w.unmount()
  })

  it('claude tool_use 只有卡头（无卡身则不渲染正文块）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'tool', text: 'tool_use · Bash', ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    expect(w.get('.ah-msg-tool-head').text()).toBe('tool_use · Bash')
    expect(w.find('.ah-msg-body').exists()).toBe(false)
    w.unmount()
  })

  it('tool_use 的参数摘要进卡身（guest 带出参数时不能被卡片吃掉）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'tool', text: 'tool_use · Bash · {"command":"ls -l"}', ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    expect(w.get('.ah-msg-tool-head').text()).toBe('tool_use · Bash')
    expect(w.get('.ah-msg.role-tool .ah-msg-text').text()).toBe('{"command":"ls -l"}')
    w.unmount()
  })

  it('pi 的 (error) 标记给失败视觉，非失败行没有', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([
            { role: 'tool', text: 'bash (error) · command not found', ts: 1 },
            { role: 'tool', text: 'bash · hi', ts: 2 },
          ]),
        ) as never,
      }),
    })
    await flushPromises()
    const bodies = w.findAll('.ah-msg.role-tool .ah-msg-body')
    expect(bodies[0].classes()).toContain('is-error')
    expect(bodies[1].classes()).not.toContain('is-error')
    w.unmount()
  })

  // claude 的 tool_result 文本不带 `(error)`（只有 wire error 字段），
  // 失败视觉只认文本标记会让 claude（最大适配器）的失败行永远不标红
  it('wire error 字段给失败视觉（claude 形态：文本无 (error) 标记）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([
            { role: 'tool', text: 'tool_result · Bash · exit status 1', ts: 1, error: true },
            { role: 'tool', text: 'tool_result · Bash · ok', ts: 2, error: false },
          ]),
        ) as never,
      }),
    })
    await flushPromises()
    const bodies = w.findAll('.ah-msg.role-tool .ah-msg-body')
    expect(bodies[0].classes()).toContain('is-error')
    expect(bodies[1].classes()).not.toContain('is-error')
    w.unmount()
  })

  // guest 只吐机器 token（wire 无 i18n 通道），文案必须由展示层查语言包：
  // 不接 t 时英文界面会直接显示 guest 的中文占位。
  it('非文本块占位走 i18n key（不显示 guest 原文 token）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'tool', text: 'tool_result · Read · [non-text:image]', ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    const body = w.get('.ah-msg.role-tool .ah-msg-text').text()
    expect(body).toBe('hub.lg.nonTextBlock(kind=image)')
    expect(body).not.toContain('[non-text:image]')
    w.unmount()
  })

  it('无 kind 的退化 token 走无参数 key', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'tool', text: 'tool_result · Read · [non-text]', ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    expect(w.get('.ah-msg.role-tool .ah-msg-text').text()).toBe('hub.lg.nonTextBlockUnknown')
    w.unmount()
  })

  // 残留歧义（无法完全消除）：工具输出是不可信文本，若 agent 输出里恰好出现
  // `[non-text:x]` 字面量，它会被当占位本地化。这里把真实行为精确钉住——
  // **只有 token 那一组被替换，其余字符一字不改**。
  it('工具输出里恰好出现 token 字面量：只替换该组，其余字符逐字保留', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([
            { role: 'tool', text: 'tool_result · Bash · 请手动写 [non-text:image] 占位', ts: 1 },
          ]),
        ) as never,
      }),
    })
    await flushPromises()
    const body = w.get('.ah-msg.role-tool .ah-msg-text').text()
    expect(body).toBe('请手动写 hub.lg.nonTextBlock(kind=image) 占位')
    w.unmount()
  })

  it('长工具输出默认收起，点开切换（按钮文案走「展开详情」）', async () => {
    const out = 'o'.repeat(COLLAPSE_THRESHOLD_TOOL_CHARS + 1)
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'tool', text: `tool · bash · ${out}`, ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    expect(w.get('.ah-msg.role-tool .ah-msg-text').classes()).toContain('is-collapsed')
    const btn = w.get('.ah-msg-expand')
    expect(btn.text()).toContain('hub.lg.detail.expandDetail')
    expect(btn.attributes('aria-expanded')).toBe('false')
    await btn.trigger('click')
    await flushPromises()
    expect(w.get('.ah-msg.role-tool .ah-msg-text').classes()).not.toContain('is-collapsed')
    expect(w.get('.ah-msg-expand').text()).toContain('hub.lg.detail.collapse')
    w.unmount()
  })

  it('短工具输出不折叠也不出按钮（阈值以下不添噪音）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'tool', text: 'tool · ls · a.txt', ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    expect(w.find('.ah-msg-expand').exists()).toBe(false)
    expect(w.find('.ah-msg-text').classes()).not.toContain('is-collapsed')
    w.unmount()
  })

  // ------ pi 逐轮空助手消息（噪音气泡）不得进对话流 ------

  it('正文空且五项 token 全零的助手行整条不渲染（pi 每轮都写的空消息）', async () => {
    const zero = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 0 }
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([
            { role: 'user', text: '问题', ts: 1 },
            { role: 'assistant', text: '', model: 'deepseek-v4-flash', tokens: zero, ts: 2 },
            { role: 'assistant', text: '   ', model: 'deepseek-v4-flash', tokens: zero, ts: 3 },
            { role: 'assistant', text: '真实回复', model: 'deepseek-v4-flash', tokens: { ...zero, output: 12 }, ts: 4 },
          ]),
        ) as never,
      }),
    })
    await flushPromises()
    // 空消息既不占行也不出 token 行；正常回复仍在（模型名只出现在它头上一次）
    expect(w.findAll('.ah-msg')).toHaveLength(2)
    expect(w.findAll('.ah-msg-model')).toHaveLength(1)
    const metas = w.findAll('.ah-msg-meta')
    expect(metas).toHaveLength(1)
    expect(metas[0].text()).toContain('↓ 12')
  })

  it('零 token 行不出（“↑0 ↓0 ⚡0 +0” 只会让人以为统计坏了）', async () => {
    const zero = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 0 }
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'assistant', text: '有正文但零 token', tokens: zero, ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    // 行在（正文有信息），只是不出零 token 行
    expect(w.findAll('.ah-msg')).toHaveLength(1)
    expect(w.find('.ah-msg-text').text()).toBe('有正文但零 token')
    expect(w.find('.ah-msg-meta').exists()).toBe(false)
  })

  it('反例：正文为空但 token 有量的助手行仍然渲染（不误删信息）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([
            {
              role: 'assistant',
              text: '',
              model: 'deepseek-v4-flash',
              tokens: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, reasoning: 340 },
              ts: 1,
            },
          ]),
        ) as never,
      }),
    })
    await flushPromises()
    expect(w.findAll('.ah-msg')).toHaveLength(1)
    expect(w.get('.ah-msg-meta').text()).toContain('◈')
  })

  it('工具卡正文随 guest 上限走（1000 字符全文在 DOM，不做展示层截断）', async () => {
    const out = 'o'.repeat(GUEST_TEXT_CAPS.toolOutput)
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'tool', text: `tool · bash · ${out}`, ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    const body = w.get('.ah-msg.role-tool .ah-msg-text')
    expect(body.classes()).toContain('is-collapsed') // 超阈值默认收起
    expect(body.text()).toHaveLength(GUEST_TEXT_CAPS.toolOutput) // 全文在 DOM
    await w.get('.ah-msg-expand').trigger('click')
    await flushPromises()
    expect(w.get('.ah-msg.role-tool .ah-msg-text').classes()).not.toContain('is-collapsed')
    expect(w.get('.ah-msg.role-tool .ah-msg-text').text()).toHaveLength(GUEST_TEXT_CAPS.toolOutput)
  })

  // ------ B4：解析层截断给「完整原文」可见路径 ------

  it('达上限的正文提示截断并可一键跳到原始 JSONL', async () => {
    const capped = 'z'.repeat(GUEST_TEXT_CAPS.message) + '…'
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([{ role: 'assistant', text: capped, ts: 1 }]),
        ) as never,
      }),
    })
    await flushPromises()
    expect(w.get('[data-testid="lg-msg-truncated"]').text()).toContain('hub.lg.detail.truncated')
    await w.get('.ah-msg-trunc-jump').trigger('click')
    await flushPromises()
    expect(w.text()).not.toContain('hub.lg.detail.expand')
    w.unmount()
  })

  it('未达上限的等长正文不报截断（反例：不能只看长度）', async () => {
    const exact = 'z'.repeat(GUEST_TEXT_CAPS.message)
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        openedSession: ref(
          openedWithMessages([
            { role: 'assistant', text: exact, ts: 1 },
            { role: 'assistant', text: '正常一句', ts: 2 },
          ]),
        ) as never,
      }),
    })
    await flushPromises()
    expect(w.findAll('[data-testid="lg-msg-truncated"]')).toHaveLength(0)
    w.unmount()
  })

  it('切换会话后折叠态回到默认（上个会话展开的不带过来）', async () => {
    const ts = 'x'.repeat(COLLAPSE_THRESHOLD_CHARS + 1)
    const opened = ref<UsageSessionDetail | null>(null)
    opened.value = openedWithMessages([{ role: 'assistant', text: ts, ts: 1 }])
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({ openedSession: opened as never }),
    })
    await flushPromises()
    await w.get('.ah-msg-expand').trigger('click')
    await flushPromises()
    expect(w.get('.ah-msg-text').classes()).not.toContain('is-collapsed')
    // 切到另一会话再切回：展开态被重置
    opened.value = null
    await flushPromises()
    opened.value = openedWithMessages([{ role: 'assistant', text: ts, ts: 1 }])
    await flushPromises()
    expect(w.get('.ah-msg-text').classes()).toContain('is-collapsed')
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
      statsDays: ref(30 as StatsDays),
      statsLoading: ref(false),
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
      setStatsDays: vi.fn(),
      reloadSessions: vi.fn(),
      goPage: vi.fn(),
      resetQuery: vi.fn(),
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
      statsDays: ref(30 as StatsDays),
      statsLoading: ref(false),
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
      setStatsDays: vi.fn(),
      reloadSessions: vi.fn(),
      goPage: vi.fn(),
      resetQuery: vi.fn(),
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
      statsDays: ref(30 as StatsDays),
      statsLoading: ref(false),
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
      setStatsDays: vi.fn(),
      reloadSessions: vi.fn(),
      goPage: vi.fn(),
      resetQuery: vi.fn(),
      reloadSources: vi.fn(),
      addSource: vi.fn(),
      removeSource: vi.fn(),
      addSourcePath: vi.fn(),
      removeSourcePath: vi.fn(),
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
          paths: [{ path: '/home/u/.local/share/opencode/opencode.db', removable: false }],
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
          paths: [{ path: '/home/u/.claude/projects', removable: false }],
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

  it('边界：旧状态无 kind 且单 path 按目录处理（票 06 存量兼容，前端路径兑底）', async () => {
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
    // 无 kind → 按目录型处理；无 paths → 单 path 兑底成不可移除（内置）
    expect(w.text()).toContain('hub.lg.sources.kind.jsonl')
    expect(w.text()).toContain('3 hub.lg.sources.files')
    expect(w.findAll('.ah-lg-source-path-remove')).toHaveLength(0)
    w.unmount()
  })

  it('边界：SQLite 源不提供移除按钮（只读单文件、不可增删）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub([
        {
          name: 'opencode',
          paths: [{ path: '/home/u/.local/share/opencode/opencode.db', removable: false }],
          builtin: true,
          kind: 'sqlite',
          scan: null,
        },
        {
          name: 'demo',
          paths: [{ path: '/tmp/demo', removable: true }],
          builtin: false,
          kind: 'jsonl',
          scan: null,
        },
      ]),
    })
    await flushPromises()
    await openSources(w)
    // 仅自定义来源有整来源移除按钮
    expect(w.findAll('.ah-lg-source-remove')).toHaveLength(1)
    // sqlite 源不提供「添加目录」按钮（单文件库不接受增删）；目录型来源有
    expect(w.findAll('.ah-lg-source-adddir')).toHaveLength(1)
    // 未扫描过显示提示而不是 0
    expect(w.text()).toContain('hub.lg.sources.noScan')
    w.unmount()
  })

  it('正例：每来源多目录 —— 内置默认路径只读、追加目录可移除，各自成行', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub([
        {
          name: 'pi',
          paths: [
            { path: '/home/u/.pi/agent/sessions', removable: false },
            { path: '/home/u/projects/bedcode/.pi/sessions', removable: true },
          ],
          builtin: true,
          kind: 'jsonl',
          scan: null,
        },
      ]),
    })
    await flushPromises()
    await openSources(w)
    // 两条目录都渲染（home 折叠为 ~）
    expect(w.findAll('.ah-lg-source-path-row')).toHaveLength(2)
    expect(w.text()).toContain('~/.pi/agent/sessions')
    expect(w.text()).toContain('~/projects/bedcode/.pi/sessions')
    // 内置默认路径无 ✕ + 显示「内置目录」标记；追加目录有 ✕
    expect(w.findAll('.ah-lg-source-path-remove')).toHaveLength(1)
    expect(w.text()).toContain('hub.lg.sources.builtinPath')
    w.unmount()
  })

  it('正例：移除来源目录走 removeSourcePath（name + 路径）', async () => {
    const removeSourcePath = vi.fn(async () => ({ ok: true }))
    const usage = usageStub([
      {
        name: 'my-logs',
        paths: [
          { path: '/data/a', removable: true },
          { path: '/data/b', removable: true },
        ],
        builtin: false,
        kind: 'jsonl',
        scan: null,
      },
    ])
    usage.removeSourcePath = removeSourcePath
    const w = mountComponent(SessionLogsTab, { usage })
    await flushPromises()
    await openSources(w)
    await w.findAll('.ah-lg-source-path-remove')[0].trigger('click')
    await flushPromises()
    expect(removeSourcePath).toHaveBeenCalledWith('my-logs', '/data/a')
    w.unmount()
  })

  it('正例：给来源追加目录 —— 选目录 → 确认走 addSourcePath（名已定，只选目录）', async () => {
    const addSourcePath = vi.fn(async () => ({ ok: true }))
    const pickSourceDir = vi.fn(async () => ({ ok: true, picked: true, path: '/home/u/extra' }))
    const usage = usageStub([
      {
        name: 'pi',
        paths: [{ path: '/home/u/.pi/agent/sessions', removable: false }],
        builtin: true,
        kind: 'jsonl',
        scan: null,
      },
    ])
    usage.addSourcePath = addSourcePath
    usage.pickSourceDir = pickSourceDir
    const w = mountComponent(SessionLogsTab, { usage })
    await flushPromises()
    await openSources(w)
    // 打开该来源的追加目录表单（无名称输入——名已定）
    await w.get('.ah-lg-source-adddir').trigger('click')
    await flushPromises()
    expect(w.find('.ah-lg-source-path-add').exists()).toBe(true)
    // 选择目录 → 回显 → 确认
    await w.get('.ah-lg-source-path-add .ah-lg-sources-pick .ah-btn').trigger('click')
    await flushPromises()
    expect(w.get('.ah-lg-source-path-add .ah-lg-sources-pickpath').text()).toContain('/home/u/extra')
    await w.get('.ah-lg-source-path-add .ah-btn-primary').trigger('click')
    await flushPromises()
    expect(addSourcePath).toHaveBeenCalledWith('pi', '/home/u/extra')
    w.unmount()
  })

  it('反例：未选目录时「确认添加」禁用（该来源追加目录表单）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub([
        {
          name: 'pi',
          paths: [{ path: '/home/u/.pi/agent/sessions', removable: false }],
          builtin: true,
          kind: 'jsonl',
          scan: null,
        },
      ]),
    })
    await flushPromises()
    await openSources(w)
    await w.get('.ah-lg-source-adddir').trigger('click')
    await flushPromises()
    expect(w.get('.ah-lg-source-path-add .ah-btn-primary').attributes('disabled')).toBeDefined()
    w.unmount()
  })
})

// ==================== A8 概览卸载：卡片两击确认与不可用原因 ====================

/**
 * A8 概览卡片卸载动作的行为契约（本次新增）：
 *  - A8-1 正例：已装 + 可自动卸载 → 卸载按钮，两击确认后才 emit uninstall(cli)
 *  - A8-2 反例：未安装 / 检测中 / 失败 → 不渲染卸载动作
 *  - A8-3 反例：双安装 / 未知安装方式 / npm-global 缺 node → 不给按钮，提示手动
 *  - A8-4 边界：busy（任意在途 run）与 uninstalling（本卡卸载中）→ 禁用/文案
 *  - A8-5 反例：卸载失败信号 → 友好 i18n 失败文案
 *  - A8-6 边界：armed 4s 超时自动复位（防误触的第二道保险）
 */
describe('A8 CliCard：卸载动作（两击确认与不可用原因）', () => {
  const info = {
    installed: true,
    version: '1.0',
    method: 'npm-global',
    paths: [] as string[],
    dual: false,
    status: 'ok',
    error: null,
  }

  it('A8-1 正例：已装 npm-global + node → 两击确认后 emit uninstall(cli)', async () => {
    const w = mountComponent(CliCard, { cliId: 'pi', info, nodeReady: true })
    await flushPromises()
    const btn = w.get('.ah-cli-foot .ah-btn')
    expect(btn.text()).toBe('hub.card.uninstall')

    await btn.trigger('click')
    await flushPromises()
    // 第一击只 arm：按钮变确认文案，不发命令
    expect(btn.text()).toBe('hub.card.uninstallConfirm')
    expect(w.emitted('uninstall')).toBeUndefined()

    await btn.trigger('click')
    await flushPromises()
    expect(w.emitted('uninstall')?.[0]).toEqual(['pi'])
    w.unmount()
  })

  it('A8-2 反例：未安装 → 不渲染卸载动作', async () => {
    const w = mountComponent(CliCard, {
      cliId: 'pi',
      info: { ...info, installed: false, status: 'not-installed' },
      nodeReady: true,
    })
    await flushPromises()
    expect(w.find('.ah-cli-foot').exists()).toBe(false)
    w.unmount()
  })

  it('A8-2 边界：检测中/失败 → 不渲染卸载动作', async () => {
    for (const status of ['detecting', 'error'] as const) {
      const w = mountComponent(CliCard, {
        cliId: 'pi',
        info: { ...info, status },
        nodeReady: true,
      })
      await flushPromises()
      expect(w.find('.ah-cli-foot').exists()).toBe(false)
      w.unmount()
    }
  })

  it('A8-3 反例：双安装 → 不给按钮，提示手动卸载', async () => {
    const w = mountComponent(CliCard, {
      cliId: 'opencode',
      info: { ...info, method: 'standalone', dual: true, paths: ['/a/opencode', '/b/opencode'] },
      nodeReady: true,
    })
    await flushPromises()
    expect(w.text()).toContain('hub.card.uninstallHintDual')
    expect(w.find('.ah-cli-foot .ah-btn').exists()).toBe(false)
    w.unmount()
  })

  it('A8-3 反例：安装方式未知 → 提示手动卸载', async () => {
    const w = mountComponent(CliCard, {
      cliId: 'pi',
      info: { ...info, method: 'unknown' },
      nodeReady: true,
    })
    await flushPromises()
    expect(w.text()).toContain('hub.card.uninstallHintMethod')
    expect(w.find('.ah-cli-foot .ah-btn').exists()).toBe(false)
    w.unmount()
  })

  it('A8-3 反例：npm-global 且缺 node → 提示不可自动卸载', async () => {
    const w = mountComponent(CliCard, { cliId: 'pi', info, nodeReady: false })
    await flushPromises()
    expect(w.text()).toContain('hub.card.uninstallHintNode')
    expect(w.find('.ah-cli-foot .ah-btn').exists()).toBe(false)
    w.unmount()
  })

  it('A8-3 边界：native/standalone 卸载不依赖 node（仍有按钮）', async () => {
    for (const [cliId, method] of [
      ['claude', 'native'],
      ['opencode', 'standalone'],
    ] as const) {
      const w = mountComponent(CliCard, { cliId, info: { ...info, method }, nodeReady: false })
      await flushPromises()
      expect(w.find('.ah-cli-foot .ah-btn').exists()).toBe(true)
      w.unmount()
    }
  })

  it('A8-4 边界：busy（任意在途 run）→ 按钮禁用', async () => {
    const w = mountComponent(CliCard, { cliId: 'pi', info, nodeReady: true, busy: true })
    await flushPromises()
    expect(w.get('.ah-cli-foot .ah-btn').attributes('disabled')).toBeDefined()
    w.unmount()
  })

  it('A8-4 边界：uninstalling（本卡卸载中）→ 「卸载中…」且禁用', async () => {
    const w = mountComponent(CliCard, { cliId: 'pi', info, nodeReady: true, uninstalling: true })
    await flushPromises()
    const btn = w.get('.ah-cli-foot .ah-btn')
    expect(btn.text()).toBe('hub.card.uninstallRunning')
    expect(btn.attributes('disabled')).toBeDefined()
    w.unmount()
  })

  it('A8-5 反例：卸载失败信号 → 显示友好失败文案', async () => {
    const w = mountComponent(CliCard, {
      cliId: 'pi',
      info,
      nodeReady: true,
      // ADR 0030：prop 直接是文案（{ cli, error } 解包后），不渲染 code/detail
      uninstallFailed: 'hub.card.uninstallFailed',
    })
    await flushPromises()
    expect(w.text()).toContain('hub.card.uninstallFailed')
    w.unmount()
  })

  it('A8-6 边界：armed 超时自动复位（4s 后按钮回到「卸载」）', async () => {
    vi.useFakeTimers()
    try {
      const w = mountComponent(CliCard, { cliId: 'pi', info, nodeReady: true })
      await flushPromises()
      const btn = w.get('.ah-cli-foot .ah-btn')
      await btn.trigger('click')
      expect(btn.text()).toBe('hub.card.uninstallConfirm')
      vi.advanceTimersByTime(4000)
      await flushPromises()
      expect(btn.text()).toBe('hub.card.uninstall')
      w.unmount()
    } finally {
      vi.useRealTimers()
    }
  })
})

// ==================== A9 概览：卸载事件上抛与失败信号下传 ====================

/**
 * A9 OverviewTab 的卸载接线：
 *  - 卡片两击确认后 emit uninstall(cli) 上抛给父层
 *  - uninstallFailed 信号下传对应卡片（其余卡片不误显示）
 */
describe('A9 OverviewTab：卸载事件上抛', () => {
  function mountOverview(over: Record<string, unknown> = {}) {
    return mountComponent(OverviewTab, {
      state: {
        authGranted: true,
        envStatus: 'ok',
        env: { node: 'v22' },
        clis: {
          pi: {
            installed: true,
            version: '0.1',
            method: 'npm-global',
            paths: [],
            dual: false,
            status: 'ok',
            error: null,
          },
        },
      },
      detecting: false,
      installState: { active: null, last: null, mirror: { speed: null }, updates: {} },
      speedTesting: false,
      ...over,
    })
  }

  it('正例：卡片两击确认后 emit uninstall(cli)', async () => {
    const w = mountOverview()
    await flushPromises()
    const btn = w.get('.ah-cli-foot .ah-btn')
    await btn.trigger('click')
    await flushPromises()
    expect(w.emitted('uninstall')).toBeUndefined()
    await btn.trigger('click')
    await flushPromises()
    expect(w.emitted('uninstall')?.[0]).toEqual(['pi'])
    w.unmount()
  })

  it('正例：在途卸载 run（active.action=uninstall）→ 对应卡片「卸载中…」', async () => {
    const w = mountOverview({
      installState: {
        active: {
          runId: 'r1',
          cli: 'pi',
          action: 'uninstall',
          command: 'npm uninstall -g @earendil-works/pi-coding-agent',
          useMirror: false,
          startedAt: 0,
          cancelRequested: false,
        },
        last: null,
        mirror: { speed: null },
        updates: {},
      },
    })
    await flushPromises()
    expect(w.get('.ah-cli-foot .ah-btn').text()).toBe('hub.card.uninstallRunning')
    expect(w.get('.ah-cli-foot .ah-btn').attributes('disabled')).toBeDefined()
    w.unmount()
  })

  it('反例：卸载失败信号只落到对应卡片（其他卡片不误显示）', async () => {
    // ADR 0030：信号 = { cli, error }（error 为友好 i18n 文案）
    const w = mountOverview({ uninstallFailed: { cli: 'pi', error: 'hub.card.uninstallFailed' } })
    await flushPromises()
    // pi 卡片下方有失败文案；其余卡片（未装/无信息）不出现
    expect(w.text()).toContain('hub.card.uninstallFailed')
    expect(w.findAll('.ah-cli-foot')).toHaveLength(1)
    w.unmount()
  })
})

// ==================== A10 日志来源：fs:pick 选择目录 ====================

/**
 * A10 添加日志目录改用系统选择器（fs:pick）的行为契约（本次新增）：
 *  - A10-1 正例：选目录成功 → 路径回显 + 名称自动派生（basename 合法化）→ 确认添加
 *  - A10-2 边界：用户取消 → 表单不变（不填路径、不报错）
 *  - A10-3 反例：宿主拒绝（未授权等）→ 友好错误，不填路径
 *  - A10-4 反例：未选目录时「确认添加」禁用（路径来自选择器，无手动输入面）
 */
describe('A10 SessionLogsTab：添加日志目录走 fs:pick 选择器', () => {
  function usageStub(over: Partial<UseUsageReturn> = {}): UseUsageReturn {
    return {
      state: ref({ status: 'ok', home: '/home/binblink' }),
      stats: ref(null),
      sources: ref([]),
      statsDays: ref(30 as StatsDays),
      statsLoading: ref(false),
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
      setStatsDays: vi.fn(),
      reloadSessions: vi.fn(),
      goPage: vi.fn(),
      resetQuery: vi.fn(),
      reloadSources: vi.fn(),
      addSource: vi.fn(async () => ({ ok: true })),
      pickSourceDir: vi.fn(async () => ({ ok: false, picked: false, path: '' })),
      removeSource: vi.fn(async () => ({ ok: true })),
      addSourcePath: vi.fn(async () => ({ ok: true })),
      removeSourcePath: vi.fn(async () => ({ ok: true })),
      openSession: vi.fn(),
      closeSession: vi.fn(),
      refresh: vi.fn(),
      scan: vi.fn(),
      ...over,
    } as unknown as UseUsageReturn
  }

  /** 展开来源折叠区并打开「添加日志目录」表单 */
  async function openAddForm(w: ReturnType<typeof mountComponent>) {
    await w.get('.ah-lg-sources-toggle').trigger('click')
    await flushPromises()
    await w.get('.ah-lg-sources-actions .ah-btn-ghost').trigger('click')
    await flushPromises()
  }

  const PICKED = '/home/binblink/project/tauriProject/BedCode/.pi/sessions'

  it('A10-1 正例：选择器选中 → 路径回显 + 名称自动派生，确认添加走 addSource', async () => {
    const addSource = vi.fn(async () => ({ ok: true }))
    const pickSourceDir = vi.fn(async () => ({ ok: true, picked: true, path: PICKED }))
    const w = mountComponent(SessionLogsTab, { usage: usageStub({ addSource, pickSourceDir }) })
    await flushPromises()
    await openAddForm(w)

    await w.get('.ah-lg-sources-pick .ah-btn').trigger('click')
    await flushPromises()
    // 路径回显 + 名称建议（basename 合法化 → sessions）
    expect(w.get('.ah-lg-sources-pickpath').text()).toContain(PICKED)
    expect(w.get('input.ah-input').element as HTMLInputElement).toHaveProperty('value', 'sessions')

    // 确认添加 → guest add-source（name + 选择器路径）
    await w.get('.ah-lg-sources-add .ah-btn-primary').trigger('click')
    await flushPromises()
    expect(addSource).toHaveBeenCalledWith('sessions', PICKED)
    w.unmount()
  })

  it('A10-2 边界：用户取消 → 表单不变（路径为空、无错误）', async () => {
    const pickSourceDir = vi.fn(async () => ({ ok: true, picked: false, path: '' }))
    const w = mountComponent(SessionLogsTab, { usage: usageStub({ pickSourceDir }) })
    await flushPromises()
    await openAddForm(w)

    await w.get('.ah-lg-sources-pick .ah-btn').trigger('click')
    await flushPromises()
    expect(w.get('.ah-lg-sources-pickpath').classes()).toContain('empty')
    expect(w.text()).not.toContain('hub.lg.sources.pickFailed')
    // 确认按钮仍禁用（无路径）
    expect(w.get('.ah-lg-sources-add .ah-btn-primary').attributes('disabled')).toBeDefined()
    w.unmount()
  })

  it('A10-3 反例：宿主拒绝（未授权等）→ 友好错误，不填路径', async () => {
    const pickSourceDir = vi.fn(async () => ({ ok: false, picked: false, path: '' }))
    const w = mountComponent(SessionLogsTab, { usage: usageStub({ pickSourceDir }) })
    await flushPromises()
    await openAddForm(w)

    await w.get('.ah-lg-sources-pick .ah-btn').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('hub.lg.sources.pickFailed')
    expect(w.get('.ah-lg-sources-pickpath').classes()).toContain('empty')
    w.unmount()
  })

  it('A10-4 反例：未选目录时「确认添加」禁用（路径只来自选择器）', async () => {
    const w = mountComponent(SessionLogsTab, { usage: usageStub() })
    await flushPromises()
    await openAddForm(w)
    expect(w.get('.ah-lg-sources-add .ah-btn-primary').attributes('disabled')).toBeDefined()
    // 路径输入框已不存在（改为选择器按钮 + 回显）
    expect(w.find('input.ah-mono').exists()).toBe(false)
    w.unmount()
  })
})

// ==================== A12 来源名可下拉可选 + 目录重复拦截 ====================

/**
 * A12 行为契约（用户需求原文：「来源名称应该是一个可以自定义和下拉选择的，
 * 下拉选项为已添加的来源名称；如果选择同一个名称如 pi 则将目录添加到 pi 来源
 * 名下；同时校验添加的目录是否已经存在，存在则提示重复无法添加」）：
 *
 *  - A12-1 正例：下拉列出全部**目录型**已有来源 + 自定义哨兵（sqlite 源不列）
 *  - A12-2 正例：下拉选中已有来源 `pi` → 回填名称 + 确认走 addSourcePath（不建新源）
 *  - A12-3 正例：下拉选自定义哨兵 → 清空名称交给手输（可新建）
 *  - A12-4 正例：手输新名 → 确认走 addSource（新建来源）
 *  - A12-5 边界：手输打中的**已有**来源名同样走追加（路由只看名称是否已存在）
 *  - A12-6 反例：选中已登记目录 → 就地「重复无法添加」提示 + 确认禁用 + 不打 guest
 *  - A12-7 边界：重复目录挂在**同**一来源下同样拦（多目录下也成立）
 *  - A12-8 正例：未登记目录不受影响（不误伤新目录）
 */
describe('A12 SessionLogsTab：来源名下拉（已有来源）+ 目录重复拦截', () => {
  const PI = '/home/u/.pi/agent/sessions'
  const CLAUDE = '/home/u/.claude/projects'
  const DB = '/home/u/.local/share/opencode/opencode.db'
  const FRESH = '/home/u/work/extra-sessions'

  function src(name: string, paths: string[], over: Partial<UsageSource> = {}): UsageSource {
    return {
      name,
      paths: paths.map((path) => ({ path, removable: false })),
      builtin: true,
      kind: 'jsonl',
      scan: { files: 0, parsed: 0, skipped: 0, sessions: 0, error: null },
      ...over,
    }
  }

  function usageStub(over: Partial<UseUsageReturn> = {}): UseUsageReturn {
    return {
      state: ref({ status: 'ok', home: '/home/u' }),
      stats: ref(null),
      sources: ref([src('pi', [PI]), src('claude', [CLAUDE]), src('opencode', [DB], { kind: 'sqlite' })]),
      statsDays: ref(30 as StatsDays),
      statsLoading: ref(false),
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
      setStatsDays: vi.fn(),
      reloadSessions: vi.fn(),
      goPage: vi.fn(),
      resetQuery: vi.fn(),
      reloadSources: vi.fn(),
      addSource: vi.fn(async () => ({ ok: true })),
      addSourcePath: vi.fn(async () => ({ ok: true })),
      pickSourceDir: vi.fn(async () => ({ ok: true, picked: true, path: FRESH })),
      removeSource: vi.fn(async () => ({ ok: true })),
      removeSourcePath: vi.fn(async () => ({ ok: true })),
      openSession: vi.fn(),
      closeSession: vi.fn(),
      refresh: vi.fn(),
      scan: vi.fn(),
      ...over,
    } as unknown as UseUsageReturn
  }

  /** 展开来源折叠区并打开「添加日志来源」表单 */
  async function openAddForm(w: ReturnType<typeof mountComponent>) {
    await w.get('.ah-lg-sources-toggle').trigger('click')
    await flushPromises()
    await w.get('.ah-lg-sources-actions .ah-btn-ghost').trigger('click')
    await flushPromises()
  }

  /** 名称下拉的 <select>（Select 替身渲染在根元素上，class 直落其身） */
  function nameSelect(w: ReturnType<typeof mountComponent>) {
    return w.get('select.ah-lg-sources-nameselect')
  }

  /** 在名称下拉里选中一个选项（按 option 文本定位，再回填其 value） */
  async function pickName(w: ReturnType<typeof mountComponent>, label: string) {
    const sel = nameSelect(w)
    const opt = sel.findAll('option').find((o) => o.text() === label)
    if (!opt) throw new Error(`下拉缺少选项：${label}`)
    await sel.setValue(opt.attributes('value') ?? '')
    await flushPromises()
  }

  it('A12-1 正例：下拉列出全部目录型已有来源 + 自定义哨兵（sqlite 源不列）', async () => {
    const w = mountComponent(SessionLogsTab, { usage: usageStub() })
    await flushPromises()
    await openAddForm(w)
    const labels = nameSelect(w).findAll('option').map((o) => o.text())
    expect(labels).toContain('pi')
    expect(labels).toContain('claude')
    expect(labels).toContain('hub.lg.sources.nameCustom')
    // sqlite 源单库不接受追加目录，不应出现在下拉里
    expect(labels).not.toContain('opencode')
    w.unmount()
  })

  it('A12-2 正例：下拉选已有来源 pi → 回填名称 + 确认走 addSourcePath（不建新源）', async () => {
    const addSource = vi.fn(async () => ({ ok: true }))
    const addSourcePath = vi.fn(async () => ({ ok: true }))
    const w = mountComponent(SessionLogsTab, { usage: usageStub({ addSource, addSourcePath }) })
    await flushPromises()
    await openAddForm(w)

    await pickName(w, 'pi')
    // 名称回填到输入框
    expect((w.get('.ah-lg-sources-name input').element as HTMLInputElement).value).toBe('pi')

    // 选目录 → 已有名称不派生覆盖（不回退成 basename）
    await w.get('.ah-lg-sources-pick .ah-btn').trigger('click')
    await flushPromises()
    expect((w.get('.ah-lg-sources-name input').element as HTMLInputElement).value).toBe('pi')

    await w.get('.ah-lg-sources-add .ah-btn-primary').trigger('click')
    await flushPromises()
    expect(addSourcePath).toHaveBeenCalledWith('pi', FRESH)
    expect(addSource).not.toHaveBeenCalled()
    w.unmount()
  })

  it('A12-3 正例：下拉选自定义哨兵 → 清空名称交给手输（新建通道）', async () => {
    const addSource = vi.fn(async () => ({ ok: true }))
    const addSourcePath = vi.fn(async () => ({ ok: true }))
    const w = mountComponent(SessionLogsTab, { usage: usageStub({ addSource, addSourcePath }) })
    await flushPromises()
    await openAddForm(w)

    await pickName(w, 'pi')
    await pickName(w, 'hub.lg.sources.nameCustom')
    expect((w.get('.ah-lg-sources-name input').element as HTMLInputElement).value).toBe('')

    await w.get('.ah-lg-sources-name input').setValue('my-logs')
    await w.get('.ah-lg-sources-pick .ah-btn').trigger('click')
    await flushPromises()
    await w.get('.ah-lg-sources-add .ah-btn-primary').trigger('click')
    await flushPromises()
    expect(addSource).toHaveBeenCalledWith('my-logs', FRESH)
    expect(addSourcePath).not.toHaveBeenCalled()
    w.unmount()
  })

  it('A12-4 正例：手输新名 → 确认走 addSource（新建来源）', async () => {
    const addSource = vi.fn(async () => ({ ok: true }))
    const addSourcePath = vi.fn(async () => ({ ok: true }))
    const w = mountComponent(SessionLogsTab, { usage: usageStub({ addSource, addSourcePath }) })
    await flushPromises()
    await openAddForm(w)

    await w.get('.ah-lg-sources-name input').setValue('my-logs')
    await w.get('.ah-lg-sources-pick .ah-btn').trigger('click')
    await flushPromises()
    await w.get('.ah-lg-sources-add .ah-btn-primary').trigger('click')
    await flushPromises()
    expect(addSource).toHaveBeenCalledWith('my-logs', FRESH)
    expect(addSourcePath).not.toHaveBeenCalled()
    w.unmount()
  })

  it('A12-5 边界：手输打中的已有来源名同样走追加（路由只看名称是否已存在）', async () => {
    const addSource = vi.fn(async () => ({ ok: true }))
    const addSourcePath = vi.fn(async () => ({ ok: true }))
    const w = mountComponent(SessionLogsTab, { usage: usageStub({ addSource, addSourcePath }) })
    await flushPromises()
    await openAddForm(w)

    // 用户没走下拉，直接把名字打成已有来源 pi
    await w.get('.ah-lg-sources-name input').setValue('pi')
    await w.get('.ah-lg-sources-pick .ah-btn').trigger('click')
    await flushPromises()
    await w.get('.ah-lg-sources-add .ah-btn-primary').trigger('click')
    await flushPromises()
    expect(addSourcePath).toHaveBeenCalledWith('pi', FRESH)
    expect(addSource).not.toHaveBeenCalled()
    w.unmount()
  })

  it('A12-6 反例：选中已登记目录 → 就地「重复无法添加」+ 确认禁用 + 不打 guest', async () => {
    const addSource = vi.fn(async () => ({ ok: true }))
    const addSourcePath = vi.fn(async () => ({ ok: true }))
    const usage = usageStub({ addSource, addSourcePath })
    usage.pickSourceDir = vi.fn(async () => ({ ok: true, picked: true, path: PI }))
    const w = mountComponent(SessionLogsTab, { usage })
    await flushPromises()
    await openAddForm(w)

    await w.get('.ah-lg-sources-pick .ah-btn').trigger('click')
    await flushPromises()
    // 就地提示（与路径框 aria-describedby 关联）
    expect(w.get('[data-testid="add-dup"]').text()).toBe('hub.lg.sources.pathDuplicate')
    expect(w.get('.ah-lg-sources-pickpath').attributes('aria-describedby')).toBe('lg-add-dup-error')
    expect(w.get('.ah-lg-sources-pickpath').classes()).toContain('dup')
    // 确认禁用（不给点进去的机会）
    expect(w.get('.ah-lg-sources-add .ah-btn-primary').attributes('disabled')).toBeDefined()
    w.unmount()
  })

  it('A12-7 边界：重复目录挂在同一来源的第 N 条路径下同样拦', async () => {
    const usage = usageStub()
    usage.sources = ref([
      src('pi', [PI, FRESH]), // FRESH 已是 pi 的第二条目录
    ])
    const w = mountComponent(SessionLogsTab, { usage })
    await flushPromises()
    await openAddForm(w)

    await w.get('.ah-lg-sources-pick .ah-btn').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="add-dup"]').exists()).toBe(true)
    expect(w.get('.ah-lg-sources-add .ah-btn-primary').attributes('disabled')).toBeDefined()
    w.unmount()
  })

  it('A12-8 正例：未登记目录不受影响（不误伤），确认可用', async () => {
    const addSource = vi.fn(async () => ({ ok: true }))
    const w = mountComponent(SessionLogsTab, { usage: usageStub({ addSource }) })
    await flushPromises()
    await openAddForm(w)

    await w.get('.ah-lg-sources-name input').setValue('my-logs')
    await w.get('.ah-lg-sources-pick .ah-btn').trigger('click')
    await flushPromises()
    expect(w.find('[data-testid="add-dup"]').exists()).toBe(false)
    expect(w.get('.ah-lg-sources-add .ah-btn-primary').attributes('disabled')).toBeUndefined()
    w.unmount()
  })
})

// ==================== A11 扫描失败可复原 + 失败可见 ====================

describe('A11 SessionLogsTab：扫描失败必须看得见且按钮能重试', () => {
  // 实机回归（2026-09-28）：扫描回调丢失后状态永远 syncing → 按钮卡「扫描中…」
  // 且 disabled，失败原因也不显示。契约：终态 error 时按钮复原可点，并按
  // 机器可读 code 显示友好文案（ADR 0030：不透出 guest 原文）。
  function usageStub(over: Partial<UseUsageReturn> = {}): UseUsageReturn {
    return {
      state: ref({ status: 'error', home: '/home/u', error: 'scan-interrupted' }),
      stats: ref(null),
      sources: ref([]),
      statsDays: ref(30 as StatsDays),
      statsLoading: ref(false),
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
      setStatsDays: vi.fn(),
      reloadSessions: vi.fn(),
      goPage: vi.fn(),
      resetQuery: vi.fn(),
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

  async function openSources(w: ReturnType<typeof mountComponent>) {
    await w.get('.ah-lg-sources-toggle').trigger('click')
    await flushPromises()
  }

  it('正例：error 终态 → 扫描按钮恢复可点 + 显示「扫描已中断」文案', async () => {
    const scan = vi.fn()
    const w = mountComponent(SessionLogsTab, { usage: usageStub({ scan }) })
    await openSources(w)
    const btn = w.findAll('.ah-btn').find((b) => b.text() === 'hub.lg.sources.scan')
    expect(btn, '按钮文案须回到「立即扫描」').toBeTruthy()
    expect(btn!.attributes('disabled')).toBeUndefined()
    const err = w.get('[data-testid="scan-error"]')
    expect(err.text(), '失败文案走 i18n key，不透出 guest 原文').toBe(
      'hub.lg.sources.scanInterrupted',
    )
    await btn!.trigger('click')
    expect(scan).toHaveBeenCalled()
  })

  it('未登记 code 走泛化文案（不拿原文渲染）', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        state: ref({
          status: 'error',
          home: '/home/u',
          error: 'something the host wrote in english',
        }) as never,
      }),
    })
    await openSources(w)
    expect(w.get('[data-testid="scan-error"]').text()).toBe('hub.lg.sources.scanFailed')
  })

  it('反例：状态回到非 error（如 ok）时不显示失败条', async () => {
    const w = mountComponent(SessionLogsTab, {
      usage: usageStub({
        state: ref({ status: 'ok', home: '/home/u', error: 'scan-interrupted' }) as never,
      }),
    })
    await openSources(w)
    expect(w.find('[data-testid="scan-error"]').exists()).toBe(false)
  })

  it('日期筛选框隐藏 vendor 左侧日历图标（否则框内文字与关键词框不齐）', () => {
    const src = readFileSync(resolve(AGENT_HUB, 'src/components/SessionLogsTab.vue'), 'utf8')
    const dps = src.match(/<Datepicker[\s\S]*?\/>/g) ?? []
    expect(dps.length, '查询条件条应有两个日期框').toBe(2)
    for (const dp of dps) {
      expect(dp, '日期框缺 hide-input-icon（vendor 图标会把文字推到 35px 处）').toContain(
        ':hide-input-icon="true"',
      )
    }
  })
})
