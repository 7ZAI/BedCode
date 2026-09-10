import type { DefineComponent } from 'vue'

/**
 * PluginGlobalDialog — 宿主全局弹窗（通用能力）
 *
 * 仅在应用根挂载一次（宿主 DesktopLayout / dev-shell App）；渲染完全由
 * 全局弹窗控制器（global-dialog.ts）驱动，插件经 context.ui.showDialog 操控。
 * 挂载方无 props 需传。
 */
export declare const PluginGlobalDialog: DefineComponent<Record<string, never>>

export default PluginGlobalDialog