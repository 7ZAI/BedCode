import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import Tooltip from '@/components/Tooltip.vue'

describe('Tooltip Component', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('should render trigger element', () => {
    const wrapper = mount(Tooltip, {
      props: {
        content: 'Test tooltip'
      },
      slots: {
        default: '<button>Hover me</button>'
      }
    })

    expect(wrapper.find('button').exists()).toBe(true)
    expect(wrapper.text()).toContain('Hover me')
  })

  it('should have correct default props', () => {
    const wrapper = mount(Tooltip, {
      props: {
        content: 'Test tooltip'
      },
      slots: {
        default: '<button>Hover me</button>'
      }
    })

    expect(wrapper.props('content')).toBe('Test tooltip')
    expect(wrapper.props('position')).toBe('top')
    expect(wrapper.props('delay')).toBe(200)
  })

  it('should accept custom position prop', () => {
    const wrapper = mount(Tooltip, {
      props: {
        content: 'Test tooltip',
        position: 'bottom'
      },
      slots: {
        default: '<button>Hover me</button>'
      }
    })

    expect(wrapper.props('position')).toBe('bottom')
  })

  it('should accept custom delay prop', () => {
    const wrapper = mount(Tooltip, {
      props: {
        content: 'Test tooltip',
        delay: 500
      },
      slots: {
        default: '<button>Hover me</button>'
      }
    })

    expect(wrapper.props('delay')).toBe(500)
  })

  it('should not show tooltip content initially', () => {
    const wrapper = mount(Tooltip, {
      props: {
        content: 'Test tooltip'
      },
      slots: {
        default: '<button>Hover me</button>'
      },
      global: {
        stubs: {
          Teleport: false,
          Transition: false
        }
      }
    })

    // tooltip 内容初始不可见（v-if="visible"，visible 初始�?false�?    const tooltipContent = wrapper.find('.fixed.z-50')
    expect(tooltipContent.exists()).toBe(false)
  })

  it('should render relative inline-block container', () => {
    const wrapper = mount(Tooltip, {
      props: {
        content: 'Test tooltip'
      },
      slots: {
        default: '<button>Hover me</button>'
      }
    })

    expect(wrapper.find('.relative.inline-block').exists()).toBe(true)
  })

  it('should set show timer on mouseenter', async () => {
    const wrapper = mount(Tooltip, {
      props: {
        content: 'Test tooltip',
        delay: 100
      },
      slots: {
        default: '<button>Hover me</button>'
      },
      attachTo: document.body
    })

    const container = wrapper.find('.relative.inline-block')
    await container.trigger('mouseenter')

    // Timer 应该已经启动
    vi.advanceTimersByTime(100)
    await wrapper.vm.$nextTick()

    // tooltip 应该�?body 中渲�?    const tooltip = document.querySelector('.fixed.z-50')
    expect(tooltip).toBeTruthy()
  })

  it('should have correct tooltip styling classes', async () => {
    const wrapper = mount(Tooltip, {
      props: {
        content: 'Test tooltip',
        delay: 0
      },
      slots: {
        default: '<button>Hover me</button>'
      },
      global: {
        stubs: {
          Teleport: false,
          Transition: false
        }
      },
      attachTo: document.body
    })

    const container = wrapper.find('.relative.inline-block')
    await container.trigger('mouseenter')
    vi.advanceTimersByTime(0)
    await wrapper.vm.$nextTick()

    const tooltip = document.querySelector('.fixed.z-50')
    expect(tooltip).toBeTruthy()
    expect(tooltip?.classList.contains('bg-dark-700')).toBe(true)
    expect(tooltip?.classList.contains('rounded-lg')).toBe(true)
  })

  it('should render tooltip content text', async () => {
    const wrapper = mount(Tooltip, {
      props: {
        content: 'Tooltip message',
        delay: 0
      },
      slots: {
        default: '<button>Hover</button>'
      },
      global: {
        stubs: {
          Teleport: false,
          Transition: false
        }
      },
      attachTo: document.body
    })

    const container = wrapper.find('.relative.inline-block')
    await container.trigger('mouseenter')
    vi.advanceTimersByTime(0)
    await wrapper.vm.$nextTick()

    const tooltip = document.querySelector('.fixed.z-50')
    expect(tooltip).toBeTruthy()
    expect(tooltip?.textContent).toContain('Tooltip message')
  })
})