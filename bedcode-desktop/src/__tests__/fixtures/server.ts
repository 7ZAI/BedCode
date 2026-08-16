/**
 * Server fixtures — 服务器状态 / 网络配置 / 性能指标
 *
 * Rust DTO 源（serde 默认 snake_case，字段名与 Rust 结构体逐一对应）：
 * - ServerStatusInfo ← src-tauri/src/server/supervisor.rs（get_server_status 返回；
 *   `uptime_secs: Option<u64>` 无 skip_serializing_if，恒序列化，未启动时为 null）
 * - NetworkConfig     ← src-tauri/src/system/config.rs（get/get/reset_server_network_config；
 *   全部字段带 serde default，序列化时恒出现）
 * - ServerMetrics     ← src-tauri/src/server/metrics.rs（get_server_metrics 返回）
 *
 * 命名规则：serde 默认 snake_case；数值宽度（u16/usize/u64/f64）统一映射为 number。
 * 对齐机制：DTO_FIELDS 清单 + 工厂内 assertDtoFields 运行时断言 + 类型级 Equals。
 */

import type { ServerStatusInfo, NetworkConfig, ServerMetrics, ServerStatus } from '@/composables/useServer'
import { assertDtoFields, type Equals, type Expect } from './drift'

// ==================== ServerStatusInfo ====================

export interface ServerStatusInfoFixture {
  status: ServerStatus
  port: number
  auto_start: boolean
  local_ips: string[]
  /** 运行时长（秒）；服务器从未启动时为 null */
  uptime_secs: number | null
}

/** 与 supervisor.rs ServerStatusInfo 字段一一对应 */
export const SERVER_STATUS_INFO_DTO_FIELDS = [
  'status',
  'port',
  'auto_start',
  'local_ips',
  'uptime_secs',
] as const

export function makeServerStatusInfo(overrides: Partial<ServerStatusInfoFixture> = {}): ServerStatusInfoFixture {
  const fixture: ServerStatusInfoFixture = {
    status: 'running',
    port: 9000,
    auto_start: false,
    local_ips: ['192.168.1.5', '127.0.0.1'],
    uptime_secs: null,
    ...overrides,
  }
  assertDtoFields(fixture, SERVER_STATUS_INFO_DTO_FIELDS, 'ServerStatusInfo')
  return fixture
}

// ==================== NetworkConfig ====================

export interface NetworkConfigFixture {
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

/** 与 config.rs NetworkConfig 字段一一对应 */
export const NETWORK_CONFIG_DTO_FIELDS = [
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

export function makeNetworkConfig(overrides: Partial<NetworkConfigFixture> = {}): NetworkConfigFixture {
  const fixture: NetworkConfigFixture = {
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
    ...overrides,
  }
  assertDtoFields(fixture, NETWORK_CONFIG_DTO_FIELDS, 'NetworkConfig')
  return fixture
}

// ==================== ServerMetrics ====================

export interface ServerMetricsFixture {
  uptime_secs: number
  connections: number
  total_http_requests: number
  http_requests_per_sec: number
  ws_messages_sent: number
  ws_messages_received: number
  ws_sent_rate: number
  ws_recv_rate: number
  cpu_usage_percent: number
  memory_usage_bytes: number
}

/** 与 metrics.rs ServerMetrics 字段一一对应 */
export const SERVER_METRICS_DTO_FIELDS = [
  'uptime_secs',
  'connections',
  'total_http_requests',
  'http_requests_per_sec',
  'ws_messages_sent',
  'ws_messages_received',
  'ws_sent_rate',
  'ws_recv_rate',
  'cpu_usage_percent',
  'memory_usage_bytes',
] as const

export function makeServerMetrics(overrides: Partial<ServerMetricsFixture> = {}): ServerMetricsFixture {
  const fixture: ServerMetricsFixture = {
    uptime_secs: 120,
    connections: 3,
    total_http_requests: 1000,
    http_requests_per_sec: 5,
    ws_messages_sent: 500,
    ws_messages_received: 400,
    ws_sent_rate: 1.5,
    ws_recv_rate: 2.5,
    cpu_usage_percent: 10,
    memory_usage_bytes: 1024,
    ...overrides,
  }
  assertDtoFields(fixture, SERVER_METRICS_DTO_FIELDS, 'ServerMetrics')
  return fixture
}

// ==================== 类型级对齐断言（编译期，防字段漂移） ====================
// fixture 类型与前端消费类型字段集合必须完全一致；前端类型缺/多字段时报编译错误。
// 注意：ServerStatusInfo 的 uptime_secs 曾缺失（Rust 恒序列化），已在此同步。

type _ServerStatusInfoEq = Expect<Equals<ServerStatusInfoFixture, ServerStatusInfo>>
type _NetworkConfigEq = Expect<Equals<NetworkConfigFixture, NetworkConfig>>
type _ServerMetricsEq = Expect<Equals<ServerMetricsFixture, ServerMetrics>>
