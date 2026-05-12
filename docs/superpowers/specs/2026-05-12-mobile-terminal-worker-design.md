# 移动端终端全局缓冲区架构设计

## 背景

- 当前移动端终端通过 WebSocket 直接接收后端推送的输出数据
- 退出终端页面后数据不再接收（因为取消了订阅）
- 需要实现：退出终端页面后继续在后台接收数据，返回时可查看历史
- **优化目标**：移除复杂的 Web Worker 架构，在 Vue 层统一管理所有会话的 buffer

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
                    │                    Vue 前端 │                      │
                    │  ┌─────────────────────────────────────────────┐ │
                    │  │            sessionBuffers (全局 Map)        │ │
                    │  │  Map<sessionId, {                           │ │
                    │  │    buffer: string[],     // 存储原始数据    │ │
                    │  │    xterm: Terminal,      // xterm 实例      │ │
                    │  │    lastIndex: number,    // 最后写入索引    │ │
                    │  │  }>                                         │ │
                    │  └─────────────────────────────────────────────┘ │
                    │                             ▲                     │
                    │                             │ 直接写入             │
                    └─────────────────────────────┼─────────────────────┘
                                          WebSocket 消息处理
                                                  │
                    ┌─────────────────────────────┼─────────────────────┐
                    ▼                             ▼                     ▼
           ┌────────────────┐          ┌────────────────┐     ┌────────────────┐
           │ TerminalView   │          │ TerminalView   │     │ TerminalView   │
           │ (会话1)        │          │ (会话2)        │     │ (会话N)        │
           └────────────────┘          └────────────────┘     └────────────────┘
```

## 架构对比

| 对比项 | 原设计 (Worker) | 新设计 (全局 Buffer) |
|-------|----------------|---------------------|
| Buffer 位置 | Worker 内存 + xterm 内存 | 统一在 Vue 全局状态 |
| 架构复杂度 | Web Worker 封装 | 直接使用 Vue composable |
| 数据流 | postMessage 序列化 | 直接内存引用 |
| 维护成本 | 需要同步 Worker 状态 | 单一数据源 |
| 开发复杂度 | 需要处理消息序列化 | 常规 Vue 开发模式 |

## 后端改动 (Rust)

### 消息分发逻辑

保持按 session_id 分发（与原设计一致）：

```rust
// 按 session_id 分发消息
for (addr, client) in clients.iter() {
    if client.authenticated && client.subscribed_sessions.contains(&session_id) {
        // 只发送给订阅了该 session_id 的客户端
        if let Err(e) = client.sender.send(msg.clone()) {
            // 处理发送失败
        }
    }
}
```

**无需为每个会话创建独立的 Worker 任务**，后端保持现有的单线程事件循环即可。

## 前端设计

### 全局状态管理 (Composable)

```typescript
// composables/useSessionBuffers.ts
import { ref, shallowRef } from 'vue'
import type { Terminal } from '@xterm/xterm'

export interface SessionBuffer {
  buffer: string[]           // 原始数据存储
  lastIndex: number          // 最后写入位置
  xterm: Terminal | null     // xterm 实例引用
  subscribers: Set<string>   // 活跃订阅者（页面实例ID）
}

// 全局缓冲区 Map
const sessionBuffers = new Map<string, SessionBuffer>()

export function useSessionBuffers() {
  /**
   * 注册会话 buffer
   * 页面首次进入时调用
   */
  function register(sessionId: string) {
    if (!sessionBuffers.has(sessionId)) {
      sessionBuffers.set(sessionId, {
        buffer: [],
        lastIndex: 0,
        xterm: null,
        subscribers: new Set()
      })
    }
    const buf = sessionBuffers.get(sessionId)!
    buf.subscribers.add(generateSubscriberId())
    return buf
  }

  /**
   * 注销会话 buffer
   * 页面完全退出时调用（无任何订阅者）
   */
  function unregister(sessionId: string) {
    const buf = sessionBuffers.get(sessionId)
    if (buf) {
      buf.subscribers.delete(generateSubscriberId())
      if (buf.subscribers.size === 0) {
        // 可选：清空 buffer 或保留部分历史
        // 保留最近 1000 行历史
        if (buf.buffer.length > 1000) {
          buf.buffer = buf.buffer.slice(-1000)
        }
      }
    }
  }

  /**
   * 写入数据到 buffer
   * WebSocket 收到数据时调用
   */
  function append(sessionId: string, data: string) {
    const buf = sessionBuffers.get(sessionId)
    if (!buf) return

    buf.buffer.push(data)
    buf.lastIndex++

    // 写入 xterm（如已初始化）
    if (buf.xterm) {
      buf.xterm.write(data)
    }
  }

  /**
   * 注册 xterm 实例
   * TerminalView 初始化时调用
   */
  function attachXterm(sessionId: string, terminal: Terminal) {
    const buf = sessionBuffers.get(sessionId)
    if (!buf) return

    buf.xterm = terminal

    // 将历史数据写入 xterm
    buf.buffer.forEach(data => terminal.write(data))
  }

  /**
   * 解除 xterm 实例
   * TerminalView 销毁时调用
   */
  function detachXterm(sessionId: string) {
    const buf = sessionBuffers.get(sessionId)
    if (buf) {
      buf.xterm = null
    }
  }

  /**
   * 获取历史数据
   */
  function getHistory(sessionId: string, fromIndex: number = 0): string[] {
    const buf = sessionBuffers.get(sessionId)
    if (!buf) return []
    return buf.buffer.slice(fromIndex)
  }

  /**
   * 获取 buffer 状态
   */
  function getStatus(sessionId: string) {
    const buf = sessionBuffers.get(sessionId)
    if (!buf) return null
    return {
      length: buf.buffer.length,
      lastIndex: buf.lastIndex,
      hasXterm: !!buf.xterm
    }
  }

  return {
    sessionBuffers,
    register,
    unregister,
    append,
    attachXterm,
    detachXterm,
    getHistory,
    getStatus
  }
}

