import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import EmptyState from '@/modules/shared/components/EmptyState.vue'

describe('EmptyState Component', () => {
  it('should render title', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Data'
      }
    })

    expect(wrapper.text()).toContain('No Data')
    expect(wrapper.find('h3').exists()).toBe(true)
  })

  it('should render description when provided', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Sessions',
        description: 'Create a new session to start'
      }
    })

    expect(wrapper.text()).toContain('Create a new session to start')
    expect(wrapper.find('p.text-sm').exists()).toBe(true)
  })

  it('should not render description when not provided', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Data'
      }
    })

    // description 是可选的，只有当提供时才渲染
    const descriptions = wrapper.findAll('p.text-sm.text-dark-400')
    expect(descriptions.length).toBe(0)
  })

  it('should render default icon', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Data'
      }
    })

    // 默认图标是空文件�?SVG
    expect(wrapper.find('svg').exists()).toBe(true)
  })

  it('should render custom icon slot', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Data'
      },
      slots: {
        icon: '<div class="custom-icon">Custom Icon</div>'
      }
    })

    expect(wrapper.find('.custom-icon').exists()).toBe(true)
    expect(wrapper.text()).toContain('Custom Icon')
  })

  it('should not render action button when actionLabel is not provided', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Data'
      }
    })

    // 没有 actionLabel 时不应该渲染按钮
    expect(wrapper.find('button').exists()).toBe(false)
  })

  it('should render action button when actionLabel is provided', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Sessions',
        actionLabel: 'Create Session'
      }
    })

    expect(wrapper.find('button').exists()).toBe(true)
    expect(wrapper.text()).toContain('Create Session')
  })

  it('should emit action event when button clicked', async () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Sessions',
        actionLabel: 'Create Session'
      }
    })

    await wrapper.find('button').trigger('click')

    expect(wrapper.emitted('action')).toBeTruthy()
    expect(wrapper.emitted('action')).toHaveLength(1)
  })

  it('should apply primary variant to action button by default', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Sessions',
        actionLabel: 'Create'
      }
    })

    const button = wrapper.find('button')
    expect(button.classes()).toContain('bg-primary-600')
  })

  it('should apply secondary variant to action button', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Sessions',
        actionLabel: 'Create',
        actionVariant: 'secondary'
      }
    })

    const button = wrapper.find('button')
    expect(button.classes()).toContain('bg-dark-700')
  })

  it('should apply danger variant to action button', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'Error',
        actionLabel: 'Retry',
        actionVariant: 'danger'
      }
    })

    const button = wrapper.find('button')
    expect(button.classes()).toContain('bg-red-600')
  })

  it('should apply ghost variant to action button', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Data',
        actionLabel: 'Refresh',
        actionVariant: 'ghost'
      }
    })

    const button = wrapper.find('button')
    expect(button.classes()).toContain('bg-transparent')
  })

  it('should apply lg icon size by default', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Data'
      }
    })

    const iconContainer = wrapper.find('.mb-4')
    expect(iconContainer.classes()).toContain('w-16')
    expect(iconContainer.classes()).toContain('h-16')
  })

  it('should apply sm icon size', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Data',
        iconSize: 'sm'
      }
    })

    const iconContainer = wrapper.find('.mb-4')
    expect(iconContainer.classes()).toContain('w-8')
    expect(iconContainer.classes()).toContain('h-8')
  })

  it('should apply md icon size', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Data',
        iconSize: 'md'
      }
    })

    const iconContainer = wrapper.find('.mb-4')
    expect(iconContainer.classes()).toContain('w-12')
    expect(iconContainer.classes()).toContain('h-12')
  })

  it('should apply xl icon size', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Data',
        iconSize: 'xl'
      }
    })

    const iconContainer = wrapper.find('.mb-4')
    expect(iconContainer.classes()).toContain('w-20')
    expect(iconContainer.classes()).toContain('h-20')
  })

  it('should render custom action slot', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Data'
      },
      slots: {
        action: '<button class="custom-action">Custom Button</button>'
      }
    })

    expect(wrapper.find('.custom-action').exists()).toBe(true)
    expect(wrapper.text()).toContain('Custom Button')
  })

  it('should have correct container classes', () => {
    const wrapper = mount(EmptyState, {
      props: {
        title: 'No Data'
      }
    })

    expect(wrapper.find('.flex.flex-col.items-center.justify-center').exists()).toBe(true)
  })
})