/// <reference types="vite/client" />

declare module 'virtual:dev-plugins' {
  export interface DevPluginRecordSpec {
    dir: string
    manifest: Record<string, any>
    entry: any
  }
  const records: DevPluginRecordSpec[]
  export default records
}
