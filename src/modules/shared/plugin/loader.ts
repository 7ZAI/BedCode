/**
 * Plugin Loader
 *
 * 加载、激活、停用插件 — 前端入口
 */

import { convertFileSrc } from '@tauri-apps/api/core'
import type { PluginInfo, PluginModule, PluginContext } from './types'
import * as pluginCmds from './commands'
import { createPluginContext } from './context'
import { clearPluginEvents } from './events'

/** 已激活的插件实例 */
interface ActivePlugin {
  manifest: PluginInfo
  module: PluginModule
  context: PluginContext
}

/** 插件加载器 */
class PluginLoaderClass {
  private plugins: Map<string, ActivePlugin> = new Map()

  /** 加载所有插件（应用启动时调用） */
  async loadAll(): Promise<void> {
    const manifests = await pluginCmds.pluginListLoaded()
    console.log(`[PluginLoader] Found ${manifests.length} plugin(s)`)

    for (const manifest of manifests) {
      if (manifest.sandbox !== 'inline') {
        console.warn(`[PluginLoader] Skipping ${manifest.id}: unsupported sandbox mode "${manifest.sandbox}"`)
        continue
      }

      // 判断是否需要按需激活
      const shouldLazy = this.shouldLazyActivate(manifest)
      if (shouldLazy) {
        console.log(`[PluginLoader] Plugin ${manifest.id} will be lazy-activated`)
        continue
      }

      // 无 contributes 的插件立即激活
      await this.loadInline(manifest)
    }
  }

  /** 按需激活插件 */
  async activate(pluginId: string): Promise<void> {
    if (this.plugins.has(pluginId)) {
      return
    }

    const info = await pluginCmds.pluginGetInfo(pluginId)
    if (!info) {
      console.error(`[PluginLoader] Plugin ${pluginId} not found`)
      return
    }

    await this.loadInline(info)
  }

  /** 停用插件 */
  async deactivate(pluginId: string): Promise<void> {
    const plugin = this.plugins.get(pluginId)
    if (!plugin) return

    // 清理所有 Disposable
    plugin.context._disposables.forEach(d => {
      try {
        d.dispose()
      } catch (e) {
        console.error(`[PluginLoader] Error disposing resource for ${pluginId}:`, e)
      }
    })

    // 清理事件监听
    clearPluginEvents(pluginId)

    // 调用插件的 deactivate
    if (plugin.module.deactivate) {
      try {
        await plugin.module.deactivate()
      } catch (e) {
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

  /** 获取已激活的插件 */
  getActivePlugin(pluginId: string): ActivePlugin | undefined {
    return this.plugins.get(pluginId)
  }

  /** 获取所有已激活插件 */
  getActivePlugins(): ActivePlugin[] {
    return Array.from(this.plugins.values())
  }

  /** 判断插件是否需要按需激活
   *
   * 有 views 的插件立即激活（需要在侧边栏/工具箱显示入口）
   * 仅声明 commands/terminal 的插件懒激活（按需调用，如命令面板触发）
   */
  private shouldLazyActivate(manifest: PluginInfo): boolean {
    const c = manifest.contributes
    if (c.views.length > 0) return false
    return (
      c.commands.length > 0 ||
      !!c.terminal
    )
  }

  /** 加载 inline 模式插件 */
  private async loadInline(manifest: PluginInfo): Promise<void> {
    const ACTIVATE_TIMEOUT = 5000

    try {
      // 通知后端标记激活
      await pluginCmds.pluginActivate(manifest.id)

      // 动态导入插件入口文件
      const entryUrl = this.convertFileUrl(manifest.extensionPath, manifest.main)
      const module = await this.importWithTimeout(entryUrl, ACTIVATE_TIMEOUT)

      // 创建 PluginContext
      const context = createPluginContext(manifest)

      // 调用 activate
      await this.activateWithTimeout(module, context, ACTIVATE_TIMEOUT)

      this.plugins.set(manifest.id, { manifest, module, context })
      console.log(`[PluginLoader] Plugin activated: ${manifest.id}`)
    } catch (e: any) {
      console.error(`[PluginLoader] Failed to activate ${manifest.id}:`, e)
      await pluginCmds.pluginMarkError(manifest.id, e.message || 'Activation failed')
    }
  }

  /** 将插件路径转换为可导入的 URL（通过 Tauri asset protocol） */
  private convertFileUrl(extensionPath: string, main: string): string {
    const filePath = `${extensionPath}/${main}`.replace(/\\/g, '/')
    return convertFileSrc(filePath)
  }

  /** 带超时的动态导入 */
  private async importWithTimeout(url: string, timeoutMs: number): Promise<PluginModule> {
    let timer: ReturnType<typeof setTimeout>
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error(`Import timeout: ${url}`)), timeoutMs)
    })
    try {
      return await Promise.race([import(/* @vite-ignore */ url), timeout])
    } finally {
      clearTimeout(timer!)
    }
  }

  /** 带超时的 activate 调用 */
  private async activateWithTimeout(
    module: PluginModule,
    context: PluginContext,
    timeoutMs: number,
  ): Promise<void> {
    let timer: ReturnType<typeof setTimeout>
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error('Activate timeout')), timeoutMs)
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
