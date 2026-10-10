/**
 * Dev Shell 应用视图模型（与宿主 shell/composables/useShellApps.ts 同构）
 * -----------------------------------------------------------------------------
 * 把注册表里的存储模型投影成 UI 需要的形状（排序、统计、派生列表），并把
 * 「启动 / 停止 / 改权限」收口到一处——组件只调动作、不直接碰数据源。
 *
 * dev-shell 与宿主同构的价值：插件在 dev-shell 里看到的启动失败、状态回写、
 * 空态文案都与真机一致，不会出现「浏览器里好好的、装到手机上就白屏」。
 */

import { computed, ref, type Component, type ComputedRef, type Ref } from 'vue'
import { logger } from '../logger'
import { getShellRegistry } from '../registry'
import type { ShellApp, ShellAppSource } from '../types'
import { pushRecent } from './useShellRecent'

/** 平台统计（应用管理页顶部） */
export interface ShellStats {
  /** 已安装应用数 */
  installed: number
  /** 运行中应用数 */
  running: number
  /** 应用数据总占用（字节）；部分应用未统计时为 undefined，UI 显示「—」而非伪造 0 */
  totalBytes: number | undefined
}

const registry = getShellRegistry()

/** 是否正在拉取清单 */
const loading: Ref<boolean> = ref(false)
/** 最近一次动作的错误原因（供 UI 展示；成功清空） */
const lastError: Ref<string | null> = ref(null)

/** 应用清单（保持注册/写入顺序） */
const apps: ComputedRef<ShellApp[]> = computed(() => registry.appsRef.value)

/** 展示排序：运行中优先，其次按名称（中文用 localeCompare，避免拼音序错乱） */
const sortedApps: ComputedRef<ShellApp[]> = computed(() =>
  [...apps.value].sort((a, b) => {
    const aRunning = a.state === 'running' ? 0 : 1
    const bRunning = b.state === 'running' ? 0 : 1
    if (aRunning !== bRunning) return aRunning - bRunning
    return a.name.localeCompare(b.name, 'zh-Hans-CN')
  }),
)

/** 运行中应用（多任务页消费） */
const runningApps: ComputedRef<ShellApp[]> = computed(() =>
  apps.value.filter((a) => a.state === 'running'),
)

/** 平台统计 */
const stats: ComputedRef<ShellStats> = computed(() => {
  const measured = apps.value.every((a) => typeof a.sizeBytes === 'number')
  return {
    installed: apps.value.length,
    running: apps.value.filter((a) => a.state === 'running').length,
    totalBytes: measured
      ? apps.value.reduce((sum, a) => sum + (a.sizeBytes ?? 0), 0)
      : undefined,
  }
})

/** 按 id 取应用（含合并后的贡献） */
function getApp(appId: string): ShellApp | undefined {
  return registry.getApp(appId)
}

/** 从所有数据源重拉应用清单 */
async function refresh(): Promise<void> {
  loading.value = true
  try {
    const sources = registry.listSources()
    if (sources.length === 0) {
      logger.warn('no app source registered; app list stays empty')
      return
    }
    // 各数据源独立失败不影响其它源：一个源挂掉不该让整个首页空白
    await Promise.all(
      sources.map(async (source) => {
        try {
          const list = await source.list()
          registry.upsertApps(source.id, list)
        } catch (e) {
          logger.error(`app source list failed: source=${source.id} ${String(e)}`)
          lastError.value = e instanceof Error ? e.message : String(e)
        }
      }),
    )
  } finally {
    loading.value = false
  }
}

/** 启动应用（幂等：已运行只记一次最近使用） */
async function launch(appId: string): Promise<boolean> {
  const app = registry.getApp(appId)
  const source = registry.getSourceOf(appId)
  if (!app || !source) {
    logger.error(`launch failed: unknown app appId=${appId}`)
    lastError.value = `unknown app: ${appId}`
    return false
  }
  if (app.state === 'running') {
    pushRecent(appId)
    return true
  }

  // 乐观置为运行中：启动是秒级操作，先动 UI 避免点击后无反馈
  registry.patchApp(appId, { state: 'running', error: undefined })
  try {
    await source.launch(appId)
    pushRecent(appId)
    lastError.value = null
    await refresh()
    return true
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e)
    logger.error(`launch failed: appId=${appId} ${message}`)
    registry.patchApp(appId, { state: 'error', error: message })
    lastError.value = message
    return false
  }
}

/** 停止应用（保留数据，回收内存与后台任务） */
async function stop(appId: string): Promise<boolean> {
  const app = registry.getApp(appId)
  const source = registry.getSourceOf(appId)
  if (!app || !source) {
    logger.error(`stop failed: unknown app appId=${appId}`)
    return false
  }

  registry.patchApp(appId, { state: 'stopped', error: undefined })
  try {
    await source.stop(appId)
    lastError.value = null
    await refresh()
    return true
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e)
    logger.error(`stop failed: appId=${appId} ${message}`)
    registry.patchApp(appId, { state: 'error', error: message })
    return false
  }
}

/**
 * 逐项变更权限授予
 *
 * @returns true 已生效；false 数据源不支持或调用失败——UI 据此保留原状并提示，
 *          不做「点了就算通过」的假成功。
 */
