/** 插件共享模块全局变量 */
interface BedCodeSharedModules {
  vue: typeof import('vue')
  'vue-i18n': typeof import('vue-i18n')
  pinia: typeof import('pinia')
  /** 宿主 i18n 实例，供插件模块级代码使用 */
  i18n: import('vue-i18n').I18n
  /** 宿主路由实例 */
  router: import('vue-router').Router
}

interface Window {
  __BEDCODE_SHARED__: BedCodeSharedModules
}

declare module '*.css' {
  const content: string
  export default content
}

declare module '@xterm/xterm/css/xterm.css' {
  const content: string
  export default content
}
