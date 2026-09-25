/**
 * PluginViewHost 二次激活（context 替换）回归测试
 *
 * 故障背景（2026-09-25 实测）：插件「启用 → 会话视图打开 → 停用 → 再启用」后，
 * 宿主 PluginViewHost 为子树注入的仍是**停用前**（前端通道令牌已回收）的旧 context
 * （旧实现只在 props 变化 watch 里 re-provide——Vue 的 provide() 只能在 setup 同步
 * 调用，watch 回调里调用实测不生效，属死代码），导致视图 onMounted 的 `load()`
 * 命令（`session.config.list` / `session.list`）携带旧令牌全员被拒
 * （后端 `缺少有效通道凭证`）→ 用户看到「会话数据加载失败 + 发生了未知错误」。
 *
 * 修复：keyed PluginContextProvider——context 对象身份（registry.contextIdentity）
 * 变化时重挂载 Provider 并在 setup 里重新 provide；本测试锁定该契约。
 */
import { describe, it, expect, afterEach, vi } from 'vitest'
import { defineComponent, inject, nextTick } from 'vue'
import { mount } from '@vue/test-utils'
import { getPluginRegistry } from '@/plugin/registry'
import PluginViewHost from '@/plugin/components/PluginViewHost.vue'
import i18n from '@/locales'
import type { PluginContext } from '@/plugin/types'

const PLUGIN_ID = 'com.bedcode.pluginviewhost-test'

/** 捕获注入 context 的子组件：每次挂载把注入值记入 seen */
function makeProbe() {
  const seen: (PluginContext | undefined)[] = []
  const Probe = defineComponent({
    setup() {
      seen.push(inject<PluginContext>('pluginContext'))
      return () => 'probe'
    },
  })
  return { seen, Probe }
}

/** 最小 context（id 字段区分对象身份；命令面留桩，承载后续 execute 断言） */
function makeContext(id: string): PluginContext {
  return { id, _disposables: [] } as unknown as PluginContext
}

const registry = getPluginRegistry()

afterEach(() => {
  registry.clearPlugin(PLUGIN_ID)
  vi.restoreAllMocks()
})

describe('PluginViewHost：插件二次激活换新 context 后子树必须拿到新 context', () => {
  it('初始挂载注入当前 context', async () => {
    const { seen, Probe } = makeProbe()
    const ctxA = makeContext('ctx-a')
    registry.setContext(PLUGIN_ID, ctxA)
    registry.registerView(PLUGIN_ID, 'sidebar', {
      id: 'view',
      title: 'view',
      component: Probe,
    })
    await nextTick()

    const wrapper = mount(PluginViewHost, {
      props: { pluginId: PLUGIN_ID, viewId: 'view' },
      global: { plugins: [i18n] },
    })
    expect(wrapper.text()).toContain('probe')
    expect(seen[0]?.id).toBe('ctx-a')
    wrapper.unmount()
  })

  it('停用（clearPlugin）后视图摘除（不渲染子树）', async () => {
    const { Probe } = makeProbe()
    const ctxA = makeContext('ctx-a')
    registry.setContext(PLUGIN_ID, ctxA)
    registry.registerView(PLUGIN_ID, 'sidebar', { id: 'view', title: 'view', component: Probe })
    await nextTick()

    const wrapper = mount(PluginViewHost, {
      props: { pluginId: PLUGIN_ID, viewId: 'view' },
      global: { plugins: [i18n] },
    })
    expect(wrapper.text()).toContain('probe')

    registry.clearPlugin(PLUGIN_ID)
    await nextTick()
    // 修复前：host 仍静态 provide 旧 context、子树在视图注销后由 resolvedComponent 摘除
    expect(wrapper.text()).not.toContain('probe')
    wrapper.unmount()
  })

  it('二次激活（同 pluginId 换新 context 对象）后重挂载子树注入新 context —— 回归锁定', async () => {
    const { seen, Probe } = makeProbe()
    const ctxA = makeContext('ctx-a')
    registry.setContext(PLUGIN_ID, ctxA)
    registry.registerView(PLUGIN_ID, 'sidebar', { id: 'view', title: 'view', component: Probe })
    await nextTick()

    const wrapper = mount(PluginViewHost, {
      props: { pluginId: PLUGIN_ID, viewId: 'view' },
      global: { plugins: [i18n] },
    })
    expect(seen[0]?.id).toBe('ctx-a')

    // 模拟「停用 → 再启用」：clearPlugin 清空（含旧 context），随后新 context 入表 + 视图重注册
    registry.clearPlugin(PLUGIN_ID)
    await nextTick()
    expect(wrapper.text()).not.toContain('probe')

    const ctxB = makeContext('ctx-b')
    registry.setContext(PLUGIN_ID, ctxB)
    registry.registerView(PLUGIN_ID, 'sidebar', { id: 'view', title: 'view', component: Probe })
    await nextTick()

    // 修复前（旧实现）：子组件仍注入 ctx-a（provide 在 host setup 后不再刷新）
    // 修复后：keyed Provider 随 contextIdentity 变化重挂载，注入 ctx-b
    expect(wrapper.text()).toContain('probe')
    expect(seen[seen.length - 1]?.id).toBe('ctx-b')
    expect(seen.every((c) => c?.id === 'ctx-a' || c?.id === 'ctx-b')).toBe(true)
    wrapper.unmount()
  })

  it('同一 context 对象重复 setContext 不触发子树重挂载（id 稳定，不抖）', async () => {
    const { seen, Probe } = makeProbe()
    const ctxA = makeContext('ctx-a')
    registry.setContext(PLUGIN_ID, ctxA)
    registry.registerView(PLUGIN_ID, 'sidebar', { id: 'view', title: 'view', component: Probe })
    await nextTick()

    const wrapper = mount(PluginViewHost, {
      props: { pluginId: PLUGIN_ID, viewId: 'view' },
      global: { plugins: [i18n] },
    })
    const mountCount = seen.length

    // loader 幂等重设同一对象（预登记 + 二次 set 同对象）：不应换 key 重挂载
    registry.setContext(PLUGIN_ID, ctxA)
    await nextTick()

    expect(seen.length).toBe(mountCount)
    expect(wrapper.text()).toContain('probe')
    wrapper.unmount()
  })
})