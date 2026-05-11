# 移动端终端 Web Worker 架构设计

## 背景

- 当前移动端终端通过 WebSocket 直接接收后端推送的输出数据
- 退出终端页面后数据不再接收（因为取消了订阅）
- 需要实现：退出终端页面后继续在后台接收数据，返回时可查看历史

## 整体架构

```
┌──────────────────────────────────────────────────────────────────┐
│                         Rust 后端                                 │
│  ┌────────────┐    ┌─────────────┐    ┌────────────────────┐    │
│  │  PTY进程   │───▶│Broadcast    │───▶│ WebSocket Server   │    │
│  │  (多会话)  │    │ Channel     │    │ 按session_id分发   │    │
│  └────────────┘    └─────────────┘    └─────────┬──────────┘    │
└─────────────────────────────────────────────────┼────────────────┘
                                                  │
                                           WebSocket
                                                  │
                    ┌─────────────────────────────┼─────────────────────┐
                    ▼                             ▼                     ▼
           ┌────────────────┐          ┌────────────────┐     ┌────────────────┐
           │  SessionWorker │          │  SessionWorker │     │  SessionWorker │
           │  (全局单例)     │          │  (全局单例)     │     │  (全局单例)     │
           │  ┌──────────┐  │          │  ┌──────────┐  │     │  ┌──────────┐  │
           │  │WebSocket │  │          │  │WebSocket │  │     │  │WebSocket │  │
           │  │连接管理  │  │          │  │连接管理  │  │     │  │连接管理  │  │
           │  ├──────────┤  │          │  ├──────────┤  │     │  ├──────────┤  │
           │  │Buffer    │  │          │  │Buffer    │  │     │  │Buffer    │  │
           │  │Map       │  │          │  │Map       │  │     │  │Map       │  │
           │  └──────────┘  │          │  └──────────┘  │     │  └──────────┘  │
           └────────┬────────┘          └────────┬────────┘     └────────┬────────┘
                    │                             │                     │
                    │      postMessage            │                     │
                    ▼                             ▼                     ▼
           ┌────────────────┐          ┌────────────────┐     ┌────────────────┐
           │ TerminalView   │          │ TerminalView   │     │ TerminalView   │
           │ (会话1)        │          │ (会话2)        │     │ (会话N)        │
           └────────────────┘          └────────────────┘     └────────────────┘
```

## 后端改动 (Rust)

### 消息分发逻辑修改

当前按客户端地址分发，改为按 session_id 分发：

```rust
// 当前：按客户端地址分发
for (addr, client) in clients.iter() {
    if client.authenticated {
        // 发送给所有认证客户端
    }
}

// 改为：按 session_id 分发
for (addr, client) in clients.iter() {
    if client.authenticated && client.subscribed_sessions.contains(&session_id) {
        // 只发送给订阅了该 session_id 的客户端
    }
}
```

## 前端 Worker 设计

### SessionWorker 职责

1. 维护全局 WebSocket 连接（与后端持久连接）
2. 按 session_id 缓存输出数据
3. 处理页面的订阅/历史请求
4. 页面退出后继续在后台接收数据

### Worker 内部数据结构

```typescript
// Worker 全局状态
const state = {
  ws: WebSocket | null,
  connected: false,
  // 按 session_id 存储数据
  buffers: new Map<string, string[]>(),
  subscribers: new Set<string>(),  // 当前活动的页面
}
```

### 通信协议

#### 页面 → Worker

```typescript
// 订阅会话（页面进入时调用）
{ type: 'subscribe', sessionId: 'xxx' }

// 取消订阅（页面退出时调用，但不停止接收数据）
{ type: 'unsubscribe', sessionId: 'xxx' }

// 获取历史数据
{ type: 'getHistory', sessionId: 'xxx', fromIndex: 0 }

// 获取缓冲区状态
{ type: 'getStatus', sessionId: 'xxx' }
```

#### Worker → 页面

```typescript
// 实时输出数据
{ type: 'output', sessionId: 'xxx', data: 'xxx', index: 100 }

// 历史数据响应
{ type: 'history', sessionId: 'xxx', data: ['xxx', ...] }

// 连接状态变化
{ type: 'status', connected: true }

// 错误信息
{ type: 'error', message: 'xxx' }
```

### 页面与 Worker 交互

```typescript
// TerminalView.vue
const worker = getSessionWorker() // 全局单例

// 页面进入时订阅
onMounted(async () => {
  await worker.subscribe(sessionId)

  // 获取历史数据写入 xterm
  const history = await worker.getHistory(sessionId)
  history.forEach(data => terminal.write(data))
})

// 页面退出时取消订阅（但 Worker 继续接收）
onUnmounted(() => {
  worker.unsubscribe(sessionId)
  // 不关闭 Worker，不取消会话订阅
})
```

## 关键设计决策

| 决策项 | 方案 | 理由 |
|-------|------|------|
| Worker 数量 | 1 个全局 Worker | 简化管理，复用连接 |
| 数据存储位置 | Worker 内存 | 符合 scrollback 理念 |
| 页面退出行为 | 不取消订阅 | 保持后台接收 |
| 后台渲染 | visibilitychange | 暂停 xterm 写入省性能 |
| 数据上限 | xterm scrollback | 让 xterm 自己管理 |

## 改动文件清单

### 后端

- `src-tauri/src/websocket/output_forwarder.rs` - 按 session_id 过滤

### 前端

- `src/workers/session.worker.ts` - 新建 Worker
- `src/composables/useSessionWorker.ts` - 新建 Worker 封装
- `src/views/mobile/TerminalView.vue` - 改为从 Worker 获取数据
- `src/components/mobile/MobileTerminal.vue` - 移除 output prop，改为直接写

## 潜在问题与解决方案

### 1. Worker 消息传递开销

**问题**：大量数据时 postMessage 有性能损耗

**解决**：批量发送或使用 SharedArrayBuffer

### 2. 浏览器后台限制

**问题**：visibilitychange 后 WebSocket 可能断开

**解决**：Tauri 可用 native WebSocket，不受浏览器限制

### 3. 内存无限增长

**问题**：多会话时 buffer 可能很大

**解决**：限制每个 session 的 buffer 大小

## 实施步骤

1. **Phase 1 - 基础架构**
   - 创建 SessionWorker
   - 实现 WebSocket 连接管理
   - 实现基本的消息收发

2. **Phase 2 - 数据管理**
   - 实现按 session_id 缓存数据
   - 实现历史数据查询
   - 实现页面订阅/取消订阅

3. **Phase 3 - 优化**
   - 添加 visibilitychange 处理
   - 优化内存使用
   - 添加错误处理和重连