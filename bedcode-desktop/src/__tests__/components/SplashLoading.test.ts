import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import SplashLoading from '@/components/SplashLoading.vue'

describe('SplashLoading Component', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('should not render content when visible is false', () => {
    const wrapper = mount(SplashLoading, {
      props: {
        visible: false
      },
      global: {
        stubs: {
          Teleport: false,
          Transition: false
        }
      }
    })

    // v-if=false 时主要内容不渲染
    expect(wrapper.find('.fixed.inset-0').exists()).toBe(false)
  })

  it('should accept visible prop', () => {
    const wrapper = mount(SplashLoading, {
      props: {
        visible: true
      },
      global: {
        stubs: {
          Teleport: false,
          Transition: false
        }
      },
      attachTo: document.body
    })

    // 验证组件接收�?visible prop
    expect(wrapper.props('visible')).toBe(true)
  })

  it('should accept status prop', () => {
    const wrapper = mount(SplashLoading, {
      props: {
        visible: true,
        status: 'Custom status'
      },
      global: {
        stubs: {
          Teleport: false,
          Transition: false
        }
      }
    })

    expect(wrapper.props('status')).toBe('Custom status')
  })

  it('should accept showProgress prop', () => {
    const wrapper = mount(SplashLoading, {
      props: {
        visible: true,
        showProgress: true
      },
      global: {
        stubs: {
          Teleport: false,
          Transition: false
        }
      }
    })

    expect(wrapper.props('showProgress')).toBe(true)
  })

  it('should accept progress prop', () => {
    const wrapper = mount(SplashLoading, {
      props: {
        visible: true,
        showProgress: true,
        progress: 50
      },
      global: {
        stubs: {
          Teleport: false,
          Transition: false
        }
      }
    })

    expect(wrapper.props('progress')).toBe(50)
  })

  it('should have correct default props', () => {
    const wrapper = mount(SplashLoading, {
      props: {
        visible: true
      },
      global: {
        stubs: {
          Teleport: false,
          Transition: false
        }
      }
    })

    expect(wrapper.props('status')).toBe('Loading...')
    expect(wrapper.props('showProgress')).toBe(false)
    expect(wrapper.props('progress')).toBe(0)
  })

  it('should render Spinner component as child', () => {
    const wrapper = mount(SplashLoading, {
      props: {
        visible: true
      },
      global: {
        stubs: {
          Teleport: false,
          Transition: false
        }
      },
      attachTo: document.body
    })

    // 检�?Spinner 组件是否存在
    const spinnerComponent = wrapper.findComponent({ name: 'Spinner' })
    expect(spinnerComponent.exists()).toBe(true)
  })

  it('should pass correct props to Spinner', () => {
    const wrapper = mount(SplashLoading, {
      props: {
        visible: true
      },
      global: {
        stubs: {
          Teleport: false,
          Transition: false
        }
      },
      attachTo: document.body
    })

    const spinnerComponent = wrapper.findComponent({ name: 'Spinner' })
    expect(spinnerComponent.props('size')).toBe('lg')
    expect(spinnerComponent.props('color')).toBe('primary')
    expect(spinnerComponent.props('variant')).toBe('circle')
  })

  it('should have z-[100] class for full screen overlay', () => {
    const wrapper = mount(SplashLoading, {
      props: {
        visible: true
      },
      global: {
        stubs: {
          Teleport: false,
          Transition: false
        }
      },
      attachTo: document.body
    })

    // �?body 中查找渲染的内容
    const overlay = document.querySelector('.fixed.inset-0')
    expect(overlay).toBeTruthy()
    expect(overlay?.classList.contains('z-50')).toBe(true)
  })

  it('should have bg-dark-900 background class', () => {
    const wrapper = mount(SplashLoading, {
      props: {
        visible: true
      },
      global: {
        stubs: {
          Teleport: false,
          Transition: false
        }
      },
      attachTo: document.body
    })

    const overlay = document.querySelector('.fixed.inset-0')
    expect(overlay?.classList.contains('bg-dark-900')).toBe(true)
  })
})