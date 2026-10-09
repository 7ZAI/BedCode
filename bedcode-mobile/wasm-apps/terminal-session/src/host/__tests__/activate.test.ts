/**
 * 宿主页域激活 行为契约测试
 * （票 2026-10-09-mobile-host-into-wasm-apps，阶段 A4/A5）
 *
 * 被测：`src/host/activate.ts`（activateHostPageDomain）。
 * 宿主页 = terminal-session 在宿主壳内的运行面（HostPage）＋ 首页「活跃会话」
 * 快捷卡片槽位；由域入口注册、由 deactivate 对称回收。本测试钉住这条接线、
 * 注册顺序（文案先于组件）与双语同步。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-P1 | registerMessages ×2 | zh-CN / en 两份文案都注册 | 2 次调用，locale 各一 |
 * | C-P2 | 注册顺序 | 文案必须在运行面 setup 前就位（否则模板取键失败） | 文案调用序号早于运行面 |
 * | C-P3 | registerSurface | 注册唯一运行面 = HostPage | 组件引用相等 |
 * | C-P4 | registerSlot | 首页槽位 id='host-sessions'、order=10 | 入参形状 |
 * | C-P5 | dispose | 运行面与槽位各自回收一次 | 两个 dispose 各 1 次 |
 * | C-P6 | i18n 双语同步 | zh-CN 与 en 的 hub 键集合完全一致（无漏译） | 键路径数组相等 |
 */
import { describe, it, expect, vi } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import { activateHostPageDomain } from '../activate'
import HostPage from '../HostPage.vue'
import { messagesEn, messagesZhCN } from '../i18n'

/** 收集对象树的键路径（双语键集合比对用） */
function keyPaths(obj: Record<string, unknown>, prefix = ''): string[] {
  return Object.entries(obj).flatMap(([key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key
    return value && typeof value === 'object'
      ? keyPaths(value as Record<string, unknown>, path)
      : [path]
  })
}

/** 最小插件上下文替身（只装本域用到的三面：i18n / ui / logger） */
function createFakeContext() {
  const surfaceDispose = vi.fn()
  const slotDispose = vi.fn()
  const registerMessages = vi.fn((_locale: string, _messages: Record<string, unknown>) => {})
  const registerSurface = vi.fn((_surface: unknown) => ({ dispose: surfaceDispose }))
  const registerSlot = vi.fn((_slot: unknown) => ({ dispose: slotDispose }))
  const logInfo = vi.fn()
  const context = {
    i18n: { registerMessages, t: (key: string) => key },
    ui: { registerSurface, registerSlot },
    logger: { info: logInfo },
  } as unknown as PluginContext
  return { context, registerMessages, registerSurface, registerSlot, surfaceDispose, slotDispose }
}

describe('C-P1/C-P2 文案注册与顺序', () => {
  it('should_registerBothLocalesBeforeSurface_when_activated', () => {
    const fake = createFakeContext()
    activateHostPageDomain(fake.context)

    expect(fake.registerMessages).toHaveBeenCalledTimes(2)
    expect(fake.registerMessages.mock.calls.map((c) => c[0]).sort()).toEqual(['en', 'zh-CN'])
    expect(fake.registerMessages.mock.calls[0][1]).toBe(messagesZhCN)
    expect(fake.registerMessages.mock.calls[1][1]).toBe(messagesEn)

    // 文案先于运行面：组件 setup 期就要能取到 hub.* 键
    const [firstMessagesCall] = fake.registerMessages.mock.invocationCallOrder
    const [surfaceCall] = fake.registerSurface.mock.invocationCallOrder
    expect(firstMessagesCall).toBeLessThan(surfaceCall)
  })
})

describe('C-P3/C-P4 运行面与槽位注册', () => {
  it('should_registerHostPageSurfaceAndSlot_when_activated', () => {
    const fake = createFakeContext()
    activateHostPageDomain(fake.context)

    expect(fake.registerSurface).toHaveBeenCalledTimes(1)
    expect(fake.registerSurface.mock.calls[0][0]).toEqual({ component: HostPage })

    expect(fake.registerSlot).toHaveBeenCalledTimes(1)
    expect(fake.registerSlot.mock.calls[0][0]).toEqual({
      id: 'host-sessions',
      component: expect.anything(),
      order: 10,
    })
  })
})

describe('C-P5 回收语义', () => {
  it('should_disposeSurfaceAndSlotOnce_when_domainDisposed', () => {
    const fake = createFakeContext()
    const domain = activateHostPageDomain(fake.context)

    domain.dispose()

    expect(fake.surfaceDispose).toHaveBeenCalledTimes(1)
    expect(fake.slotDispose).toHaveBeenCalledTimes(1)
  })
})

describe('C-P6 i18n 双语同步', () => {
  it('should_haveIdenticalKeySets_when_bothLocalesAuthored', () => {
    const zhKeys = keyPaths(messagesZhCN as unknown as Record<string, unknown>).sort()
    const enKeys = keyPaths(messagesEn as unknown as Record<string, unknown>).sort()

    expect(zhKeys.length).toBeGreaterThan(0)
    expect(enKeys).toEqual(zhKeys)
  })
})
