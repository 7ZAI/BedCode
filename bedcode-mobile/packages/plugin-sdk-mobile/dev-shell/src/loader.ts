/**
 * 调试对象加载器（dev-shell 版）
 * -----------------------------------------------------------------------------
 * 加载两类调试对象，**同一条路径**：
 *   ① 内置应用「模拟终端」（`src/apps/mock-terminal/`）——自带 manifest + activate
 *   ② 被调试插件（`virtual:dev-plugins`，vite 已解析入口与 plugin.json）
 *
 * 之所以统一：新宿主壳的核心形态是「应用自持运行面，壳只挂载」，预览环境里若内置页
 * 走另一条路（壳自己的一页），开发者就无法在 dev-shell 里验证壳的挂载路径是否成立。
 *
 * 加载流程：登记记录 → 建 mock context（先于 activate）→ activate() → 置状态；
 * 失败置 Error 并记日志（错误留在应用列表上可见，不静默吞掉）。
 * 全部加载完成后发射 appStartup 生命周期事件（与宿主一致）。
 */
import { reactive, ref } from 'vue'
import type { PluginContext, PluginModule } from '../../src/types'
import { createMockContext } from './mock-context'
import { emitDevEvent } from './mock/session'
import { registerFileTransferMock, disposeFileTransferMock } from './mock/file-transfer'
import {
  getDevMock,
  getPluginRecord,
  plugins,
  pushLog,
  registerDevMock,
  type DevPluginRecord,
} from './registry'
import * as mockTerminal from './apps/mock-terminal'

export const ready = ref(false)

/** 在给定记录上跑一遍激活（建 context → 登记领域数据 → 调 activate） */
function runActivate(record: DevPluginRecord, module: PluginModule | null): void {
  const context: PluginContext = createMockContext(record.id)
  record.context = context
  try {
    if (module?.devMock) {
      record.devMockDisposable = registerDevMock(record.id, module.devMock)
      pushLog('info', record.id, '已注册 devMock（领域种子数据）')
    }
    // 对等/传输域 mock（纯通用接线：仅当 devMock 含对应种子子域才注入，不感知插件身份）
    const devMockSeed = module?.devMock
    if (devMockSeed?.peer || devMockSeed?.transfer) {
      registerFileTransferMock(context, devMockSeed, record.id)
      pushLog('info', record.id, '已注册领域命令 mock（通用接线，种子来自 devMock）')
    }
    record.entry = module
    if (module && typeof module.activate === 'function') {
      void module.activate(context)
    } else if (!module) {
      // 内置应用：activate 同步执行（无 WASM 后端，无异步初始化）
      mockTerminal.activate(context)
    }
    record.state = 'activated'
    record.error = undefined
    pushLog('info', record.id, 'activate() 成功')
  } catch (e: any) {
    record.state = 'error'
    record.error = e?.message || String(e)
    pushLog('error', record.id, `activate() 失败: ${record.error}`)
  }
}

/** 新建记录并激活（首屏加载调试对象时用） */
function activateRecord(record: DevPluginRecord, module: PluginModule | null): void {
  plugins.value.push(record)
  runActivate(record, module)
}

/**
 * 复用已有记录重新激活（停用后再启动走这里）
 *
 * 刻意**不新建记录**：同 id 出现两条记录后，`getPluginRecord` 会命中先注册的那条
 * （停用态、持有已释放的 context），插件组件 inject 到过期上下文，且应用列表里
 * 同一个应用会显示两行。复用是最小改动，也与「记录即该应用的持有者」一致。
 */
function reviveRecord(
  existing: DevPluginRecord,
  fresh: DevPluginRecord,
  module: PluginModule | null,
): void {
  existing.name = fresh.name
  existing.manifest = fresh.manifest
  runActivate(existing, module)
}

/** 内置应用记录（与应用记录同构，只多了 builtin 标记） */
function makeBuiltinRecord(): DevPluginRecord {
  return reactive({
    id: mockTerminal.MOCK_TERMINAL_ID,
    name: mockTerminal.MOCK_TERMINAL_MANIFEST.name,
    manifest: mockTerminal.MOCK_TERMINAL_MANIFEST,
    entry: null,
    state: 'loaded' as const,
    context: null,
    builtin: true,
  })
}

/**
 * 启动单个调试对象（幂等：已激活的跳过）
 *
 * 壳的「启动应用」动作经 `shell/adapters/devAppSource.ts` 落到这里——停用后再启动
 * 就是重新跑一次 activate，与被调试插件的行为一致。
 */
