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

/** Vite 环境变量（仅声明本项目用到的字段，避免引入完整 vite/client 类型） */
interface ImportMeta {
  readonly env: {
    readonly DEV: boolean
    readonly PROD: boolean
    readonly MODE: string
    /** 终端输出传输层（"ws" 默认 | "channel"）：Channel 原生 IPC 替代 WS 环回 */
    readonly VITE_TERMINAL_TRANSPORT?: string
  }
}

declare module '*.css' {
  const content: string
  export default content
}

declare module '@xterm/xterm/css/xterm.css' {
  const content: string
  export default content
}
