/**
 * Android 系统返回键分发 行为契约测试
 *
 * 被测：`src/plugin/back-button.ts`（宿主前端唯一原生监听 + 应用/平台两层分发）
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-B1 | `dispatchBackPress` 应用段 | 应用处理器返回 true = 已消费 | 后续应用与平台处理器都不被调用 |
 * | C-B2 | 同上（未消费） | 返回 undefined / false 时不吞返回 | 下一层处理器仍被调用 |
 * | C-B3 | `dispatchBackPress` 平台段 | 平台处理器后注册优先（栈式兜底） | 后注册者先收到；它消费后先注册者不再收到 |
 * | C-B4 | `subscribe*` 返回的 dispose | 退订后不再参与分发 | 处理器零调用 |
 * | C-B5 | `invokeHandler` 异常隔离 | 单个处理器抛错不打断整条链 | 后续处理器照常收到，且错误落日志 |
 * | C-B6 | 无消费者 | 所有处理器都不消费 / 无订阅者 | 不抛错；无订阅者时不注册原生监听 |
 * | C-B7 | `ensureNative` 生命周期 | 有订阅者才持有原生监听；全部退订即摘除 | onBackButtonPress 注册一次 → unregister 被调用 |
 * | C-B8 | 原生事件入口 | 系统返回经原生 handler 进入同一分发链 | 应用处理器收到载荷 |
 *
 * 每个用例 `vi.resetModules()` 后重新 import：被测模块持有原生监听单例状态
 * （`unregisterNative` / `registering`），共享实例会让「上一个用例遗留的异步注册」
 * 跨用例改写计数，断言失去确定性。
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'

// Tauri app 插件是跨进程边界：按契约替身（记录原生 handler + unregister）
const tauri = vi.hoisted(() => ({
  handlers: [] as Array<(payload: { canGoBack: boolean }) => void>,
  unregister: vi.fn(async () => {}),
  onBackButtonPress: vi.fn(async (handler: (payload: { canGoBack: boolean }) => void) => {
    tauri.handlers.push(handler)
    return { unregister: tauri.unregister }
  }),
}))

const logger = vi.hoisted(() => ({
  error: vi.fn(),
  warn: vi.fn(),
  info: vi.fn(),
  debug: vi.fn(),
  log: vi.fn(),
}))

vi.mock('@tauri-apps/api/app', () => ({
  onBackButtonPress: tauri.onBackButtonPress,
}))
vi.mock('@/utils/frontendLogger', () => ({ logger }))

type BackButtonModule = typeof import('@/plugin/back-button')

/** 每个用例自己的被测模块实例（详见文件头注） */
let mod: BackButtonModule
/** 已注册的退订函数（afterEach 统一清理，避免用例间串味） */
const disposers: Array<() => void> = []

function subscribeApp(handler: Parameters<BackButtonModule['subscribeAppBackPressed']>[0]): void {
  disposers.push(mod.subscribeAppBackPressed(handler))
}

function subscribeHost(handler: Parameters<BackButtonModule['subscribeHostBackPressed']>[0]): void {
  disposers.push(mod.subscribeHostBackPressed(handler))
}

beforeEach(async () => {
  vi.resetModules()
  tauri.handlers.length = 0
  tauri.onBackButtonPress.mockClear()
  tauri.unregister.mockClear()
  logger.error.mockClear()
  mod = await import('@/plugin/back-button')
})

afterEach(() => {
  for (const dispose of disposers.splice(0)) dispose()
})

describe('C-B1/C-B2 应用层消费语义', () => {
  it('should_stopDispatch_when_appHandlerConsumes', () => {
    const consuming = vi.fn(() => true)
    const nextApp = vi.fn(() => true)
    const host = vi.fn(() => true)
    subscribeApp(consuming)
    subscribeApp(nextApp)
    subscribeHost(host)

    mod.dispatchBackPress({ canGoBack: false })

    expect(consuming).toHaveBeenCalledWith({ canGoBack: false })
    expect(nextApp).not.toHaveBeenCalled()
    expect(host).not.toHaveBeenCalled()
  })

  it('should_continueDispatch_when_appHandlerDoesNotConsume', () => {
    const undecided = vi.fn(() => undefined)
    const rejected = vi.fn(() => false)
    const host = vi.fn(() => true)
    subscribeApp(undecided)
    subscribeApp(rejected)
    subscribeHost(host)

    mod.dispatchBackPress({ canGoBack: true })

    expect(undecided).toHaveBeenCalledTimes(1)
    expect(rejected).toHaveBeenCalledTimes(1)
    // 应用都没消费 → 平台兜底必须拿到同一次返回（否则用户按返回无反应）
    expect(host).toHaveBeenCalledWith({ canGoBack: true })
  })
})

