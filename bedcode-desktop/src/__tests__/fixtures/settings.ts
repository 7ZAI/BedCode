/**
 * AppConfig fixtures — get_app_settings / save_app_settings 线协议
 *
 * Rust DTO 源：src-tauri/src/system/config.rs 的 AppConfig
 * （serde 默认 snake_case；network/session/ui/channels/terminal/log 六个小节
 *  get_app_settings 响应恒全量序列化——serde(default) 只影响反序列化缺省，
 *  不影响序列化，故 fixture 取「全部小节都在」的最大线协议形态）。
 *
 * 消费方差异：前端 stores/settings.ts 的 Settings 类型是 AppConfig 的子集视图
 * （只取 network/session/ui 三个小节；network 又只声明 port/qr_host/prevent_sleep，
 *  qr_host 为前端自建扩展字段，Rust NetworkConfig 无此字段——save 时被 serde
 *  忽略，属既有设计）。channels/terminal/log 前端不消费，fixture 一并建模防漂移。
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

export interface AppConfigSessionFixture {
  default_environment: string
  default_wsl_distro: string | null
  default_working_dir: string | null
  default_command: string | null
  session_timeout: number
}

/** 与 config.rs SessionConfig 字段一一对应 */
export const APP_CONFIG_SESSION_DTO_FIELDS = [
  'default_environment',
  'default_wsl_distro',
  'default_working_dir',
  'default_command',
  'session_timeout',
] as const

export interface AppConfigUiFixture {
  theme: string
  theme_palette: string
  font_size: number
  terminal_font_size: number
  terminal_font_family: string
  terminal_theme: string
  show_preview: boolean
  language: string
  /** None 序列化为 null（不启用背景图片） */
  terminal_bg_image: string | null
  terminal_bg_opacity: number
}

/** 与 config.rs UiConfig 字段一一对应 */
export const APP_CONFIG_UI_DTO_FIELDS = [
  'theme',
  'theme_palette',
  'font_size',
  'terminal_font_size',
  'terminal_font_family',
  'terminal_theme',
  'show_preview',
  'language',
  'terminal_bg_image',
  'terminal_bg_opacity',
] as const

export interface AppConfigChannelsFixture {
  status_broadcast_capacity: number
  restart_broadcast_capacity: number
  event_broadcast_capacity: number
  pty_subscription_capacity: number
  global_queue_capacity: number
  global_queue_max_bytes: number
  ws_event_capacity: number
  lifecycle_capacity: number
}

/** 与 config.rs ChannelsConfig 字段一一对应 */
export const APP_CONFIG_CHANNELS_DTO_FIELDS = [
  'status_broadcast_capacity',
  'restart_broadcast_capacity',
  'event_broadcast_capacity',
  'pty_subscription_capacity',
  'global_queue_capacity',
  'global_queue_max_bytes',
  'ws_event_capacity',
  'lifecycle_capacity',
] as const

export interface AppConfigTerminalFixture {
  default_cols: number
  default_rows: number
  flush_interval_ms: number
  merge_output: boolean
  max_buffer_size: number
  read_buffer_size: number
}

/** 与 config.rs TerminalConfig 字段一一对应 */
export const APP_CONFIG_TERMINAL_DTO_FIELDS = [
  'default_cols',
  'default_rows',
  'flush_interval_ms',
  'merge_output',
  'max_buffer_size',
  'read_buffer_size',
] as const

export interface AppConfigLogFixture {
  file_level: string
  console_filter: string
  rotation: string
  max_files: number
  console_in_release: boolean
}

/** 与 config.rs LogConfig 字段一一对应 */
export const APP_CONFIG_LOG_DTO_FIELDS = [
  'file_level',
  'console_filter',
  'rotation',
  'max_files',
  'console_in_release',
] as const

export interface AppConfigFixture {
  network: AppConfigNetworkFixture
  session: AppConfigSessionFixture
  ui: AppConfigUiFixture
  channels: AppConfigChannelsFixture
  terminal: AppConfigTerminalFixture
  log: AppConfigLogFixture
}

/** 与 config.rs AppConfig 顶层六小节一一对应 */
export const APP_CONFIG_DTO_FIELDS = [
  'network',
  'session',
  'ui',
  'channels',
  'terminal',
  'log',
] as const

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
    session: {
      default_environment: 'windows',
      default_wsl_distro: null,
      default_working_dir: null,
      default_command: 'claude',
      session_timeout: 3600,
    },
    ui: {
      theme: 'system',
      theme_palette: 'warm',
      font_size: 12,
      terminal_font_size: 12,
      terminal_font_family: 'Consolas',
      terminal_theme: 'dracula',
      show_preview: true,
      language: 'zh-CN',
      terminal_bg_image: null,
      terminal_bg_opacity: 30,
    },
    channels: {
      status_broadcast_capacity: 64,
      restart_broadcast_capacity: 64,
      event_broadcast_capacity: 256,
      pty_subscription_capacity: 1024,
      global_queue_capacity: 25000,
      global_queue_max_bytes: 128 * 1024 * 1024,
      ws_event_capacity: 1024,
      lifecycle_capacity: 16,
    },
    terminal: {
      default_cols: 120,
      default_rows: 40,
      flush_interval_ms: 30,
      merge_output: true,
      max_buffer_size: 64 * 1024,
      read_buffer_size: 4096,
    },
    log: {
      file_level: 'info',
      console_filter: 'bedcode_lib=debug,actix_web=info,actix_http=info',
      rotation: 'daily',
      max_files: 7,
      console_in_release: false,
    },
    ...overrides,
  }
  // 顶层与各嵌套小节键集合分别断言：任一节漂移即失败
  assertDtoFields(fixture, APP_CONFIG_DTO_FIELDS, 'AppConfig')
  assertDtoFields(fixture.network, APP_CONFIG_NETWORK_DTO_FIELDS, 'AppConfig.network')
  assertDtoFields(fixture.session, APP_CONFIG_SESSION_DTO_FIELDS, 'AppConfig.session')
  assertDtoFields(fixture.ui, APP_CONFIG_UI_DTO_FIELDS, 'AppConfig.ui')
  assertDtoFields(fixture.channels, APP_CONFIG_CHANNELS_DTO_FIELDS, 'AppConfig.channels')
  assertDtoFields(fixture.terminal, APP_CONFIG_TERMINAL_DTO_FIELDS, 'AppConfig.terminal')
  assertDtoFields(fixture.log, APP_CONFIG_LOG_DTO_FIELDS, 'AppConfig.log')
  return fixture
}
