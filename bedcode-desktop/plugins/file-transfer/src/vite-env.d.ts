/// <reference types="vite/client" />

/** Vue 单文件组件类型声明（tsc --noEmit 检查用） */
declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<{}, {}, any>
  export default component
}

/** 内联 CSS 字符串导入（`?inline` 后缀，vite 构建期把 CSS 转为字符串） */
declare module '*.css?inline' {
  const css: string
  export default css
}
