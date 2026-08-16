/**
 * Plugin fixtures — 插件清单（DesktopPluginInfo 线协议）
 *
 * Rust DTO 源：src-tauri/src/plugin/types.rs 的 DesktopPluginInfo
 * （#[serde(rename_all = "camelCase")]：id/name/version/.../pluginType/extensionPath；
 *  底层复用 packages/plugin-sdk-desktop/rust/src/types.rs 的 PluginType/PluginState/PluginContributes）。
 *
 * 命名规则：整体 camelCase（Rust rename_all）；注意与 server/session fixture 的
 * snake_case 相反，以各 DTO 的 serde 属性为准。
 * 特殊点：
 * - state 为 adjacently tagged 枚举：{state:"Activated"} / {state:"Error", error:"..."}
 * - rust_library 为 String 恒序列化（无 rust 库的插件为空字符串）
 * - icon / installed_at 带 skip_serializing_if，None 时不出现在 JSON（fixture 取「出现」形态）
 * - contributes 的 terminal/configuration/lifecycle 为 Option 无 skip，None 序列化为 null；
 *   provides/subscribes 为 Vec<String> 恒出现（前端 plugin/types.ts 尚未声明这两个字段，为子集视图）
 *
 * 对齐机制：DTO_FIELDS 清单 + 工厂内 assertDtoFields 运行时断言（含 contributes 嵌套键集合）；
 * 类型级仅对标量字段做 Required<Pick> 相等断言（contributes 结构按线协议保留 null，不做严格相等）。
 */

import type {
  PluginInfo,
  PluginType,
  PluginState,
  CommandContribution,
  ViewContribution,
  TerminalContribution,
  ToolProviderContribution,
  FileHandlerContribution,
  PluginConfiguration,
  LifecycleContribution,
} from '@/plugin/types'
import { assertDtoFields, type Equals, type Expect } from './drift'

// ==================== contributes（线协议全字段） ====================

export interface FixturePluginContributes {
  commands: CommandContribution[]
  views: ViewContribution[]
  terminal: TerminalContribution | null
  toolProviders: ToolProviderContribution[]
  fileHandlers: FileHandlerContribution[]
  configuration: PluginConfiguration | null
  lifecycle: LifecycleContribution | null
  provides: string[]
  subscribes: string[]
}

/** 与 types.rs PluginContributes（camelCase）字段一一对应 */
export const CONTRIBUTES_DTO_FIELDS = [
  'commands',
  'views',
  'terminal',
  'toolProviders',
  'fileHandlers',
  'configuration',
  'lifecycle',
  'provides',
  'subscribes',
] as const

export function makePluginContributes(overrides: Partial<FixturePluginContributes> = {}): FixturePluginContributes {
  const fixture: FixturePluginContributes = {
    commands: [],
    views: [],
    terminal: null,
    toolProviders: [],
    fileHandlers: [],
    configuration: null,
    lifecycle: null,
    provides: [],
    subscribes: [],
    ...overrides,
  }
  assertDtoFields(fixture, CONTRIBUTES_DTO_FIELDS, 'PluginContributes')
  return fixture
}

// ==================== DesktopPluginInfo ====================

export interface PluginInfoFixture {
  id: string
  name: string
  version: string
  description: string
  author: string
  main: string
  sandbox: string
  pluginType: PluginType
  /** WASM 库文件名（无 rust 库的插件为空字符串，Rust 恒序列化） */
  rustLibrary: string
  permissions: string[]
  state: PluginState
  extensionPath: string
  contributes: FixturePluginContributes
  /** manifest.icon 透传（None 时被 skip，fixture 取出现形态） */
  icon: string
  source: string
  sizeBytes: number
  /** 安装时间（unix 毫秒） */
  installedAt: number
}

/** 与 plugin/types.rs DesktopPluginInfo（camelCase）字段一一对应 */
export const PLUGIN_INFO_DTO_FIELDS = [
  'id',
  'name',
  'version',
  'description',
  'author',
  'main',
  'sandbox',
  'pluginType',
  'rustLibrary',
  'permissions',
  'state',
  'extensionPath',
  'contributes',
  'icon',
  'source',
  'sizeBytes',
  'installedAt',
] as const

export function makePluginInfo(overrides: Partial<PluginInfoFixture> = {}): PluginInfoFixture {
  const fixture: PluginInfoFixture = {
    id: 'com.bedcode.demo',
    name: 'Demo Plugin',
    version: '1.0.0',
    description: 'A demo plugin',
    author: 'BedCode Team',
    main: 'dist/main.js',
    sandbox: 'inline',
    pluginType: 'ts-only',
    rustLibrary: '',
    permissions: [],
    state: { state: 'Activated' },
    extensionPath: '/opt/bedcode/plugins/com.bedcode.demo',
    contributes: makePluginContributes(),
    icon: 'icon.svg',
    source: 'builtin',
    sizeBytes: 10240,
    installedAt: 1735689600000,
    ...overrides,
  }
  assertDtoFields(fixture, PLUGIN_INFO_DTO_FIELDS, 'PluginInfo')
  return fixture
}

// ==================== 类型级对齐断言（编译期，对标量字段） ====================
// contributes 因线协议保留 null（前端为可选子集视图），仅运行时断言其键集合。

type PluginScalarKeys =
  | 'id' | 'name' | 'version' | 'description' | 'author' | 'main' | 'sandbox'
  | 'pluginType' | 'rustLibrary' | 'permissions' | 'state' | 'extensionPath'
  | 'icon' | 'source' | 'sizeBytes' | 'installedAt'

type _PluginScalarEq = Expect<
  Equals<Required<Pick<PluginInfoFixture, PluginScalarKeys>>, Required<Pick<PluginInfo, PluginScalarKeys>>>
>
