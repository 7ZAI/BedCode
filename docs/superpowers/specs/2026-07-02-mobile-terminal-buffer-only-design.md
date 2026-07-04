# 移动端多会话终端 Buffer-Only 架构设计

**日期**: 2026-07-02
**状态**: 待实现

## 背景

移动端多会话终端当前使用 keep-alive 缓存完整 xterm Terminal 实例。每个缓存实例持有：
- xterm Terminal 对象（~1-2MB scrollback buffer + 内部状态）
- DOM 子树（~200+ 节点，display:none 下仍存在）
- WebGL 渲染器（已优化为停用时释放，但重建时有闪烁）
- ResizeObserver（已优化为停用时断开）

3-5 个会话时内存占用 ~10-15MB，切换时有 WebGL 重建闪烁。核心矛盾：**为不可见的会话维护完整 xterm 实例和 DOM 子树是过度投入**。

## 目标

- 后台会话持续接收数据，切换时即时显示完整历史
- 消除 WebGL 闪烁
- 减少后台会话资源占用（xterm 实例 + DOM → 轻量 JS buffer）
- 简化 TerminalView 生命周期代码

## 方案：Buffer-Only 后台 + 按需创建 xterm

**核心思路**：分离数据接收和渲染。后台会话只维护轻量 JS buffer，不持有 xterm 实例。切换到某会话时，创建 xterm 并一次性写入 buffer 中的历史数据。

### 架构图

```
┌─────────────────────────────────────────┐
│  TerminalBufferStore (Pinia)            │
│  ┌──────────┐ ┌──────────┐ ┌─────────┐ │
│  │ session-a│ │ session-b│ │session-c│ │
│  │ chunks[] │ │ chunks[] │ │chunks[] │ │
│  │ lastIndex│ │ lastIndex│ │lastIndex│ │
│  │ hasGap   │ │ hasGap   │ │hasGap   │ │
│  │subscribed│ │subscribed│ │subscribed│ │
│  └──────────┘ └──────────┘ └─────────┘ │
│  + 全局 ws_output 监听器（1个）          │
└─────────────────────────────────────────┘
         │
    活跃会话：buffer → xterm.write()（实时）
    非活跃会话：buffer 只追加，无 xterm
```

## 数据结构

### SessionBuffer

```typescript
interface SessionBuffer {
  /** 原始输出 chunks，每个是 Uint8Array（解码后的 base64 数据） */
  chunks: Uint8Array[]
  /** 总字节数，用于容量控制 */
  totalBytes: number
  /** 最后接收的输出索引（去重游标，对应 payload.index） */
  lastIndex: number
  /** 最后接收的 end_index（合并消息的结束索引） */
  lastEndIndex: number
  /** buffer 是否有缺口（溢出丢弃或断连期间缺失数据） */
  hasGap: boolean
  /** 该会话是否已向后端订阅 */
  subscribed: boolean
  /** 会话是否已停止（停止后不再接收新数据，但保留 buffer 供查看历史） */
  sessionStopped: boolean
}
```

### 容量策略

- 每会话上限：2MB（与 xterm scrollback 5000 行对齐）
- 溢出时：丢弃最旧 chunks，标记 `hasGap = true`
- 切换到该会话时：
  - `hasGap = false` → 直接从 buffer 写入 xterm（即时，<50ms）
  - `hasGap = true` → `ws_subscribe_session` 从后端补全历史（有网络延迟）

### 为什么 chunks 用 Uint8Array[] 而非单个大数组

- 追加 O(1)（push 新 chunk）
- 丢弃旧数据 O(1)（shift 最旧 chunk）
- 避免频繁内存重分配
- 写入 xterm 时可逐 chunk write，也可合并后一次性 write

## TerminalBufferStore (Pinia)

### 状态

```typescript
const buffers = reactive<Map<string, SessionBuffer>>(new Map())
```

### 核心方法

| 方法 | 说明 |
|------|------|
| `ensureBuffer(sessionId)` | 确保会话有 buffer，不存在则创建 |
| `appendChunk(sessionId, data, index, endIndex)` | 追加输出数据，更新去重游标，容量溢出时丢弃旧 chunks + 标记 hasGap |
| `getBuffer(sessionId)` | 获取会话 buffer |
| `markSubscribed(sessionId)` | 标记已订阅后端 |
| `markUnsubscribed(sessionId)` | 标记未订阅（断连时） |
| `markSessionStopped(sessionId)` | 标记会话停止 |
| `clearBuffer(sessionId)` | 清理会话 buffer（会话删除时） |
| `clearAllBuffers()` | 清理所有 buffer |
| `resubscribeAll()` | 连接恢复时重新订阅所有有 buffer 的活跃会话 |

### 全局 ws_output 监听器

Store 初始化时启动全局 `ws_output` 监听器（与当前 `useTerminalOutput` 相同模式），收到事件后：
1. 解码 `data_base64` → Uint8Array
2. 索引去重：`payload.index <= buffer.lastIndex` 则跳过
3. 追加到对应 session 的 `chunks`
4. 更新 `lastIndex` / `lastEndIndex`
5. 如果有活跃的实时 handler（TerminalView 注册的），同时回调

## TerminalView 生命周期重构

### 新生命周期（移除 keep-alive）

