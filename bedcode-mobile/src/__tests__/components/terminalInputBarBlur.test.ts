/**
 * TerminalInputBar 键盘收起退出编辑态契约测试
 *
 * 场景：Android 返回键/下拉手势收起系统键盘时，WebView 的 textarea 仍保有
 * 焦点（输入光标常驻）。TerminalView 通过双通道键盘检测在偏移归零瞬间调用
 * TerminalInputBar 公开的 blurInput()，期望：输入光标消失、编辑态（isFocused）
 * 翻转。本测试用与 TerminalView 相同的 template ref → 公开 API 驱动方式验证。
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { nextTick, ref } from 'vue'
import { mount } from '@vue/test-utils'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue([]),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({ error: vi.fn(), warning: vi.fn() }),
}))

vi.mock('@/composables/useMobileSettings', () => ({
  useMobileSettings: () => ({
    settings: { value: { vibrate: false } },
  }),
}))

// 只提供本次测试用到的 store 面（render 所需 + 面板轮播交互）。
// Pinia store 实例上 ref 会被自动解包：presetCommands/visiblePanelShortcuts/
// shortcutConfig 直接是数组（非 { value: [] }），展开语法才可用
vi.mock('@/stores/inputAssistant', () => ({
  useInputAssistantStore: () => ({
    presetCommands: [],
    visiblePanelShortcuts: [],
    shortcutConfig: [],
    getQuickBarItems: () => [],
    recordShortcut: vi.fn(),
    recordCustomCommand: vi.fn(),
  }),
}))

import TerminalInputBarBlurHost from '@/__tests__/integration/fixtures/terminalInputBarBlurHost.vue'

describe('TerminalInputBar 键盘收起退出编辑态', () => {
  let wrapper: ReturnType<typeof mount>

  beforeEach(() => {
    wrapper = mount(TerminalInputBarBlurHost, {
      global: {
        // 真实应用中 safeArea 由 App.vue inject；组件内已用可选链兜底，
        // 这里显式提供以消除 Vue warn 并贴近真实行为
        provide: {
          safeArea: ref({ top: 0, bottom: 0, navigationBar: 0 }),
        },
      },
    })
  })

  it('初始未聚焦：isFocused() 为 false，输入框无光标', () => {
    const host = wrapper.vm as unknown as {
      isFocused: () => boolean
      blurInput: () => void
    }
    expect(host.isFocused()).toBe(false)
  })

  it('聚焦后进入编辑态，blurInput() 退出编辑态并移除输入光标', async () => {
    const host = wrapper.vm as unknown as {
      isFocused: () => boolean
      blurInput: () => void
    }
    const textarea = wrapper.find('textarea')
    expect(textarea.exists()).toBe(true)

    // 聚焦（等价用户点按输入框，@focus → handleFocus → 编辑态开启）
    await textarea.trigger('focus')
    expect(host.isFocused()).toBe(true)

    // 键盘被系统收起：父组件调用 blurInput()。happy-dom 不模拟
    // activeElement，改用 spy 元素原生 blur 验证「移除光标」动作确实发出
    const blurSpy = vi.spyOn(textarea.element as HTMLTextAreaElement, 'blur')
    host.blurInput()
    expect(blurSpy).toHaveBeenCalledTimes(1)

    // 真实浏览器中原生 blur 会派发 blur 事件 → handleBlur → 编辑态翻转
    // （happy-dom 不自动派发，显式触发补上事件路径）
    await textarea.trigger('blur')
    expect(host.isFocused()).toBe(false)
  })

  it('未聚焦时 blurInput() 为 no-op，不抛错', async () => {
    const host = wrapper.vm as unknown as {
      isFocused: () => boolean
      blurInput: () => void
    }
    expect(() => {
      host.blurInput()
    }).not.toThrow()
    await nextTick()
    expect(host.isFocused()).toBe(false)
  })
})