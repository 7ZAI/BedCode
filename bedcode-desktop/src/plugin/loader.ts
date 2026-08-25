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
import { getPluginRegistry } from './registry'

/** 已激活的插件实例 */
interface ActivePlugin {
  manifest: PluginInfo
  module: PluginModule
  context: PluginContext
}

/** 插件加载器 */
class PluginLoaderClass {
  private plugins: Map<string, ActivePlugin> = new Map()

  /** 加载所有插件（应用启动时调用）
   *
   * 根据 Rust 后端返回的插件状态决定前端加载策略：
   * - Rust 端已 Activated 的插件：加载前端 TS 模块（UI 组件注册）
   * - Degraded 插件：同样加载前端模块 —— 后端实例在运行、phase 3 扩展点已注册，
   *   UI 入口应可见；仅 console.warn 标注降级原因（spec §3.6）
   * - 其余状态（含 Activating 中间态）：跳过，等待用户手动激活
   * - Rust-only 插件：完全由后端管理，前端无需处理
   */
  async loadAll(): Promise<void> {
    console.log('[PluginLoader] loadAll() started')
    const manifests = await pluginCmds.pluginListLoaded()
    console.log(`[PluginLoader] Found ${manifests.length} plugin(s) from backend`)

    for (const manifest of manifests) {
      console.log(
        `[PluginLoader] Processing plugin: ${manifest.id} (type=${manifest.pluginType}, state=${manifest.state.state}, sandbox=${manifest.sandbox})`,
      )

      // Rust-only 插件：Rust 端已通过静态注册激活，前端无需加载
      if (manifest.pluginType === 'rust') {
        console.log(`[PluginLoader] Rust plugin ${manifest.id} managed by backend, skipping`)
        continue
      }

      if (manifest.sandbox !== 'inline') {
        console.warn(
          `[PluginLoader] Skipping ${manifest.id}: unsupported sandbox mode "${manifest.sandbox}"`,
        )
        continue
      }

      // 加载门禁：Activated 正常加载；Degraded 也放行（实例在运行，仅降级）；
      // 其余状态跳过。功能门禁归类依据 spec §5.1 开放问题裁决：先放行 + UI 降级标识
      const stateName = manifest.state.state

      if (stateName !== 'Activated' && stateName !== 'Degraded') {
        console.log(
          `[PluginLoader] Plugin ${manifest.id} not activated (state: ${stateName}), skipping frontend load`,
        )
        continue
      }

      if (stateName === 'Degraded') {
        console.warn(
          `[PluginLoader] Plugin ${manifest.id} is DEGRADED, loading frontend module anyway. Reason: ${manifest.state.error}`,
        )
      }

      // Rust+TS 插件：Rust 端已激活，前端只加载 TS 入口文件（UI 组件）
      if (manifest.pluginType === 'rust-ts') {
        console.log(`[PluginLoader] Loading Rust+TS plugin frontend: ${manifest.id}`)
        await this.loadFrontendOnly(manifest)
      } else {
        // TS-only 插件：Rust 端已激活（自动激活），前端加载入口但不重复调用 pluginActivate
        console.log(
          `[PluginLoader] Loading TS-only plugin frontend (already activated): ${manifest.id}`,
        )
        await this.loadFrontendForAlreadyActivated(manifest)
      }
    }

    console.log(
      `[PluginLoader] loadAll() complete, ${this.plugins.size} plugin(s) with frontend modules loaded`,
    )
  }

  /** 加载 Rust+TS 插件的前端部分（不触发后端 activate，Rust 端已激活） */
  private async loadFrontendOnly(manifest: PluginInfo): Promise<void> {
    const ACTIVATE_TIMEOUT = 5000
    // 失败上报需标注发生在哪一步（issue 04：导入/激活/失败三条路径各上报一次）
    let stage: 'import' | 'activate' = 'import'

    try {
      // 不调用 pluginActivate — Rust 端已通过静态注册激活
      const entryUrl = this.convertFileUrl(manifest.extensionPath, manifest.main)
      console.log(`[PluginLoader] Importing frontend module: ${entryUrl}`)
      const module = await this.importWithTimeout(entryUrl, ACTIVATE_TIMEOUT)
      console.log(`[PluginLoader] Frontend module imported: ${manifest.id}`)
      await this.reportLoadDiagnostic(manifest.id, 'import', true)

      stage = 'activate'
      const context = createPluginContext(manifest)
      await this.activateWithTimeout(module, context, ACTIVATE_TIMEOUT)
      console.log(`[PluginLoader] Frontend activate() called: ${manifest.id}`)
      await this.reportLoadDiagnostic(manifest.id, 'activate', true)

      this.plugins.set(manifest.id, { manifest, module, context })
      // 将 context 存入 registry，供 PluginViewHost provide 给组件树
      getPluginRegistry().setContext(manifest.id, context)
      console.log(`[PluginLoader] Rust+TS plugin frontend loaded: ${manifest.id}`)
    } catch (e: any) {
      console.error(`[PluginLoader] Failed to load frontend for ${manifest.id}:`, e)
      await this.reportLoadDiagnostic(
        manifest.id,
        stage,
        false,
        e.message || 'Frontend load failed',
      )
      await pluginCmds.pluginMarkError(manifest.id, e.message || 'Frontend load failed')
    }
  }

