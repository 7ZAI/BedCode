/**
 * SDK 类型环境声明（模板自身用）
 *
 * 模板是 create 命令生成的样板工程，生成前不依赖 node_modules——本声明让
 * tsserver 在模板目录内可解析 SDK 包名（避免 Cannot find module 假阳性）。
 * 生成的插件工程 `pnpm install` 后以真实 SDK 类型为准（ambient 声明优先级
 * 低于实际模块解析，自动被覆盖），此文件仅作模板开发期的类型兜底。
 */
declare module '@binblink/bedcode-plugin-sdk-mobile' {
  /** 插件上下文（最小可编译形状；生成工程以 SDK 真实类型为准） */
  export interface PluginContext {
    [key: string]: unknown
  }
}

declare module '@binblink/bedcode-plugin-sdk-mobile/ui' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>
  export default component
}

declare module '@binblink/bedcode-plugin-sdk-mobile/vite' {
  export function bedcodePlugin(options?: Record<string, unknown>): unknown
}