export async function activatePlugin(pluginId: string): Promise<void> {
  const existing = getPluginRecord(pluginId)
  if (existing?.state === 'activated') return
  // 非「已停用」态先走一遍对称拆解（释放旧订阅与监听）；「已停用」态已拆过，
  // 再拆一次是空转——deactivatePlugin 对它直接早退
  if (existing && existing.state !== 'deactivated') {
    await deactivatePlugin(pluginId)
  }
  if (pluginId === mockTerminal.MOCK_TERMINAL_ID) {
    const fresh = makeBuiltinRecord()
    if (existing) reviveRecord(existing, fresh, null)
    else activateRecord(fresh, null)
    return
  }
  const specs = await loadSpecs()
  const spec = specs.find((s) => s.id === pluginId)
  if (!spec) {
    pushLog('error', pluginId, 'activatePlugin: 未找到该调试对象（可能已从工程中移除）')
    return
  }
  const fresh = makeRecord(spec)
  if (existing) reviveRecord(existing, fresh, spec.module)
  else activateRecord(fresh, spec.module)
}

/** 载入被调试插件清单（vite 虚拟模块，缓存以免重复 import） */
let specsPromise: Promise<DevSpec[]> | null = null

interface DevSpec {
  id: string
  name: string
  manifest: Record<string, unknown>
  module: PluginModule
}

function loadSpecs(): Promise<DevSpec[]> {
  if (!specsPromise) {
    specsPromise = (async () => {
      const records = (await import('virtual:dev-plugins')).default as Array<{
        dir: string
        manifest: Record<string, unknown>
        entry: any
      }>
      return records.map((spec, index) => {
        const manifest = (spec.manifest && spec.manifest.id ? spec.manifest : {}) as Record<string, unknown>
        const id = typeof manifest.id === 'string' ? manifest.id : `dev-plugin-${index}`
        const name = typeof manifest.name === 'string' ? manifest.name : id
        pushLog('info', id, `发现被调试插件（${spec.dir}）`)
        return { id, name, manifest, module: spec.entry as PluginModule }
      })
    })()
  }
  return specsPromise
}

function makeRecord(spec: DevSpec): DevPluginRecord {
  return reactive({
    id: spec.id,
    name: spec.name,
    manifest: spec.manifest,
    entry: spec.module,
    state: 'loaded' as const,
    context: null,
  })
}

/** 激活全部调试对象（内置应用先行，被调试插件随后） */
export async function loadPlugins(): Promise<void> {
  if (ready.value) return
  try {
    // ① 内置应用：保证壳在任何插件都未加载时也有一个形态正确的应用可渲染
    activateRecord(makeBuiltinRecord(), null)

    // ② 被调试插件
    for (const spec of await loadSpecs()) {
      if (plugins.value.some((p) => p.id === spec.id)) continue
      activateRecord(makeRecord(spec), spec.module)
    }
  } catch (e: any) {
    pushLog('error', 'dev-shell', `加载调试对象失败: ${e?.message || e}`)
  }

  // 与宿主一致：应用启动完成后再触发 onStartup 生命周期；
  // 队列种子注入（mobileApi 无 pluginId，需等全部 devMock 注册完成后播种）
  emitDevEvent('plugin:lifecycle:appStartup', {})
  syncQueueSeedNow()
  ready.value = true
}

/** 全部插件加载完成后同步队列种子（mobileApi 无 pluginId，惰性播种入口） */
export function syncQueueSeedNow(): void {
  // 动态 import 避免 loader ↔ mock 循环依赖
  void import('./mock/mobile-api').then((m) => m.syncQueueSeed())
}

/** 停用单个调试对象（dispose 全部资源 + 调用 deactivate） */
export async function deactivatePlugin(pluginId: string): Promise<void> {
  const record = getPluginRecord(pluginId)
  if (!record || record.state === 'deactivated') return
  record.devMockDisposable?.dispose()
  record.devMockDisposable = undefined
  if (getDevMock(pluginId)?.peer || getDevMock(pluginId)?.transfer) disposeFileTransferMock()
  const context = record.context as PluginContext | null
  if (context) {
    for (const d of [...context._disposables]) {
      try {
        d.dispose()
      } catch (e) {
        pushLog('warn', pluginId, `dispose 资源失败: ${e}`)
      }
    }
    context._disposables.length = 0
  }
  const module = record.entry as PluginModule | null
  try {
    if (module && typeof module.deactivate === 'function') {
      await module.deactivate()
    } else if (!module && record.builtin) {
      mockTerminal.deactivate()
    }
  } catch (e) {
    pushLog('warn', pluginId, `deactivate() 失败: ${e}`)
  }
  record.state = 'deactivated'
  pushLog('info', pluginId, '已停用')
}

/** 停用全部调试对象（页面卸载前调用） */
export async function deactivateAll(): Promise<void> {
  for (const record of [...plugins.value]) {
    await deactivatePlugin(record.id)
  }
  emitDevEvent('plugin:lifecycle:appShutdown', {})
}