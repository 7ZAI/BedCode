import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import Button from '@/modules/shared/components/Button.vue'

describe('Button Component', () => {
  it('should render with default props', () => {
    const wrapper = mount(Button, {
      slots: {
        default: 'Click me'
      }
    })

    expect(wrapper.text()).toContain('Click me')
    expect(wrapper.find('button').exists()).toBe(true)
  })

  it('should emit click event', async () => {
    const wrapper = mount(Button, {
      slots: {
        default: 'Click me'
      }
    })

    await wrapper.find('button').trigger('click')

    expect(wrapper.emitted('click')).toBeTruthy()
    expect(wrapper.emitted('click')).toHaveLength(1)
  })

  it('should apply variant class', () => {
    const wrapper = mount(Button, {
      props: {
        variant: 'primary'
      },
      slots: {
        default: 'Primary Button'
      }
    })

    expect(wrapper.classes()).toContain('bg-primary-600')
  })

  it('should be disabled when disabled prop is true', () => {
    const wrapper = mount(Button, {
      props: {
        disabled: true
      },
      slots: {
        default: 'Disabled Button'
      }
    })

    expect(wrapper.find('button').element.disabled).toBe(true)
  })

  it('should not emit click when disabled', async () => {
    const wrapper = mount(Button, {
      props: {
        disabled: true
      },
      slots: {
        default: 'Disabled Button'
      }
    })

    await wrapper.find('button').trigger('click')

    expect(wrapper.emitted('click')).toBeFalsy()
  })

  it('should apply size class', () => {
    const wrapper = mount(Button, {
      props: {
        size: 'lg'
      },
      slots: {
        default: 'Large Button'
      }
    })

    expect(wrapper.classes()).toContain('py-3')
  })

  it('should render with icon slot', () => {
    const wrapper = mount(Button, {
      slots: {
        default: 'With Icon',
        icon: '<span class="icon">🔍</span>'
      }
    })

    expect(wrapper.html()).toContain('🔍')
  })
})