// 生成唯一订阅者ID
function generateSubscriberId() {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`
}
```

### WebSocket 管理 (Composable)

```typescript
// composables/useTerminalWs.ts
import { ref, onUnmounted } from 'vue'
import { useSessionBuffers } from './useSessionBuffers'

export function useTerminalWs() {
  const connected = ref(false)
  const wsRef = ref<WebSocket | null>(null)
  const { append, register, unregister } = useSessionBuffers()

  let ws: WebSocket | null = null

  /**
   * 建立 WebSocket 连接
   * 使用 Tauri 的 WebSocket 或原生 WebSocket
   */
  async function connect() {
    if (ws && ws.readyState === WebSocket.OPEN) return

    // 获取 WebSocket URL
    const wsUrl = await getWebSocketUrl()

    ws = new WebSocket(wsUrl)
    ws.binaryType = 'arraybuffer'

    ws.onopen = () => {
      connected.value = true
      // 认证
      ws?.send(JSON.stringify({ type: 'auth', token: getAuthToken() }))
    }

    ws.onmessage = (event) => {
      // 处理后端推送的消息
      const msg = parseMessage(event.data)
      if (msg.type === 'pty-output') {
        append(msg.sessionId, msg.data)
      }
    }

    ws.onclose = () => {
      connected.value = false
      // 自动重连
      setTimeout(connect, 3000)
    }

    ws.onerror = (error) => {
      console.error('WebSocket error:', error)
    }

    wsRef.value = ws
  }

  /**
   * 订阅会话
   */
  function subscribe(sessionId: string) {
    register(sessionId)
    send({ type: 'subscribe', sessionId })
  }

  /**
   * 取消订阅
   */
  function unsubscribe(sessionId: string) {
    unregister(sessionId)
    send({ type: 'unsubscribe', sessionId })
  }

  /**
   * 发送消息到后端
   */
  function send(data: any) {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(data))
    }
  }

  /**
   * 发送终端输入
   */
  function sendInput(sessionId: string, data: string) {
    send({ type: 'pty-input', sessionId, data })
  }

  /**
   * 调整终端大小
   */
  function resize(sessionId: string, cols: number, rows: number) {
    send({ type: 'pty-resize', sessionId, cols, rows })
  }

  onUnmounted(() => {
    if (ws) {
      ws.close()
    }
  })

  return {
    connected,
    connect,
    subscribe,
    unsubscribe,
    sendInput,
    resize
  }
}
```

### TerminalView 使用示例

```vue
<!-- views/mobile/TerminalView.vue -->
<template>
  <div class="terminal-container" ref="containerRef">
    <MobileTerminal
      :session-id="sessionId"
      @input="handleInput"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import MobileTerminal from '@/components/mobile/MobileTerminal.vue'
import { useTerminalWs } from '@/composables/useTerminalWs'
import { useSessionBuffers } from '@/composables/useSessionBuffers'

const props = defineProps<{
  sessionId: string
}>()

const containerRef = ref<HTMLElement>()
const { connected, connect, subscribe, unsubscribe, sendInput, resize } = useTerminalWs()
const { attachXterm, detachXterm, getHistory } = useSessionBuffers()

// 页面实例ID
const pageId = ref('')

onMounted(async () => {
  pageId.value = `${props.sessionId}-${Date.now()}`

  // 建立连接并订阅
  await connect()
  subscribe(props.sessionId)

  // 等待 MobileTerminal 初始化完成
  // 通过事件或 composable 获取 xterm 实例
})

onUnmounted(() => {
  // 取消订阅但不关闭连接
  unsubscribe(props.sessionId)
})

function handleInput(data: string) {
  sendInput(props.sessionId, data)
}

function handleResize(cols: number, rows: number) {
  resize(props.sessionId, cols, rows)
}
</script>
```

### MobileTerminal 组件

```typescript
// components/mobile/MobileTerminal.vue
<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { useSessionBuffers } from '@/composables/useSessionBuffers'

const props = defineProps<{
  sessionId: string
}>()

const emit = defineEmits<{
  input: [data: string]
}>()

const terminalRef = ref<HTMLElement>()
let terminal: Terminal | null = null
let fitAddon: FitAddon | null = null

const { attachXterm, detachXterm, getHistory } = useSessionBuffers()

onMounted(() => {
  // 初始化 xterm
  terminal = new Terminal({
    cursorBlink: true,
    fontSize: 14,
    fontFamily: 'monospace',
    theme: {
      background: '#1a1a1a',
      foreground: '#ffffff'
    }
  })

  fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)

  terminal.open(terminalRef.value!)
  fitAddon.fit()

  // 注册到全局 buffer
  attachXterm(props.sessionId, terminal)

  // 处理输入
  terminal.onData(data => {
    emit('input', data)
  })

  // 处理 resize
  terminal.onResize(({ cols, rows }) => {
    // 通知后端和 WebSocket 管理器
  })
})

onUnmounted(() => {
  detachXterm(props.sessionId)
  terminal?.dispose()
})

// 窗口大小变化时自适应
watch(() => props.sessionId, () => {
  fitAddon?.fit()
})
</script>

<template>
  <div ref="terminalRef" class="terminal" />
</template>

<style scoped>
.terminal {
  width: 100%;
  height: 100%;
}
</style>
```

## 通信协议

### 后端 → 前端 (WebSocket)

```typescript
// PTY 输出
{
  type: 'pty-output',
  sessionId: 'xxx',
  data: 'xxx',           // ANSI 转义后的字符串
  timestamp: 1234567890
}

// 会话状态变化
{
  type: 'session-status',
  sessionId: 'xxx',
  status: 'Running' | 'WaitingInput' | 'Stopped'
}

// 错误信息
{
  type: 'error',
  sessionId: 'xxx',
  message: 'xxx'
}
```

### 前端 → 后端 (WebSocket)

```typescript
// 订阅会话
{
  type: 'subscribe',
  sessionId: 'xxx'
}

// 取消订阅
{
  type: 'unsubscribe',
  sessionId: 'xxx'
}

// 发送输入
{
  type: 'pty-input',
  sessionId: 'xxx',
  data: 'xxx'
}

// 调整终端大小
{
  type: 'pty-resize',
  sessionId: 'xxx',
  cols: 80,
  rows: 24
}

// 认证
{
  type: 'auth',
  token: 'xxx'
}
```

## 关键设计决策

| 决策项 | 方案 | 理由 |
|-------|------|------|
| Buffer 位置 | Vue 全局 Map | 单一数据源，避免重复 |
| xterm 实例管理 | 分离注册/销毁 | 页面退出不丢失数据 |
| WebSocket | 全局单例 | 复用连接，减少资源 |
| 数据保留 | 1000 行历史 | 平衡内存和历史需求 |
| 后台接收 | visibilitychange | 暂停渲染省性能 |

## 改动文件清单

### 后端 (最小改动)

- `src-tauri/src/websocket/` - 确认按 session_id 分发逻辑（可能已实现）

### 前端

- `src/composables/useSessionBuffers.ts` - 新建：全局 Buffer 管理
- `src/composables/useTerminalWs.ts` - 新建：WebSocket 连接管理
- `src/views/mobile/TerminalView.vue` - 修改：接入新的 composable
- `src/components/mobile/MobileTerminal.vue` - 修改：注册到全局 buffer

## 潜在问题与解决方案

### 1. 内存无限增长

**问题**：多会话时 buffer 可能很大

**解决**：
- 限制每个 session 的 buffer 大小（1000 行）
- 页面完全退出后可以进一步缩减

### 2. 页面退出后数据丢失

**问题**：刷新页面会清空 Vue 状态

**解决**：
- 短期：提示用户刷新会导致历史丢失
- 长期：可考虑持久化到 localStorage 或 IndexDB

### 3. 多页面同时打开同一会话

**问题**：两个 TerminalView 同时显示同一会话

**解决**：
- 使用 subscribers Set 追踪活跃页面
- 多个页面时共享同一 xterm 实例引用（需要设计）
- 或限制为同一会话只能打开一个页面

## 实施步骤

1. **Phase 1 - 基础架构**
   - 创建 useSessionBuffers composable
   - 实现 WebSocket 连接和消息收发
   - 实现基本的 subscribe/unsubscribe

2. **Phase 2 - 数据管理**
   - 实现全局 buffer 存储
   - 实现历史数据查询
   - 实现页面注册/注销

3. **Phase 3 - 集成**
   - 修改 TerminalView 接入
   - 修改 MobileTerminal 组件
   - 测试多会话场景

4. **Phase 4 - 优化**
   - 添加 visibilitychange 处理
   - 优化内存使用
   - 添加错误处理和重连