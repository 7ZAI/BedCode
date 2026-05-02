import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import Toggle from '@/components/common/Toggle.vue'

describe('Toggle Component', () => {
  describe('rendering', () => {
    it('should render toggle switch', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
        },
      })

      expect(wrapper.find('input[type="checkbox"]').exists()).toBe(true)
      expect(wrapper.find('label').exists()).toBe(true)
    })

    it('should render with label when provided', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
          label: 'Enable feature',
        },
      })

      expect(wrapper.text()).toContain('Enable feature')
    })

    it('should not render label when not provided', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
        },
      })

      expect(wrapper.find('span').exists()).toBe(false)
    })
  })

  describe('checked state', () => {
    it('should be unchecked when modelValue is false', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
        },
      })

      const input = wrapper.find('input[type="checkbox"]')
      expect((input.element as HTMLInputElement).checked).toBe(false)
    })

    it('should be checked when modelValue is true', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: true,
        },
      })

      const input = wrapper.find('input[type="checkbox"]')
      expect((input.element as HTMLInputElement).checked).toBe(true)
    })

    it('should apply active background class when checked', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: true,
        },
      })

      const toggle = wrapper.find('.w-10.h-6')
      expect(toggle.classes()).toContain('bg-primary-600')
    })

    it('should apply inactive background class when unchecked', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
        },
      })

      const toggle = wrapper.find('.w-10.h-6')
      expect(toggle.classes()).toContain('bg-dark-600')
    })

    it('should position toggle correctly when checked', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: true,
        },
      })

      const knob = wrapper.find('.absolute.top-1')
      expect(knob.classes()).toContain('translate-x-5')
    })

    it('should position toggle correctly when unchecked', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
        },
      })

      const knob = wrapper.find('.absolute.top-1')
      expect(knob.classes()).toContain('translate-x-0')
    })
  })

  describe('disabled state', () => {
    it('should be disabled when disabled prop is true', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
          disabled: true,
        },
      })

      const input = wrapper.find('input[type="checkbox"]')
      expect((input.element as HTMLInputElement).disabled).toBe(true)
    })

    it('should not be disabled by default', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
        },
      })

      const input = wrapper.find('input[type="checkbox"]')
      expect((input.element as HTMLInputElement).disabled).toBe(false)
    })

    it('should apply disabled styling when disabled', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
          disabled: true,
        },
      })

      const toggle = wrapper.find('.w-10.h-6')
      expect(toggle.classes()).toContain('opacity-50')
      expect(toggle.classes()).toContain('cursor-not-allowed')
    })
  })

  describe('events', () => {
    it('should emit update:modelValue when toggled', async () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
        },
      })

      await wrapper.find('input[type="checkbox"]').setValue(true)

      expect(wrapper.emitted('update:modelValue')).toBeTruthy()
      expect(wrapper.emitted('update:modelValue')![0]).toEqual([true])
    })

    it('should emit false when unchecking', async () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: true,
        },
      })

      await wrapper.find('input[type="checkbox"]').setValue(false)

      expect(wrapper.emitted('update:modelValue')![0]).toEqual([false])
    })

    it('should emit event on change', async () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
        },
      })

      const input = wrapper.find('input[type="checkbox"]')
      await input.trigger('change')

      expect(wrapper.emitted('update:modelValue')).toBeTruthy()
    })
  })

  describe('accessibility', () => {
    it('should have proper input type', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
        },
      })

      const input = wrapper.find('input')
      expect(input.attributes('type')).toBe('checkbox')
    })

    it('should be screen reader accessible with sr-only class', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
        },
      })

      const input = wrapper.find('input')
      expect(input.classes()).toContain('sr-only')
    })

    it('should have cursor pointer on label', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
        },
      })

      const label = wrapper.find('label')
      expect(label.classes()).toContain('cursor-pointer')
    })
  })

  describe('label slot behavior', () => {
    it('should show label text on the right side of toggle', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
          label: 'Dark Mode',
        },
      })

      const label = wrapper.find('label')
      expect(label.text()).toContain('Dark Mode')
    })

    it('should have proper gap between toggle and label', () => {
      const wrapper = mount(Toggle, {
        props: {
          modelValue: false,
          label: 'Test',
        },
      })

      const label = wrapper.find('label')
      expect(label.classes()).toContain('gap-3')
    })
  })
})
