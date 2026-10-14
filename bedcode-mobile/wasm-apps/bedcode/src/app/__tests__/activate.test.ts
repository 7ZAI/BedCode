/**
 * 应用壳域激活 行为契约测试
 * （票 2026-10-10：全量 UI 下沉 —— 底部导航 + 页签容器）
 *
 * 被测：`src/app/activate.ts`（activateAppDomain）。
 * 运行面 = terminal-session 在宿主壳内的入口（旧宿主主流程的承继面）。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-AA1 | registerMessages ×2 | zh-CN / en 两份文案都注册 | 2 次调用，locale 各一 |
 * | C-AA2 | 注册顺序 | 文案必须在运行面 setup 前就位 | 文案调用序号早于运行面 |
 * | C-AA3 | registerSurface | 注册唯一运行面 = AppRoot | 组件引用相等 |
 * | C-AA4 | dispose | 运行面回收一次 | dispose 1 次 |
 * | C-AA5 | i18n 双语同步 | zh-CN 与 en 的 app 键集合完全一致 | 键路径数组相等 |
 * | C-AA6 | 导航文案齐备 | 4 个内置页签文案双语都在 | app.nav.{connection,sessions,toolbox,settings} |
 */
import { describe, it, expect, vi } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import { activateAppDomain } from '../activate'
import AppRoot from '../AppRoot.vue'
import { messagesEn, messagesZhCN } from '../i18n'
import {
  messagesEn as settingsEn,
  messagesZhCN as settingsZhCN,
} from '../../settings/i18n'

/** 收集对象树的键路径（双语键集合比对用） */
function keyPaths(obj: Record<string, unknown>, prefix = ''): string[] {
  return Object.entries(obj).flatMap(([key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key
    return value && typeof value === 'object'
      ? keyPaths(value as Record<string, unknown>, path)
      : [path]
  })
}

/** 最小插件上下文替身（本域用到的面：i18n / ui / logger） */
function createFakeContext() {
  const surfaceDispose = vi.fn()
  const registerMessages = vi.fn((_locale: string, _messages: Record<string, unknown>) => {})
  const registerSurface = vi.fn((_surface: unknown) => ({ dispose: surfaceDispose }))
  const context = {
    i18n: { registerMessages, t: (key: string) => key },
    ui: { registerSurface },
    logger: { info: vi.fn() },
  } as unknown as PluginContext
  return { context, registerMessages, registerSurface, surfaceDispose }
}

describe('C-AA1/C-AA2 文案注册与顺序', () => {
  it('should_registerBothLocalesBeforeSurface_when_activated', () => {
    const fake = createFakeContext()
    activateAppDomain(fake.context)

    expect(fake.registerMessages).toHaveBeenCalledTimes(4)
    expect(fake.registerMessages.mock.calls.map((c) => c[0]).sort()).toEqual([
      'en',
      'en',
      'zh-CN',
      'zh-CN',
    ])
    expect(fake.registerMessages.mock.calls.map((c) => c[1])).toEqual([
      messagesZhCN,
      messagesEn,
      settingsZhCN,
      settingsEn,
    ])

    const [firstMessagesCall] = fake.registerMessages.mock.invocationCallOrder
    const [surfaceCall] = fake.registerSurface.mock.invocationCallOrder
    expect(firstMessagesCall).toBeLessThan(surfaceCall)
  })

  it('should_registerSettingsMessages_when_runSurfaceContainsSettingsTab', () => {
    // 反例防护：设置页签在运行面内，漏注册 settings.* 会让整页回显键名
    const fake = createFakeContext()
    activateAppDomain(fake.context)

    const registered = fake.registerMessages.mock.calls.map((c) => c[1])
    expect(registered).toContain(settingsZhCN)
    expect(registered).toContain(settingsEn)
  })
})

describe('C-AA3 运行面注册', () => {
  it('should_registerAppRootSurface_when_activated', () => {
    const fake = createFakeContext()
    activateAppDomain(fake.context)

    expect(fake.registerSurface).toHaveBeenCalledTimes(1)
    expect(fake.registerSurface.mock.calls[0][0]).toEqual({ component: AppRoot })
  })
})

describe('C-AA4 回收语义', () => {
  it('should_disposeSurfaceOnce_when_domainDisposed', () => {
    const fake = createFakeContext()
    const domain = activateAppDomain(fake.context)

    domain.dispose()

    expect(fake.surfaceDispose).toHaveBeenCalledTimes(1)
  })
})

describe('C-AA5/C-AA6 i18n 双语', () => {
  it('should_haveIdenticalKeySets_when_bothLocalesAuthored', () => {
    const zhKeys = keyPaths(messagesZhCN as unknown as Record<string, unknown>).sort()
    const enKeys = keyPaths(messagesEn as unknown as Record<string, unknown>).sort()

    expect(zhKeys.length).toBeGreaterThan(0)
    expect(enKeys).toEqual(zhKeys)
  })

  it('should_carryAllBuiltinTabLabels_when_authored', () => {
    // 4 个内置页签的文案必须双语齐备：缺一条导航项就回显 key（页面露英文/键名）
    const required = [
      'app.nav.connection',
      'app.nav.sessions',
      'app.nav.toolbox',
      'app.nav.settings',
      'app.nav.label',
    ]
    const zhKeys = keyPaths(messagesZhCN as unknown as Record<string, unknown>)
    const enKeys = keyPaths(messagesEn as unknown as Record<string, unknown>)

    for (const key of required) {
      expect(zhKeys).toContain(key)
      expect(enKeys).toContain(key)
    }
  })

  it('should_translateEveryNavLabel_when_localesDiffer', () => {
    // 反例防护：zh 与 en 的导航文案不得相同（复制粘贴漏翻的典型症状）
    const navKeys = ['connection', 'sessions', 'toolbox', 'settings', 'label']
    for (const key of navKeys) {
      const zh = (messagesZhCN.app.nav as Record<string, string>)[key]
      const en = (messagesEn.app.nav as Record<string, string>)[key]

      expect(zh).toBeTruthy()
      expect(en).toBeTruthy()
      expect(zh).not.toBe(en)
    }
  })
})