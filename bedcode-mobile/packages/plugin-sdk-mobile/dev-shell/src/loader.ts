/**
 * 插件加载器（dev-shell 版）
 *
 * 从 virtual:dev-plugins 取插件说明（vite 已解析入口与 plugin.json），
 * 依次创建 mock context → activate()，失败时置 Error 状态并记日志。
 * 所有插件激活完成后发射 appStartup 生命周期事件。
 */
import { reactive, ref } from 'vue'
import type { PluginContext, PluginModule } from '../../src/types'
import { createMockContext } from './mock-context'
import { emitDevEvent } from './mock/session'
import {
  getPluginRecord,
  plugins,
  pushLog,
  registerDevMock,
  type DevPluginRecord,
} from './registry'

export const ready = ref(false)

/**
 * 激活所有被调试插件（幂等：已激活的跳过）
 */
export async function loadPlugins(): Promise<void> {
  if (ready.value) return
  try {
    const records = (await import('virtual:dev-plugins')).default as Array<{
      dir: string
      manifest: Record<string, any>
      entry: any
    }>
    for (const spec of records) {
      const manifest = spec.manifest && spec.manifest.id ? spec.manifest : {}
      const pluginId: string = manifest.id || `dev-plugin-${plugins.value.length}`
      const pluginName: string = manifest.name || pluginId
      const record: DevPluginRecord = reactive({
        id: pluginId,
        name: pluginName,
        manifest: spec.manifest || {},
        entry: spec.entry,
        state: 'loaded',
        context: null,
      })
      plugins.value.push(record)
      pushLog('info', pluginId, `开始加载（${spec.dir}）`)

      try {
        // 领域数据（devMock）先注册，createMockContext 按 pluginId 合并
        const module = spec.entry as PluginModule
        if (module.devMock) {
          record.devMockDisposable = registerDevMock(pluginId, module.devMock)
          pushLog('info', pluginId, '已注册 devMock（领域种子数据）')
        }
        const context: PluginContext = createMockContext(pluginId)
        record.context = context
        if (typeof module.activate === 'function') {
          await module.activate(context)
          record.state = 'activated'
          pushLog('info', pluginId, 'activate() 成功')
        } else {
          record.state = 'loaded'
          pushLog('warn', pluginId, '入口模块未导出 activate()，仅完成加载')
        }
      } catch (e: any) {
        record.state = 'error'
        record.error = e?.message || String(e)
        pushLog('error', pluginId, `activate() 失败: ${record.error}`)
      }
    }
  } catch (e: any) {
    pushLog('error', 'dev-shell', `加载插件失败: ${e?.message || e}`)
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

/** 停用单个插件（dispose 全部资源 + 调用 deactivate） */
export async function deactivatePlugin(pluginId: string): Promise<void> {
  const record = getPluginRecord(pluginId)
  if (!record || record.state === 'deactivated') return
  record.devMockDisposable?.dispose()
  record.devMockDisposable = undefined
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
  const module = record.entry as PluginModule
  if (typeof module.deactivate === 'function') {
    try {
      await module.deactivate()
    } catch (e) {
      pushLog('warn', pluginId, `deactivate() 失败: ${e}`)
    }
  }
  record.state = 'deactivated'
  pushLog('info', pluginId, '已停用')
}

/** 停用全部插件（页面卸载前调用） */
export async function deactivateAll(): Promise<void> {
  for (const record of [...plugins.value]) {
    await deactivatePlugin(record.id)
  }
  emitDevEvent('plugin:lifecycle:appShutdown', {})
}
