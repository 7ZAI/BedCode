import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import NotificationBadge from '@/components/NotificationBadge.vue'

describe('NotificationBadge Component', () => {
  it('should render trigger element', () => {
    const wrapper = mount(NotificationBadge, {
      slots: {
        default: '<button>Notification</button>'
      }
    })

    expect(wrapper.find('button').exists()).toBe(true)
    expect(wrapper.text()).toContain('Notification')
  })

  it('should not show badge when count is 0 and variant is count', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 0,
        variant: 'count'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    expect(wrapper.find('span.absolute').exists()).toBe(false)
  })

  it('should show badge when count > 0 and variant is count', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 5,
        variant: 'count'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    expect(wrapper.find('span.absolute').exists()).toBe(true)
    expect(wrapper.text()).toContain('5')
  })

  it('should always show badge when variant is dot', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 0,
        variant: 'dot'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    expect(wrapper.find('span.absolute').exists()).toBe(true)
  })

  it('should display max+ when count exceeds max', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 150,
        max: 99,
        variant: 'count'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    expect(wrapper.text()).toContain('99+')
  })

  it('should display exact count when count equals max', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 99,
        max: 99,
        variant: 'count'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    expect(wrapper.text()).toContain('99')
    expect(wrapper.text()).not.toContain('99+')
  })

  it('should apply danger color class by default', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 5,
        variant: 'count'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    const badge = wrapper.find('span.absolute')
    expect(badge.classes()).toContain('bg-red-500')
    expect(badge.classes()).toContain('text-white')
  })

  it('should apply primary color class', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 5,
        variant: 'count',
        color: 'primary'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    const badge = wrapper.find('span.absolute')
    expect(badge.classes()).toContain('bg-primary-500')
  })

  it('should apply warning color class', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 5,
        variant: 'count',
        color: 'warning'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    const badge = wrapper.find('span.absolute')
    expect(badge.classes()).toContain('bg-yellow-500')
    expect(badge.classes()).toContain('text-dark-900')
  })

  it('should apply success color class', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 5,
        variant: 'count',
        color: 'success'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    const badge = wrapper.find('span.absolute')
    expect(badge.classes()).toContain('bg-green-500')
  })

  it('should apply top-right position by default', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 5
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    const badge = wrapper.find('span.absolute')
    expect(badge.classes()).toContain('-top-1')
    expect(badge.classes()).toContain('-right-1')
  })

  it('should apply top-left position', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 5,
        position: 'top-left'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    const badge = wrapper.find('span.absolute')
    expect(badge.classes()).toContain('-top-1')
    expect(badge.classes()).toContain('-left-1')
  })

  it('should apply bottom-right position', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 5,
        position: 'bottom-right'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    const badge = wrapper.find('span.absolute')
    expect(badge.classes()).toContain('-bottom-1')
    expect(badge.classes()).toContain('-right-1')
  })

  it('should apply md size by default for count', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 5,
        variant: 'count'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    const badge = wrapper.find('span.absolute')
    expect(badge.classes()).toContain('min-w-5')
    expect(badge.classes()).toContain('h-5')
  })

  it('should apply sm size for count', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 5,
        variant: 'count',
        size: 'sm'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    const badge = wrapper.find('span.absolute')
    expect(badge.classes()).toContain('min-w-4')
    expect(badge.classes()).toContain('h-4')
  })

  it('should apply lg size for count', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        count: 5,
        variant: 'count',
        size: 'lg'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    const badge = wrapper.find('span.absolute')
    expect(badge.classes()).toContain('min-w-6')
    expect(badge.classes()).toContain('h-6')
  })

  it('should apply correct size for dot variant', () => {
    const wrapper = mount(NotificationBadge, {
      props: {
        variant: 'dot',
        size: 'lg'
      },
      slots: {
        default: '<span>Icon</span>'
      }
    })

    const badge = wrapper.find('span.absolute')
    expect(badge.classes()).toContain('w-3')
    expect(badge.classes()).toContain('h-3')
  })
})