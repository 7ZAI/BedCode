/**
 * ToolboxView KeepAlive 生命周期测试
 *
 * 复现真实场景：ToolboxView 经 MobileSwipeContainer 的 KeepAlive 缓存，
 * 用户离开工具箱（deactivate）→ 在插件页禁用/重启用插件（registry 变更）
 * → 返回工具箱（reactivate）。验证 reactivate 后入口列表如实呈现新入口，
 * 而非残留陈旧二级页或丢失入口（用户反馈：再启用后入口卡片不显示）。
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { ref } from 'vue'
import { mount } from '@vue/test-utils'

vi.mock('vue-router', () => ({
  useRouter: () => ({ push: vi.fn() }),
}))
vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (k: string) => k }),
}))
vi.mock('@/composables/usePresetTasks', () => ({
  usePresetTasks: () => ({ tasks: ref([]), load: vi.fn() }),
}))

import ToolboxKeepAliveHost from '@/__tests__/integration/fixtures/toolboxKeepAliveHost.vue'
import ToolboxView from '@/views/ToolboxView.vue'
import { getPluginRegistry } from '@/plugin/registry'

const PLUGIN_ID = 'com.bedcode.file-transfer'
const VIEW_TITLE = '文件传输'

const registry = getPluginRegistry()

function registerFileTransferView(): void {
  registry.registerToolboxPage(PLUGIN_ID, {
    id: 'file-transfer.toolbox',
    title: VIEW_TITLE,
    icon: 'M8 7h12',
    component: {},
    entry: undefined,
  })
}

function clearFileTransferView(): void {
  registry.clearPlugin(PLUGIN_ID)
}

function pageTitleText(wrapper: ReturnType<typeof mount>): string {
  return wrapper.find('.page-title').text()
}

beforeEach(() => {
  clearFileTransferView()
})

afterEach(() => {
  clearFileTransferView()
})

describe('ToolboxView KeepAlive 生命周期', () => {
  it('离开→禁用→重启用→返回：reactivate 后入口列表如实呈现，不残留二级页', async () => {
    registerFileTransferView() // view1
    const wrapper = mount(ToolboxKeepAliveHost, {
      global: {
        stubs: { PluginViewHost: { template: '<div class="pvhost-stub" />' } },
      },
    })
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain(VIEW_TITLE)

    // 进入二级页（activePluginView = view1）
    const entryBtn = wrapper.findAll('button').find((b) => b.text().includes(VIEW_TITLE))!
    await entryBtn.trigger('click')
    await wrapper.vm.$nextTick()
    expect(pageTitleText(wrapper)).toBe(VIEW_TITLE)

    // 离开工具箱（KeepAlive 缓存 ToolboxView，deactivated）
    ;(wrapper.vm as any).deactivate()
    await wrapper.vm.$nextTick()

    // 在插件页禁用：registry 移除 view1
    clearFileTransferView()
    await wrapper.vm.$nextTick()

    // 重启用：registry 以新对象 view2 重新注册（同 pluginId 不同引用）
    registerFileTransferView()
    await wrapper.vm.$nextTick()

    // 返回工具箱（reactivate → onActivated）
    ;(wrapper.vm.$nextTick)()
    ;(wrapper.vm as any).reactivate()
    await wrapper.vm.$nextTick()

    // 不应残留二级页（标题非入口标题），应回到入口列表并呈现 view2 入口
    expect(pageTitleText(wrapper)).not.toBe(VIEW_TITLE)
    expect(wrapper.text()).toContain(VIEW_TITLE)
  })

  it('二级页严格跟随停用：暂存旧注册对象失效，getter 返回 null、入口列表呈现', async () => {
    // 场景（spec D7 验收不变量）：用户停留在插件二级页 → 切到插件管理停用 →
    // 返回工具箱。旧注册对象已不在 registry 现存入口中，activePluginView
    // getter 必须按引用相等复核返回 null，呈现入口列表（此时为空态占位），
    // 不得残留已停用插件的二级页组件。
    registerFileTransferView() // view1
    const wrapper = mount(ToolboxKeepAliveHost, {
      global: {
        stubs: { PluginViewHost: { template: '<div class="pvhost-stub" />' } },
      },
    })
    await wrapper.vm.$nextTick()

    // 进入二级页（activePluginView 暂存 view1 引用）
    const entryBtn = wrapper.findAll('button').find((b) => b.text().includes(VIEW_TITLE))!
    await entryBtn.trigger('click')
    await wrapper.vm.$nextTick()
    expect(pageTitleText(wrapper)).toBe(VIEW_TITLE)

    // 离开 → 插件管理停用（仅 clearPlugin，不重注册）→ 返回
    ;(wrapper.vm as any).deactivate()
    await wrapper.vm.$nextTick()
    clearFileTransferView()
    ;(wrapper.vm as any).reactivate()
    await wrapper.vm.$nextTick()

    // getter 复核：暂存旧对象不在现存入口 → null，二级页隐藏
    // （script setup 绑定经内部实例 setupState 访问，computed 在此解包；
    //   ToolboxView 是 fixture 的子组件，需先定位）
    const toolbox = wrapper.findComponent(ToolboxView)
    expect((toolbox.vm.$ as any).setupState.activePluginView).toBeNull()
    expect(pageTitleText(wrapper)).not.toBe(VIEW_TITLE)
    // 入口列表呈现：空态占位（i18n mock 原样返回 key）
    expect(wrapper.text()).toContain('mobile.toolbox.pluginViews')
  })
})