describe('C-B3 平台层栈式兜底', () => {
  it('should_callNewestHostHandlerFirst_when_multipleHostHandlersRegistered', () => {
    const outer = vi.fn(() => true)
    const inner = vi.fn(() => true)
    subscribeHost(outer)
    subscribeHost(inner)

    mod.dispatchBackPress({ canGoBack: false })

    // 后注册（内层）优先；它消费后外层不再收到
    expect(inner).toHaveBeenCalledTimes(1)
    expect(outer).not.toHaveBeenCalled()
  })

  it('should_fallThroughToOlderHostHandler_when_newestDoesNotConsume', () => {
    const outer = vi.fn(() => true)
    const inner = vi.fn(() => false)
    subscribeHost(outer)
    subscribeHost(inner)

    mod.dispatchBackPress({ canGoBack: false })

    expect(inner).toHaveBeenCalledTimes(1)
    expect(outer).toHaveBeenCalledTimes(1)
  })
})

describe('C-B4 退订', () => {
  it('should_notInvokeHandler_when_disposed', () => {
    const handler = vi.fn(() => true)
    const disposeApp = mod.subscribeAppBackPressed(handler)
    const disposeHost = mod.subscribeHostBackPressed(handler)

    disposeApp()
    disposeHost()
    mod.dispatchBackPress({ canGoBack: false })

    expect(handler).not.toHaveBeenCalled()
  })
})

describe('C-B5 异常隔离', () => {
  it('should_keepDispatching_when_handlerThrows', () => {
    const broken = vi.fn(() => {
      throw new Error('handler boom')
    })
    const host = vi.fn(() => true)
    subscribeApp(broken)
    subscribeHost(host)

    expect(() => mod.dispatchBackPress({ canGoBack: false })).not.toThrow()

    expect(host).toHaveBeenCalledTimes(1)
    expect(logger.error).toHaveBeenCalledTimes(1)
  })
})

describe('C-B6 无消费者', () => {
  it('should_doNothing_when_noHandlerConsumes', () => {
    const handler = vi.fn(() => false)
    subscribeApp(handler)

    expect(() => mod.dispatchBackPress({ canGoBack: true })).not.toThrow()
    expect(handler).toHaveBeenCalledWith({ canGoBack: true })
  })

  it('should_notRegisterNativeListener_when_noSubscriberEverRegistered', async () => {
    mod.dispatchBackPress({ canGoBack: true })
    await new Promise((resolve) => setTimeout(resolve, 0))

    // 无订阅者时不得注册原生监听（否则系统默认的「返回即退出」永久失效）
    expect(tauri.onBackButtonPress).not.toHaveBeenCalled()
  })
})

describe('C-B7/C-B8 原生监听生命周期', () => {
  it('should_registerOnceForManySubscribers_andReleaseWhenAllDisposed', async () => {
    const a = mod.subscribeAppBackPressed(() => false)
    const b = mod.subscribeHostBackPressed(() => false)
    await vi.waitFor(() => expect(tauri.onBackButtonPress).toHaveBeenCalledTimes(1))

    a()
    // 仍有订阅者：监听必须在场（C-B7 前半）
    expect(tauri.unregister).not.toHaveBeenCalled()

    b()
    // 全部退订：摘除监听，恢复系统默认返回（主屏按返回 = 退出应用）
    await vi.waitFor(() => expect(tauri.unregister).toHaveBeenCalledTimes(1))
  })

  it('should_routeNativeBackPressIntoDispatch_when_listenerFires', async () => {
    const app = vi.fn(() => true)
    subscribeApp(app)
    await vi.waitFor(() => expect(tauri.handlers).toHaveLength(1))

    tauri.handlers[0]({ canGoBack: true })

    expect(app).toHaveBeenCalledWith({ canGoBack: true })
  })
})
