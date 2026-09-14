import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import MarkdownEditor from '../src/ui/MarkdownEditor.vue'

/**
 * MarkdownEditor 组件测试
 *
 * 只测外部行为：v-model 契约、工具栏交互（插入结果 + 光标落点）、
 * 预览切换（v-html 渲染 + 草稿不丢失）、labels 覆盖、链接协议防护。
 */
function textareaValue(wrapper: ReturnType<typeof mount>): string {
  const el = wrapper.get('[data-testid="markdown-textarea"]').element as HTMLTextAreaElement
  return el.value
}

describe('MarkdownEditor', () => {
  it('modelValue 初始渲染进 textarea，输入派发 update:modelValue', async () => {
    const wrapper = mount(MarkdownEditor, { props: { modelValue: 'hello' } })
    expect(textareaValue(wrapper)).toBe('hello')

    const ta = wrapper.get('[data-testid="markdown-textarea"]')
    await ta.setValue('hello world')
    const emitted = wrapper.emitted('update:modelValue')!
    expect(emitted[emitted.length - 1]).toEqual(['hello world'])
  })

  it('外部 modelValue 变化同步进草稿（受控方向）', async () => {
    const wrapper = mount(MarkdownEditor, { props: { modelValue: 'a' } })
    await wrapper.setProps({ modelValue: 'b' })
    expect(textareaValue(wrapper)).toBe('b')
  })

  it('bold 工具栏：无选区插入模板并选中占位，派发新值', async () => {
    const wrapper = mount(MarkdownEditor, { props: { modelValue: 'hello' } })
    const ta = wrapper.get('[data-testid="markdown-textarea"]').element as HTMLTextAreaElement
    ta.selectionStart = 5
    ta.selectionEnd = 5

    await wrapper.get('[title="Bold"]').trigger('click')
    await nextTick()

    expect(textareaValue(wrapper)).toBe('hello**bold**')
    const emitted = wrapper.emitted('update:modelValue')!
    expect(emitted[emitted.length - 1]).toEqual(['hello**bold**'])
    expect(ta.selectionStart).toBe(7)
    expect(ta.selectionEnd).toBe(11)
  })

  it('bold 工具栏：有选区包裹选区', async () => {
    const wrapper = mount(MarkdownEditor, { props: { modelValue: 'hello world' } })
    const ta = wrapper.get('[data-testid="markdown-textarea"]').element as HTMLTextAreaElement
    ta.selectionStart = 0
    ta.selectionEnd = 5

    await wrapper.get('[title="Bold"]').trigger('click')
    await nextTick()

    expect(textareaValue(wrapper)).toBe('**hello** world')
  })

  it('h2 工具栏：行级动作作用于光标所在行', async () => {
    const wrapper = mount(MarkdownEditor, { props: { modelValue: 'title' } })
    await wrapper.get('[title="Heading 2"]').trigger('click')
    await nextTick()
    expect(textareaValue(wrapper)).toBe('## title')
  })

  it('预览切换：marked 渲染 v-html，切回编辑草稿不丢失', async () => {
    const wrapper = mount(MarkdownEditor, { props: { modelValue: '**bold**' } })

    await wrapper.get('[title="Preview"]').trigger('click')
    const preview = wrapper.get('[data-testid="markdown-preview"]')
    expect(preview.html()).toContain('<strong>bold</strong>')

    await wrapper.get('[title="Edit"]').trigger('click')
    expect(textareaValue(wrapper)).toBe('**bold**')
  })

  it('toolbar=false 隐藏工具栏；preview=false 隐藏预览切换', () => {
    const wrapper = mount(MarkdownEditor, {
      props: { modelValue: 'x', toolbar: false, preview: false },
    })
    expect(wrapper.find('.mde-toolbar').exists()).toBe(false)
  })

  it('labels 覆盖内置英文 title（i18n 中立）', () => {
    const wrapper = mount(MarkdownEditor, {
      props: { modelValue: 'x', labels: { bold: '加粗' } },
    })
    expect(wrapper.find('[title="加粗"]').exists()).toBe(true)
  })

  it('disabled 时 textarea 与工具栏按钮禁用，点击不派发', async () => {
    const wrapper = mount(MarkdownEditor, { props: { modelValue: 'x', disabled: true } })
    const ta = wrapper.get('[data-testid="markdown-textarea"]').element as HTMLTextAreaElement
    expect(ta.disabled).toBe(true)

    await wrapper.get('[title="Bold"]').trigger('click')
    await nextTick()
    expect(textareaValue(wrapper)).toBe('x')
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
  })

  it('链接协议白名单：javascript: 链接降级为纯文本（不输出 href）', async () => {
    const wrapper = mount(MarkdownEditor, {
      props: { modelValue: '[x](javascript:alert(1))' },
    })
    await wrapper.get('[title="Preview"]').trigger('click')
    const preview = wrapper.get('[data-testid="markdown-preview"]')
    expect(preview.html()).not.toContain('href="javascript:')
  })

  it('预览 XSS 防护：raw HTML 整体转义为文本，不生成可执行节点', async () => {
    const wrapper = mount(MarkdownEditor, {
      props: { modelValue: '<img src=x onerror=alert(1)>\n\n<div>raw</div>' },
    })
    await wrapper.get('[title="Preview"]').trigger('click')
    // 断言 v-html 实际载荷（安全边界在载荷而非 DOM）：必须是实体转义文本。
    // 注意：happy-dom 对 `&lt;tag&gt;` 的 innerHTML 解析会把解码后的尖括号
    // 重新 tokenize 成元素（实测 .html() 显示 <img> 元素），与真实浏览器
    // （实体解码只产文本节点，从不重新成签）行为不同——DOM 序列化断言不可靠。
    const payload = wrapper.vm.previewHtml as string
    expect(payload).not.toContain('<img')
    expect(payload).not.toContain('<div')
    expect(payload).toContain('&lt;img src=x onerror=alert(1)&gt;')
  })
})
