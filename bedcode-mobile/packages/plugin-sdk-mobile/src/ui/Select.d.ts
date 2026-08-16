import type { DefineComponent } from 'vue'

/** 下拉选项（value 同时作为 v-model 绑定值） */
export interface SelectOption {
  value: string | number
  label: string
}

/** Select 组件 props */
export interface SelectProps {
  modelValue: string | number
  /** 顶部标签文本（可选） */
  label?: string
  options: SelectOption[]
  /** 未选中时的占位文本；设置后下拉顶部出现"清除选择"行 */
  placeholder?: string
  disabled?: boolean
  required?: boolean
  error?: string
  /** md：宿主表单默认（44px/14px 触摸目标）；sm：插件紧凑布局（36px/12px） */
  size?: 'md' | 'sm'
}

/**
 * 移动端共享下拉选择组件（替代原生 <select>）
 *
 * 用法：
 * ```vue
 * <Select v-model="value" :options="options" size="sm" @open="refresh" />
 * ```
 * - 触发 `update:modelValue`（选中/清除）
 * - 触发 `open`（下拉展开时，可用于静默刷新选项）
 * - 面板 Teleport 到 body，safe-stack overlay 层，样式由 --mobile-* token 驱动
 */
export declare const Select: DefineComponent<SelectProps>

export default Select
