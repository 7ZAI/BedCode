/// <reference types="vite/client" />
/** 应用版本号，由 Vite 编译时从 tauri.conf.json 注入 */
declare const __APP_VERSION__: string

declare module '*.css' {
  const content: string
  export default content
}

// .vue SFC 模块声明（与桌面端 env.d.ts 一致）：vue-tsc/volar 原生解析 .vue 不受影响，
// 该 shim 供纯 tsserver（pi-lens 对 .ts 文件走普通 typescript 服务器）解析 '@/...vue' 导入，
// 避免 typeScript:2307 Cannot find module 假阳性
declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>
  export default component
}

declare module '@xterm/xterm/css/xterm.css' {
  const content: string
  export default content
}

declare module '*.md?raw' {
  const content: string
  export default content
}