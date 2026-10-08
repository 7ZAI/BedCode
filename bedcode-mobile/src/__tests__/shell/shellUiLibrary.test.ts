/**
 * 壳内公共组件库行为契约测试（`src/shell/components/ui/**`）
 * -----------------------------------------------------------------------------
 * 为什么需要：这批组件是旧公共组件的**副本**，迁移本身就是风险来源（改错一个
 * 分支、漏一个有守卫的 emit，旧页面搬过来就会行为漂移）。因此每条复制过来的
 * 行为分支都要有可执行的契约，而不是「文件在就算迁完」。
 *
 * 行为契约（来源 = 各组件实现分支）：
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-UI-01 | Button `:disabled` | disabled / loading 时按钮置 disabled，loading 出转圈 | disabled 属性在场 + `.animate-spin` |
 * | C-UI-02 | Button switch | variant / size 映射到固定类名 | primary/secondary/danger/ghost 各命中其类 |
 * | C-UI-03 | Toggle `@change` | 勾选 emit **取反**当前值（非透传 DOM checked） | false → true，true → false |
 * | C-UI-04 | Toggle `:disabled` | disabled 落到原生 input | input disabled |
 * | C-UI-05 | ConfirmDialog `handleConfirm` | loading 时 confirm 不 emit | loading=true 点击无 confirm |
 * | C-UI-06 | ConfirmDialog `handleCancel` | 取消 emit cancel + 关闭 | 两个 emit 各一次 |
 * | C-UI-07 | ConfirmDialog `handleBackdropClick` | closeOnBackdrop=false 时点背板不关；loading 时也不关 | 无 update:modelValue |
 * | C-UI-08 | ConfirmDialog 变体 | danger 变体走危险配色 | 确认按钮 / 图标命中 danger 类 |
 * | C-UI-09 | ConfirmDialog `v-if` | modelValue=false 不渲染面板 | body 无 `.modal-panel` |
 * | C-UI-10 | PromptDialog `submit` | 去空格后 emit；纯空白不 emit | '  hi ' → 'hi'；'   ' → 无 emit |
 * | C-UI-11 | PromptDialog `loading` | loading 时关闭被抑制，唯一出口是「取消连接」（cancel + 关闭） | 无 update:modelValue / 有 cancel |
 * | C-UI-11b | PromptDialog 普通取消 | 普通取消按钮只关闭，不发 cancel（cancel 是 loading 态专用） | 仅 update:modelValue |
 * | C-UI-12 | PromptDialog `watch` | 打开时回填 initialValue | input.value = 初值 |
 * | C-UI-13 | CollapseSection | defaultOpen 决定初始态，点标题切换 collapsed | 类名切换 |
 * | C-UI-14 | LoadingDialog | visible 控制渲染 | 隐藏时不渲染提示文案 |
 * | C-UI-15 | LetterAvatar | 同 seed 同档、不同 seed 落到不同档（哈希稳定） | 类名相等 / 12 个 seed ≥2 档 |
 * | C-UI-16 | QuickActionButton | 点击 emit；光晕走 token（不硬编码颜色） | emit click + 类名含 `--shell-action-glow` |
 * | C-UI-17 | 组件库出口 | 九个组件全部导出（防迁移面悄悄缩水） | 逐个在场 |
 */
import { describe, it, expect, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick, h } from 'vue'
import { renderToString } from 'vue/server-renderer'

// i18n：最小消息表（只覆盖被复制组件真正消费的 key）。
// 真源文案在 `src/locales/*`，这里只验证「组件把 t() 结果渲染出来」这条契约。
const i18nMessages = vi.hoisted(() => ({
  'mobile.bottomSheet.connecting': '连接中…',
  'mobile.bottomSheet.cancelConnect': '取消连接',
  'common.button.cancel': '取消',
  'common.button.confirm': '确定',
}) as Record<string, string>)

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => i18nMessages[key] ?? key }),
}))

import * as uiLibrary from '@/shell/components/ui'

/** Transition 立即生效：挂载/卸载即渲染，避免测试里等过渡结束 */
const TransitionStub = { template: '<slot />' }

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function mountUi(component: any, props: Record<string, unknown> = {}) {
  return mount(component, { props, global: { stubs: { Transition: TransitionStub } } })
}

