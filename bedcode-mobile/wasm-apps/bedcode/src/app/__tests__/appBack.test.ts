/**
 * 应用内返回栈 行为契约测试（移动端系统返回键：二级页 → 应用主页面 → 平台兜底）
 *
 * 被测：`src/app/appBack.ts`（`createAppBackHandler` / `registerAppBack`）
 * 触发背景：真机反馈「从 wasm-app 里的二级页面（终端沉浸页）返回，直接回到平台主界面，
 * 而不是回到应用主页面」——应用层没人接管返回，系统返回直接落到壳的平台兜底。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-A1 | `createAppBackHandler` 正例 | 有可退层 | 后退一步 + 返回 true（消费本次返回） |
 * | C-A2 | 同上，反例 | 各层都到头（应用主页面） | 返回 false 且不调用任何 goBack（返回交平台兜底） |
 * | C-A3 | 声明顺序 = 优先级 | 多层同时可退 | 只后退最靠前那层，后面的层不动 |
 * | C-A4 | 逐级后退 | 连续两次返回 | 第一次退第一层，第二次退第二层 |
 * | C-A5 | 层内多级 | 单层内部还有上级 | 连续返回逐级消费，层内到头后才不消费 |
 * | C-A6 | `registerAppBack` 副作用 | 取消接管 | 返回 context 的 Disposable（dispose 直通，身份一致） |
 * | C-A7 | 清单权限 | 注册 `ui.onBackPressed` | plugin.json 声明 `ui:back`（否则注册即抛权限错误） |
 */
import { describe, it, expect } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import manifest from '../../../plugin.json'
import { createAppBackHandler, registerAppBack, type AppBackLayer } from '../appBack'

/** 返回键通道替身（跨进程边界：宿主壳的 `context.ui.onBackPressed`） */
interface BackChannel {
  context: PluginContext
  /** 模拟一次系统返回，返回本次是否被应用消费 */
  pressBack(): boolean
  /** 注册返回的 Disposable 被 dispose 的次数 */
  disposeCount(): number
}

function makeBackChannel(): BackChannel {
  let handler: ((payload: { canGoBack: boolean }) => boolean | void) | null = null
  let disposeCount = 0
  const context = {
    ui: {
      onBackPressed(next: (payload: { canGoBack: boolean }) => boolean | void) {
        handler = next
        return {
          dispose() {
            disposeCount += 1
          },
        }
      },
    },
  } as unknown as PluginContext
  return {
    context,
    pressBack: () => handler?.({ canGoBack: false }) === true,
    disposeCount: () => disposeCount,
  }
}

/**
 * 二级层替身：`depth` = 该层内部还可后退的级数
 * （0 = 未打开/到头；1 = 打开着；2 = 打开且内部还有上一级）
 */
function makeLayer(name: string, depth: number, log: string[]): AppBackLayer {
  let remaining = depth
  return {
    canGoBack: () => remaining > 0,
    goBack: () => {
      remaining -= 1
      log.push(name)
    },
  }
}

describe('createAppBackHandler', () => {
  it('C-A1 有可退层：后退一步并消费本次返回', () => {
    const log: string[] = []
    const handler = createAppBackHandler([makeLayer('terminal', 1, log)])

    expect(handler()).toBe(true)
    expect(log).toEqual(['terminal'])
  })

  it('C-A2 各层都到头：不消费（交平台兜底）且不调用任何 goBack', () => {
    const log: string[] = []
    const handler = createAppBackHandler([
      makeLayer('terminal', 0, log),
      makeLayer('settings-sheet', 0, log),
    ])

    expect(handler()).toBe(false)
    expect(log).toEqual([])
  })

  it('C-A3 多层可退：只后退声明顺序最靠前的那层', () => {
    const log: string[] = []
    const handler = createAppBackHandler([
      makeLayer('settings-sheet', 1, log),
      makeLayer('conversation-drawer', 1, log),
      makeLayer('provider-config', 1, log),
    ])

    expect(handler()).toBe(true)
    expect(log).toEqual(['settings-sheet'])
  })

  it('C-A4 连续返回：逐级后退（第一层退掉后轮到第二层）', () => {
    const log: string[] = []
    const handler = createAppBackHandler([
      makeLayer('settings-sheet', 1, log),
      makeLayer('conversation-drawer', 1, log),
    ])

    expect(handler()).toBe(true)
    expect(handler()).toBe(true)
    expect(log).toEqual(['settings-sheet', 'conversation-drawer'])
    // 两层都退掉后回到应用主页面：不再消费
    expect(handler()).toBe(false)
  })

  it('C-A5 层内多级：层内逐级消费，直到层内到头', () => {
    const log: string[] = []
    const handler = createAppBackHandler([makeLayer('provider-config', 2, log)])

    expect(handler()).toBe(true)
    expect(handler()).toBe(true)
    expect(log).toEqual(['provider-config', 'provider-config'])
    expect(handler()).toBe(false)
  })
})

describe('registerAppBack', () => {
  it('C-A6 注册到 context.ui.onBackPressed，并直通其 Disposable', () => {
    const channel = makeBackChannel()
    const log: string[] = []
    const disposable = registerAppBack(channel.context, [makeLayer('terminal', 1, log)])

    expect(channel.pressBack()).toBe(true)
    expect(log).toEqual(['terminal'])

    expect(channel.disposeCount()).toBe(0)
    disposable.dispose()
    expect(channel.disposeCount()).toBe(1)
  })

  it('C-A7 清单声明 ui:back（否则 ui.onBackPressed 抛权限错误，返回键接管失效）', () => {
    expect(manifest.permissions).toContain('ui:back')
  })
})
