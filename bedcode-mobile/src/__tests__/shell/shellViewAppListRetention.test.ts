/**
 * ShellView 卸载→重挂的应用清单保留 行为契约测试
 * （2026-10-16 用户反馈：文件传输应用进入设置页（插件整页路由）返回后显示「应用不存在或已卸载」）
 *
 * 背景：插件子页（registerRoute / openPage）是 vue-router 整页路由，进入即把 `/mobile/shell`
 * 的 ShellView 卸载；壳内屏幕栈是模块级单例，返回时重挂的 ShellView 直接渲染 app-run 屏。
 * 旧实现卸载时 `clearSource` 清空应用清单记录 → 重挂窗口内 apps 为空、listReady 却仍是 true，
 * 运行屏把「清单还没回来」判成「应用不存在」；运行面贡献虽保留也解析不到
 * （resolveSurface 先要求清单记录在册）。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-V1 | ShellView.onUnmounted | 卸载只摘数据源，不清清单记录 | 重挂窗口 getApp / resolveSurface 仍命中 |
 * | C-V2 | 运行屏可见行为 | 重挂后清单重拉未落地期间 | 渲染应用运行面，不出现 notFound |
 * | C-V3 | 重拉对账 | 清单落地且不含该应用（确已卸载） | 记录被回收，notFound 是终态 |
 *
 * 隔离：registry / useShellApps / listReady 都是模块级单例；本文件挂真实 ShellView
 * （ShellHost 用空壳替身、数据源适配器用可控替身），appId 用测试专用值，afterEach 清记录。
 */
import { describe, it, expect, afterEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import type { ShellApp } from '@/shell/types'

const APP_ID = 'test.file-transfer'

/** 数据源清单行为（用例按需替换；默认空清单）。经 vi.hoisted 供 mock 工厂安全引用 */
const harness = vi.hoisted(() => ({
  sourceId: 'plugin-system',
  list: async (): Promise<unknown[]> => [],
}))

// 只替 `useI18n`（组件取文案）；`createI18n` 必须保留——useShellApps 经 @/locales 建全局实例
vi.mock('vue-i18n', async (importOriginal) => {
  const actual = await importOriginal<typeof import('vue-i18n')>()
  return { ...actual, useI18n: () => ({ t: (key: string) => key }) }
})
vi.mock('@/shell/adapters/pluginAppSource', () => ({
  PLUGIN_APP_SOURCE_ID: harness.sourceId,
  createPluginAppSource: () => ({
    id: harness.sourceId,
    list: () => harness.list(),
    launch: async () => {},
    stop: async () => {},
  }),
}))
// ShellHost 会牵出整棵屏幕树与宿主事件桥；本文件只验 ShellView 的生命周期契约，用空壳替身
vi.mock('@/shell/components/ShellHost.vue', () => ({
  default: { name: 'ShellHost', template: '<div class="host-stub" />' },
}))

import ShellView from '@/shell/views/ShellView.vue'
import ShellAppSurface from '@/shell/components/ShellAppSurface.vue'
import { getShellRegistry } from '@/shell/registry'
import { useShellApps } from '@/shell/composables/useShellApps'

const FakeSurface = defineComponent({
  name: 'FakeFileTransferSurface',
  setup: () => () => h('div', 'file-transfer-main'),
})

function appFixture(): ShellApp {
  return {
    id: APP_ID,
    name: '文件传输',
    version: '1.0.0',
    state: 'running',
    permissions: [],
    contributions: {},
  }
}

/** 首次挂载壳并让清单落地（模拟用户已在用壳），再注册运行面（模拟插件 activate） */
async function enterShellWithApp() {
  harness.list = async () => [appFixture()]
  const shell = mount(ShellView)
  await flushPromises()
  getShellRegistry().registerSurface(APP_ID, { component: FakeSurface })
  return shell
}

afterEach(() => {
  getShellRegistry().clearApp(APP_ID)
})

describe('C-V1 卸载保留清单记录', () => {
  it('should_keepAppResolvable_when_shellUnmountsForPluginRoute', async () => {
    const shell = await enterShellWithApp()

    shell.unmount()

    const apps = useShellApps()
    expect(apps.getApp(APP_ID), '壳卸载后清单记录应仍在（屏幕栈跨挂载存活）').toBeDefined()
    expect(apps.resolveSurface(APP_ID)).toBe(FakeSurface)
  })
})

describe('C-V2 重挂窗口直接渲染运行面', () => {
  it('should_renderSurface_notNotFound_when_listRefetchPendingAfterRemount', async () => {
    const shell = await enterShellWithApp()
    shell.unmount()

    // 返回壳：重挂触发的清单重拉尚未落地（悬挂请求）
    harness.list = () => new Promise<unknown[]>(() => {})
    const remounted = mount(ShellView)
    const surface = mount(ShellAppSurface, { props: { appId: APP_ID } })
    await flushPromises()

    expect(surface.findComponent(FakeSurface).exists(), '重挂窗口运行面应立即可解析').toBe(true)
    expect(surface.text()).not.toContain('shell.run.notFound')
    remounted.unmount()
  })
})

describe('C-V3 重拉清单对账', () => {
  it('should_dropRecord_when_refetchedListNoLongerHasApp', async () => {
    const shell = await enterShellWithApp()
    shell.unmount()

    // 重挂后清单落地且不含该应用：视为已卸载，记录回收，notFound 是终态
    harness.list = async () => []
    const remounted = mount(ShellView)
    await flushPromises()
    const surface = mount(ShellAppSurface, { props: { appId: APP_ID } })

    expect(surface.text()).toContain('shell.run.notFound')
    remounted.unmount()
  })
})