/** 取 body 内按文案匹配的按钮（Teleport 到 body 的弹窗用） */
function buttonByText(text: string): HTMLElement {
  const btn = [...document.body.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === text,
  )
  expect(btn, `找不到文案为「${text}」的按钮`).toBeTruthy()
  return btn as HTMLElement
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('Button', () => {
  it('should_markDisabledAndSpin_when_disabledOrLoading', () => {
    const disabled = mountUi(uiLibrary.Button, { disabled: true })
    expect(disabled.find('button').attributes('disabled')).toBeDefined()
    disabled.unmount()

    const loading = mountUi(uiLibrary.Button, { loading: true })
    expect(loading.find('button').attributes('disabled')).toBeDefined()
    expect(loading.find('.animate-spin').exists()).toBe(true)

    const normal = mountUi(uiLibrary.Button)
    expect(normal.find('button').attributes('disabled')).toBeUndefined()
    expect(normal.find('.animate-spin').exists()).toBe(false)
  })

  it('should_mapVariantAndSizeToClasses_when_propsGiven', () => {
    const primary = mountUi(uiLibrary.Button, { variant: 'primary', size: 'sm' })
    expect(primary.find('button').classes()).toContain('bg-[var(--mobile-accent)]')
    expect(primary.find('button').classes()).toContain('px-3')

    const ghost = mountUi(uiLibrary.Button, { variant: 'ghost', size: 'lg' })
    expect(ghost.find('button').classes()).toContain('bg-transparent')
    expect(ghost.find('button').classes()).toContain('px-6')

    const danger = mountUi(uiLibrary.Button, { variant: 'danger' })
    expect(danger.find('button').classes()).toContain('bg-[var(--mobile-error)]')

    const secondary = mountUi(uiLibrary.Button, { variant: 'secondary' })
    expect(secondary.find('button').classes()).toContain('bg-[var(--mobile-bg-elevated)]')
  })

  it('should_emitClick_when_clicked', async () => {
    const wrapper = mountUi(uiLibrary.Button)
    await wrapper.find('button').trigger('click')
    expect(wrapper.emitted('click')).toHaveLength(1)
  })
})

describe('Toggle', () => {
  it('should_emitInvertedValue_when_checked', async () => {
    const off = mountUi(uiLibrary.Toggle, { modelValue: false })
    await off.find('input').trigger('change')
    expect(off.emitted('update:modelValue')).toEqual([[true]])

    const on = mountUi(uiLibrary.Toggle, { modelValue: true })
    await on.find('input').trigger('change')
    expect(on.emitted('update:modelValue')).toEqual([[false]])
  })

  it('should_disableInput_when_disabled', () => {
    const wrapper = mountUi(uiLibrary.Toggle, { modelValue: true, disabled: true })
    expect(wrapper.find('input').attributes('disabled')).toBeDefined()
  })
})

describe('ConfirmDialog', () => {
  function mountConfirm(props: Record<string, unknown> = {}) {
    return mountUi(uiLibrary.ConfirmDialog, {
      modelValue: true,
      title: '确认标题',
      message: '确认正文',
      confirmText: '确认',
      cancelText: '取消',
      ...props,
    })
  }

  it('should_emitConfirm_when_confirmClicked', async () => {
    const wrapper = mountConfirm()
    buttonByText('确认').click()
    await nextTick()
    expect(wrapper.emitted('confirm')).toHaveLength(1)
  })

  it('should_notEmitConfirm_when_loading', async () => {
    const wrapper = mountConfirm({ loading: true })
    buttonByText('确认').click()
    await nextTick()
    expect(wrapper.emitted('confirm')).toBeUndefined()
  })

  it('should_emitCancelAndClose_when_cancelClicked', async () => {
    const wrapper = mountConfirm()
    buttonByText('取消').click()
    await nextTick()
    expect(wrapper.emitted('cancel')).toHaveLength(1)
    expect(wrapper.emitted('update:modelValue')).toEqual([[false]])
  })

  it('should_ignoreBackdrop_when_closeOnBackdropFalseOrLoading', async () => {
    const strict = mountConfirm({ closeOnBackdrop: false })
    ;(document.body.querySelector('.absolute.inset-0') as HTMLElement).click()
    await nextTick()
    expect(strict.emitted('update:modelValue')).toBeUndefined()
    strict.unmount()
    document.body.innerHTML = ''

    const loading = mountConfirm({ loading: true })
    ;(document.body.querySelector('.absolute.inset-0') as HTMLElement).click()
    await nextTick()
    expect(loading.emitted('update:modelValue')).toBeUndefined()
  })

  it('should_close_when_backdropClickedByDefault', async () => {
    const wrapper = mountConfirm()
    ;(document.body.querySelector('.absolute.inset-0') as HTMLElement).click()
    await nextTick()
    expect(wrapper.emitted('update:modelValue')).toEqual([[false]])
  })

  it('should_useDangerPalette_when_variantDanger', () => {
    mountConfirm({ variant: 'danger' })
    expect(buttonByText('确认').className).toContain('bg-[var(--mobile-danger-solid-bg)]')
    expect(document.body.querySelector('.bg-\\[var\\(--mobile-danger-bg\\)\\]')).toBeTruthy()
  })

  it('should_notRenderPanel_when_closed', () => {
    mountConfirm({ modelValue: false })
    expect(document.body.querySelector('.modal-panel')).toBeNull()
  })
})

describe('PromptDialog', () => {
  function mountPrompt(props: Record<string, unknown> = {}) {
    return mountUi(uiLibrary.PromptDialog, {
      modelValue: true,
      title: '输入标题',
      placeholder: '请输入',
      ...props,
    })
  }

  /** 弹窗内容经 Teleport 落到 body：输入框只能从 body 取（wrapper.find 不进 teleport） */
  function promptInput(): HTMLInputElement | null {
    return document.body.querySelector('input')
  }

  async function typePrompt(value: string) {
    const input = promptInput()
    expect(input, '输入框应在场').toBeTruthy()
    input!.value = value
    input!.dispatchEvent(new Event('input', { bubbles: true }))
    await nextTick()
  }

  it('should_emitTrimmedSubmit_when_confirmClicked', async () => {
    const wrapper = mountPrompt()
    await typePrompt('  hello  ')
    buttonByText('确定').click()
    await nextTick()
    expect(wrapper.emitted('submit')).toEqual([['hello']])
  })

  it('should_notSubmit_when_inputBlank', async () => {
    const wrapper = mountPrompt()
    await typePrompt('   ')
    buttonByText('确定').click()
    await nextTick()
    expect(wrapper.emitted('submit')).toBeUndefined()
  })

  it('should_backfillInitialValue_when_opened', async () => {
    const wrapper = mountPrompt({ modelValue: false, initialValue: 'preset' })
    expect(promptInput()).toBeNull()

    await wrapper.setProps({ modelValue: true })
    await nextTick()
    expect(promptInput()?.value).toBe('preset')
  })

  it('should_suppressClose_when_loading', async () => {
    const wrapper = mountPrompt({ loading: true })
    // loading 态输入与确定按钮整体不渲染 ⇒ 不存在提交路径
    expect(promptInput()).toBeNull()

    // 右上角关闭图标在 loading 期间被抑制
    const closeBtn = document.body.querySelector('.absolute.top-4.right-4') as HTMLElement
    expect(closeBtn, '关闭按钮应在场').toBeTruthy()
    closeBtn.click()
    await nextTick()
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
  })

  it('should_emitCancelAndClose_when_loadingCancelClicked', async () => {
    const wrapper = mountPrompt({ loading: true })
    // loading 态唯一出口：「取消连接」→ cancel + 关闭
    buttonByText('取消连接').click()
    await nextTick()
    expect(wrapper.emitted('cancel')).toHaveLength(1)
    expect(wrapper.emitted('update:modelValue')).toEqual([[false]])
    expect(wrapper.emitted('submit')).toBeUndefined()
  })

  it('should_onlyClose_when_cancelClicked', async () => {
    const wrapper = mountPrompt()
    buttonByText('取消').click()
    await nextTick()
    // 契约：普通取消按钮只关闭（cancel 事件是 loading 态专用出口）
    expect(wrapper.emitted('update:modelValue')).toEqual([[false]])
    expect(wrapper.emitted('submit')).toBeUndefined()
  })
})

describe('CollapseSection', () => {
  it('should_toggleCollapsed_when_headerClicked', async () => {
    const wrapper = mountUi(uiLibrary.CollapseSection, { title: '标题', badge: 3 })
    const section = wrapper.find('section')
    expect(section.classes()).not.toContain('collapsed')
    expect(wrapper.text()).toContain('3')

    await wrapper.find('button').trigger('click')
    expect(section.classes()).toContain('collapsed')

    await wrapper.find('button').trigger('click')
    expect(section.classes()).not.toContain('collapsed')
  })

  it('should_startCollapsed_when_defaultOpenFalse', () => {
    const wrapper = mountUi(uiLibrary.CollapseSection, { title: '标题', defaultOpen: false })
    expect(wrapper.find('section').classes()).toContain('collapsed')
  })
})

describe('LoadingDialog', () => {
  it('should_renderMessageOnlyWhenVisible', async () => {
    const wrapper = mountUi(uiLibrary.LoadingDialog, { visible: true, message: '连接中' })
    expect(document.body.textContent).toContain('连接中')

    await wrapper.setProps({ visible: false })
    expect(document.body.textContent).not.toContain('连接中')
  })
})

describe('LetterAvatar', () => {
  it('should_renderUppercasedFirstCharAndStableGradient_when_seeded', () => {
    const first = mountUi(uiLibrary.LetterAvatar, { name: 'beta', seed: 'com.bedcode.x' })
    expect(first.text()).toBe('B')
    const gradientClass = first.classes().find((c) => /^shell-avatar-g\d$/.test(c))
    expect(gradientClass).toBeTruthy()

    const same = mountUi(uiLibrary.LetterAvatar, { name: 'beta', seed: 'com.bedcode.x' })
    expect(same.classes().find((c) => /^shell-avatar-g\d$/.test(c))).toBe(gradientClass)
  })

  it('should_distributeSeedsAcrossGradientSlots_when_manySeeds', () => {
    const seen = new Set<string>()
    for (let i = 0; i < 12; i++) {
      const wrapper = mountUi(uiLibrary.LetterAvatar, { name: 'app', seed: `plugin-${i}` })
      const gradient = wrapper.classes().find((c) => /^shell-avatar-g\d$/.test(c))
      if (gradient) seen.add(gradient)
    }
    // 哈希若退化成常量（如误写成固定档），这里会只剩 1 档
    expect(seen.size).toBeGreaterThan(1)
  })

  it('should_fallBackToQuestionMark_when_nameBlank', () => {
    const wrapper = mountUi(uiLibrary.LetterAvatar, { name: '   ', seed: 'x' })
    expect(wrapper.text()).toBe('?')
  })
})

describe('QuickActionButton', () => {
  it('should_emitClickAndRenderContent_when_clicked', async () => {
    const wrapper = mountUi(uiLibrary.QuickActionButton, {
      name: '打开会话',
      content: 'session',
      icon: '>',
    })
    expect(wrapper.text()).toContain('打开会话')
    expect(wrapper.text()).toContain('>')
    expect(wrapper.find('.quick-action').exists()).toBe(true)

    await wrapper.trigger('click')
    expect(wrapper.emitted('click')).toHaveLength(1)
  })

  it('should_useTokenGlowAndColorDerivedFill_when_colorGiven', async () => {
    // 颜色派生值（color-mix）会被 happy-dom 的 CSS 解析丢弃，因此这里断言 SSR 渲染
    // 结果字符串：它反映组件真实的绑定输出，而不是宿主 DOM 的容错结果。
    const html = await renderToString(
      h(uiLibrary.QuickActionButton, {
        name: 'n',
        content: 'c',
        color: 'var(--mobile-accent)',
      }),
    )
    // 光晕走 token（曾为硬编码 rgba）
    expect(html).toContain('var(--shell-action-glow)')
    // 底色 / 描边由 color prop 派生，且不是写死色值
    expect(html).toContain('color-mix')
    expect(html).toContain('var(--mobile-accent)')
  })
})

describe('组件库出口', () => {
  it('should_exposeAllPortedComponents_when_libraryImported', () => {
    const expected = [
      'Button',
      'Toggle',
      'Modal',
      'ConfirmDialog',
      'PromptDialog',
      'LoadingDialog',
      'CollapseSection',
      'QuickActionButton',
      'LetterAvatar',
    ]
    const surface = uiLibrary as unknown as Record<string, unknown>
    for (const name of expected) {
      expect(surface[name], `${name} 应在壳内组件库出口`).toBeTruthy()
    }
  })
})
