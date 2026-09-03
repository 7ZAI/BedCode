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
  registerDevMock,
  type DevPluginRecord,
} from './registry'

// 领域命令 mock（纯通用接线）：浏览器中 WASM 后端不可用，按插件 devMock
// 种子子域（peer / transfer）判断是否注入，不感知具体插件身份
import { registerFileTransferMock } from './mock/file-transfer'

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
        // 领域种子数据（devMock）先注册：mock 命令实现按 pluginId 消费（与移动端同构）
        const module = spec.entry as PluginModule
        if (module.devMock) {
          record.devMockDisposable = registerDevMock(pluginId, module.devMock)
          pushLog('info', pluginId, '已注册 devMock（领域种子数据）')
        }
        const context: PluginContext = createMockContext(pluginId, spec.dir)
        record.context = context
        // mock 命令先于 activate() 注册：插件 activate/首帧即会拉设置与设备，
        // 后注册会错过首轮命令（空态假象）；与移动端 loader 同构。
        // 是否注入由插件 devMock 的领域种子子域决定，dev-shell 不写死插件清单
        if (module.devMock?.peer || module.devMock?.transfer) {
          record.mockDisposable = registerFileTransferMock(context, pluginId)
          pushLog('info', pluginId, '已注册领域命令 mock（通用接线，种子来自插件 devMock）')
        }
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
  ready.value = true
}

/** 停用单个插件（dispose 全部资源 + 调用 deactivate） */
export async function deactivatePlugin(pluginId: string): Promise<void> {
  const record = getPluginRecord(pluginId)
  if (!record || record.state === 'deactivated') return
  record.devMockDisposable?.dispose()
  record.devMockDisposable = undefined
  // 清理领域命令 mock（停止进度模拟等定时器）
  record.mockDisposable?.dispose()
  record.mockDisposable = undefined
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
}
