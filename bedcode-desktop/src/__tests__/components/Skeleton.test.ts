import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import Skeleton from '@/components/Skeleton.vue'

describe('Skeleton Component', () => {
  it('should render with default shape (text)', () => {
    const wrapper = mount(Skeleton)

    expect(wrapper.find('.bg-dark-700').exists()).toBe(true)
    expect(wrapper.find('.animate-pulse').exists()).toBe(true)
    expect(wrapper.find('.bg-dark-700').classes()).toContain('h-4')
    expect(wrapper.find('.bg-dark-700').classes()).toContain('w-full')
  })

  it('should render circle shape', () => {
    const wrapper = mount(Skeleton, {
      props: {
        shape: 'circle'
      }
    })

    expect(wrapper.find('.bg-dark-700').classes()).toContain('rounded-full')
  })

  it('should render rect shape', () => {
    const wrapper = mount(Skeleton, {
      props: {
        shape: 'rect'
      }
    })

    // rect shape 没有额外�?shape class
    expect(wrapper.find('.bg-dark-700').exists()).toBe(true)
  })

  it('should apply custom width', () => {
    const wrapper = mount(Skeleton, {
      props: {
        width: '200px'
      }
    })

    const skeleton = wrapper.find('.bg-dark-700')
    expect(skeleton.attributes('style')).toContain('width: 200px')
  })

  it('should apply custom height', () => {
    const wrapper = mount(Skeleton, {
      props: {
        height: '50px'
      }
    })

    const skeleton = wrapper.find('.bg-dark-700')
    expect(skeleton.attributes('style')).toContain('height: 50px')
  })

  it('should apply numeric width as pixels', () => {
    const wrapper = mount(Skeleton, {
      props: {
        width: 100
      }
    })

    const skeleton = wrapper.find('.bg-dark-700')
    expect(skeleton.attributes('style')).toContain('width: 100px')
  })

  it('should apply numeric height as pixels', () => {
    const wrapper = mount(Skeleton, {
      props: {
        height: 32
      }
    })

    const skeleton = wrapper.find('.bg-dark-700')
    expect(skeleton.attributes('style')).toContain('height: 32px')
  })

  it('should apply both width and height', () => {
    const wrapper = mount(Skeleton, {
      props: {
        width: '150px',
        height: '20px'
      }
    })

    const skeleton = wrapper.find('.bg-dark-700')
    const style = skeleton.attributes('style')
    expect(style).toContain('width: 150px')
    expect(style).toContain('height: 20px')
  })

  it('should have animate-pulse class', () => {
    const wrapper = mount(Skeleton)

    expect(wrapper.find('.animate-pulse').exists()).toBe(true)
  })

  it('should have bg-dark-700 class', () => {
    const wrapper = mount(Skeleton)

    expect(wrapper.find('.bg-dark-700').exists()).toBe(true)
  })

  it('should have rounded class for text shape', () => {
    const wrapper = mount(Skeleton, {
      props: {
        shape: 'text'
      }
    })

    expect(wrapper.find('.bg-dark-700').classes()).toContain('rounded')
  })

  it('should use default size for circle when no dimensions specified', () => {
    const wrapper = mount(Skeleton, {
      props: {
        shape: 'circle'
      }
    })

    const skeleton = wrapper.find('.bg-dark-700')
    expect(skeleton.classes()).toContain('w-10')
    expect(skeleton.classes()).toContain('h-10')
  })

  it('should override default circle size with custom dimensions', () => {
    const wrapper = mount(Skeleton, {
      props: {
        shape: 'circle',
        width: 50,
        height: 50
      }
    })

    const skeleton = wrapper.find('.bg-dark-700')
    expect(skeleton.attributes('style')).toContain('width: 50px')
    expect(skeleton.attributes('style')).toContain('height: 50px')
  })
})