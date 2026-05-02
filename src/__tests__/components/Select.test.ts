import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import Select from '@/components/common/Select.vue'

// Mock uuid - must match the import pattern in the component
vi.mock('uuid', () => ({
  v4: () => 'test-uuid-select',
}))

describe('Select Component', () => {
  const defaultOptions = [
    { value: 'option1', label: 'Option 1' },
    { value: 'option2', label: 'Option 2' },
    { value: 'option3', label: 'Option 3' },
  ]

  describe('rendering', () => {
    it('should render select element', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
        },
      })

      expect(wrapper.find('select').exists()).toBe(true)
    })

    it('should render all options', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
        },
      })

      const options = wrapper.findAll('option')
      expect(options).toHaveLength(3)
    })

    it('should render option labels correctly', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
        },
      })

      const options = wrapper.findAll('option')
      expect(options[0].text()).toBe('Option 1')
      expect(options[1].text()).toBe('Option 2')
      expect(options[2].text()).toBe('Option 3')
    })

    it('should render option values correctly', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
        },
      })

      const options = wrapper.findAll('option')
      expect(options[0].attributes('value')).toBe('option1')
      expect(options[1].attributes('value')).toBe('option2')
      expect(options[2].attributes('value')).toBe('option3')
    })

    it('should render label when provided', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
          label: 'Select Option',
        },
      })

      expect(wrapper.find('label').exists()).toBe(true)
      expect(wrapper.find('label').text()).toContain('Select Option')
    })
  })

  describe('placeholder', () => {
    it('should render placeholder option when provided', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
          placeholder: 'Choose an option',
        },
      })

      const placeholderOption = wrapper.find('option[value=""]')
      expect(placeholderOption.exists()).toBe(true)
      expect(placeholderOption.text()).toBe('Choose an option')
      expect(placeholderOption.attributes('disabled')).toBeDefined()
    })

    it('should not render placeholder when not provided', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
        },
      })

      const options = wrapper.findAll('option')
      expect(options[0].attributes('value')).toBe('option1')
    })
  })

  describe('value binding', () => {
    it('should select the correct option based on modelValue', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: 'option2',
          options: defaultOptions,
        },
      })

      const select = wrapper.find('select').element as HTMLSelectElement
      expect(select.value).toBe('option2')
    })

    it('should emit update:modelValue on change', async () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
        },
      })

      await wrapper.find('select').setValue('option2')

      expect(wrapper.emitted('update:modelValue')).toBeTruthy()
      expect(wrapper.emitted('update:modelValue')![0]).toEqual(['option2'])
    })

    it('should work with numeric values', async () => {
      const numericOptions = [
        { value: 1, label: 'One' },
        { value: 2, label: 'Two' },
      ]

      const wrapper = mount(Select, {
        props: {
          modelValue: 1,
          options: numericOptions,
        },
      })

      const select = wrapper.find('select').element as HTMLSelectElement
      expect(select.value).toBe('1')
    })
  })

  describe('disabled state', () => {
    it('should be disabled when disabled prop is true', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
          disabled: true,
        },
      })

      expect((wrapper.find('select').element as HTMLSelectElement).disabled).toBe(true)
    })

    it('should apply disabled styling', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
          disabled: true,
        },
      })

      const select = wrapper.find('select')
      expect(select.classes()).toContain('opacity-50')
      expect(select.classes()).toContain('cursor-not-allowed')
    })
  })

  describe('required state', () => {
    it('should have required attribute when required', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
          required: true,
        },
      })

      expect(wrapper.find('select').attributes('required')).toBeDefined()
    })

    it('should show required asterisk in label', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
          label: 'Required Select',
          required: true,
        },
      })

      expect(wrapper.find('label').text()).toContain('*')
      expect(wrapper.find('.text-red-400').exists()).toBe(true)
    })
  })

  describe('error state', () => {
    it('should display error message when provided', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
          error: 'Please select an option',
        },
      })

      expect(wrapper.find('.text-red-400').exists()).toBe(true)
      expect(wrapper.find('.text-red-400').text()).toBe('Please select an option')
    })
  })

  describe('accessibility', () => {
    it('should have id for label association', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
          label: 'Select Field',
        },
      })

      const selectId = wrapper.find('select').attributes('id')
      const labelFor = wrapper.find('label').attributes('for')

      expect(selectId).toBe(labelFor)
    })
  })

  describe('styling', () => {
    it('should have focus styling classes', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
        },
      })

      const select = wrapper.find('select')
      expect(select.classes()).toContain('focus:border-primary-500')
      expect(select.classes()).toContain('focus:ring-1')
      expect(select.classes()).toContain('focus:ring-primary-500')
    })

    it('should have dropdown icon', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
        },
      })

      // Check for SVG dropdown icon
      expect(wrapper.find('svg').exists()).toBe(true)
    })

    it('should have custom styling to hide default arrow', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: defaultOptions,
        },
      })

      const select = wrapper.find('select')
      expect(select.classes()).toContain('appearance-none')
    })
  })

  describe('edge cases', () => {
    it('should handle empty options array', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: '',
          options: [],
        },
      })

      const options = wrapper.findAll('option')
      expect(options).toHaveLength(0)
    })

    it('should handle single option', () => {
      const wrapper = mount(Select, {
        props: {
          modelValue: 'only',
          options: [{ value: 'only', label: 'Only Option' }],
        },
      })

      const options = wrapper.findAll('option')
      expect(options).toHaveLength(1)
    })
  })
})
