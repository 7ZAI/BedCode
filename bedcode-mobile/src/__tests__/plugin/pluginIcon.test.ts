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

  it('SVG 消毒：script 注入被移除（XSS 纵深防御）', () => {
    const wrapper = mountIcon('<svg viewBox="0 0 24 24"><path d="M4 4h16v16H4z"/><script>alert("xss")</script></svg>')
    // 消毒后 <script> 整体移除，恶意载荷不进入 DOM
    expect(wrapper.find('script').exists()).toBe(false)
    expect(wrapper.html()).not.toContain('alert')
    // 合法 path 保留
    expect(wrapper.find('svg path').exists()).toBe(true)
  })

  it('SVG 消毒：事件属性（onclick）被移除', () => {
    const wrapper = mountIcon('<svg viewBox="0 0 24 24" onclick="alert(1)"><path d="M4 4h16v16H4z"/></svg>')
    const svg = wrapper.find('svg')
    expect(svg.attributes('onclick')).toBeUndefined()
    expect(wrapper.html()).not.toContain('alert(1)')
  })

  it('SVG 消毒：javascript: href 被移除', () => {
    const wrapper = mountIcon('<svg viewBox="0 0 24 24"><a href="javascript:alert(1)"><path d="M4 4h16v16H4z"/></a></svg>')
    expect(wrapper.html()).not.toContain('javascript:')
    // 链接元素仍在但危险 href 已被清空
    const anchor = wrapper.find('a')
    expect(anchor.exists()).toBe(true)
    expect(anchor.attributes('href')).toBeUndefined()
  })

  it('SVG 消毒：foreignObject 注入被移除', () => {
    const wrapper = mountIcon('<svg viewBox="0 0 24 24"><foreignObject><body xmlns="http://www.w3.org/1999/xhtml">xss</body></foreignObject><path d="M4 4h16v16H4z"/></svg>')
    expect(wrapper.find('foreignObject').exists()).toBe(false)
    expect(wrapper.html()).not.toContain('xss')
    expect(wrapper.find('svg path').exists()).toBe(true)
  })

  it('无 icon 时回退到字母头像', () => {
    const wrapper = mountIcon('')
    expect(wrapper.find('.letter-avatar').exists()).toBe(true)
  })
})