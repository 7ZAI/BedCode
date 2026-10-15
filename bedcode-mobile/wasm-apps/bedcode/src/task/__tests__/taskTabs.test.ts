/**
 * 任务页三页签（预设任务 / 定时任务 / 任务记录）行为契约测试
 * （2026-10-16：工具箱改名「任务」+ 还原预设任务页签）
 *
 * 被测：`src/task/components/AutoTaskToolboxView.vue`（页签容器 + 面板停靠）
 * 与 `src/task/components/PresetTasksTab.vue`（预设任务页签的列表与执行分流）。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-TT1 | 需求「tabs 顺序为 预设/定时/记录」 | tabs 数组即视觉顺序 | 三个页签按钮按此顺序渲染 |
 * | C-TT2 | 需求「增加 tabs 预设任务」 | 首屏落在预设任务页签 | 第 1 个按钮为激活态，且第 1 个面板为 --active |
 * | C-TT3 | `panelClass(index)` 分支 | 当前页居中、已过页停左、未到页停右 | 切到第 2 页签后三面板停靠态各归其位 |
 * | C-TT4 | 需求「还原原本的预设任务页面」 | 卡片取 SDK `getPresetTasks()` 真源渲染 | 每条任务渲染一行，行内含任务正文 |
 * | C-TT5 | 需求「还原功能」 | 无任务时给空态而非空白页 | 渲染 preset.noTasks |
 * | C-TT6 | `startExecuteFlow` 反例分支 | 未连接时不得执行 | 发 warning toast 且向 app 域发 open-connection，不调 executeTask |
 * | C-TT8 | 需求「预设任务不弹窗，直接显示在 tabs 页面上」 | 编辑表单（textarea + 添加/保存）常驻页签面板顶部，不走 TaskEditDialog 弹窗 | 挂载即渲染内联表单；输入内容点「添加」经 addTask 落库并清空；点卡片编辑载入任务内容并切按钮为「保存」 |
 *
 * 面板下标与 tabs 下标一一对应（`panelClass` 直接吃下标）：少补一个面板不会抛错，
 * 但会让页签切换时高亮落到别的面板上——故 C-TT3 断言的是三个面板各自的停靠类，
 * 而不是「至少有一个 active」。
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { ref } from 'vue'
import { mount } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import AutoTaskToolboxView from '../components/AutoTaskToolboxView.vue'
import type { PresetTask } from '../../terminal/model'
import { installMockMobileApi, uninstallMockMobileApi, installFakeHost, uninstallFakeHost } from '../../terminal/__tests__/testKit'

/** 任务 i18n 文案表（与 `../i18n` 同步：域内相对键 → 中文） */
const ZH: Record<string, string> = {
  'tasks.tabs.preset': '预设任务',
  'tasks.tabs.scheduled': '定时任务',
  'tasks.tabs.history': '任务记录',
  'preset.noTasks': '暂无预设任务',
  'preset.emptyHint': '保存常用指令，一键发送到终端',
  'preset.connectFirst': '请先连接桌面端',
  'preset.confirmExecute': '确认执行',
  'preset.execute': '执行',
  'preset.selectSession': '选择会话',
  'preset.willSendToTerminal': '指令将发送到所选会话的终端',
  'preset.sentToTerminal': '已发送到终端',
  'preset.delete': '删除',
  'toolbox.add': '添加',
  'toolbox.taskContent': '任务内容',
  'toolbox.editTask': '编辑预设任务',
  'toolbox.insertAiTemplate': 'Prompt模板',
  'toolbox.taskContentPlaceholder': '发送到终端的指令内容',
  'toolbox.selectProject': '选择工程目录',
  'common.button.save': '保存',
  cancel: '取消',
  delete: '删除',
}

/** 记录 toast 的宿主 dialogs 替身（events 替身供 useTaskHistory 建订阅） */
function makeContext(toasts: { message: string; type?: string }[]): PluginContext {
  return {
    i18n: { t: (key: string) => ZH[key] ?? key },
    dialogs: {
      showToast: (message: string, type?: string) => toasts.push({ message, type }),
    },
    events: { on: () => ({ dispose() {} }) },
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(), log: vi.fn() },
  } as unknown as PluginContext
}

/** 预设任务真源替身（形状对齐宿主 usePresetTasks 共享模块） */
function installPresetTasks(tasks: PresetTask[], calls: { execute: string[] }) {
  const shared = ((window as any).__BEDCODE_SHARED__ ??= {})
  shared.presetTasks = {
    usePresetTasks: () => ({
      tasks: ref(tasks),
      load: vi.fn(async () => {}),
      addTask: vi.fn(async () => {}),
      updateTask: vi.fn(async () => {}),
      deleteTask: vi.fn(async () => {}),
      executeTask: vi.fn(async (task: PresetTask) => {
        calls.execute.push(task.id)
      }),
      reconcileWithQueue: vi.fn(async () => {}),
    }),
  }
}

