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
  /** md：宿主表单默认（--input-height 36px）；sm：插件紧凑布局（32px/12px） */
  size?: 'md' | 'sm'
}

/**
 * 宿主共享下拉选择组件（替代原生 <select>）
 *
 * 用法：
 * ```vue
 * <Select v-model="value" :options="options" size="sm" @open="refresh" />
 * ```
 * - 触发 `update:modelValue`（选中/清除）
 * - 触发 `open`（下拉展开时，可用于静默刷新选项）
 */
export declare const Select: DefineComponent<SelectProps>

export default Select
