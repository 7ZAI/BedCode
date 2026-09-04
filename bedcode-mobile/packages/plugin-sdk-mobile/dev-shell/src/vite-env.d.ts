/// <reference types="vite/client" />

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>
  export default component
}

declare module 'virtual:dev-plugins' {
  export interface DevPluginRecordSpec {
    /** 插件目录（绝对路径） */
    dir: string
    /** plugin.json 解析结果 */
    manifest: Record<string, unknown>
    /** 插件前端入口模块（activate/deactivate） */
    entry: any
  }
  const records: DevPluginRecordSpec[]
  export default records
}