function presetTask(overrides: Partial<PresetTask> & { id: string }): PresetTask {
  return {
    content: '跑一遍单测',
    createdAt: '2026-10-16T08:00:00Z',
    updatedAt: '2026-10-16T08:00:00Z',
    repeatable: true,
    status: 'unused',
    pendingTaskId: null,
    pendingSessionId: null,
    ...overrides,
  }
}

let toasts: { message: string; type?: string }[]
let execCalls: { execute: string[] }

beforeEach(() => {
  toasts = []
  execCalls = { execute: [] }
  installFakeHost()
  installMockMobileApi({ isConnected: false, isPaired: false })
  installPresetTasks([], execCalls)
})

afterEach(() => {
  uninstallFakeHost()
  uninstallMockMobileApi()
})

function mountView() {
  return mount(AutoTaskToolboxView, {
    global: { provide: { pluginContext: makeContext(toasts) } },
  })
}

describe('C-TT1/C-TT2 页签集合与首屏落点', () => {
  it('should_renderTabsInPresetScheduledHistoryOrder_when_viewMounted', () => {
    const wrapper = mountView()
    const labels = wrapper.findAll('.att-tab').map((b) => b.text())

    expect(labels).toEqual(['预设任务', '定时任务', '任务记录'])
  })

  it('should_activatePresetTabOnFirstScreen_when_viewMounted', () => {
    const wrapper = mountView()
    const tabs = wrapper.findAll('.att-tab')

    expect(tabs[0].classes()).toContain('att-tab-active')
    expect(tabs[1].classes()).not.toContain('att-tab-active')
    expect(tabs[2].classes()).not.toContain('att-tab-active')
    expect(wrapper.findAll('.att-tab-panel')[0].classes()).toContain('att-tab-panel--active')
  })
})

describe('C-TT3 面板停靠态随页签', () => {
  it('should_dockPanelsByIndex_when_switchedToSecondTab', async () => {
    const wrapper = mountView()
    await wrapper.findAll('.att-tab')[1].trigger('click')

    const panels = wrapper.findAll('.att-tab-panel')
    expect(panels).toHaveLength(3)
    expect(panels[0].classes()).toContain('att-tab-panel--left')
    expect(panels[1].classes()).toContain('att-tab-panel--active')
    expect(panels[2].classes()).toContain('att-tab-panel--right')
  })

  it('should_dockPanelsByIndex_when_switchedToThirdTab', async () => {
    const wrapper = mountView()
    await wrapper.findAll('.att-tab')[2].trigger('click')

    const panels = wrapper.findAll('.att-tab-panel')
    expect(panels[0].classes()).toContain('att-tab-panel--left')
    expect(panels[1].classes()).toContain('att-tab-panel--left')
    expect(panels[2].classes()).toContain('att-tab-panel--active')
  })
})

describe('C-TT4/C-TT5 预设任务列表与空态', () => {
  it('should_renderOneRowPerPresetTask_when_tasksExist', async () => {
    installPresetTasks(
      [presetTask({ id: 't1' }), presetTask({ id: 't2', content: '发布构建产物' })],
      execCalls,
    )
    const wrapper = mountView()
    await wrapper.vm.$nextTick()

    const rows = wrapper.findAll('.preset-card')
    expect(rows).toHaveLength(2)
    expect(rows[0].text()).toContain('跑一遍单测')
    expect(rows[1].text()).toContain('发布构建产物')
  })

  it('should_showEmptyState_when_noPresetTasks', async () => {
    const wrapper = mountView()
    await wrapper.vm.$nextTick()

    expect(wrapper.findAll('.preset-card')).toHaveLength(0)
    expect(wrapper.text()).toContain('暂无预设任务')
    // 反例防护：空态仍保留顶部常驻编辑表单（新增入口不靠弹窗）
    expect(wrapper.find('.content-input').exists()).toBe(true)
  })
})

describe('C-TT6 执行分流：未连接不得执行', () => {
  it('should_warnAndAskForConnection_when_executedWhileDisconnected', async () => {
    installPresetTasks([presetTask({ id: 't1' })], execCalls)
    const wrapper = mountView()
    await wrapper.vm.$nextTick()

    await wrapper.find('.action-cyan').trigger('click')

    expect(toasts).toEqual([{ message: '请先连接桌面端', type: 'warning' }])
    expect(execCalls.execute).toEqual([])
    expect(wrapper.emitted('open-connection')).toHaveLength(1)
  })
})

