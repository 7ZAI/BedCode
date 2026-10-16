/**
 * Vite 环境类型声明
 *
 * 声明 `*.vue` SFC 模块（vue-tsc 类型检查用，与 auto-task 插件同款垫片）与
 * `*.css?inline` 导入为字符串（index.ts 运行时注入用），
 * 使 tsc/vue-tsc 对插件源码类型检查通过。
 */
declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>
  export default component
}

declare module '*.css?inline' {
  const content: string
  export default content
}
