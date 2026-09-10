/**
 * ToolboxView 深层子组件 KeepAlive 测试
 *
 * 忠实复刻真实结构：KeepAlive > 中间组件 > ToolboxView（深层子组件，
 * 对应 MobileSwipeContainer v-for <component :is> 渲染的 ToolboxView）。
 * 复现用户反馈：enable→disable→enable 后工具箱入口消失。
 * 验证 reactivate 后深层子组件是否如实呈现 registry 当前入口。
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

import ToolboxDeepChildHost from '@/__tests__/integration/fixtures/toolboxDeepChildHost.vue'
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

beforeEach(() => {
  clearFileTransferView()
})
afterEach(() => {
  clearFileTransferView()
})

describe('ToolboxView 深层子组件 KeepAlive', () => {
  it('enable→disable→enable 后 reactivate 入口如实呈现', async () => {
    // 1. 启用：注册入口，挂载（ToolboxView 作为深层子组件被 KeepAlive 缓存）
    registerFileTransferView()
    const wrapper = mount(ToolboxDeepChildHost, {
      global: { stubs: { PluginViewHost: { template: '<div class="pvhost-stub" />' } } },
    })
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain(VIEW_TITLE)

    // 2. 离开工具箱（中间组件被 KeepAlive 缓存，ToolboxView 作为深层子组件 deactivated）
    ;(wrapper.vm as any).deactivate()
    await wrapper.vm.$nextTick()

    // 3. 禁用：registry 移除入口
    clearFileTransferView()
    await wrapper.vm.$nextTick()

    // 4. 重启用：registry 以新对象重新注册
    registerFileTransferView()
    await wrapper.vm.$nextTick()

    // 5. 返回工具箱（reactivate 深层子组件）
    ;(wrapper.vm as any).reactivate()
    await wrapper.vm.$nextTick()

    // 入口应如实呈现（registry 当前含入口）
    expect(wrapper.text()).toContain(VIEW_TITLE)
  })
})
