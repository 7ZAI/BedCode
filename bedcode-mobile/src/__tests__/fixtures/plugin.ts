/**
 * Plugin fixtures — 插件管理面板 + loader 门禁测试的工厂
 *
 * Rust DTO 源：`bedcode-mobile/src-tauri/src/plugin/types.rs:MobilePluginInfo` +
 * `bedcode-plugin-api-mobile::types::PluginState` / `PluginManifest` / `PluginContributes`
 *
 * 字段形态（camelCase / kebab-case 与 Rust serde rename_all 一致）：
 * - state: 联合类型（PluginState = Loaded | Activating | Activated | Degraded{error} |
 *   NeedsApproval | Deactivated | Error{error}）— 7 变体
 * - pluginType: PluginType = 'rust' | 'rust-ts' | 'ts-only' | 'wasm' (kebab-case)
 * - contributes: 命令/视图/导航/设置/终端/配置/生命周期 7 类（多数可为 null）
 *
 * 注：移动端 PluginInfo 比桌面端多 `source: string` / `extensionPath` /
 * `sizeBytes` / `installedAt` 字段，无 `sandbox` 字段——与 desktop
 * `__tests__/fixtures/index.ts` 的 plugin factory 不通用，单独维护
 */

import type { PluginInfo } from '@/plugin/types'
import type {
  MobilePluginContributes,
  PluginState,
} from '@binblink/bedcode-plugin-sdk-mobile'

/**
 * 构造最小可用的 PluginInfo
 *
 * 默认值遵循：pluginType='rust-ts' / state=Activated / source='apk-asset' /
 * extensionPath='' / sizeBytes=0 / installedAt=0；contributes 全空骨架
 */
export function makePluginInfo(
  partial: Partial<PluginInfo> & Pick<PluginInfo, 'id' | 'name'>,
): PluginInfo {
  const contributes: MobilePluginContributes = {
    commands: [],
    views: [],
    terminal: null,
    navTab: null,
    settings: null,
    configuration: null,
    lifecycle: null,
  }
  const state: PluginState = { state: 'Activated' }
  return {
    id: partial.id,
    name: partial.name,
    version: '1.0.0',
    description: '',
    author: '',
    main: '',
    pluginType: 'rust-ts',
    permissions: [],
    contributes,
    source: 'apk-asset',
    extensionPath: '',
    sizeBytes: 0,
    installedAt: 0,
    ...partial,
    // 防止 partial 漏 contributes 时把默认空骨架冲掉
    contributes: { ...contributes, ...(partial.contributes ?? {}) },
    // state 优先取 partial.state（已声明的联合类型变体），缺省走默认 Activated
    state: (partial.state ?? state) as PluginState,
  }
}

/** 构造 Degraded 状态插件 */
export function makeDegradedPluginInfo(o: {
  id: string
  name: string
  error: string
}): PluginInfo {
  return makePluginInfo({
    id: o.id,
    name: o.name,
    state: { state: 'Degraded', error: o.error },
  })
}

/** 构造 Activating 中间态插件（loader.loadAll 跳过） */
export function makeActivatingPluginInfo(o: {
  id: string
  name: string
}): PluginInfo {
  return makePluginInfo({
    id: o.id,
    name: o.name,
    state: { state: 'Activating' },
  })
}
