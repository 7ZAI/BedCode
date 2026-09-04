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
})
