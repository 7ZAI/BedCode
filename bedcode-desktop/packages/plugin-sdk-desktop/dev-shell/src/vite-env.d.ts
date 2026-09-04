/// <reference types="vite/client" />

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>
  export default component
}

declare module 'virtual:dev-plugins' {
  export interface DevPluginRecordSpec {
    dir: string
    manifest: Record<string, unknown>
    entry: any
  }
  const records: DevPluginRecordSpec[]
  export default records
}
