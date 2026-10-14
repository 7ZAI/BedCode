/**
 * 壳运行面解析优先级 行为契约测试
 * （票 2026-10-09-mobile-host-into-wasm-apps，阶段 A2：运行面 ① 应用自带 → ② 数据源延迟解析）
 *
 * 被测：`src/shell/composables/useShellApps.ts::resolveSurface`。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-R1 | `contributions.surface?.component ?? source.resolveSurface` | 应用自带运行面优先 | 返回自带组件（同一引用，非副本） |
 * | C-R2 | 同上（左值为空时回退） | 无自带运行面时回退数据源解析 | 返回数据源组件 |
 * | C-R3 | 同上（两级都空） | 都没有时不留白 | undefined（由运行屏渲染空态） |
 * | C-R4 | `registry.getApp` 缺省分支 | 未知 appId | undefined |
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { defineComponent, h, type Component } from 'vue'
import { getShellRegistry } from '@/shell/registry'
import { useShellApps } from '@/shell/composables/useShellApps'
import type { ShellApp, ShellAppSource } from '@/shell/types'

const SOURCE_ID = 'surface-priority-source'
const APP_WITH_OWN_SURFACE = 'com.test.with-own-surface'
const APP_SOURCE_ONLY = 'com.test.source-only'
const APP_NEITHER = 'com.test.neither'

const ownSurface: Component = defineComponent({
  name: 'OwnSurface',
  setup: () => () => h('div', 'own'),
})
const sourceSurface: Component = defineComponent({
  name: 'SourceSurface',
  setup: () => () => h('div', 'source'),
})

const shell = getShellRegistry()

function app(id: string): ShellApp {
  return { id, name: id, version: '1.0.0', state: 'stopped', permissions: [], contributions: {} }
}

beforeEach(() => {
  const source: ShellAppSource = {
    id: SOURCE_ID,
    list: async () => [],
    launch: async () => {},
    stop: async () => {},
    // 数据源延迟解析：仅 APP_NEITHER 无解析结果（用于区分「回退」与「都空」）
    resolveSurface: (appId: string) => (appId === APP_NEITHER ? undefined : sourceSurface),
  }
  shell.registerSource(source)
  shell.upsertApps(SOURCE_ID, [app(APP_WITH_OWN_SURFACE), app(APP_SOURCE_ONLY), app(APP_NEITHER)])
  shell.registerSurface(APP_WITH_OWN_SURFACE, { component: ownSurface })
})

afterEach(() => {
  for (const id of [APP_WITH_OWN_SURFACE, APP_SOURCE_ONLY, APP_NEITHER]) shell.clearApp(id)
  shell.clearSource(SOURCE_ID)
})

describe('useShellApps.resolveSurface 优先级', () => {
  it('should_preferOwnSurfaceOverSource_when_bothResolve', () => {
    const { resolveSurface } = useShellApps()
    // 同一引用：注册表若把组件换成副本，缓存身份会漂、组件状态会丢
    expect(resolveSurface(APP_WITH_OWN_SURFACE)).toBe(ownSurface)
  })

  it('should_fallBackToSource_when_noOwnSurface', () => {
    const { resolveSurface } = useShellApps()
    expect(resolveSurface(APP_SOURCE_ONLY)).toBe(sourceSurface)
  })

  it('should_returnUndefined_when_neitherOwnNorSourceResolves', () => {
    const { resolveSurface } = useShellApps()
    expect(resolveSurface(APP_NEITHER)).toBeUndefined()
  })

  it('should_returnUndefined_when_appUnknown', () => {
    const { resolveSurface } = useShellApps()
    expect(resolveSurface('com.test.unknown-app')).toBeUndefined()
  })
})
