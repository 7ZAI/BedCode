/// <reference types="vite/client" />

// .vue SFC 模块声明（与宿主主应用 env.d.ts 一致）：vue-tsc/volar 原生解析不受影响，
// 该 shim 供纯 tsserver（对 .ts 文件走普通 typescript 服务器）解析 './xxx.vue' 导入，
// 避免 typeScript:2307 Cannot find module 假阳性
/// <reference types="vite/client" />
declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>
  export default component
}
