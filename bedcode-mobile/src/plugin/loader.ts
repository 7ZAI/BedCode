/**
 * Plugin Loader
 *
 * 加载、激活、停用插件的 TS/Vue 前端模块
 * 插件代码编译进主 bundle，通过 import() 从 http://tauri.localhost/ 加载
 */

import type { PluginInfo, PluginModule, PluginContext } from './types'
import { nextTick } from 'vue'
import { logger } from '@/utils/frontendLogger'
import * as pluginCmds from './commands'
import { toPluginAssetUrl } from './assets'
import { createPluginContext } from './context'
import { clearPluginEvents } from './events'
import { getPluginRegistry } from './registry'
import { disposeAppRunScope, loadPluginModuleScoped } from './app-run-scope'

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

  /**
   * 应用启动时只预载「启动时自动激活」应用的前端模块
   *
   * 移动端是应用模型：启动只扫描已安装 wasm-app，不启动任何应用。只有显式
   * 打开自启动开关（`plugin.autostart.{id}`，默认关）的应用才会在 boot 预载，
   * 其余应用由用户在宿主壳里点击后经 `activate()` 加载（后端激活 + 前端模块）。
   */
  async loadAll(): Promise<void> {
    const manifests = await pluginCmds.pluginListLoaded()
    logger.log(`[PluginLoader] Found ${manifests.length} plugin(s)`)
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
        logger.log(`[PluginLoader] Scan retry: found ${manifests.length} plugin(s)`)
        if (manifests.length > 0) {
          await this.loadManifests(manifests)
          return
        }
      } catch (e) {
        // 后端尚未就绪时命令异常：继续轮询，直到超时
        logger.warn('[PluginLoader] Scan retry query failed, will retry:', e)
      }
      if (Date.now() - this.scanRetryStartedAt < STARTUP_SCAN_TIMEOUT_MS) {
        this.scanRetryTimer = setTimeout(tick, STARTUP_SCAN_POLL_MS)
      } else {
        logger.warn('[PluginLoader] Plugin scan not ready after timeout; entries need manual re-toggle')
      }
    }
    this.scanRetryTimer = setTimeout(tick, STARTUP_SCAN_POLL_MS)
  }

  /** 逐插件加载前端模块（幂等：重试/重复调用时跳过已加载插件） */
  private async loadManifests(manifests: PluginInfo[]): Promise<void> {
    for (const manifest of manifests) {
      // Rust-only 插件：前端无需加载
      if (manifest.pluginType === 'rust') {
        logger.log(`[PluginLoader] Rust plugin ${manifest.id} managed by backend`)
        continue
      }

      if (this.plugins.has(manifest.id)) continue

      // 门禁取「启动时自动激活」偏好（`plugin.autostart.{id}`，缺省关），
      // 而非插件运行时状态、也不是启用偏好：
      // - 不用运行时状态：后端 boot 自动激活是异步任务，前端首次查
      //   `plugin_list_loaded` 可能拿到尚未完成的 Loaded 态，据此跳过会
      //   让本应自启的应用漏载；偏好是持久化存储，查询时立即可用。
      // - 不用启用偏好（`plugin.enabled.{id}`）：启用只表示「应用可用」，
      //   与 boot 是否启动解耦（持久化 ≠ 自启动）。默认一个应用都不自启，
      //   前端自然也不预载，避免「前端已加载但后端未运行」的悬空状态
      //   （loader.activate 会因 plugins Map 已有记录而短路后端激活）。
      const isAutostart = await pluginCmds.pluginIsAutostart(manifest.id)
      if (!isAutostart) {
        logger.log(`[PluginLoader] Plugin ${manifest.id} not autostart, skipping frontend load`)
        continue
      }

      // 按运行时状态补充门禁（意图优先，状态辅助）：
      // - activating 中间态：后端仍在激活进行中，跳过本次前端加载，等最终态轮询兜底
      // - degraded：后端实例在运行、命令可用，UI 入口照常挂载，仅标注降级原因
      // 状态值取线上 camelCase（真源 = 共享 crate 的 PluginState serde 属性）
      const st = manifest.state
      if (st.state === 'activating') {
        logger.log(`[PluginLoader] Plugin ${manifest.id} activating, deferring frontend load`)
        continue
      }
      if (st.state === 'degraded') {
        logger.warn(`[PluginLoader] Plugin ${manifest.id} loaded but degraded: ${st.error}`)
      }

      await this.loadFrontend(manifest)
    }
  }

  /**
   * 激活指定插件
   *
   * 后端激活与前端加载分步收口：后端激活失败仅标记错误（无实例存活可拆）；
   * 前端加载失败的对称拆解（clearPlugin + 后端停用 + markError）由 loadFrontend
   * 内部完成，此处不重复处理
   */
  async activate(pluginId: string): Promise<void> {
    if (this.plugins.has(pluginId)) return

    const info = await pluginCmds.pluginGetInfo(pluginId)
    if (!info) {
      logger.error(`[PluginLoader] Plugin ${pluginId} not found`)
      return
    }

    try {
      await pluginCmds.pluginActivate(pluginId)
    } catch (e: any) {
      logger.error(`[PluginLoader] Failed to activate ${pluginId}:`, e)
      await pluginCmds.pluginMarkError(pluginId, e.message || 'Activation failed')
      return
    }
    await this.loadFrontend(info)
  }

  /**
   * 停用插件
   *
   * 对称拆解（幂等）：即使前端模块从未加载成功（plugins Map 无记录），后端
   * WASM 实例也可能因 plugin_activate 成功而存活（持有 mDNS browse 句柄等），
   * 必须通知后端停用，保证「前端注册表为空 ⇔ 后端非存活」。前端侧各步存在则
   * 执行、缺失则跳过；重复调用无副作用（后端对未激活/已停用状态幂等早退）。
   *
   * 除资源回收外还作废运行期 store 作用域（disposeAppRunScope）：停止应用 =
   * store 停止运行，再次 activate 是全新 store；模块在内不在内都执行（无作用域时
   * 为 no-op）。
   */
  async deactivate(pluginId: string): Promise<void> {
    const plugin = this.plugins.get(pluginId)

    if (plugin) {
      // 清理所有 Disposable
      plugin.context._disposables.forEach((d: { dispose(): void }) => {
        try { d.dispose() } catch (e) {
          logger.error(`[PluginLoader] Error disposing resource for ${pluginId}:`, e)
        }
      })

      // 调用插件的 deactivate（先于注册表清理：若模块停用期间再注册，随后即被清掉）
      if (plugin.module.deactivate) {
        try { await plugin.module.deactivate() } catch (e) {
          logger.error(`[PluginLoader] Error in deactivate for ${pluginId}:`, e)
        }
      }

      this.plugins.delete(pluginId)
    }

    // 清理事件监听与注册表：前端模块未加载时也执行（激活期失败残留兜底）。
    // 兜底清理失败不中断后续流程，避免注册表残留
    try {
      clearPluginEvents(pluginId)
    } catch (e) {
      logger.error(`[PluginLoader] Error clearing events for ${pluginId}:`, e)
    }

    try {
      getPluginRegistry().clearPlugin(pluginId)
    } catch (e) {
      logger.error(`[PluginLoader] Error clearing registry for ${pluginId}:`, e)
    }

    // 通知后端（无论前端模块是否加载过）
    try {
      await pluginCmds.pluginDeactivate(pluginId)
    } catch (e) {
      logger.error(`[PluginLoader] Error notifying backend for deactivation of ${pluginId}:`, e)
    }

    // 运行期 store 作用域作废：等一次 Vue flush 让运行面组件随注册表清理先卸载，
    // 再 $dispose 本次运行期的全部 store——组件仍持有 store 时提前作废会把半死的
    // store 留在渲染路径上（后端停用是 IPC 往返，回来时 flush 通常已跑完，此处兜底）。
    // 作用域丢弃后，再次激活 useStore() 会新建 store（回到初始 state）
    await nextTick()
    disposeAppRunScope(pluginId)

    logger.log(`[PluginLoader] Plugin deactivated: ${pluginId}`)
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
      //
      // 装载走运行期作用域的「装载窗口」：窗口内插件顶层 defineStore 绑定到本插件，
      // 之后每次 useStore 解析本插件当前运行期的 Pinia（停止即作废，见 app-run-scope.ts）。
      // 窗口是全局标记，内部已串行化——并发装载不会让两个插件的 store 串绑
      const module = await loadPluginModuleScoped(manifest.id, () =>
        this.importWithTimeout(toPluginAssetUrl(manifest.extensionPath, manifest.main)),
      )

      const context = createPluginContext(manifest)
      // 关键：先注册 context 再激活 —— activate 内 registerSurface / registerSlot 等会立即
      // 更新壳注册表的响应式状态，宿主可能随之渲染插件组件，必须保证
      // PluginViewHost provide 能取到 context，否则插件组件 inject 得到 undefined 崩溃
      getPluginRegistry().setContext(manifest.id, context)
      await this.activateWithTimeout(module, context)

      this.plugins.set(manifest.id, { manifest, module, context })
      logger.log(`[PluginLoader] Plugin frontend loaded: ${manifest.id}`)
    } catch (e: any) {
      logger.error(`[PluginLoader] Failed to load frontend for ${manifest.id}:`, e)
      // 激活失败：摘除激活期间可能残留的注册（含动态路由），避免半激活状态
      getPluginRegistry().clearPlugin(manifest.id)
      // 装载失败可能已建过运行期作用域（模块求值到一半抛错）：一并作废，不留半截状态
      disposeAppRunScope(manifest.id)
      // 对称拆解后端：plugin_activate 可能已成功（WASM 实例存活，状态
      // Activated/Degraded），必须先于 markError 停用——后端 deactivate 仅对
      // Activated/Degraded 生效，markError 置 Error 后实例将无法再拆解。
      // 保证「前端注册表为空 ⇔ 后端非存活」，重启用从干净状态重启（不命中幂等早退）
      try {
        await pluginCmds.pluginDeactivate(manifest.id)
      } catch (deactivateErr) {
        logger.error(`[PluginLoader] Error tearing down backend for ${manifest.id}:`, deactivateErr)
      }
      await pluginCmds.pluginMarkError(manifest.id, e.message || 'Frontend load failed')
    }
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
