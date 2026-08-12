/**
 * 事件名常量契约测试
 *
 * 这些常量与 Rust SDK `constants.rs` 双写同步（单一事实来源），
 * 断言字面量防止两端漂移：宿主 host.emit_event / 消息总线 bus_publish
 * 与插件 context.events.on 共用同一事件名。
 */
import { describe, it, expect } from 'vitest'
import {
  EVENT_TASK_STATUS_CHANGED,
  EVENT_SESSION_MODE_CHANGED,
  EVENT_TASK_QUEUE_CHANGED,
} from '../src/constants'

describe('事件名常量（与 Rust SDK constants.rs 同步）', () => {
  it('任务状态变更事件名', () => {
    expect(EVENT_TASK_STATUS_CHANGED).toBe('task:status-changed')
  })

  it('会话自动授权模式变更事件名', () => {
    expect(EVENT_SESSION_MODE_CHANGED).toBe('session:mode-changed')
  })

  it('任务队列变更事件名', () => {
    expect(EVENT_TASK_QUEUE_CHANGED).toBe('task:queue-changed')
  })
})
