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

/** xterm 基础样式声明（与宿主 env.d.ts 同款；插件侧经 vite 提取 + inlinePluginCss 内联进 index.js） */
declare module '@xterm/xterm/css/xterm.css' {}
