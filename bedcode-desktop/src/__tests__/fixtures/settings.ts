/**
 * AppConfig fixtures — get_app_settings / save_app_settings 线协议
 *
 * Rust DTO 源：src-tauri/src/system/config.rs 的 AppConfig
 * （serde 默认 snake_case；network/session/ui/channels/terminal/log 六个小节
 *  get_app_settings 响应恒全量序列化——serde(default) 只影响反序列化缺省，
 *  不影响序列化，故 fixture 取「全部小节都在」的最大线协议形态）。
 *
 * 消费方差异：前端 stores/settings.ts 的 Settings 类型是 AppConfig 的子集视图
 * （只取 network/ui 两个小节；session 段已从 Rust DTO 整体退役——默认值真源在
 *  `com.bedcode.terminal-session` 插件存储，2026-09-25 随宿主业务配置下沉删除）。
 *  channels/terminal/log 前端不消费，fixture 一并建模防漂移。
 *
 * 命名规则：serde 默认 snake_case。
 * 对齐机制：DTO_FIELDS 清单（含嵌套小节）+ 工厂内 assertDtoFields 运行时断言。
 */

import { assertDtoFields } from './drift'

// ==================== AppConfig（system/config.rs） ====================

export interface AppConfigNetworkFixture {
  port: number
  auto_start: boolean
  prevent_sleep: boolean
  workers: number
  keep_alive_secs: number
  client_request_timeout_secs: number
  client_disconnect_timeout_secs: number
  max_connections: number
  backlog: number
  tcp_nodelay: boolean
  shutdown_timeout_secs: number
  ws_max_frame_size_kb: number
  ws_max_message_size_mb: number
  metrics_enabled: boolean
}

/** 与 config.rs NetworkConfig 字段一一对应（与 fixtures/server.ts 的 makeNetworkConfig 同构，独立建模保持 AppConfig 自洽） */
export const APP_CONFIG_NETWORK_DTO_FIELDS = [
  'port',
  'auto_start',
  'prevent_sleep',
  'workers',
  'keep_alive_secs',
  'client_request_timeout_secs',
  'client_disconnect_timeout_secs',
  'max_connections',
  'backlog',
  'tcp_nodelay',
  'shutdown_timeout_secs',
  'ws_max_frame_size_kb',
  'ws_max_message_size_mb',
  'metrics_enabled',
] as const

export interface AppConfigUiFixture {
  theme: string
  theme_palette: string
  font_size: number
  terminal_font_size: number
  terminal_font_family: string
  terminal_theme: string
  language: string
  /** None 序列化为 null（不启用背景图片） */
  terminal_bg_image: string | null
  terminal_bg_opacity: number
  animations_enabled: boolean
}

/** 与 config.rs UiConfig 字段一一对应 */
export const APP_CONFIG_UI_DTO_FIELDS = [
  'theme',
  'theme_palette',
  'font_size',
  'terminal_font_size',
  'terminal_font_family',
  'terminal_theme',
  'language',
  'terminal_bg_image',
  'terminal_bg_opacity',
  'animations_enabled',
] as const

export interface AppConfigChannelsFixture {
  lifecycle_capacity: number
}

/** 与 config.rs ChannelsConfig 字段一一对应 */
export const APP_CONFIG_CHANNELS_DTO_FIELDS = ['lifecycle_capacity'] as const

export interface AppConfigTerminalFixture {
  default_cols: number
  default_rows: number
  read_buffer_size: number
}

/** 与 config.rs TerminalConfig 字段一一对应 */
export const APP_CONFIG_TERMINAL_DTO_FIELDS = [
  'default_cols',
  'default_rows',
  'read_buffer_size',
] as const

export interface AppConfigLogFixture {
  file_level: string
  console_filter: string
  rotation: string
  max_files: number
  capacity_bytes: number
  format: string
  console_in_release: boolean
}

/** 与 config.rs LogConfig 字段一一对应 */
export const APP_CONFIG_LOG_DTO_FIELDS = [
  'file_level',
  'console_filter',
  'rotation',
  'max_files',
  'capacity_bytes',
  'format',
  'console_in_release',
] as const

export interface AppConfigFixture {
  network: AppConfigNetworkFixture
  ui: AppConfigUiFixture
  channels: AppConfigChannelsFixture
  terminal: AppConfigTerminalFixture
  log: AppConfigLogFixture
}

/** 与 config.rs AppConfig 顶层五小节一一对应 */
export const APP_CONFIG_DTO_FIELDS = ['network', 'ui', 'channels', 'terminal', 'log'] as const

export function makeAppConfig(overrides: Partial<AppConfigFixture> = {}): AppConfigFixture {
  const fixture: AppConfigFixture = {
    network: {
      port: 9000,
      auto_start: false,
      prevent_sleep: true,
      workers: 4,
      keep_alive_secs: 30,
      client_request_timeout_secs: 30,
      client_disconnect_timeout_secs: 30,
      max_connections: 100,
      backlog: 1024,
      tcp_nodelay: true,
      shutdown_timeout_secs: 10,
      ws_max_frame_size_kb: 64,
      ws_max_message_size_mb: 16,
      metrics_enabled: false,
    },
    ui: {
      theme: 'system',
      theme_palette: 'warm',
      font_size: 12,
      terminal_font_size: 12,
      terminal_font_family: 'Consolas',
      terminal_theme: 'dracula',
      language: 'zh-CN',
      terminal_bg_image: null,
      terminal_bg_opacity: 30,
      animations_enabled: true,
    },
    channels: {
      lifecycle_capacity: 16,
    },
    terminal: {
      default_cols: 120,
      default_rows: 40,
      read_buffer_size: 4096,
    },
    log: {
      file_level: 'info',
      console_filter: 'bedcode_lib=debug,actix_web=info,actix_http=info',
      rotation: 'daily',
      max_files: 7,
      capacity_bytes: 512 * 1024 * 1024,
      format: 'text',
      console_in_release: false,
    },
    ...overrides,
  }
  // 顶层与各嵌套小节键集合分别断言：任一节漂移即失败
  assertDtoFields(fixture, APP_CONFIG_DTO_FIELDS, 'AppConfig')
  assertDtoFields(fixture.network, APP_CONFIG_NETWORK_DTO_FIELDS, 'AppConfig.network')
  assertDtoFields(fixture.ui, APP_CONFIG_UI_DTO_FIELDS, 'AppConfig.ui')
  assertDtoFields(fixture.channels, APP_CONFIG_CHANNELS_DTO_FIELDS, 'AppConfig.channels')
  assertDtoFields(fixture.terminal, APP_CONFIG_TERMINAL_DTO_FIELDS, 'AppConfig.terminal')
  assertDtoFields(fixture.log, APP_CONFIG_LOG_DTO_FIELDS, 'AppConfig.log')
  return fixture
}
