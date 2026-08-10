/**
 * 插件加载器（dev-shell 版，桌面端）
 *
 * 从 virtual:dev-plugins 取插件说明，依次创建 mock context → activate()，
 * 失败时置 Error 状态并记日志。
 */
import { reactive, ref } from 'vue'
import type { PluginContext, PluginModule } from '../../src/types'
import { createMockContext } from './mock-context'
import {
  getPluginRecord,
  plugins,
  pushLog,
  type DevPluginRecord,
} from './registry'

// dev-shell 专用 mock：浏览器中 WASM 后端不可用，为特定插件注入模拟命令与事件
import { registerFileTransferMock, disposeFileTransferMock } from './mock/file-transfer'

export const ready = ref(false)

/** 激活所有被调试插件（幂等） */
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
        dir: spec.dir,
        state: 'loaded',
        context: null,
      })
      plugins.value.push(record)
      pushLog('info', pluginId, `开始加载（${spec.dir}）`)

      try {
        const context: PluginContext = createMockContext(pluginId, spec.dir)
        record.context = context
        const module = spec.entry as PluginModule
        if (typeof module.activate === 'function') {
          await module.activate(context)
          record.state = 'activated'
          pushLog('info', pluginId, 'activate() 成功')
          // 注入插件 mock（浏览器无 WASM 后端，模拟命令与事件以展示完整 UI）
          if (pluginId === 'com.bedcode.file-transfer') {
            registerFileTransferMock(context)
            pushLog('info', pluginId, '已注入 dev-shell mock 数据')
          }
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
  ready.value = true
}

/** 停用单个插件（dispose 全部资源 + 调用 deactivate） */
export async function deactivatePlugin(pluginId: string): Promise<void> {
  const record = getPluginRecord(pluginId)
  if (!record || record.state === 'deactivated') return
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
  // 清理插件 mock（停止进度模拟定时器）
  if (pluginId === 'com.bedcode.file-transfer') {
    disposeFileTransferMock()
  }
  record.state = 'deactivated'
  pushLog('info', pluginId, '已停用')
}

/** 停用全部插件（页面卸载前调用） */
export async function deactivateAll(): Promise<void> {
  for (const record of [...plugins.value]) {
    await deactivatePlugin(record.id)
  }
}
