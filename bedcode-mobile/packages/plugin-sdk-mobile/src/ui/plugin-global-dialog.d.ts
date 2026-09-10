import type { DefineComponent } from 'vue'

/**
 * PluginGlobalDialog — 移动端宿主全局弹窗（通用能力）
 *
 * 与桌面端同构，仅在应用根挂载一次（宿主 App.vue / dev-shell App）；
 * 渲染完全由全局弹窗控制器（global-dialog.ts）驱动。
 */
export declare const PluginGlobalDialog: DefineComponent<Record<string, never>>

export default PluginGlobalDialog
