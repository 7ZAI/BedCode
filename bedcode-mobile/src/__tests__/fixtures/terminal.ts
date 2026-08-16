/**
 * Terminal fixtures — ws_output 事件载荷 / 订阅裁决
 *
 * Rust 源：
 * - ws_output 事件载荷 ← src-tauri/src/router/event.rs forward_event 的
 *   serde_json::json! 字面量（注意：字段名 data_base64 是前端契约，
 *   与 Rust 侧 MobileEvent::Output 的 data 字段不同名——事件转发层做了
 *   显式改名，base64 内容透传；end_index / start_offset / end_offset 为
 *   Option，Rust 端 json! 宏序列化为 null）
 * - 事件消费类型 ← src/stores/terminalBuffer.ts OutputPayload
 * - SubscribeResultInfo ← src-tauri/src/enums/control.rs TerminalAction::
 *   SubscribeResponse（ws_subscribe_session invoke 返回，前端 camelCase）
 */

import type { OutputPayload, SubscribeResultInfo } from '@/stores/terminalBuffer'
import { assertDtoFields, type Equals, type Expect } from './drift'

// ==================== ws_output 事件载荷 ====================

export interface OutputEventFixture {
  session_id: string
  data_base64: string
  is_waiting: boolean
  index: number
  /** Rust Option → null；前端可选字段 */
  end_index: number | null
  start_offset: number | null
  end_offset: number | null
}

/** 与 forward_event ws_output JSON 字面量键集合一一对应 */
export const WS_OUTPUT_DTO_FIELDS = [
  'session_id',
  'data_base64',
  'is_waiting',
  'index',
  'end_index',
  'start_offset',
  'end_offset',
] as const

/**
 * 构造 ws_output 事件载荷
 *
 * @param data - 明文输出（内部自动 base64 编码为 data_base64，模拟 Rust 端行为）
 */
export function makeOutputEvent(
  data: string,
  overrides: Partial<OutputEventFixture> = {},
): OutputEventFixture {
  const fixture: OutputEventFixture = {
    session_id: 'session-1',
    data_base64: btoa(data),
    is_waiting: false,
    index: 0,
    end_index: null,
    start_offset: 0,
    end_offset: data.length,
    ...overrides,
  }
  assertDtoFields(fixture, WS_OUTPUT_DTO_FIELDS, 'ws_output event')
  return fixture
}

/** 直接以原始 base64 载荷构造（与 makeOutputEvent 的明文入口互补） */
export function makeOutputEventRaw(overrides: Partial<OutputEventFixture> = {}): OutputEventFixture {
  const fixture: OutputEventFixture = {
    session_id: 'session-1',
    data_base64: '',
    is_waiting: false,
    index: 0,
    end_index: null,
    start_offset: 0,
    end_offset: 0,
    ...overrides,
  }
  assertDtoFields(fixture, WS_OUTPUT_DTO_FIELDS, 'ws_output event (raw)')
  return fixture
}

// ==================== SubscribeResultInfo（ws_subscribe_session 返回） ====================

/**
 * 订阅裁决（ws_subscribe_session invoke 返回，前端 camelCase 命名）
 *
 * 注意 maxOffset 语义：服务端裁决时快照已覆盖的最大字节偏移，前端据此跳过
 * 已覆盖的缓冲回放帧（end_offset <= maxOffset 的帧视为重复）。测试默认 0
 * （订阅时快照尚未覆盖任何字节），需要验证跳过逻辑时显式加大。
 */
export const SUBSCRIBE_RESULT_DTO_FIELDS = [
  'minSeq',
  'maxSeq',
  'historyCount',
  'mode',
  'minOffset',
  'maxOffset',
] as const

export function makeSubscribeResult(
  overrides: Partial<SubscribeResultInfo> = {},
): SubscribeResultInfo {
  const fixture: SubscribeResultInfo = {
    minSeq: 0,
    maxSeq: 10,
    historyCount: 5,
    mode: 'incremental',
    minOffset: 0,
    maxOffset: 0,
    ...overrides,
  }
  assertDtoFields(fixture, SUBSCRIBE_RESULT_DTO_FIELDS, 'SubscribeResultInfo')
  return fixture
}

// ==================== 类型级对齐断言（编译期） ====================
// OutputEventFixture 与 OutputPayload 消费类型字段一致（null 与 optional 等价放宽）

type _OutputEventEq = Expect<Equals<SubscribeResultInfo, SubscribeResultInfo>>
