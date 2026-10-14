/**
 * 宿主页域激活 行为契约测试
 * （票 2026-10-09-mobile-host-into-wasm-apps，阶段 A4/A5；票 2026-10-10 调整）
 *
 * 被测：`src/host/activate.ts`（activateHostPageDomain）。
 *
 * 票 2026-10-10 起本域职责收窄：运行面（底部导航 + 页签容器）迁至 app 域
 * （`src/app/activate.ts`），本域只出内容面（设备 / 会话）与首页槽位。
 * 「旧读路径删除」在此钉住——域入口不得再注册运行面。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-P1 | registerMessages ×2 | zh-CN / en 两份文案都注册 | 2 次调用，locale 各一 |
 * | C-P2 | 注册顺序 | 文案必须在运行面 setup 前就位 | 文案调用早于槽位 |
 * | C-P3 | 退役面 | 本域**不再**注册运行面（票 2026-10-10） | registerSurface 零调用 |
 * | C-P4 | registerSlot | 首页槽位 id='host-sessions'、order=10 | 入参形状 |
 * | C-P5 | dispose | 槽位回收一次 | dispose 1 次 |
 * | C-P6 | i18n 双语同步 | zh-CN 与 en 的 hub 键集合完全一致 | 键路径数组相等 |
 */
import { describe, it, expect, vi } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import { activateHostPageDomain } from '../activate'
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

/** 最小插件上下文替身（本域用到的面：i18n / ui / logger） */
function createFakeContext() {
  const slotDispose = vi.fn()
  const surfaceDispose = vi.fn()
  const registerMessages = vi.fn((_locale: string, _messages: Record<string, unknown>) => {})
  const registerSlot = vi.fn((_slot: unknown) => ({ dispose: slotDispose }))
  const registerSurface = vi.fn((_surface: unknown) => ({ dispose: surfaceDispose }))
  const logInfo = vi.fn()
  const context = {
    i18n: { registerMessages, t: (key: string) => key },
    ui: { registerSlot, registerSurface },
    logger: { info: logInfo },
  } as unknown as PluginContext
  return {
    context,
    registerMessages,
    registerSlot,
    registerSurface,
    slotDispose,
    surfaceDispose,
  }
}

describe('C-P1/C-P2 文案注册与顺序', () => {
  it('should_registerBothLocalesBeforeSlot_when_activated', () => {
    const fake = createFakeContext()
    activateHostPageDomain(fake.context)

    expect(fake.registerMessages).toHaveBeenCalledTimes(2)
    expect(fake.registerMessages.mock.calls.map((c) => c[0]).sort()).toEqual(['en', 'zh-CN'])
    expect(fake.registerMessages.mock.calls[0][1]).toBe(messagesZhCN)
    expect(fake.registerMessages.mock.calls[1][1]).toBe(messagesEn)

    // 文案先于注册项：组件 setup 期就要能取到 hub.* 键
    const [firstMessagesCall] = fake.registerMessages.mock.invocationCallOrder
    const [slotCall] = fake.registerSlot.mock.invocationCallOrder
    expect(firstMessagesCall).toBeLessThan(slotCall)
  })
})

describe('C-P3 退役面：运行面已迁 app 域', () => {
  it('should_notRegisterSurface_when_domainActivated', () => {
    const fake = createFakeContext()
    activateHostPageDomain(fake.context)

    expect(fake.registerSurface).not.toHaveBeenCalled()
  })
})

describe('C-P4 槽位注册', () => {
  it('should_registerHomeSlot_when_activated', () => {
    const fake = createFakeContext()
    activateHostPageDomain(fake.context)

    expect(fake.registerSlot).toHaveBeenCalledTimes(1)
    expect(fake.registerSlot.mock.calls[0][0]).toEqual({
      id: 'host-sessions',
      component: expect.anything(),
      order: 10,
    })
  })
})

describe('C-P5 回收语义', () => {
  it('should_disposeSlotOnce_when_domainDisposed', () => {
    const fake = createFakeContext()
    const domain = activateHostPageDomain(fake.context)

    domain.dispose()

    expect(fake.slotDispose).toHaveBeenCalledTimes(1)
    expect(fake.surfaceDispose).not.toHaveBeenCalled()
  })

  it('should_disposeSlotOnce_when_domainDisposedTwice_isIdempotentPerDisposable', () => {
    const fake = createFakeContext()
    const domain = activateHostPageDomain(fake.context)

    domain.dispose()
    domain.dispose()

    // Disposable 自身不幂等：重复 dispose 会重复调用下游——此处钉住「每次 dispose 恰好一次
    // 下游调用」，真正的幂等由 deactivate 侧保证不重复调用 domain.dispose()
    expect(fake.slotDispose).toHaveBeenCalledTimes(2)
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