async function setPermissionGrant(
  appId: string,
  permissionKey: string,
  granted: boolean,
): Promise<boolean> {
  const source = registry.getSourceOf(appId)
  if (!source?.setPermissionGrant) {
    logger.warn(`permission control unsupported: appId=${appId} permission=${permissionKey}`)
    return false
  }

  const app = registry.getApp(appId)
  if (!app) return false

  try {
    const ok = await source.setPermissionGrant(appId, permissionKey, granted)
    if (!ok) return false
    registry.patchApp(appId, {
      permissions: app.permissions.map((p) =>
        p.key === permissionKey ? { ...p, granted } : p,
      ),
    })
    return true
  } catch (e) {
    logger.error(`set permission failed: appId=${appId} permission=${permissionKey} ${String(e)}`)
    return false
  }
}

/** 该应用的权限是否可逐项控制（取决于数据源能力，不是 UI 拍脑袋） */
function supportsPermissionControl(appId: string): boolean {
  return typeof registry.getSourceOf(appId)?.setPermissionGrant === 'function'
}

/**
 * 解析应用运行面组件
 *
 * 优先级：应用注册面 → 数据源延迟解析。两级都没有时返回 undefined，由运行屏渲染
 * 「该应用尚未提供运行面」空态——空态要写明原因，不能是空白页。
 *
 * 与宿主同口径：不做「找不到就回退到另一种形态」的链式查找——那会让应用只注册
 * 一个内嵌片段就被壳当成整个应用的主面，与真机行为不一致。
 */
function resolveSurface(appId: string): Component | undefined {
  const app = registry.getApp(appId)
  if (!app) return undefined
  return app.contributions.surface?.component ?? registry.getSourceOf(appId)?.resolveSurface?.(appId)
}

/** 应用自带主题色（仅作用于应用内部） */
function surfaceAccent(appId: string): string | undefined {
  return registry.getApp(appId)?.contributions.surface?.accent
}

/** 是否有数据源支持安装本地包（「添加应用」区按能力渲染，不提供点了没反应的按钮） */
function supportsInstall(): boolean {
  return registry.listSources().some((s) => typeof s.installFromLocalPackage === 'function')
}

/** 安装本地应用包；返回 false 表示用户取消或安装失败（错误已由 refresh/lastError 暴露） */
async function installFromLocalPackage(): Promise<boolean> {
  const source = registry
    .listSources()
    .find((s): s is ShellAppSource & Required<Pick<ShellAppSource, 'installFromLocalPackage'>> =>
      typeof s.installFromLocalPackage === 'function',
    )
  if (!source) {
    logger.warn('install unsupported by every registered source')
    return false
  }
  try {
    const ok = await source.installFromLocalPackage()
    if (ok) await refresh()
    return ok
  } catch (e) {
    logger.error(`install local package failed ${String(e)}`)
    lastError.value = e instanceof Error ? e.message : String(e)
    return false
  }
}

/** 是否可卸载应用（内置应用通常不可） */
function supportsRemove(appId: string): boolean {
  return typeof registry.getSourceOf(appId)?.remove === 'function'
}

/** 卸载应用并删除数据 */
async function removeApp(appId: string): Promise<boolean> {
  const source = registry.getSourceOf(appId)
  if (!source?.remove) {
    logger.warn(`remove unsupported: appId=${appId}`)
    return false
  }
  try {
    const ok = await source.remove(appId)
    if (ok) {
      registry.clearApp(appId)
      await refresh()
    }
    return ok
  } catch (e) {
    logger.error(`remove app failed: appId=${appId} ${String(e)}`)
    lastError.value = e instanceof Error ? e.message : String(e)
    return false
  }
}

/** 宿主壳应用视图模型 */
export interface ShellApps {
  apps: ComputedRef<ShellApp[]>
  sortedApps: ComputedRef<ShellApp[]>
  runningApps: ComputedRef<ShellApp[]>
  stats: ComputedRef<ShellStats>
  loading: ComputedRef<boolean>
  lastError: ComputedRef<string | null>
  getApp: typeof getApp
  refresh: typeof refresh
  launch: typeof launch
  stop: typeof stop
  setPermissionGrant: typeof setPermissionGrant
  supportsPermissionControl: typeof supportsPermissionControl
  resolveSurface: typeof resolveSurface
  surfaceAccent: typeof surfaceAccent
  supportsInstall: typeof supportsInstall
  installFromLocalPackage: typeof installFromLocalPackage
  supportsRemove: typeof supportsRemove
  removeApp: typeof removeApp
}

/** 获取宿主壳应用视图模型（单例） */
export function useShellApps(): ShellApps {
  return {
    apps,
    sortedApps,
    runningApps,
    stats,
    loading: computed(() => loading.value),
    lastError: computed(() => lastError.value),
    getApp,
    refresh,
    launch,
    stop,
    setPermissionGrant,
    supportsPermissionControl,
    resolveSurface,
    surfaceAccent,
    supportsInstall,
    installFromLocalPackage,
    supportsRemove,
    removeApp,
  }
}