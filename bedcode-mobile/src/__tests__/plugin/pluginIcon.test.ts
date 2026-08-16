/**
 * PluginIcon 图标类型渲染测试
 *
 * 验证 manifest.icon 的四种形态各自落到正确分支，重点覆盖：
 * 原始 SVG path data（M 开头）应渲染为内联 <path>，而非作为文本显示
 */
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import PluginIcon from '@/components/PluginIcon.vue'

/** 自动任务插件的原始 SVG path data（旧版误判为 emoji 导致显示一串数字） */
const SVG_PATH = 'M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 7l2 2 4-4'

function mountIcon(icon: string) {
  return mount(PluginIcon, {
    props: { icon, name: 'Auto Task', pluginId: 'com.bedcode.auto-task' },
    global: {
      stubs: { LetterAvatar: { template: '<div class="letter-avatar" />' } },
    },
  })
}

describe('PluginIcon', () => {
  it('原始 SVG path data 应渲染为内联 <path>，而非文本内容', () => {
    const wrapper = mountIcon(SVG_PATH)
    const path = wrapper.find('svg path')
    expect(path.exists()).toBe(true)
    expect(path.attributes('d')).toBe(SVG_PATH)
    // 不应把 path data 作为文本节点显示
    expect(wrapper.text().includes('M9 5H7')).toBe(false)
  })

  it('内联 <svg> 标记走消毒渲染分支', () => {
    const wrapper = mountIcon('<svg viewBox="0 0 24 24"><path d="M4 4h16v16H4z"/></svg>')
    expect(wrapper.find('svg').exists()).toBe(true)
  })

  it('emoji 图标直接渲染为文本', () => {
    const wrapper = mountIcon('🧩')
    expect(wrapper.text()).toContain('🧩')
  })

  it('无 icon 时回退到字母头像', () => {
    const wrapper = mountIcon('')
    expect(wrapper.find('.letter-avatar').exists()).toBe(true)
  })
})