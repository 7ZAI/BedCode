/// <reference types="vite/client" />
/** 应用版本号，由 Vite 编译时从 tauri.conf.json 注入 */
declare const __APP_VERSION__: string

declare module '*.css' {
  const content: string
  export default content
}

declare module '@xterm/xterm/css/xterm.css' {
  const content: string
  export default content
}

declare module '*.md?raw' {
  const content: string
  export default content
}