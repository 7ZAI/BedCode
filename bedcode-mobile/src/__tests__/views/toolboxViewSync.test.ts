/**
 * ToolboxView 工具箱视图同步测试
 *
 * 验证用户反馈的双 bug（同一根因：KeepAlive 缓存下 activePluginView
 * 不与注册表同步）：
 *   A. 禁用插件后切回工具箱，二级页仍残留（应被摘除）
 *   B. 重启用后入口不出现（实为二级页残留掩盖入口列表）
 *
 * 协作实体：真实 plugin registry（toolboxViews 响应式数组 + registerToolboxPage
 * / clearPlugin）+ 挂载的 ToolboxView（watch + onActivated 同步）。
 * 轻量 mock：vue-router / vue-i18n / usePresetTasks / PluginViewHost（stub）。
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { ref } from 'vue'
import { mount } from '@vue/test-utils'

// ==================== 轻量 mock（不涉及注册表，注册表用真实单例）====================

vi.mock('vue-router', () => ({
  useRouter: () => ({ push: vi.fn() }),
}))
vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (k: string) => k }),
}))
vi.mock('@/composables/usePresetTasks', () => ({
  usePresetTasks: () => ({ tasks: ref([]), load: vi.fn() }),
}))

import ToolboxView from '@/views/ToolboxView.vue'
import { getPluginRegistry } from '@/plugin/registry'

// ==================== 测试基建 ====================

const PLUGIN_ID = 'com.bedcode.file-transfer'
const VIEW_ID = 'file-transfer.toolbox'
const VIEW_TITLE = '文件传输'

const registry = getPluginRegistry()

/** 注册文件传输工具箱视图（entry 缺省走文本入口分支，便于按标题定位点击） */
function registerFileTransferView(): void {
  registry.registerToolboxPage(PLUGIN_ID, {
    id: VIEW_ID,
    title: VIEW_TITLE,
    icon: 'M8 7h12',
    // 二级页 PluginViewHost 被 stub，component 仅占位
    component: {},
    entry: undefined,
  })
}

/** 模拟插件停用：loader.deactivate → clearPlugin 移除该插件全部注册 */
function clearFileTransferView(): void {
  registry.clearPlugin(PLUGIN_ID)
}

function mountToolbox() {
  return mount(ToolboxView, {
    global: {
      // 二级页内 PluginViewHost 不真实渲染，避免渲染占位 component
      stubs: { PluginViewHost: { template: '<div class="pvhost-stub" />' } },
    },
  })
}

/** 入口列表与二级页共用 .page-title，按当前渲染态取首个标题文本判别视图态 */
function pageTitleText(wrapper: ReturnType<typeof mountToolbox>): string {
  return wrapper.find('.page-title').text()
}

beforeEach(() => {
  clearFileTransferView()
})

afterEach(() => {
  clearFileTransferView()
})

describe('ToolboxView 工具箱视图同步', () => {
  it('插件停用移除视图 → 二级页摘除；重启用注册 → 入口重现', async () => {
    // 1. 启用：注册工具箱视图，挂载后入口列表含文件传输入口
    registerFileTransferView()
    const wrapper = mountToolbox()
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain(VIEW_TITLE)

    // 2. 进入文件传输二级页（点击入口）
    const entryBtn = wrapper.findAll('button').find((b) => b.text().includes(VIEW_TITLE))!
    expect(entryBtn).toBeTruthy()
    await entryBtn.trigger('click')
    await wrapper.vm.$nextTick()
    // 二级页显示：标题切为入口标题
    expect(pageTitleText(wrapper)).toBe(VIEW_TITLE)

    // 3. 停用插件：注册表移除其工具箱视图 → watch 摘除失效 activePluginView
    clearFileTransferView()
    await wrapper.vm.$nextTick()
    // 回到入口列表：标题不再是入口标题，且列表不再含文件传输（已停用）
    expect(pageTitleText(wrapper)).not.toBe(VIEW_TITLE)
    expect(wrapper.text()).not.toContain(VIEW_TITLE)

    // 4. 重启用：插件再次注册工具箱视图 → 入口重现（此前被残留二级页掩盖）
    registerFileTransferView()
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain(VIEW_TITLE)
  })

  it('入口列表态下停用/重启用：不残留二级页，入口随注册表如实显隐', async () => {
    // 停用前未进入二级页（activePluginView 为 null）
    registerFileTransferView()
    const wrapper = mountToolbox()
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain(VIEW_TITLE)

    // 停用：入口消失
    clearFileTransferView()
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).not.toContain(VIEW_TITLE)

    // 重启用：入口回来
    registerFileTransferView()
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain(VIEW_TITLE)
  })

  it('重启用替换入口对象（同 pluginId 新引用）→ 摘除陈旧二级页引用，入口列表如实呈现', async () => {
    // 复现用户 bug：进入二级页后停用+重启用，activePluginView 仍指向旧入口对象，
    // 重启用时 loader 重新 registerToolboxPage 以【新对象】替换注册表入口（同 pluginId）。
    // 按 pluginId 判定会误以为「插件仍在」而保留陈旧引用 → 二级页渲染陈旧组件；
    // 按引用相等（includes）判定才能识别旧对象已不在注册表 → 摘除。
    registerFileTransferView() // view1
    const wrapper = mountToolbox()
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain(VIEW_TITLE)

    // 进入二级页（activePluginView = view1）
    const entryBtn = wrapper.findAll('button').find((b) => b.text().includes(VIEW_TITLE))!
    await entryBtn.trigger('click')
    await wrapper.vm.$nextTick()
    expect(pageTitleText(wrapper)).toBe(VIEW_TITLE)

    // 重启用：以新对象 view2 替换注册表里的 view1（同 pluginId，不同引用）
    registerFileTransferView() // view2
    await wrapper.vm.$nextTick()
    // 陈旧 view1 已不在注册表 → includes 判定失效 → 摘除 → 回到入口列表
    expect(pageTitleText(wrapper)).not.toBe(VIEW_TITLE)
    // 入口列表呈现新对象 view2 的入口卡片
    expect(wrapper.text()).toContain(VIEW_TITLE)
  })
})
