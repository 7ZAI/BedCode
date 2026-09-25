/**
 * 插件视图注册表的解析契约（含响应式）
 *
 * 被测：`registry.getViewComponent` 的可见性语义。这里有一条单靠 `views` Map
 * 兑现不了的契约——**晚注册必须对已挂载的消费方可见**：独立窗口路由
 * （`/terminal-window/:id`、`/plugin/window/:pluginId/:viewId`）在 `loadAll`
 * 完成前就挂载了视图宿主，那时容错(server null)的结果必须能被随后的注册冲掉，
 * 否则白屏 + 「插件视图未找到」永久驻留（2026-09-24 终端窗口故障的生产根因）。
 *
 * 因此这里用 `watchEffect` 复现「已挂载的消费方」（PluginViewHost 的 computed
 * 同形态），断言注册 / 注销两个方向的可见性。
 */
import { describe, it, expect, afterEach } from 'vitest'
import { nextTick, watchEffect } from 'vue'
import { getPluginRegistry } from '@/plugin/registry'
import type { PluginContext } from '@/plugin/types'

const PLUGIN_A = 'com.bedcode.registry-test-a'
const PLUGIN_B = 'com.bedcode.registry-test-b'
const WINDOW_VIEW = 'test.window'
const OTHER_VIEW = 'test.other'

/** 最小 context 桩（id 字段区分对象身份） */
function makeContext(id: string): PluginContext {
  return { id, _disposables: [] } as unknown as PluginContext
}

/** 记录消费方每一次求值结果（等价于 PluginViewHost 的 computed 求值序列） */
function trackResolution(pluginId: string, viewId: string) {
  const seen: unknown[] = []
  const stop = watchEffect(() => {
    seen.push(getPluginRegistry().getViewComponent(pluginId, viewId))
  })
  return { seen, stop }
}

describe('插件视图注册表的响应式解析', () => {
  const registry = getPluginRegistry()

  afterEach(() => {
    registry.clearPlugin(PLUGIN_A)
    registry.clearPlugin(PLUGIN_B)
  })

  it('晚注册的视图对已挂载的消费方可见（终端窗口白屏根因）', async () => {
    const Component = { template: '<div />' }
    const { seen, stop } = trackResolution(PLUGIN_A, WINDOW_VIEW)
    expect(seen).toEqual([undefined])

    registry.registerView(PLUGIN_A, 'page', {
      id: WINDOW_VIEW,
      title: 'window',
      component: Component,
    })
    await nextTick()

    // 消费方必须在注册后重新求值并拿到组件（不只是再调用一次才可见）
    expect(seen).toHaveLength(2)
    expect(seen[1]).toBe(Component)
    stop()
  })

  it('注册的键含插件属主：不同插件同名视图互不串台', async () => {
    const A = { template: '<div>a</div>' }
    const B = { template: '<div>b</div>' }
    registry.registerView(PLUGIN_A, 'page', { id: WINDOW_VIEW, title: 'a', component: A })
    registry.registerView(PLUGIN_B, 'page', { id: WINDOW_VIEW, title: 'b', component: B })

    expect(registry.getViewComponent(PLUGIN_A, WINDOW_VIEW)).toBe(A)
    expect(registry.getViewComponent(PLUGIN_B, WINDOW_VIEW)).toBe(B)
    // 反例：未注册的 viewId 与未注册的插件 id 恒为 undefined
    expect(registry.getViewComponent(PLUGIN_A, OTHER_VIEW)).toBeUndefined()
    expect(registry.getViewComponent('com.bedcode.absent', WINDOW_VIEW)).toBeUndefined()
  })

  it('dispose 注销后摘除可见（不再是永久缓存的组件）', async () => {
    const Component = { template: '<div />' }
    const { seen, stop } = trackResolution(PLUGIN_A, WINDOW_VIEW)
    expect(seen).toEqual([undefined])

    const handle = registry.registerView(PLUGIN_A, 'page', {
      id: WINDOW_VIEW,
      title: 'window',
      component: Component,
    })
    await nextTick()
    expect(seen[1]).toBe(Component)

    handle.dispose()
    await nextTick()

    expect(seen).toHaveLength(3)
    expect(seen[2]).toBeUndefined()
    stop()
  })

  it('插件停用（clearPlugin）后其全部视图摘除可见', async () => {
    const Component = { template: '<div />' }
    const { seen, stop } = trackResolution(PLUGIN_A, WINDOW_VIEW)
    registry.registerView(PLUGIN_A, 'page', {
      id: WINDOW_VIEW,
      title: 'window',
      component: Component,
    })
    await nextTick()
    expect(seen[1]).toBe(Component)

    registry.clearPlugin(PLUGIN_A)
    await nextTick()

    expect(seen[2]).toBeUndefined()
    stop()
  })
})

describe('插件上下文身份（contextIdentity）的响应式契约', () => {
  const registry = getPluginRegistry()

  afterEach(() => {
    registry.clearPlugin(PLUGIN_A)
    registry.clearPlugin(PLUGIN_B)
  })

  it('setContext 后返回稳定身份；同对象幂等；clearPlugin 后归 null；换新对象身份变化', async () => {
    const tracked: (string | null)[] = []
    const stop = watchEffect(() => {
      tracked.push(registry.contextIdentity(PLUGIN_A))
    })
    expect(tracked[0]).toBeNull()

    const ctxA = makeContext('ctx-a')
    registry.setContext(PLUGIN_A, ctxA)
    await nextTick()
    const idA = tracked[tracked.length - 1]
    expect(idA).toMatch(/^ctx:\d+$/)

    // 幂等重设同一对象：身份不变（keyed Provider 不重挂载）
    registry.setContext(PLUGIN_A, ctxA)
    await nextTick()
    expect(tracked[tracked.length - 1]).toBe(idA)

    // 二次激活换新对象：身份变化（keyed Provider 据此重挂载重新 provide）
    registry.setContext(PLUGIN_A, makeContext('ctx-b'))
    await nextTick()
    expect(tracked[tracked.length - 1]).not.toBe(idA)

    // clearPlugin：归 null
    registry.clearPlugin(PLUGIN_A)
    await nextTick()
    expect(tracked[tracked.length - 1]).toBeNull()
    stop()
  })

  it('不同插件互不串台：各持各自身份', async () => {
    registry.setContext(PLUGIN_A, makeContext('a'))
    registry.setContext(PLUGIN_B, makeContext('b'))
    await nextTick()
    const idA = registry.contextIdentity(PLUGIN_A)
    const idB = registry.contextIdentity(PLUGIN_B)
    expect(idA).not.toBeNull()
    expect(idB).not.toBeNull()
    expect(idA).not.toBe(idB)
  })

  it('clearContext 摘除后归 null（激活失败回滚路径）', async () => {
    registry.setContext(PLUGIN_A, makeContext('a'))
    await nextTick()
    expect(registry.contextIdentity(PLUGIN_A)).not.toBeNull()

    registry.clearContext(PLUGIN_A)
    await nextTick()
    expect(registry.contextIdentity(PLUGIN_A)).toBeNull()
  })
})
