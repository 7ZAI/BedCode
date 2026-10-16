/**
 * 设备壳应用注册表投影 + 生命周期动作（模块级单例，壳内唯一事实源）
 *
 * 数据源全部走既有宿主通道：
 * - 清单：`plugin_list_loaded`（PluginInfo，manifest 权限 / 贡献 / 来源 / 体积）
 * - 启停：`pluginLoader.activate/deactivate`（启用前先 preauthorize，与 usePluginManager 同口径）
 * - 审批：`pluginApprove`（整单裁决，批准集由 Rust 端计算）
 * - 卸载：`pluginUninstall`
 *
 * 归属（AGENTS §5.1）：这里只有「注册表投影 + 生命周期动作的机械转发」，无业务编排。
 */
import { computed, ref } from 'vue'
import {
  pluginApprove,
  pluginListLoaded,
  pluginPreauthorize,
  pluginUninstall,
} from '@/plugin/commands'
import { pluginLoader } from '@/plugin/loader'
import { logger } from '@/utils/frontendLogger'
import { IPC_TIMEOUT_CODE, showUserError } from '@/utils/userError'
import type { PluginInfo } from '@/plugin/types'
import {
  HIGH_RISK_PERMISSIONS,
  appDisplayState,
  isRunningState,
  toneOf,
  type AppTone,
} from './state'
import type { AppDisplayState } from './types'

/** 启停操作总超时（后端激活/停用含 hooks 清理与授权弹窗，给足余量；与 usePluginManager 同源） */
const TOGGLE_TIMEOUT_MS = 30000

const plugins = ref<PluginInfo[]>([])
const loading = ref(false)
/** 正在切换启停 / 审批 / 卸载的应用 id（防重复点击 + 遮罩态） */
const busyId = ref<string | null>(null)
const busyKind = ref<'activate' | 'deactivate' | 'approve' | 'uninstall' | null>(null)
/** 应用管理页当前选中的应用 id（权限页「管理」跳转共用；模块级保证跨系统页同步） */
const detailAppId = ref<string | null>(null)

function appOf(pluginId: string): PluginInfo | undefined {
  return plugins.value.find((p) => p.id === pluginId)
}

/** 加载应用清单（幂等；壳启动与启停/审批/卸载成功后刷新） */
async function load(): Promise<void> {
  if (loading.value) return
  loading.value = true
  try {
    // 确保前端插件运行时就绪（main.ts 已触发 ensureLoaded，这里幂等等待）
    await pluginLoader.ensureLoaded()
    plugins.value = await pluginListLoaded()
  } catch (e: any) {
    logger.error('[ShellApps] load failed:', e)
    showUserError(e)
  } finally {
    loading.value = false
  }
}

/** 带超时的启停（与 usePluginManager 同口径：兜底后端挂起不无限转圈） */
async function runWithTimeout<T>(op: Promise<T>): Promise<T> {
  return Promise.race([
    op,
    new Promise<never>((_, reject) =>
      setTimeout(
        () => reject({ code: IPC_TIMEOUT_CODE, params: { seconds: TOGGLE_TIMEOUT_MS / 1000 } }),
        TOGGLE_TIMEOUT_MS,
      ),
    ),
  ])
}

/** 启用（含 preauthorize 授权前置；needsApproval 应用须先走 approveApp） */
async function activate(pluginId: string): Promise<boolean> {
  const app = appOf(pluginId)
  if (!app || busyId.value) return false
  if (appDisplayState(app.state) === 'approval') return false
  busyId.value = pluginId
  busyKind.value = 'activate'
  try {
    await pluginPreauthorize(pluginId)
    await runWithTimeout(pluginLoader.activate(pluginId))
    await load()
    return true
  } catch (e: any) {
    logger.error(`[ShellApps] activate(${pluginId}) failed:`, e)
    showUserError(e, { retry: () => activate(pluginId) })
    return false
  } finally {
    busyId.value = null
    busyKind.value = null
  }
}

/** 停用 */
async function deactivate(pluginId: string): Promise<boolean> {
  const app = appOf(pluginId)
  if (!app || busyId.value) return false
  busyId.value = pluginId
  busyKind.value = 'deactivate'
  try {
    await runWithTimeout(pluginLoader.deactivate(pluginId))
    await load()
    return true
  } catch (e: any) {
    logger.error(`[ShellApps] deactivate(${pluginId}) failed:`, e)
    showUserError(e, { retry: () => deactivate(pluginId) })
    return false
  } finally {
    busyId.value = null
    busyKind.value = null
  }
}

/** 审批（整单裁决）：成功返回实际授予的权限位清单；调用方决定是否继续启用 */
async function approve(pluginId: string): Promise<string[]> {
  const app = appOf(pluginId)
  if (!app || busyId.value) return []
  busyId.value = pluginId
  busyKind.value = 'approve'
  try {
    const granted = await runWithTimeout(pluginApprove(pluginId))
    await load()
    return granted
  } catch (e: any) {
    logger.error(`[ShellApps] approve(${pluginId}) failed:`, e)
    showUserError(e)
    return []
  } finally {
    busyId.value = null
    busyKind.value = null
  }
}

/** 卸载（调用方保证已停用；删安装目录 + 插件存储 + 启用状态，不可恢复） */
async function uninstall(pluginId: string): Promise<boolean> {
  const app = appOf(pluginId)
  if (!app || busyId.value) return false
  busyId.value = pluginId
  busyKind.value = 'uninstall'
  try {
    await runWithTimeout(pluginUninstall(pluginId))
    await load()
    return true
  } catch (e: any) {
    logger.error(`[ShellApps] uninstall(${pluginId}) failed:`, e)
    showUserError(e, { retry: () => uninstall(pluginId) })
    return false
  } finally {
    busyId.value = null
    busyKind.value = null
  }
}

// ==================== 派生投影 ====================

/** 壳展示态（响应式） */
const displayStates = computed<Record<string, AppDisplayState>>(() =>
  Object.fromEntries(plugins.value.map((p) => [p.id, appDisplayState(p.state)])),
)

const tones = computed<Record<string, AppTone>>(() =>
  Object.fromEntries(plugins.value.map((p) => [p.id, toneOf(p.id)])),
)

const approvals = computed<PluginInfo[]>(() =>
  plugins.value.filter((p) => displayStates.value[p.id] === 'approval'),
)

const runningCount = computed(() => plugins.value.filter((p) => isRunningState(p.state)).length)

/** 高危位计数（manifest 声明） */
function riskCount(app: PluginInfo): number {
  return app.permissions.filter((p) => HIGH_RISK_PERMISSIONS.has(p)).length
}

export function useShellApps() {
  return {
    plugins,
    loading,
    busyId,
    busyKind,
    detailAppId,
    displayStates,
    tones,
    approvals,
    runningCount,
    appOf,
    riskCount,
    load,
    activate,
    deactivate,
    approve,
    uninstall,
  }
}