```
onMounted:
  1. initTerminal() — 创建 xterm + FitAddon + WebLinksAddon
  2. 从 buffer 写入历史：遍历 chunks → xterm.write(chunk)
  3. 如果 buffer.hasGap → ws_subscribe_session 补全历史
  4. 注册实时 handler（新数据同时写 buffer + xterm）
  5. 如果 buffer 未订阅后端 → ws_subscribe_session
  6. fitTerminal()
  7. 后台加载 WebGL（不阻塞显示）

onUnmounted:
  1. 注销实时 handler
  2. disposeTerminal() — 释放 xterm + WebGL
  3. 如果会话已停止 → ws_leave_session + 可选清理 buffer
```

### 移除的复杂逻辑

| 移除项 | 原因 |
|--------|------|
| `onActivated` / `onDeactivated` | 不再使用 keep-alive |
| `webglAddonRef` + `loadWebglRenderer` / `disposeWebglRenderer` | WebGL 随 xterm 生命周期自然创建/销毁 |
| `resizeObserverRef` 断开/重连 | 不再需要，组件销毁时自然清理 |
| `isSubscribing` 防重入 | 不再有 onActivated 并发问题 |
| `isActive` + `v-show` | 不再需要，组件销毁即不可见 |
| `subscribedSessionIdRef` | buffer store 统一管理订阅状态 |
| `sessionId` watch 的复杂重建 | 每次进入都是新组件实例 |

### 切换体验优化

1. **不等待 WebGL**：先创建 xterm 用 canvas 渲染器显示内容，WebGL 后台加载完成后自动切换
2. **Loading overlay**：复用现有 `isTerminalReady` + loading overlay，xterm 初始化完成后淡出
3. **路由过渡动画**：Vue Router `<transition>` 淡入淡出，掩盖组件重建的短暂延迟

## 后端订阅策略

| 场景 | 操作 |
|------|------|
| 首次进入会话 X | `ws_subscribe_session(X)` → `buffer.subscribed = true` |
| 离开会话 X | **不调用** `ws_leave_session`，buffer 继续接收 |
| 再次进入会话 X | `buffer.subscribed = true`，跳过订阅，直接从 buffer 写入 |
| 会话 X 停止 | `ws_leave_session(X)` + `buffer.sessionStopped = true` |
| 会话 X 删除 | `ws_leave_session(X)` + `clearBuffer(X)` |
| 连接断开 | 所有 buffer `subscribed = false` + `hasGap = true` |
| 连接恢复 | `resubscribeAll()`：增量订阅所有有 buffer 的活跃会话 |

## 连接断开/恢复处理

### 断开

- 所有 buffer 的 `subscribed` 标记为 `false`
- buffer 保留数据，标记 `hasGap = true`（断开期间数据缺失）
- 当前活跃 TerminalView：清理实时 handler，显示"已断开"状态

### 恢复

- 遍历所有有 buffer 且 `sessionStopped = false` 的会话
- `ws_subscribe_session(sessionId, startSeq = buffer.lastEndIndex + 1)`
- 如果后端 `minSeq > startSeq`（环形缓冲区已覆盖）：
  - 清空 buffer chunks
  - 标记 `hasGap = true`
  - 用后端返回的新全量历史重建
- 当前活跃 TerminalView：重新注册实时 handler，增量写入缺失数据

## 文件变更清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `stores/terminalBuffer.ts` | **新建** | Pinia store：响应式 buffer Map、全局 ws_output 监听器、chunks 追加、容量控制、订阅状态管理、连接恢复重订阅逻辑 |
| `composables/useTerminalBuffer.ts` | **新建** | TerminalView 用的 composable：从 store 读取 buffer、注册/注销实时 handler、写入历史到 xterm |
| `views/TerminalView.vue` | **重构** | 移除 keep-alive 依赖，简化生命周期，从 buffer 写入历史 |
| `composables/useTerminalOutput.ts` | **删除** | 被 useTerminalBuffer + terminalBuffer store 替代 |
| `components/MobileLayout.vue` | **修改** | 移除 TerminalView 的 keep-alive 缓存 |
| `router/index.ts` | **修改** | 移除 terminal 路由的 keepAlive meta |

### TerminalView.vue 变化量

- 删除：~150 行（onActivated/onDeactivated、WebGL 管理、ResizeObserver 重连、isSubscribing 等）
- 新增：~50 行（从 buffer 写入历史、注册实时 handler）
- 净减：~100 行

### 不涉及的文件

- `TerminalInputBar.vue` — 无变化
- `useMobileCommands.ts` — 无变化（API 不变）
- `useMobileConnection.ts` — 无变化
- Rust 后端 — 无变化

## 资源对比

| 指标 | 当前方案 (keep-alive) | Buffer-Only 方案 |
|------|----------------------|------------------|
| 后台会话内存/会话 | ~5MB (xterm + DOM + buffer) | ~2MB (buffer only) |
| WebGL 上下文占用 | 活跃 1 + 重建闪烁 | 活跃 1（随 xterm 生命周期） |
| 切换延迟 | ~150ms (WebGL 重建) | ~50ms (xterm 创建 + buffer 写入) |
| 切换闪烁 | 有（WebGL 重建） | 无（canvas 先渲染） |
| 代码复杂度 | 高（4 个生命周期钩子 + 多个 watch） | 低（2 个生命周期钩子） |
| 10+ 会话支持 | 差（内存 + WebGL 限制） | 好（buffer 轻量） |
