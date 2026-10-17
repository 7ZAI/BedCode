/**
 * 启动后激活插件的前端补加载契约（2026-10-18）
 *
 * 现场（Linux dev 实测）：启动期持久化激活被移到宿主启动钩子里异步执行后，
 * 前端 `pluginLoader.loadAll()` 在页面启动时查到 agent-hub 仍是 `activating` 而跳过
 * → 激活在 30s 后才完成，但没有任何东西再加载它的前端模块 → **整个会话界面缺失**。
 *
 * 契约两条：
 * - C1 `pluginsNeedingFrontendLoad`：只选「后端已运行（activated / degraded）且前端未加载」
 *   的非 rust-only 插件（与 `loadAll` 同一门禁口径）；
 * - C2 `loadMissingActivated`：并发触发只跑一趟（事件可能连续到达），跑完后句柄释放，
 *   后续事件仍能再补一轮。
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { pluginLoader, pluginsNeedingFrontendLoad } from '@/plugin/loader'
import type { PluginInfo } from '@/plugin/types'

function manifest(id: string, state: string, pluginType = 'rust-ts'): PluginInfo {
  return {
    id,
    name: id,
    version: '1.0.0',
    main: 'index.js',
    pluginType,
    permissions: [],
    state: state === 'degraded' ? { state, error: 'dir denied' } : { state },
    extensionPath: `/tmp/${id}`,
    contributes: {},
  } as unknown as PluginInfo
}

describe('pluginsNeedingFrontendLoad（补加载判据）', () => {
  it('C1 正例：已激活 / 降级且未加载的插件被选中（含顺序保持）', () => {
    const manifests = [
      manifest('com.a', 'activated'),
      manifest('com.b', 'degraded'),
      manifest('com.c', 'activating'),
      manifest('com.d', 'loaded'),
      manifest('com.e', 'deactivated'),
      manifest('com.f', 'activated', 'rust'),
    ]

    const selected = pluginsNeedingFrontendLoad(manifests, new Set())

    // 变异对照：门禁改成「非 activated 一律不选」→ 漏掉 degraded（b）本断言转红；
    // 漏掉「未加载」过滤 → 已加载插件被重复 import（用例 C2 的场景）
    expect(selected.map((m) => m.id)).toEqual(['com.a', 'com.b'])
  })

  it('C1 反例：已加载的插件不再入选（重复 import 会让插件二次 activate）', () => {
    const manifests = [manifest('com.a', 'activated'), manifest('com.b', 'activated')]

    expect(pluginsNeedingFrontendLoad(manifests, new Set(['com.a'])).map((m) => m.id)).toEqual(['com.b'])
    expect(pluginsNeedingFrontendLoad(manifests, new Set(['com.a', 'com.b']))).toEqual([])
  })

  it('C1 边界：rust-only 与中间态一律不入选（前端无模块 / 等激活完成）', () => {
    const manifests = [
      manifest('com.rust', 'activated', 'rust'),
      manifest('com.activating', 'activating'),
      manifest('com.error', 'error'),
    ]

    expect(pluginsNeedingFrontendLoad(manifests, new Set())).toEqual([])
  })
})

describe('loadMissingActivated（并发合并 + 幂等）', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('C2 并发触发只跑一趟补加载（事件连续到达不得重复扫描/重复 import）', async () => {
    let release: (() => void) | null = null
    const sync = vi
      .spyOn(pluginLoader as unknown as { syncMissingActivated: () => Promise<void> }, 'syncMissingActivated')
      .mockImplementation(
        () =>
          new Promise<void>((resolve) => {
            release = resolve
          }),
      )

    const first = pluginLoader.loadMissingActivated()
    const second = pluginLoader.loadMissingActivated()
    release?.()
    await Promise.all([first, second])

    // 变异对照：去掉在途句柄 → 两次调用各跑一趟（本断言 1 → 2 转红）
    expect(sync).toHaveBeenCalledTimes(1)
  })

  it('C2 一轮结束后句柄释放：下一次事件仍能补加载', async () => {
    const sync = vi
      .spyOn(pluginLoader as unknown as { syncMissingActivated: () => Promise<void> }, 'syncMissingActivated')
      .mockResolvedValue(undefined)

    await pluginLoader.loadMissingActivated()
    await pluginLoader.loadMissingActivated()

    expect(sync).toHaveBeenCalledTimes(2)
  })
})
