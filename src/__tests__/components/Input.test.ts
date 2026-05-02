import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import Input from '@/components/common/Input.vue'

// Mock uuid - must match the import pattern in the component
vi.mock('uuid', () => ({
  v4: () => 'test-uuid-1234',
}))

describe('Input Component', () => {
  describe('rendering', () => {
    it('should render input element', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
        },
      })

      expect(wrapper.find('input').exists()).toBe(true)
    })

    it('should render label when provided', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          label: 'Username',
        },
      })

      expect(wrapper.find('label').exists()).toBe(true)
      expect(wrapper.find('label').text()).toContain('Username')
    })

    it('should not render label when not provided', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
        },
      })

      expect(wrapper.find('label').exists()).toBe(false)
    })

    it('should render required asterisk when required', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          label: 'Email',
          required: true,
        },
      })

      expect(wrapper.find('label').text()).toContain('*')
      expect(wrapper.find('.text-red-400').exists()).toBe(true)
    })
  })

  describe('input types', () => {
    it('should default to text type', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
        },
      })

      expect(wrapper.find('input').attributes('type')).toBe('text')
    })

    it('should support password type', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          type: 'password',
        },
      })

      expect(wrapper.find('input').attributes('type')).toBe('password')
    })

    it('should support email type', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          type: 'email',
        },
      })

      expect(wrapper.find('input').attributes('type')).toBe('email')
    })

    it('should support number type', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: 0,
          type: 'number',
        },
      })

      expect(wrapper.find('input').attributes('type')).toBe('number')
    })

    it('should support url type', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          type: 'url',
        },
      })

      expect(wrapper.find('input').attributes('type')).toBe('url')
    })
  })

  describe('value binding', () => {
    it('should display modelValue', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: 'test value',
        },
      })

      expect((wrapper.find('input').element as HTMLInputElement).value).toBe('test value')
    })

    it('should emit update:modelValue on input', async () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
        },
      })

      await wrapper.find('input').setValue('new value')

      expect(wrapper.emitted('update:modelValue')).toBeTruthy()
      expect(wrapper.emitted('update:modelValue')![0]).toEqual(['new value'])
    })

    it('should work with number modelValue', async () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: 42,
          type: 'number',
        },
      })

      expect((wrapper.find('input').element as HTMLInputElement).value).toBe('42')
    })
  })

  describe('placeholder', () => {
    it('should display placeholder when provided', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          placeholder: 'Enter your name',
        },
      })

      expect(wrapper.find('input').attributes('placeholder')).toBe('Enter your name')
    })
  })

  describe('disabled state', () => {
    it('should be disabled when disabled prop is true', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          disabled: true,
        },
      })

      expect((wrapper.find('input').element as HTMLInputElement).disabled).toBe(true)
    })

    it('should apply disabled styling', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          disabled: true,
        },
      })

      const input = wrapper.find('input')
      expect(input.classes()).toContain('opacity-50')
      expect(input.classes()).toContain('cursor-not-allowed')
    })
  })

  describe('readonly state', () => {
    it('should be readonly when readonly prop is true', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          readonly: true,
        },
      })

      expect((wrapper.find('input').element as HTMLInputElement).readOnly).toBe(true)
    })
  })

  describe('error state', () => {
    it('should display error message when error prop is provided', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          error: 'This field is required',
        },
      })

      expect(wrapper.find('.text-red-400').exists()).toBe(true)
      expect(wrapper.find('.text-red-400').text()).toBe('This field is required')
    })

    it('should apply error border styling', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          error: 'Error',
        },
      })

      const input = wrapper.find('input')
      expect(input.classes()).toContain('border-red-500')
    })

    it('should use normal border when no error', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
        },
      })

      const input = wrapper.find('input')
      expect(input.classes()).toContain('border-dark-600')
    })
  })

  describe('help text', () => {
    it('should display help text when provided and no error', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          help: 'Enter a valid email address',
        },
      })

      expect(wrapper.find('.text-dark-500').exists()).toBe(true)
      expect(wrapper.find('.text-dark-500').text()).toBe('Enter a valid email address')
    })

    it('should not show help text when error is present', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          error: 'Invalid email',
          help: 'Enter a valid email address',
        },
      })

      expect(wrapper.find('.text-dark-500').exists()).toBe(false)
      expect(wrapper.find('.text-red-400').text()).toBe('Invalid email')
    })
  })

  describe('slots', () => {
    it('should render prefix slot', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
        },
        slots: {
          prefix: '<span class="prefix-icon">@</span>',
        },
      })

      expect(wrapper.html()).toContain('prefix-icon')
      expect(wrapper.find('.absolute.left-3').exists()).toBe(true)
    })

    it('should render suffix slot', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
        },
        slots: {
          suffix: '<span class="suffix-icon">🔍</span>',
        },
      })

      expect(wrapper.html()).toContain('suffix-icon')
      expect(wrapper.find('.absolute.right-3').exists()).toBe(true)
    })

    it('should apply left padding when prefix slot is used', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
        },
        slots: {
          prefix: '@',
        },
      })

      const input = wrapper.find('input')
      expect(input.classes()).toContain('pl-10')
    })

    it('should apply right padding when suffix slot is used', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
        },
        slots: {
          suffix: '🔍',
        },
      })

      const input = wrapper.find('input')
      expect(input.classes()).toContain('pr-10')
    })
  })

  describe('accessibility', () => {
    it('should have id for label association', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          label: 'Email',
        },
      })

      const inputId = wrapper.find('input').attributes('id')
      const labelFor = wrapper.find('label').attributes('for')

      expect(inputId).toBe(labelFor)
    })

    it('should have required attribute when required', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
          required: true,
        },
      })

      expect(wrapper.find('input').attributes('required')).toBeDefined()
    })
  })

  describe('focus styling', () => {
    it('should have focus styling classes', () => {
      const wrapper = mount(Input, {
        props: {
          modelValue: '',
        },
      })

      const input = wrapper.find('input')
      expect(input.classes()).toContain('focus:border-primary-500')
      expect(input.classes()).toContain('focus:ring-1')
      expect(input.classes()).toContain('focus:ring-primary-500')
    })
  })
})
