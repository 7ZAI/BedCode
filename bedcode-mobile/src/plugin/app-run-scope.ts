/**
 * 应用运行期 store 作用域（App Run Scope）
 * -----------------------------------------------------------------------------
 * 一次「应用运行期」= `pluginLoader.activate()` 到 `pluginLoader.deactivate()`
 * （多任务上滑 / × / 详情「停用」/ 卸载都走后者）。运行期结束即**作废该运行期的
 * 全部 Pinia store**，再次启动是全新运行期——用户口径：停止应用就是停止 store
 * 的运行，再次点击是启动 store。
 *
 * 为什么必须由宿主做（而不是让插件自觉 clear）：
 * - 插件前端模块经动态 import 装载，URL 不变 → ESM 模块缓存命中，模块求值只发生
 *   一次；模块顶层的 `defineStore` 只调用一次，store 定义被缓存。若不介入，第二次
 *   运行期 `useStore()` 会从宿主 Pinia 取回**上一次运行期的 store 实例**（状态残留）。
 * - 包装 `defineStore` 的收益：包装后的 `useStore()` 恒把「该插件当前运行期的
 *   Pinia」作为显式实参传入（`useStore(pinia)`），组件、activate 期调用、事件回调
 *   三种上下文都落在同一作用域——不依赖 `activePinia` 全局态，也不受宿主 Pinia 的
 *   注入链影响。
 *
 * 绑定规则（谁属于哪个插件）：
 * - 插件 `defineStore` 发生在模块求值期（顶层 `const useX = defineStore(...)`，现状
 *   全部如此）。宿主在 `loadPluginModuleScoped` 的「装载窗口」内记录当前插件 id，
 *   窗口内定义的 store 绑定该插件；之后每次 `useStore()` 解析的是该插件的**当前**
 *   运行期作用域，所以模块只求值一次也不影响第二次运行期拿到新作用域。
 * - 装载窗口是全局标记，故装载**必须串行**：并发装载会让 A 的模块求值撞上 B 的
 *   窗口，把 store 绑到错误的插件。本模块用 promise 链串行化，调用方无需关心。
 * - 模块顶层之外的 `defineStore`（如异步 chunk 里）不做猜测：回退宿主 Pinia 并
 *   warn（fail-visible）。契约：store 定义必须在插件模块顶层。
 *
 * 与后端实例的关系：移动宿主停用只调 guest `deactivate`，WASM 实例/Store 保留复用
 * （Android 上重新实例化成本高）。既有契约是「宿主复用实例、插件在 deactivate 自清
 * 运行时状态」，本模块把同一条契约补到前端 store 层。
 */

import { createPinia, defineStore as baseDefineStore, type Pinia } from 'pinia'
import { logger } from '@/utils/frontendLogger'

/** 作用域内已创建的 store（只需要 $dispose；Pinia 的 store 实例均带该方法） */
interface DisposableStore {
  $dispose(): void
}

/** 单个插件的运行期 store 作用域 */
interface AppRunScope {
  pluginId: string
  pinia: Pinia
  /** 已创建 store（Set 去重：同一 store 多次 useStore 只登记一次） */
  stores: Set<DisposableStore>
  disposed: boolean
}

/** pluginId → 当前运行期作用域（dispose 后即摘除） */
const scopes = new Map<string, AppRunScope>()

/**
 * 插件模块装载窗口内记录的插件 id
 *
 * 装载窗口由 `loadPluginModuleScoped` 开合；窗口外为 null。
 */
let moduleLoadPluginId: string | null = null

/** 装载串行链：保证任一时刻只有一个装载窗口（见模块头「装载必须串行」） */
let moduleLoadChain: Promise<void> = Promise.resolve()

/** 取插件当前运行期的 store 作用域；不存在或已作废则新建 */
function acquireRunScope(pluginId: string): AppRunScope {
  const existing = scopes.get(pluginId)
  if (existing && !existing.disposed) return existing
  const scope: AppRunScope = {
    pluginId,
    pinia: createPinia(),
    stores: new Set(),
    disposed: false,
  }
  scopes.set(pluginId, scope)
  return scope
}

