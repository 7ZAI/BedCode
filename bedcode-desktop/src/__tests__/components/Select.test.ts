import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
// 单源化后宿主 Select 与 SDK 为同一组件，测试直接覆盖 SDK 实现
import Select from '@bedcode/plugin-sdk-desktop/ui'

/**
 * Select 组件已从原生 <select> 重构为自定义下拉（button 触发器 + Teleport 面板）。
 * 选项渲染在 teleport 到 body 的 <ul>/<li> 中，因此通过 document 查询断言。
 */
describe('Select Component', () => {
  const defaultOptions = [
    { value: 'option1', label: 'Option 1' },
    { value: 'option2', label: 'Option 2' },
    { value: 'option3', label: 'Option 3' },
  ]

  type SelectWrapper = VueWrapper<InstanceType<typeof Select>>

  function mountSelect(props: Record<string, unknown> = {}): SelectWrapper {
    document.body.innerHTML = ''
    return mount(Select, {
      props: { modelValue: '', options: defaultOptions, ...props },
      attachTo: document.body,
    })
  }

  function optionsList(): HTMLElement | null {
    return document.querySelector('.fixed.z-\\[60\\] ul') ?? document.querySelector('ul')
  }

  beforeEach(() => {
    document.body.innerHTML = ''
  })

  describe('rendering', () => {
    it('should render trigger button', () => {
      const wrapper = mountSelect()

      expect(wrapper.find('button').exists()).toBe(true)
    })

    it('should render all options', () => {
      mountSelect()

      const lis = document.querySelectorAll('ul li')
      expect(lis).toHaveLength(3)
    })

    it('should render option labels correctly', () => {
      mountSelect()

      const lis = document.querySelectorAll('ul li')
      expect(lis[0].textContent).toBe('Option 1')
      expect(lis[1].textContent).toBe('Option 2')
      expect(lis[2].textContent).toBe('Option 3')
    })

    it('should render label when provided', () => {
      const wrapper = mountSelect({ label: 'Select Option' })

      expect(wrapper.find('label').exists()).toBe(true)
      expect(wrapper.find('label').text()).toContain('Select Option')
    })
  })

  describe('placeholder', () => {
    it('should render placeholder in trigger when nothing selected', () => {
      const wrapper = mountSelect({ placeholder: 'Choose an option' })

      expect(wrapper.find('button').text()).toContain('Choose an option')
    })

    it('should render placeholder option in panel when provided', () => {
      mountSelect({ placeholder: 'Choose an option' })

      const lis = document.querySelectorAll('ul li')
      // 占位符选项排在第一位
      expect(lis[0].textContent).toBe('Choose an option')
      expect(lis).toHaveLength(4)
    })

    it('should not render placeholder option when not provided', () => {
      mountSelect()

      const lis = document.querySelectorAll('ul li')
      expect(lis[0].textContent).toBe('Option 1')
    })
  })

  describe('value binding', () => {
    it('should show the correct option label based on modelValue', () => {
      const wrapper = mountSelect({ modelValue: 'option2' })

      expect(wrapper.find('button').text()).toContain('Option 2')
    })

    it('should emit update:modelValue when option clicked', async () => {
      const wrapper = mountSelect()

      const li = document.querySelectorAll('ul li')[1] as HTMLElement
      li.click()
      await wrapper.vm.$nextTick()

      expect(wrapper.emitted('update:modelValue')).toBeTruthy()
      expect(wrapper.emitted('update:modelValue')![0]).toEqual(['option2'])
    })

    it('should work with numeric values', () => {
      const numericOptions = [
        { value: 1, label: 'One' },
        { value: 2, label: 'Two' },
      ]
      const wrapper = mountSelect({ modelValue: 1, options: numericOptions })

      expect(wrapper.find('button').text()).toContain('One')
    })

    it('should open panel when trigger clicked', async () => {
      const wrapper = mountSelect()

      expect(document.querySelector('ul')?.style.display).not.toBe('none')
      await wrapper.find('button').trigger('click')
      await wrapper.vm.$nextTick()

      // v-show 控制显示，点击后应可见（v-show 样式 display 不为 none）
      const panel = document.querySelector('ul')
      expect(panel?.parentElement?.style.display).not.toBe('none')
    })
  })

  describe('disabled state', () => {
    it('should be disabled when disabled prop is true', () => {
      const wrapper = mountSelect({ disabled: true })

      expect((wrapper.find('button').element as HTMLButtonElement).disabled).toBe(true)
    })

    it('should apply disabled styling', () => {
      const wrapper = mountSelect({ disabled: true })

      const button = wrapper.find('button')
      expect(button.classes()).toContain('opacity-50')
      expect(button.classes()).toContain('cursor-not-allowed')
    })
  })

  describe('required state', () => {
    it('should show required asterisk in label', () => {
      const wrapper = mountSelect({ label: 'Required Select', required: true })

      expect(wrapper.find('label').text()).toContain('*')
      expect(wrapper.find('.text-red-500').exists()).toBe(true)
    })
  })

  describe('error state', () => {
    it('should display error message when provided', () => {
      const wrapper = mountSelect({ error: 'Please select an option' })

      expect(wrapper.find('.text-red-500').exists()).toBe(true)
      expect(wrapper.find('.text-red-500').text()).toBe('Please select an option')
    })

    it('should apply error border styling', () => {
      const wrapper = mountSelect({ error: 'Please select an option' })

      expect(wrapper.find('button').classes()).toContain('border-red-500')
    })
  })

  describe('accessibility', () => {
    it('should have id for label association', () => {
      const wrapper = mountSelect({ label: 'Select Field' })

      const buttonId = wrapper.find('button').attributes('id')
      const labelFor = wrapper.find('label').attributes('for')

      expect(buttonId).toBe(labelFor)
    })
  })

  describe('styling', () => {
    it('should have focus styling classes', () => {
      const wrapper = mountSelect()

      const button = wrapper.find('button')
      expect(button.classes()).toContain('focus:border-brand')
      expect(button.classes()).toContain('focus:shadow-input-focus')
    })

    it('should have dropdown icon', () => {
      const wrapper = mountSelect()

      // 触发器内的下拉箭头 SVG
      expect(wrapper.find('button svg').exists()).toBe(true)
    })

    it('should render dropdown panel with options', () => {
      mountSelect()

      // 面板通过 Teleport 渲染到 body
      const panel = optionsList()
      expect(panel).toBeTruthy()
      expect(panel?.querySelectorAll('li')).toHaveLength(3)
    })
  })

  describe('edge cases', () => {
    it('should handle empty options array', () => {
      mountSelect({ options: [] })

      const lis = document.querySelectorAll('ul li')
      expect(lis).toHaveLength(0)
    })

    it('should handle single option', () => {
      mountSelect({ modelValue: 'only', options: [{ value: 'only', label: 'Only Option' }] })

      const lis = document.querySelectorAll('ul li')
      expect(lis).toHaveLength(1)
      expect(lis[0].textContent).toBe('Only Option')
    })
  })
})
