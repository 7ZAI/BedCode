/**
 * 应用运行面容器（挂载点 + 空态）行为契约测试
 * （2026-10-16：区分「清单未落地」与「应用不存在」两种状态）
 *
 * 被测：`src/shell/components/ShellAppSurface.vue`。
 *
 * 背景：应用清单异步拉取，壳先渲染、清单后到（冷启动窗口；壳卸载不清记录本身由
 * `shellViewAppListRetention.test.ts` 钉住，两者互补）。旧实现把「查不到应用」一律
 * 当终态，加载窗口会闪一句「应用不存在」（用户 2026-10-16 反馈）。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-S1 | 运行面优先 | 应用注册了 surface | 渲染该 surface |
 * | C-S2 | 预留位 | 应用在册但没注册 surface | 渲染 ShellPlaceholderSurface |
 * | C-S3 | 加载态（回归锁） | 清单未拉完（listReady=false）且查不到应用 | 加载文案，**不**出 notFound |
 * | C-S4 | 空态 | 清单已就绪仍查不到应用 | notFound 文案 |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { ref } from 'vue'
import type { ShellApp } from '@/shell/types'

const apps = ref<ShellApp[]>([])
const listReady = ref(true)
/** 有运行面的应用 id（其余应用走数据源解析，此处一律无） */
const appWithSurface = ref<string | null>(null)

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))
vi.mock('@/shell/composables/useShellApps', () => ({
  useShellApps: () => ({
    getApp: (id: string) => apps.value.find((a: ShellApp) => a.id === id),
    resolveSurface: (id: string) => (id === appWithSurface.value ? { name: 'FakeSurface' } : undefined),
    listReady,
  }),
}))
vi.mock('@/shell/components/ShellPlaceholderSurface.vue', () => ({
  default: { name: 'ShellPlaceholderSurface', props: ['app'], template: '<div class="placeholder" />' },
}))

import ShellAppSurface from '@/shell/components/ShellAppSurface.vue'

function app(id: string): ShellApp {
  return { id, name: id, version: '1.0.0', state: 'running', permissions: [], contributions: {} }
}

beforeEach(() => {
  apps.value = [app('a')]
  listReady.value = true
  appWithSurface.value = null
})

describe('C-S1/C-S2 挂载优先级', () => {
  it('should_renderSurface_when_appRegisteredSurface', () => {
    appWithSurface.value = 'a'
    const wrapper = mount(ShellAppSurface, { props: { appId: 'a' } })

    expect(wrapper.findComponent({ name: 'FakeSurface' }).exists()).toBe(true)
    expect(wrapper.text()).not.toContain('shell.run.notFound')
  })

  it('should_renderPlaceholder_when_appKnownButNoSurface', () => {
    const wrapper = mount(ShellAppSurface, { props: { appId: 'a' } })

    expect(wrapper.find('.placeholder').exists()).toBe(true)
    expect(wrapper.text()).not.toContain('shell.run.notFound')
  })
})

describe('C-S3/C-S4 空态与加载态', () => {
  it('should_showNotFound_when_appMissingAndListReady', () => {
    apps.value = []
    listReady.value = true
    const wrapper = mount(ShellAppSurface, { props: { appId: 'missing' } })

    expect(wrapper.text()).toContain('shell.run.notFound')
    expect(wrapper.text()).not.toContain('shell.common.loading')
  })

  it('should_showLoading_when_appMissingButListStillLoading', () => {
    // 回归锁：这就是「从插件设置页返回闪应用不存在」的那段窗口
    apps.value = []
    listReady.value = false
    const wrapper = mount(ShellAppSurface, { props: { appId: 'missing' } })

    expect(wrapper.text()).toContain('shell.common.loading')
    expect(wrapper.text()).not.toContain('shell.run.notFound')
  })

  it('should_stopShowingLoading_when_listLandsWithoutTheApp', async () => {
    apps.value = []
    listReady.value = false
    const wrapper = mount(ShellAppSurface, { props: { appId: 'missing' } })
    expect(wrapper.text()).toContain('shell.common.loading')

    // 清单拉完仍未命中 → 加载态必须让位给空态（否则永远转圈）
    listReady.value = true
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('shell.run.notFound')
    expect(wrapper.text()).not.toContain('shell.common.loading')
  })
})