  /** 加载 TS-only 插件的前端模块（Rust 端已激活，跳过 pluginActivate 调用）
   *
   * 用于启动时 Rust 端已根据持久化状态自动激活的 TS-only 插件，
   * 避免重复调用 pluginActivate
   */
  private async loadFrontendForAlreadyActivated(manifest: PluginInfo): Promise<void> {
    const ACTIVATE_TIMEOUT = 5000
    // 失败上报需标注发生在哪一步（issue 04：导入/激活/失败三条路径各上报一次）
    let stage: 'import' | 'activate' = 'import'

    try {
      // 跳过 pluginActivate — Rust 端已通过自动激活处理
      const entryUrl = this.convertFileUrl(manifest.extensionPath, manifest.main)
      console.log(`[PluginLoader] Importing frontend module (already activated): ${entryUrl}`)
      const module = await this.importWithTimeout(entryUrl, ACTIVATE_TIMEOUT)
      console.log(`[PluginLoader] Frontend module imported: ${manifest.id}`)
      await this.reportLoadDiagnostic(manifest.id, 'import', true)

      stage = 'activate'
      const context = createPluginContext(manifest)
      await this.activateWithTimeout(module, context, ACTIVATE_TIMEOUT)
      console.log(`[PluginLoader] Frontend activate() called: ${manifest.id}`)
      await this.reportLoadDiagnostic(manifest.id, 'activate', true)

      this.plugins.set(manifest.id, { manifest, module, context })
      getPluginRegistry().setContext(manifest.id, context)
      console.log(`[PluginLoader] Plugin frontend loaded (already activated): ${manifest.id}`)
    } catch (e: any) {
      console.error(`[PluginLoader] Failed to load frontend for ${manifest.id}:`, e)
      await this.reportLoadDiagnostic(
        manifest.id,
        stage,
        false,
        e.message || 'Frontend load failed',
      )
      await pluginCmds.pluginMarkError(manifest.id, e.message || 'Frontend load failed')
    }
  }

  /** 按需激活插件 */
  async activate(pluginId: string): Promise<void> {
    console.log(`[PluginLoader] activate(${pluginId}) called`)

    if (this.plugins.has(pluginId)) {
      console.log(`[PluginLoader] Plugin ${pluginId} already has frontend module loaded, skipping`)
      return
    }

    const info = await pluginCmds.pluginGetInfo(pluginId)
    if (!info) {
      console.error(`[PluginLoader] Plugin ${pluginId} not found in backend`)
      return
    }

    console.log(
      `[PluginLoader] Plugin ${pluginId} info: type=${info.pluginType}, state=${info.state.state}`,
    )
    await this.loadInline(info)
  }

