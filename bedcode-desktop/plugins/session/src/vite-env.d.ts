/**
 * Vite 客户端类型声明（`*.css?inline` 等资源导入的 TS 类型来源）
 */
/// <reference types="vite/client" />

/** Vue 单文件组件类型声明（tsc --noEmit 检查用） */
declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<{}, {}, any>
  export default component
}