describe('C-TT7 执行分流：单活跃会话免选会话', () => {
  it('should_executeDirectly_when_connectedWithExactlyOneSession', async () => {
    // 「还原原本的功能」的正例：连上且只有一个活跃会话时不该再问一次选哪个会话
    const api = installMockMobileApi({ isConnected: true, isPaired: true })
    api.activeSessions.value = [{ id: 'session-aaaa1111', name: '构建机' }]
    installPresetTasks([presetTask({ id: 't1' })], execCalls)
    const wrapper = mountView()
    await wrapper.vm.$nextTick()

    await wrapper.find('.action-cyan').trigger('click')
    await wrapper.vm.$nextTick()

    // 确认弹窗 Teleport 到 body：先确认弹窗在场且尚未执行
    expect(document.body.textContent).toContain('指令将发送到所选会话的终端')
    expect(execCalls.execute).toEqual([])

    const confirmBtn = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === '执行',
    )
    expect(confirmBtn).toBeTruthy()
    confirmBtn!.click()
    await wrapper.vm.$nextTick()

    expect(execCalls.execute).toEqual(['t1'])
    expect(toasts).toEqual([{ message: '已发送到终端', type: 'success' }])
  })

  it('should_askWhichSession_when_connectedWithMultipleSessions', async () => {
    const api = installMockMobileApi({ isConnected: true, isPaired: true })
    api.activeSessions.value = [
      { id: 'session-aaaa1111', name: '构建机' },
      { id: 'session-bbbb2222', name: '测试机' },
    ]
    installPresetTasks([presetTask({ id: 't1' })], execCalls)
    const wrapper = mountView()
    await wrapper.vm.$nextTick()

    await wrapper.find('.action-cyan').trigger('click')
    await wrapper.vm.$nextTick()

    expect(document.body.textContent).toContain('选择会话')
    expect(document.body.textContent).toContain('构建机')
    expect(document.body.textContent).toContain('测试机')
    // 反例防护：多会话时不得擅自挑第一个就发
    expect(execCalls.execute).toEqual([])
  })
})

/** 记录新增/更新的预设任务真源替身（断言内联表单的保存路径） */
function installPresetTasksWithCalls(
  tasks: PresetTask[],
  calls: { execute: string[]; added: { content: string; repeatable: boolean }[] },
) {
  const shared = ((window as any).__BEDCODE_SHARED__ ??= {})
  shared.presetTasks = {
    usePresetTasks: () => ({
      tasks: ref(tasks),
      load: vi.fn(async () => {}),
      addTask: vi.fn(async (data: { content: string; repeatable: boolean }) => {
        calls.added.push({ ...data })
      }),
      updateTask: vi.fn(async () => {}),
      deleteTask: vi.fn(async () => {}),
      executeTask: vi.fn(async () => {}),
      reconcileWithQueue: vi.fn(async () => {}),
    }),
  }
}

describe('C-TT8 编辑表单常驻页面顶部（不弹窗）', () => {
  it('should_showInlineEditor_when_mounted (no popup for add)', async () => {
    installPresetTasks([], execCalls)
    const wrapper = mountView()
    await wrapper.vm.$nextTick()

    // 编辑表单直接显示在页签面板内（非 Teleport 弹窗）
    const textarea = wrapper.find('.att-tab-body textarea.content-input')
    expect(textarea.exists()).toBe(true)
    expect(wrapper.find('.content-input').text()).toBe('')
    // 新增按钮是页面内的提交按钮，不是唤起弹窗的入口
    const addBtn = wrapper
      .findAll('.att-tab-body button')
      .find((b) => b.text().includes('添加'))
    expect(addBtn).toBeTruthy()
    expect(addBtn!.element.closest('.att-tab-body')).toBeTruthy()
  })

  it('should_saveNewTask_viaInlineForm_when_clickAdd', async () => {
    const calls: { execute: string[]; added: { content: string; repeatable: boolean }[] } = {
      execute: [],
      added: [],
    }
    installPresetTasksWithCalls([], calls)
    const wrapper = mountView()
    await wrapper.vm.$nextTick()

    const textarea = wrapper.find('.content-input')
    await textarea.setValue('构建并上传安装包')
    await wrapper.vm.$nextTick()

    // 提交按钮随内容可用；点击后经 addTask 落库
    const addBtn = wrapper
      .findAll('.att-tab-body button')
      .find((b) => b.text().includes('添加'))
    expect(addBtn!.attributes('disabled')).toBeUndefined()
    await addBtn!.trigger('click')
    await wrapper.vm.$nextTick()

    expect(calls.added).toEqual([{ content: '构建并上传安装包', repeatable: true }])
    // 保存成功后输入框清空并保留焦点（支持连续录入）
    expect((textarea.element as HTMLTextAreaElement).value).toBe('')
  })

  it('should_loadTaskIntoEditor_when_clickEdit (no popup for edit)', async () => {
    installPresetTasks([presetTask({ id: 't1', content: '发布构建产物', repeatable: false })], execCalls)
    const wrapper = mountView()
    await wrapper.vm.$nextTick()

    await wrapper.find('.action-zinc').trigger('click')
    await wrapper.vm.$nextTick()

    // 内容载入顶部编辑区（仍在页签面板内，未弹窗）
    const textarea = wrapper.find('.content-input')
    expect((textarea.element as HTMLTextAreaElement).value).toBe('发布构建产物')
    // 按钮切换为「保存」
    const saveBtn = wrapper
      .findAll('.att-tab-body button')
      .find((b) => b.text().includes('保存'))
    expect(saveBtn).toBeTruthy()
  })
})