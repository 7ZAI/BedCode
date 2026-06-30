import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import Spinner from '@/modules/shared/components/Spinner.vue'

describe('Spinner Component', () => {
  it('should render with default props (circle variant)', () => {
    const wrapper = mount(Spinner)

    // 默认�?circle variant，应该渲�?SVG
    expect(wrapper.find('svg').exists()).toBe(true)
    expect(wrapper.find('svg').classes()).toContain('animate-spin')
  })

  it('should apply size class', () => {
    const wrapper = mount(Spinner, {
      props: {
        size: 'lg'
      }
    })

    // lg 尺寸对应 w-6 h-6
    expect(wrapper.find('.inline-flex').classes()).toContain('w-6')
    expect(wrapper.find('.inline-flex').classes()).toContain('h-6')
  })

  it('should apply color class for circle variant', () => {
    const wrapper = mount(Spinner, {
      props: {
        color: 'white'
      }
    })

    expect(wrapper.find('svg').classes()).toContain('text-white')
  })

  it('should render dots variant', () => {
    const wrapper = mount(Spinner, {
      props: {
        variant: 'dots'
      }
    })

    // dots variant 应该渲染 3 个圆�?    expect(wrapper.find('svg').exists()).toBe(false)
    expect(wrapper.findAll('span.rounded-full.animate-bounce')).toHaveLength(3)
  })

  it('should render pulse variant', () => {
    const wrapper = mount(Spinner, {
      props: {
        variant: 'pulse'
      }
    })

    // pulse variant 应该渲染 ping 动画
    expect(wrapper.find('svg').exists()).toBe(false)
    expect(wrapper.find('span.animate-ping').exists()).toBe(true)
  })

  it('should apply correct dot size for dots variant', () => {
    const wrapper = mount(Spinner, {
      props: {
        variant: 'dots',
        size: 'xl'
      }
    })

    // xl 尺寸�?dots 应该�?w-3 h-3
    const dots = wrapper.findAll('.animate-bounce')
    dots.forEach(dot => {
      expect(dot.classes()).toContain('w-3')
      expect(dot.classes()).toContain('h-3')
    })
  })

  it('should apply correct color for dots variant', () => {
    const wrapper = mount(Spinner, {
      props: {
        variant: 'dots',
        color: 'danger'
      }
    })

    // danger 颜色应该使用 text-red-500
    const dots = wrapper.findAll('.animate-bounce')
    expect(dots[0].classes()).toContain('text-red-500')
  })

  it('should apply success color', () => {
    const wrapper = mount(Spinner, {
      props: {
        color: 'success'
      }
    })

    expect(wrapper.find('svg').classes()).toContain('text-green-500')
  })

  it('should apply warning color', () => {
    const wrapper = mount(Spinner, {
      props: {
        color: 'warning'
      }
    })

    expect(wrapper.find('svg').classes()).toContain('text-yellow-500')
  })

  it('should apply correct color for pulse variant', () => {
    const wrapper = mount(Spinner, {
      props: {
        variant: 'pulse',
        color: 'dark'
      }
    })

    expect(wrapper.find('span.animate-ping').classes()).toContain('text-dark-400')
  })

  it('should have staggered animation delay for dots', () => {
    const wrapper = mount(Spinner, {
      props: {
        variant: 'dots'
      }
    })

    const dots = wrapper.findAll('.animate-bounce')

    // 第一个点延迟 0ms，第二个 150ms，第三个 300ms
    expect(dots[0].attributes('style')).toContain('animation-delay: 0ms')
    expect(dots[1].attributes('style')).toContain('animation-delay: 150ms')
    expect(dots[2].attributes('style')).toContain('animation-delay: 300ms')
  })

  it('should render sm size correctly', () => {
    const wrapper = mount(Spinner, {
      props: {
        size: 'sm'
      }
    })

    expect(wrapper.find('.inline-flex').classes()).toContain('w-4')
    expect(wrapper.find('.inline-flex').classes()).toContain('h-4')
  })

  it('should render xl size correctly', () => {
    const wrapper = mount(Spinner, {
      props: {
        size: 'xl'
      }
    })

    expect(wrapper.find('.inline-flex').classes()).toContain('w-8')
    expect(wrapper.find('.inline-flex').classes()).toContain('h-8')
  })
})