  /** 停用插件 */
  async deactivate(pluginId: string): Promise<void> {
    console.log(`[PluginLoader] deactivate(${pluginId}) called`)

    const plugin = this.plugins.get(pluginId)
    if (!plugin) {
      console.warn(
        `[PluginLoader] Plugin ${pluginId} has no frontend module loaded, nothing to deactivate`,
      )
      return
    }

    // 清理所有 Disposable
    plugin.context._disposables.forEach((d) => {
      try {
        d.dispose()
      } catch (e) {
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
      try {
        await plugin.module.deactivate()
        console.log(`[PluginLoader] Plugin deactivate() called: ${pluginId}`)
      } catch (e) {
        console.error(`[PluginLoader] Error in deactivate for ${pluginId}:`, e)
      }
    }

    // 通知后端
    try {
      await pluginCmds.pluginDeactivate(pluginId)
      console.log(`[PluginLoader] Backend notified of deactivation: ${pluginId}`)
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

  /** 热重载插件（开发模式）
   *
   * 停用旧插件 → 重新加载 TS 入口（带缓存破坏）→ 重新激活。
   * Rust 端热重载由 PluginHost::reload_wasm_plugin() 处理，
   * 此方法只负责前端 TS 模块的重载。
   */
  async reloadPlugin(pluginId: string): Promise<void> {
    const plugin = this.plugins.get(pluginId)
    const ACTIVATE_TIMEOUT = 5000

    // 1. 停用旧插件（清理 disposables、事件、注册表、调用 deactivate）。
    // 清理顺序契约：先清注册表/事件，再调 deactivate——因此插件的
    // deactivate 不得依赖 registry/context（此时已不可用），注册表清理是
    // 兜底语义（deactivate 挂起也保证注册表干净）。错误均记录不中断，
    // 热重载失败时残留痕迹可查
    if (plugin) {
      plugin.context._disposables.forEach((d) => {
        try {
          d.dispose()
        } catch {
          /* ignore */
        }
      })
      try {
        clearPluginEvents(pluginId)
      } catch (e) {
        console.error(`[PluginLoader] Error clearing events for ${pluginId}:`, e)
      }
      try {
        getPluginRegistry().clearPlugin(pluginId)
      } catch (e) {
        console.error(`[PluginLoader] Error clearing registry for ${pluginId}:`, e)
      }

      if (plugin.module.deactivate) {
        try {
          await plugin.module.deactivate()
        } catch (e) {
          console.error(`[PluginLoader] Error in deactivate for ${pluginId}:`, e)
        }
      }
      this.plugins.delete(pluginId)
    }

    // 2. 获取最新插件信息
    const info = await pluginCmds.pluginGetInfo(pluginId)
    if (!info) {
      console.error(`[PluginLoader] Cannot reload: plugin ${pluginId} not found`)
      return
    }

    // 3. 重新加载 TS 入口（添加时间戳破坏浏览器缓存）
    const entryUrl = this.convertFileUrl(info.extensionPath, info.main) + '?t=' + Date.now()
    let stage: 'import' | 'activate' = 'import'

    try {
      const module = await this.importWithTimeout(entryUrl, ACTIVATE_TIMEOUT)
      await this.reportLoadDiagnostic(pluginId, 'import', true)

      stage = 'activate'
      const context = createPluginContext(info)
      await this.activateWithTimeout(module, context, ACTIVATE_TIMEOUT)
      await this.reportLoadDiagnostic(pluginId, 'activate', true)

      this.plugins.set(pluginId, { manifest: info, module, context })
      getPluginRegistry().setContext(pluginId, context)
      console.log(`[PluginLoader] Plugin hot-reloaded: ${pluginId}`)
    } catch (e: any) {
      console.error(`[PluginLoader] Failed to hot-reload ${pluginId}:`, e)
      await this.reportLoadDiagnostic(
        pluginId,
        stage,
        false,
        e.message || 'Hot reload failed',
      )
      await pluginCmds.pluginMarkError(pluginId, e.message || 'Hot reload failed')
    }
  }

  /** 加载 inline 模式插件 */
  private async loadInline(manifest: PluginInfo): Promise<void> {
    const ACTIVATE_TIMEOUT = 5000
    // 失败上报需标注发生在哪一步（issue 04：导入/激活/失败三条路径各上报一次）
    let stage: 'import' | 'activate' = 'import'

    try {
      // 通知后端标记激活
      console.log(`[PluginLoader] Calling backend pluginActivate for ${manifest.id}`)
      await pluginCmds.pluginActivate(manifest.id)
      console.log(`[PluginLoader] Backend pluginActivate succeeded for ${manifest.id}`)

      // 动态导入插件入口文件
      const entryUrl = this.convertFileUrl(manifest.extensionPath, manifest.main)
      console.log(`[PluginLoader] Importing frontend module: ${entryUrl}`)
      const module = await this.importWithTimeout(entryUrl, ACTIVATE_TIMEOUT)
      console.log(`[PluginLoader] Frontend module imported: ${manifest.id}`)
      await this.reportLoadDiagnostic(manifest.id, 'import', true)

      stage = 'activate'
      // 创建 PluginContext
      const context = createPluginContext(manifest)

      // 调用 activate
      await this.activateWithTimeout(module, context, ACTIVATE_TIMEOUT)
      console.log(`[PluginLoader] Frontend activate() called: ${manifest.id}`)
      await this.reportLoadDiagnostic(manifest.id, 'activate', true)

      this.plugins.set(manifest.id, { manifest, module, context })
      // 将 context 存入 registry，供 PluginViewHost provide 给组件树
      getPluginRegistry().setContext(manifest.id, context)
      console.log(`[PluginLoader] Plugin activated: ${manifest.id}`)
    } catch (e: any) {
      console.error(`[PluginLoader] Failed to activate ${manifest.id}:`, e)
      await this.reportLoadDiagnostic(
        manifest.id,
        stage,
        false,
        e.message || 'Activation failed',
      )
      await pluginCmds.pluginMarkError(manifest.id, e.message || 'Activation failed')
    }
  }

  /** 将插件路径转换为可导入的 URL（通过 Tauri asset protocol） */
  private convertFileUrl(extensionPath: string, main: string): string {
    const filePath = `${extensionPath}/${main}`.replace(/\\/g, '/')
    return convertFileSrc(filePath)
  }

  /** 上报前端模块加载诊断到宿主落盘日志（spec §3.7 / issue 04）
   *
   * 宿主内部诊断通道：结果仅写入 tracing（runtime.*.log），不入状态机。
   * invoke 失败静默吞掉 —— 诊断命令不可用时绝不阻塞插件加载流程。
   */
  private async reportLoadDiagnostic(
    pluginId: string,
    stage: 'import' | 'activate',
    ok: boolean,
    detail?: string,
  ): Promise<void> {
    try {
      await pluginCmds.pluginFrontendLoadReport(pluginId, stage, ok, detail)
    } catch {
      // 诊断通道不可用不影响加载流程
    }
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
