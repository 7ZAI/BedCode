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

/** 启动扫描就绪轮询间隔（毫秒） */
const STARTUP_SCAN_POLL_MS = 250

/** 启动扫描就绪等待上限（毫秒） */
const STARTUP_SCAN_TIMEOUT_MS = 20000

/** 插件加载器 */
class PluginLoaderClass {
  private plugins: Map<string, ActivePlugin> = new Map()
  private scanRetryTimer: ReturnType<typeof setTimeout> | null = null
  private scanRetryStartedAt = 0

  /** 应用启动时加载所有已启用插件的前端模块 */
  async loadAll(): Promise<void> {
    const manifests = await pluginCmds.pluginListLoaded()
    console.log(`[PluginLoader] Found ${manifests.length} plugin(s)`)
    await this.loadManifests(manifests)

    // 后端在 setup 的异步任务里解压内置插件 → 初始化 WASM 运行时 → 扫描加载，
    // 实测全程数秒（设备上 WASM 编译慢）。前端首次查询 plugin_list_loaded 极可能
    // 落在扫描完成前，拿到空列表 → 遍历 0 个 manifest → 扩展点（工具箱/导航/设置/
    // 终端）全部丢失，直到手动重开插件才恢复。此处异步轮询兜底：扫描完成（列表
    // 非空）即补载；不阻塞 app.mount()（挂载不受插件初始化延迟）。
    if (manifests.length === 0) {
      this.scheduleScanRetry()
    }
  }

  /** 启动扫描未完成时按间隔轮询补载，直到列表非空或超时放弃 */
  private scheduleScanRetry(): void {
    if (this.scanRetryTimer) return
    this.scanRetryStartedAt = Date.now()
    const tick = async () => {
      this.scanRetryTimer = null
      try {
        const manifests = await pluginCmds.pluginListLoaded()
        console.log(`[PluginLoader] Scan retry: found ${manifests.length} plugin(s)`)
        if (manifests.length > 0) {
          await this.loadManifests(manifests)
          return
        }
      } catch (e) {
        // 后端尚未就绪时命令异常：继续轮询，直到超时
        console.warn('[PluginLoader] Scan retry query failed, will retry:', e)
      }
      if (Date.now() - this.scanRetryStartedAt < STARTUP_SCAN_TIMEOUT_MS) {
        this.scanRetryTimer = setTimeout(tick, STARTUP_SCAN_POLL_MS)
      } else {
        console.warn('[PluginLoader] Plugin scan not ready after timeout; entries need manual re-toggle')
      }
    }
    this.scanRetryTimer = setTimeout(tick, STARTUP_SCAN_POLL_MS)
  }

  /** 逐插件加载前端模块（幂等：重试/重复调用时跳过已加载插件） */
  private async loadManifests(manifests: PluginInfo[]): Promise<void> {
    for (const manifest of manifests) {
      // Rust-only 插件：前端无需加载
      if (manifest.pluginType === 'rust') {
        console.log(`[PluginLoader] Rust plugin ${manifest.id} managed by backend`)
        continue
      }

      if (this.plugins.has(manifest.id)) continue

      // 以持久化启用状态为准，而非当前运行时激活状态。
      // 原因：后端在 setup 的异步任务里自动激活已启用插件，前端首次查询
      // plugin_list_loaded 时可能尚未完成，插件仍处于 Loaded 状态，若据此
      // 跳过会导致重启后 UI 扩展点（工具箱/导航/设置/终端）丢失，直到手动
      // 重开插件才恢复。启用状态是持久化存储，查询时立即可用，不受该竞态影响。
      const isEnabled = await pluginCmds.pluginIsEnabled(manifest.id)
      if (!isEnabled) {
        console.log(`[PluginLoader] Plugin ${manifest.id} not enabled, skipping frontend load`)
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

    // 清理事件监听（兜底清理失败不中断后续流程，避免注册表残留）
    try {
      clearPluginEvents(pluginId)
    } catch (e) {
      console.error(`[PluginLoader] Error clearing events for ${pluginId}:`, e)
    }

    // 清理注册表中的 context 和 UI 注册
    try {
      getPluginRegistry().clearPlugin(pluginId)
    } catch (e) {
      console.error(`[PluginLoader] Error clearing registry for ${pluginId}:`, e)
    }

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
      // 关键：先注册 context 再激活 —— activate 内 registerToolboxPage 等会立即更新响应式
      // 注册表，宿主可能随之渲染插件组件（如 ToolboxView 入口卡），必须保证
      // PluginViewHost provide 能取到 context，否则插件组件 inject 得到 undefined 崩溃
      getPluginRegistry().setContext(manifest.id, context)
      await this.activateWithTimeout(module, context)

      this.plugins.set(manifest.id, { manifest, module, context })
      console.log(`[PluginLoader] Plugin frontend loaded: ${manifest.id}`)
    } catch (e: any) {
      console.error(`[PluginLoader] Failed to load frontend for ${manifest.id}:`, e)
      // 激活失败：摘除激活期间可能残留的注册（含动态路由），避免半激活状态
      getPluginRegistry().clearPlugin(manifest.id)
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
