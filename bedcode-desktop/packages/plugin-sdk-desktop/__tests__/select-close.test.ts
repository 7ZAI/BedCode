import { describe, it, expect, afterEach } from 'vitest'
import { mount, type VueWrapper } from '@vue/test-utils'
import { nextTick } from 'vue'
import Select from '../src/ui/Select.vue'

/**
 * Select 下拉关闭行为回归测试
 *
 * 历史 bug：window 以捕获阶段监听 scroll（scroll 不冒泡但捕获路径会收到
 * 任意后代元素滚动），导致选项过多时面板自身列表一滚动就触发 close——
 * 滚轮/拖动滚动条失效、面板瞬间收起。修复：面板内部滚动一律忽略。
 */

/** 几百个选项：迫使面板内列表出现滚动条（SELECT_MAX_PANEL_HEIGHT = 240px） */
const options = Array.from({ length: 300 }, (_, i) => ({
  value: `model-${i}`,
  label: `模型 ${i}`,
}))

function getPanel(): HTMLElement {
  const el = document.querySelector<HTMLElement>('.fixed')
  expect(el).toBeTruthy()
  return el!
}

async function openPanel(wrapper: VueWrapper): Promise<void> {
  await wrapper.find('button').trigger('click')
  // 等 open() 内的 nextTick(computePosition) 完成（面板定位/列表 maxHeight）
  await nextTick()
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('Select 下拉关闭行为', () => {
  it('面板自身列表滚动（滚轮/拖动滚动条）不应关闭下拉', async () => {
    const wrapper = mount(Select, {
      attachTo: document.body,
      props: { modelValue: 'model-0', options },
    })
    await openPanel(wrapper)

    const panel = getPanel()
    expect(panel.style.display).not.toBe('none')
    const ul = panel.querySelector('ul')!
    // happy-dom 无布局引擎（scrollHeight 恒为 0），以 maxHeight 受约束证明列表可滚动
    expect(ul.style.maxHeight).toBeTruthy()

    // 面板内 ul 滚动（用户滚轮/滚动条拖动触发 scroll）
    ul.dispatchEvent(new Event('scroll'))
    await nextTick()

    expect(panel.style.display).not.toBe('none')
    wrapper.unmount()
  })

  it('面板外滚动（页面/输入区/消息列表）应关闭下拉', async () => {
    const wrapper = mount(Select, {
      attachTo: document.body,
      props: { modelValue: 'model-0', options },
    })
    await openPanel(wrapper)

    window.dispatchEvent(new Event('scroll'))
    await nextTick()

    expect(getPanel().style.display).toBe('none')
    wrapper.unmount()
  })

  it('点击选项应正常选中并收起（值正确派发）', async () => {
    const wrapper = mount(Select, {
      attachTo: document.body,
      props: { modelValue: 'model-0', options },
    })
    await openPanel(wrapper)

    const panel = getPanel()
    const lis = panel.querySelectorAll('li')
    expect(lis.length).toBeGreaterThan(0)

    // 先滚动面板（模拟用户找后面的选项），再点击可视区内的选项
    const ul = panel.querySelector('ul')!
    ul.dispatchEvent(new Event('scroll'))
    const li = lis[0] as HTMLElement
    li.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))
    li.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await nextTick()

    const emitted = wrapper.emitted('update:modelValue')
    expect(emitted).toBeTruthy()
    expect(emitted![0]).toEqual(['model-0'])
    expect(panel.style.display).toBe('none')
    wrapper.unmount()
  })
})
