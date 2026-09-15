import type { DefineComponent } from 'vue'
import type { MarkdownSyntaxAction } from './markdown-editor-syntax'

/** 工具栏 title / 预览切换文案 key（labels 覆盖，默认内置英文） */
export type MarkdownEditorLabelKey = MarkdownSyntaxAction | 'preview' | 'edit' | 'toolbar'

/** MarkdownEditor 组件 props */
export interface MarkdownEditorProps {
  /** v-model 绑定值 */
  modelValue: string
  /** 空内容占位提示 */
  placeholder?: string
  /** 是否显示语法工具栏（默认 true） */
  toolbar?: boolean
  /** 是否显示预览切换按钮（默认 true） */
  preview?: boolean
  /** 禁用编辑与工具栏（保存中/预览锁定态） */
  disabled?: boolean
  /** 工具栏 title / 预览切换文案覆盖（不传用内置英文） */
  labels?: Partial<Record<MarkdownEditorLabelKey, string>>
}

/**
 * SDK 公共轻量 markdown 编辑器（textarea 内核 + 语法工具栏 + 预览切换）
 *
 * 用法：
 * ```vue
 * <MarkdownEditor v-model="draft" placeholder="Write markdown..." />
 * ```
 * - 触发 `update:modelValue`（输入 / 工具栏语法插入）
 * - 预览渲染用 marked（与移动端同库同版本，渲染行为一致）
 * - 样式走宿主设计 token，暗色/亮色随主题自动跟随
 */
export declare const MarkdownEditor: DefineComponent<MarkdownEditorProps>

export default MarkdownEditor
