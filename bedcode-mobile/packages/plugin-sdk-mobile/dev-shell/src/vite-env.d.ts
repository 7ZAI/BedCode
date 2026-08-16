/// <reference types="vite/client" />

declare module 'virtual:dev-plugins' {
  export interface DevPluginRecordSpec {
    /** 插件目录（绝对路径） */
    dir: string
    /** plugin.json 解析结果 */
    manifest: Record<string, any>
    /** 插件前端入口模块（activate/deactivate） */
    entry: any
  }
  const records: DevPluginRecordSpec[]
  export default records
}
