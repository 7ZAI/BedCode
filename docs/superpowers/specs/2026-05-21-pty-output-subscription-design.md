# PTY 输出订阅消费机制设计

**日期**: 2026-05-21
**状态**: 已批准

## 1. 背景与目标

设计一个 PTY 输出订阅消费机制，满足"持久化订阅 + 实时广播"的经典场景：

- 新订阅者能从过去某一时刻开始连续消费
- 全程不重复、不丢失
- 支持客户端指定起始位置

### 核心需求

1. **多客户端独立订阅**：每个客户端独立消费，互不干扰
2. **历史消息回放**：新订阅者可获取历史输出（默认从头补完）
3. **实时消息推送**：订阅后只接收新消息
4. **不重不丢**：历史部分不重复，实时部分不跳过

## 2. 核心架构

```
┌─────────────────────────────────────────────────────────────────┐
│                      PtyOutputManager                           │
│  ┌─────────────────┐    ┌─────────────────────────────────────┐ │
│  │  RingBuffer     │    │     Broadcast Channel               │ │
│  │  [msg 0]        │    │     (实时推送新消息)                 │ │
│  │  [msg 1]        │───▶│  ┌──▶ Client A (从 seq=5000 开始)    │ │
│  │  ...            │    │  ├──▶ Client B (从 seq=0 开始)       │ │
│  │  [msg 9999]     │    │  └──▶ Client C (从头开始)            │ │
│  │                 │    │                                      │ │
│  │  max_seq: 10000 │    │     每个订阅者独立接收               │ │
│  └─────────────────┘    └─────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

## 3. 核心组件

### 3.1 全局序号 (seq)

每条消息绑定一个全局递增的序号 `u64`，确保消息有序且可追溯。

序号由 `pty_process.rs` 中的 `next_output_index()` 函数生成，需确认其原子性和全局唯一性。

### 3.2 环形缓冲区 (RingBuffer)

```rust
pub struct OutputRingBuffer {
    buffer: Vec<Option<PtyOutputEvent>>,
    capacity: usize,
    head: usize,                      // 指向最旧消息
    count: usize,                     // 当前消息数
    max_seq: AtomicU64,               // 当前最大序号
    total_produced: AtomicU64,        // 历史总消息数
}
```

**关键操作**：

| 操作 | 复杂度 | 说明 |
|------|--------|------|
| `push(event)` | O(1) | 写入新消息，环覆盖 |
| `get_since(seq)` | O(n) | 返回 seq 之后的所有消息 |
| `clear()` | O(1) | 清空缓冲区 |

### 3.3 订阅状态

```rust
pub struct Subscription {
    client_id: String,
    session_id: String,
    start_seq: u64,              // 客户端指定起始序号
    subscribed_at: i64,
    active: bool,
}
```

### 3.4 订阅管理器

```rust
pub struct PtySubscriptionManager {
    sessions: RwLock<HashMap<String, Arc<PtySessionSubscriptions>>>,
}

struct PtySessionSubscriptions {
    session_id: String,
    ring_buffer: Arc<OutputRingBuffer>,
    broadcast_tx: broadcast::Sender<PtyOutputEvent>,
    subscriptions: RwLock<HashMap<String, Subscription>>,
}
```

## 4. 订阅流程

```
客户端认证成功 (on_authenticated)
        │
        ▼
┌──────────────────────────────────────┐
│ 1. 获取当前 ring_buffer 状态         │
│    - max_seq = atomic_load()         │
│    - total_produced                  │
└──────────────────────────────────────┘
        │
        ▼
┌──────────────────────────────────────┐
│ 2. 创建订阅                           │
│    - 读取客户端 start_seq 参数       │
│    - start_seq = None → 从 0 开始    │
│    - 注册到 subscriptions            │
└──────────────────────────────────────┘
        │
        ▼
┌──────────────────────────────────────┐
│ 3. 发送历史消息（异步，不阻塞）       │
│    for msg in history {              │
│        if msg.index >= start_seq {   │
│            send_to_client(msg)       │
│        }                             │
│    }                                 │
└──────────────────────────────────────┘
        │
        ▼
┌──────────────────────────────────────┐
│ 4. 订阅实时广播                       │
│    - 接收 broadcast::Receiver        │
│    - 过滤：msg.index >= start_seq    │
│    - 发送实时消息                     │
└──────────────────────────────────────┘
```

## 5. 客户端接口

### 5.1 订阅请求

客户端认证成功后，通过消息机制订阅：

```json
{
  "type": "subscribe",
  "session_id": "xxx",
  "start_seq": 5000  // 可选，不指定则从头补完
}
```

### 5.2 订阅响应

```json
{
  "type": "subscribe_response",
  "session_id": "xxx",
  "current_max_seq": 15000,
  "history_count": 10000
}
```

### 5.3 行为规则

| 场景 | 行为 |
|------|------|
| `start_seq = null` | 从缓冲区最早消息开始（从头补完） |
| `start_seq = 5000` | 从 seq=5000 开始消费（包含 5000） |
| `start_seq > max_seq` | 只收实时消息（无历史） |

## 6. 与现有代码整合

### 6.1 整合点

| 现有组件 | 整合方式 |
|----------|----------|
| `PtySession.output_tx` | 保留用于实时广播，新增 `OutputRingBuffer` |
| `PtySession.output_cache` | 迁移到 `OutputRingBuffer` |
| `OutputForwarder` | 改为基于 `SubscriptionManager` 的精确推送 |
| `WebSocketManager.on_authenticated()` | 触发订阅创建 |

### 6.2 新增文件

- `src-tauri/src/desktop/pty/subscription.rs` - 订阅管理器实现

### 6.3 修改文件

- `src-tauri/src/desktop/pty/pty_process.rs` - 新增 RingBuffer
- `src-tauri/src/desktop/websocket_manager.rs` - 认证成功后触发订阅

## 7. 性能与资源

| 指标 | 数值 | 说明 |
|------|------|------|
| 缓冲区容量 | 10000 条 | 可配置 |
| 每条消息估算 | ~2KB | 含 base64 数据 |
| 单会话内存 | ~20MB | 10000 × 2KB |
| 历史发送 | 异步 | 不阻塞认证流程 |
| 实时推送 | broadcast | 内置流控，LAGGED 自动丢弃 |

## 8. 异常处理

| 场景 | 处理 |
|------|------|
| 订阅时 session 不存在 | 返回错误 |
| 客户端断线 | 自动取消订阅，清理资源 |
| 实时消息 lag | broadcast LAGGED 事件，消息丢失（客户端可重新订阅） |
| 缓冲区满 | 环覆盖最旧消息 |

## 9. 后续扩展

- [ ] 支持客户端断线重连时指定 `start_seq` 继续消费
- [ ] 支持按会话配置不同缓冲区容量
- [ ] 添加订阅状态查询 API

## 10. 验收标准

1. 新订阅客户端能获取历史消息（默认从头补完）
2. 客户端可指定 `start_seq` 从任意位置开始
3. 实时消息不丢失、不重复
4. 多客户端独立订阅，互不干扰
5. 客户端断线后资源正确释放