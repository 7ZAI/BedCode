/**
 * Plugin Loader
 *
 * 加载、激活、停用插件的 TS/Vue 前端模块
 * 插件代码编译进主 bundle，通过 import() 从 http://tauri.localhost/ 加载
 */

import type { PluginInfo, PluginModule, PluginContext } from './types'
import { convertFileSrc } from '@tauri-apps/api/core'
import * as pluginCmds from './commands'
import { createPluginContext } from './context'
import { clearPluginEvents } from './events'
import { getPluginRegistry } from './registry'

/** 已激活的插件实例 */
interface ActivePlugin {
  manifest: PluginInfo
  module: PluginModule
  context: PluginContext
}

/** 前端模块导入超时（毫秒） */
const IMPORT_TIMEOUT = 5000

/** 插件加载器 */
class PluginLoaderClass {
  private plugins: Map<string, ActivePlugin> = new Map()

  /** 应用启动时加载所有已激活插件的前端模块 */
  async loadAll(): Promise<void> {
    const manifests = await pluginCmds.pluginListLoaded()
    console.log(`[PluginLoader] Found ${manifests.length} plugin(s)`)

    for (const manifest of manifests) {
      // Rust-only 插件：前端无需加载
      if (manifest.pluginType === 'rust') {
        console.log(`[PluginLoader] Rust plugin ${manifest.id} managed by backend`)
        continue
      }

      const isActivated = manifest.state.state === 'Activated'
      if (!isActivated) {
        console.log(`[PluginLoader] Plugin ${manifest.id} not activated (state: ${manifest.state.state}), skipping`)
        continue
      }

      await this.loadFrontend(manifest)
    }
  }

  /** 激活指定插件 */
  async activate(pluginId: string): Promise<void> {
    if (this.plugins.has(pluginId)) return

    const info = await pluginCmds.pluginGetInfo(pluginId)
    if (!info) {
      console.error(`[PluginLoader] Plugin ${pluginId} not found`)
      return
    }

    try {
      await pluginCmds.pluginActivate(pluginId)
      await this.loadFrontend(info)
    } catch (e: any) {
      console.error(`[PluginLoader] Failed to activate ${pluginId}:`, e)
      await pluginCmds.pluginMarkError(pluginId, e.message || 'Activation failed')
    }
  }

  /** 停用插件 */
  async deactivate(pluginId: string): Promise<void> {
    const plugin = this.plugins.get(pluginId)
    if (!plugin) return

    // 清理所有 Disposable
    plugin.context._disposables.forEach((d: { dispose(): void }) => {
      try { d.dispose() } catch (e) {
        console.error(`[PluginLoader] Error disposing resource for ${pluginId}:`, e)
      }
    })

    // 清理事件监听
    clearPluginEvents(pluginId)

    // 清理注册表中的 context 和 UI 注册
    getPluginRegistry().clearPlugin(pluginId)

    // 调用插件的 deactivate
    if (plugin.module.deactivate) {
      try { await plugin.module.deactivate() } catch (e) {
        console.error(`[PluginLoader] Error in deactivate for ${pluginId}:`, e)
      }
    }

    // 通知后端
    try {
      await pluginCmds.pluginDeactivate(pluginId)
    } catch (e) {
      console.error(`[PluginLoader] Error notifying backend for deactivation of ${pluginId}:`, e)
    }

    this.plugins.delete(pluginId)
    console.log(`[PluginLoader] Plugin deactivated: ${pluginId}`)
  }

  /** 获取已激活插件 */
  getActivePlugin(pluginId: string): ActivePlugin | undefined {
    return this.plugins.get(pluginId)
  }

  /** 加载前端模块（内部方法） */
  private async loadFrontend(manifest: PluginInfo): Promise<void> {
    try {
      // 经 Tauri asset protocol 从插件目录直读前端模块
      // Android 上自动变为 http://tauri.localhost/，与桌面端 convertFileSrc 方案一致
      const module = await this.importWithTimeout(this.convertFileUrl(manifest.extensionPath, manifest.main))

      const context = createPluginContext(manifest)
      await this.activateWithTimeout(module, context)

      this.plugins.set(manifest.id, { manifest, module, context })
      getPluginRegistry().setContext(manifest.id, context)
      console.log(`[PluginLoader] Plugin frontend loaded: ${manifest.id}`)
    } catch (e: any) {
      console.error(`[PluginLoader] Failed to load frontend for ${manifest.id}:`, e)
      await pluginCmds.pluginMarkError(manifest.id, e.message || 'Frontend load failed')
    }
  }

  /** 将插件路径转换为可导入的 URL（通过 Tauri asset protocol） */
  private convertFileUrl(extensionPath: string, main: string): string {
    const filePath = `${extensionPath}/${main}`.replace(/\\/g, '/')
    return convertFileSrc(filePath)
  }

  /** 带超时的动态导入 */
  private async importWithTimeout(url: string): Promise<PluginModule> {
    let timer: ReturnType<typeof setTimeout>
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error(`Import timeout: ${url}`)), IMPORT_TIMEOUT)
    })
    try {
      return await Promise.race([
        import(/* @vite-ignore */ url),
        timeout,
      ])
    } finally {
      clearTimeout(timer!)
    }
  }

  /** 带超时的 activate 调用 */
  private async activateWithTimeout(
    module: PluginModule,
    context: PluginContext,
  ): Promise<void> {
    let timer: ReturnType<typeof setTimeout>
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error('Activate timeout')), IMPORT_TIMEOUT)
    })
    try {
      await Promise.race([module.activate(context), timeout])
    } finally {
      clearTimeout(timer!)
    }
  }
}

/** 全局单例 */
export const pluginLoader = new PluginLoaderClass()
