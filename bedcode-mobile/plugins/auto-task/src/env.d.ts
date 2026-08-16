/**
 * Vite 环境类型声明
 *
 * 声明 `*.css?inline` 导入为字符串（index.ts 运行时注入用），
 * 使 tsc/vue-tsc 对插件源码类型检查通过。
 */
declare module '*.css?inline' {
  const content: string
  export default content
}