/**
 * 在「插件模块装载窗口」内执行装载动作（通常是动态 `import()`）
 *
 * 窗口内调用的 `defineStore` 绑定到 `pluginId`；窗口开合期间不允许其它装载并发
 * （内部 promise 链串行化）。`load` 抛错时窗口同样关闭（finally）。
 */
export async function loadPluginModuleScoped<T>(
  pluginId: string,
  load: () => Promise<T>,
): Promise<T> {
  const previous = moduleLoadChain
  let release!: () => void
  moduleLoadChain = new Promise<void>((resolve) => {
    release = resolve
  })
  await previous
  moduleLoadPluginId = pluginId
  try {
    return await load()
  } finally {
    moduleLoadPluginId = null
    release()
  }
}

/**
 * 包装 Pinia 的 `defineStore`（宿主经 `window.__BEDCODE_SHARED__.pinia` 暴露给插件）
 *
 * - 在装载窗口内调用：返回的 `useStore()` 恒把该插件当前运行期作用域的 Pinia 作为
 *   显式实参传入，并把创建的 store 登记到作用域（供 `disposeAppRunScope` 作废）。
 * - 窗口外调用：回退原始 `useStore`（宿主 Pinia 语义）并 warn——不猜测归属。
 *
 * 两种签名（`defineStore(id, setup)` / `defineStore(options)`）都支持。
 */
export function scopedDefineStore(
  idOrOptions: unknown,
  setupOrOptions?: unknown,
  options?: unknown,
): unknown {
  const pluginId = moduleLoadPluginId
  const useStore =
    typeof idOrOptions === 'string'
      ? (baseDefineStore as (...args: unknown[]) => any)(idOrOptions, setupOrOptions, options)
      : (baseDefineStore as (...args: unknown[]) => any)(idOrOptions, setupOrOptions)

  if (!pluginId) {
    const storeId =
      typeof idOrOptions === 'string' ? idOrOptions : ((idOrOptions as { id?: string })?.id ?? '')
    logger.warn(
      `[AppRunScope] defineStore('${storeId}') called outside a plugin module load window: ` +
        'store falls back to the host Pinia and survives app stop. ' +
        'Define stores at the top level of the plugin module (see SDK docs).',
    )
    return useStore
  }

  const boundPluginId: string = pluginId
  const useScopedStore = function useScopedStore(): unknown {
    const scope = acquireRunScope(boundPluginId)
    const store = useStore(scope.pinia)
    scope.stores.add(store)
    return store
  }
  // 与 Pinia 的 defineStore 返回形状对齐：$id 供 mapStores / devtools 读取
  ;(useScopedStore as { $id?: string }).$id = (useStore as { $id?: string }).$id
  return useScopedStore
}

/**
 * 作废插件当前运行期的 store 作用域（幂等）
 *
 * 对作用域内每个 store 调 `$dispose()`（停止其 effect 并从 Pinia 摘除），随后丢弃
 * 整个作用域。之后任何 `useStore()` 都会新建 store（回到 setup 初始 state）。
 *
 * 单个 store `$dispose` 抛错不阻断其余 store（逐个 try）：停在半途会让后面的 store
 * 跨运行期残留，是比日志噪声更严重的状态泄漏。
 */
export function disposeAppRunScope(pluginId: string): void {
  const scope = scopes.get(pluginId)
  if (!scope || scope.disposed) return

  // 先摘册再 dispose：dispose 期间若有回调再次 useStore，会拿到新建作用域
  // （初始 state），而不是这个正在拆除的作用域
  scopes.delete(pluginId)
  scope.disposed = true

  const total = scope.stores.size
  let disposedCount = 0
  for (const store of scope.stores) {
    try {
      store.$dispose()
      disposedCount += 1
    } catch (e) {
      logger.error(`[AppRunScope] store $dispose failed: pluginId=${pluginId}`, e)
    }
  }
  scope.stores.clear()
  logger.log(
    `[AppRunScope] disposed run scope: pluginId=${pluginId} stores=${disposedCount}/${total}`,
  )
}

/**
 * 清空全部运行期作用域（测试与 Hot Reload 用）
 *
 * 生产路径不需要：作用域随 `deactivate` 单个作废，进程内不存在跨插件批量重置的需求。
 */
export function _resetAppRunScopesForTest(): void {
  scopes.clear()
  moduleLoadPluginId = null
  moduleLoadChain = Promise.resolve()
}
