/// <reference types="vite/client" />

/** Vue 单文件组件类型声明（tsc --noEmit 检查用） */
declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<{}, {}, any>
  export default component
}
