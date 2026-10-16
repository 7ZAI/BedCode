/**
 * dev-shell 数据源（调试记录 → 壳应用） 行为契约测试
 * -----------------------------------------------------------------------------
 * 契约来源：dev-shell/src/shell/adapters/devAppSource.ts
 *
 * 覆盖：状态映射、权限授予投影（dev-shell 跳过权限检查）、图标归一
 *      （manifest 的 icon 是文件名，不能当 emoji 透传）、权限开关恒为不支持、
 *      运行面解析只认注册面。
 */
import { describe, it, expect, afterEach } from 'vitest'
import { defineComponent, h } from 'vue'
import { getShellRegistry } from '../../src/shell/registry'
import { createDevAppSource, toShellApp } from '../../src/shell/adapters/devAppSource'
import { plugins, type DevPluginRecord } from '../../src/registry'
import { reactive } from 'vue'

function record(id: string, patch: Partial<DevPluginRecord> = {}): DevPluginRecord {
  return reactive({
    id,
    name: id,
    manifest: {} as Record<string, unknown>,
    entry: null,
    state: 'loaded' as const,
    context: null,
    ...patch,
  }) as DevPluginRecord
}

const registry = getShellRegistry()
// 数据源对启动/停止不做假设（依赖倒置）：这里给一组空实现，测的是投影与解析
const source = createDevAppSource({ activate: async () => {}, deactivate: async () => {} })
let counter = 0

afterEach(() => {
  registry.clearSource(source.id)
  plugins.value.length = 0
})

describe('状态映射', () => {
  it('should_mapActivatedToRunning', () => {
    expect(toShellApp(record('a1', { state: 'activated' })).state).toBe('running')
  })

  it('should_mapErrorToError_when_recordFailed', () => {
    const app = toShellApp(record('a1', { state: 'error', error: 'boom' }))

    expect(app.state).toBe('error')
    expect(app.error).toBe('boom')
  })

  it('should_mapDeactivatedToDisabled', () => {
    expect(toShellApp(record('a1', { state: 'deactivated' })).state).toBe('disabled')
  })

  it('should_mapLoadedToStopped', () => {
    expect(toShellApp(record('a1', { state: 'loaded' })).state).toBe('stopped')
  })
})

describe('权限投影', () => {
  it('should_grantDeclaredPermissions_when_running', () => {
    const app = toShellApp(
      record('a1', {
        state: 'activated',
        manifest: { permissions: ['storage', 'fs:read'] },
      }),
    )

    expect(app.permissions.map((p) => p.key)).toEqual(['storage', 'fs:read'])
    expect(app.permissions.every((p) => p.granted)).toBe(true)
  })

  it('should_lockStoragePermission', () => {
    const app = toShellApp(
      record('a1', { state: 'activated', manifest: { permissions: ['storage', 'bus'] } }),
    )

    const storage = app.permissions.find((p) => p.key === 'storage')
    const bus = app.permissions.find((p) => p.key === 'bus')
    expect(storage?.locked).toBe(true)
    expect(bus?.locked).toBe(false)
  })

  it('should_notGrantPermissions_when_appFailedToStart', () => {
    // 「明明没起来」却在列表里显示成满权限在跑，是最容易被误读的状态
    const app = toShellApp(
      record('a1', { state: 'error', error: 'x', manifest: { permissions: ['storage'] } }),
    )

    expect(app.permissions[0].granted).toBe(false)
  })

  it('should_ignoreNonStringPermissionEntries', () => {
    const app = toShellApp(
      record('a1', { state: 'activated', manifest: { permissions: ['bus', 42, null] } }),
    )

    expect(app.permissions.map((p) => p.key)).toEqual(['bus'])
  })
})

describe('图标与元信息', () => {
  it('should_fallbackToGenericGlyph_when_iconIsFileName', () => {
    // manifest 的 icon 是文件名（icon.svg）；直接透传会被壳判成 emoji 并渲染字面量
    const app = toShellApp(record('a1', { manifest: { icon: 'icon.svg' } }))

    expect(app.icon).not.toBe('icon.svg')
    expect(typeof app.icon).toBe('string')
  })

  it('should_passThroughEmojiIcon', () => {
    expect(toShellApp(record('a1', { manifest: { icon: '🧩' } })).icon).toBe('🧩')
  })

  it('should_leaveIconUndefined_when_absent', () => {
    expect(toShellApp(record('a1')).icon).toBeUndefined()
  })

  it('should_defaultVersion_when_manifestHasNone', () => {
    expect(toShellApp(record('a1')).version).toBe('0.0.0')
  })

  it('should_markBuiltinAsOfficial', () => {
    expect(toShellApp(record('a1', { builtin: true })).official).toBe(true)
    expect(toShellApp(record('a2')).official).toBe(false)
  })
})

describe('数据源行为', () => {
  it('should_listEveryRecordAsApp', async () => {
    counter += 1
    const a1 = record(`list-a1-${counter}`)
    const a2 = record(`list-a2-${counter}`)
    plugins.value.push(a1, a2)

    const list = await source.list()

    expect(list.map((a) => a.id)).toEqual([a1.id, a2.id])
  })

  it('should_refusePerPermissionGrant', async () => {
    // 预览环境没有可写的权限真源；返回 true 会让开发者误以为权限逻辑已验证
    await expect(source.setPermissionGrant?.('a1', 'fs:read', true)).resolves.toBe(false)
  })

  it('should_resolveRegisteredSurface', () => {
    counter += 1
    const appId = `surf-${counter}`
    registry.upsertApps(source.id, [
      {
        id: appId,
        name: appId,
        version: '1.0.0',
        state: 'stopped',
        permissions: [],
        contributions: {},
      },
    ])
    const surface = defineComponent({ name: 'S', setup: () => () => h('div') })
    registry.registerSurface(appId, { component: surface })

    expect(source.resolveSurface?.(appId)).toBe(surface)
  })

  it('should_returnUndefinedSurface_when_appRegisteredNone', () => {
    counter += 1
    const appId = `nosurf-${counter}`
    registry.upsertApps(source.id, [
      {
        id: appId,
        name: appId,
        version: '1.0.0',
        state: 'stopped',
        permissions: [],
        contributions: {},
      },
    ])

    // 不回退任何其它形态：运行屏会显式渲染空态并说明原因
    expect(source.resolveSurface?.(appId)).toBeUndefined()
  })
})