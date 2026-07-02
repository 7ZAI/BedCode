# Mobile Terminal Buffer-Only Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace keep-alive cached xterm instances with a lightweight JS buffer store, so background sessions receive data without holding full xterm/DOM resources, and switching displays history instantly.

**Architecture:** Separate data reception from rendering. A Pinia store (`terminalBuffer`) holds per-session Uint8Array chunks + metadata. A composable (`useTerminalBuffer`) provides the interface for TerminalView to read buffer history, register real-time handlers, and manage subscriptions. TerminalView drops keep-alive, uses normal mount/unmount lifecycle.

**Tech Stack:** Vue 3 + Pinia + xterm.js + @xterm/addon-webgl + @tauri-apps/api/event

---

## File Structure

| File | Responsibility |
|------|----------------|
| `src/stores/terminalBuffer.ts` | Pinia store: reactive buffer Map, global ws_output listener, chunks append, capacity control, subscription state, connection recovery |
| `src/composables/useTerminalBuffer.ts` | TerminalView composable: read buffer, register/unregister real-time handler, write history to xterm, subscribe/unsubscribe session |
| `src/views/TerminalView.vue` | Terminal UI: simplified lifecycle (onMounted/onUnmounted only), reads buffer on mount, writes history to xterm |
| `src/composables/useTerminalOutput.ts` | DELETED — replaced by store + composable |
| `src/components/MobileLayout.vue` | Remove keep-alive for terminal route |
| `src/router/index.ts` | Remove keepAlive meta from terminal route |

---

### Task 1: Create TerminalBuffer Pinia Store

**Files:**
- Create: `bedcode-mobile/src/stores/terminalBuffer.ts`

- [ ] **Step 1: Write the store file**

```typescript
/**
 * Terminal Buffer Store
 *
 * 全局终端输出缓冲区 — 分离数据接收与渲染
 * 后台会话只维护轻量 JS buffer，不持有 xterm 实例
 * 切换到某会话时，从 buffer 一次性写入历史数据到 xterm
 */

import { defineStore } from 'pinia'
import { reactive, ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ==================== Types ====================

/** ws_output 事件载荷 */
export interface OutputPayload {
  session_id: string
  data_base64: string
  index: number
  end_index?: number
  is_waiting: boolean
}

/** 实时输出回调 — TerminalView 注册，新数据同时写 buffer + xterm */
export interface RealtimeHandler {
  onOutput: (data: Uint8Array, payload: OutputPayload) => void
}

/** 单会话缓冲区 */
export interface SessionBuffer {
  /** 原始输出 chunks，每个是 Uint8Array（解码后的 base64 数据） */
  chunks: Uint8Array[]
  /** 总字节数，用于容量控制 */
  totalBytes: number
  /** 最后接收的输出索引（去重游标） */
  lastIndex: number
  /** 最后接收的 end_index（合并消息的结束索引） */
  lastEndIndex: number
  /** buffer 是否有缺口（溢出丢弃或断连期间缺失） */
  hasGap: boolean
  /** 该会话是否已向后端订阅 */
  subscribed: boolean
  /** 会话是否已停止 */
  sessionStopped: boolean
}

// ==================== Constants ====================

/** 每会话 buffer 上限（与 xterm scrollback 5000 行对齐） */
const MAX_BUFFER_BYTES = 2 * 1024 * 1024

// ==================== Store ====================

export const useTerminalBufferStore = defineStore('terminalBuffer', () => {
  // ==================== State ====================

  /** sessionId → SessionBuffer */
  const buffers = reactive(new Map<string, SessionBuffer>())

  /** sessionId → 实时回调（TerminalView 注册的） */
  const realtimeHandlers = reactive(new Map<string, RealtimeHandler>())

  /** 全局 ws_output 监听器 unlisten 函数 */
  const unlistenRef = ref<UnlistenFn | null>(null)
  /** 是否已启动全局监听 */
  let listenerStarted = false

  // ==================== Global Listener ====================

  /** 启动全局 ws_output 监听器（只启动一次） */
  async function startGlobalListener() {
    if (listenerStarted) return
    listenerStarted = true

    unlistenRef.value = await listen<OutputPayload>('ws_output', (event) => {
      const payload = event.payload
      const sessionId = payload.session_id
      const buffer = buffers.get(sessionId)

      // 没有 buffer 的会话忽略（未被任何终端访问过）
      if (!buffer) return

      // 会话已停止后不再接收
      if (buffer.sessionStopped) return

      // 索引去重
      if (payload.index !== undefined && payload.index <= buffer.lastIndex) {
        return
      }

      // 解码 base64 → Uint8Array
      const data = decodeBase64(payload.data_base64)

      // 追加到 buffer
      appendToBuffer(sessionId, data, payload.index, payload.end_index ?? payload.index)

      // 同时回调实时 handler（TerminalView 可见时）
      const handler = realtimeHandlers.get(sessionId)
      if (handler) {
        handler.onOutput(data, payload)
      }
    })
  }

  /** 停止全局监听器 */
  function stopGlobalListener() {
    if (unlistenRef.value) {
      unlistenRef.value()
      unlistenRef.value = null
    }
    listenerStarted = false
  }

  // ==================== Buffer Operations ====================

  /** 确保会话有 buffer，不存在则创建 */
  function ensureBuffer(sessionId: string): SessionBuffer {
    let buffer = buffers.get(sessionId)
    if (!buffer) {
      buffer = {
        chunks: [],
        totalBytes: 0,
        lastIndex: -1,
        lastEndIndex: -1,
        hasGap: false,
        subscribed: false,
        sessionStopped: false,
      }
      buffers.set(sessionId, buffer)
      // 有 buffer 时需要全局监听器
      startGlobalListener()
    }
    return buffer
  }

  /** 追加输出数据到 buffer */
  function appendToBuffer(sessionId: string, data: Uint8Array, index: number, endIndex: number) {
    const buffer = ensureBuffer(sessionId)

    buffer.chunks.push(data)
    buffer.totalBytes += data.length
    buffer.lastIndex = index
    buffer.lastEndIndex = endIndex

    // 容量溢出时丢弃最旧 chunks
    while (buffer.totalBytes > MAX_BUFFER_BYTES && buffer.chunks.length > 1) {
      const removed = buffer.chunks.shift()!
      buffer.totalBytes -= removed.length
      buffer.hasGap = true
    }
  }

  /** 获取会话 buffer */
  function getBuffer(sessionId: string): SessionBuffer | undefined {
    return buffers.get(sessionId)
  }

  /** 标记已订阅后端 */
  function markSubscribed(sessionId: string) {
    const buffer = ensureBuffer(sessionId)
    buffer.subscribed = true
  }

  /** 标记未订阅（断连时） */
  function markUnsubscribed(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.subscribed = false
    }
  }

  /** 标记所有 buffer 未订阅（连接断开时） */
  function markAllUnsubscribed() {
    for (const buffer of buffers.values()) {
      buffer.subscribed = false
      buffer.hasGap = true
    }
  }

  /** 标记会话停止 */
  function markSessionStopped(sessionId: string) {
    const buffer = buffers.get(sessionId)
    if (buffer) {
      buffer.sessionStopped = true
    }
  }

  /** 清理单个会话 buffer */
  function clearBuffer(sessionId: string) {
    buffers.delete(sessionId)
    realtimeHandlers.delete(sessionId)
    // 所有 buffer 都清理后，关闭全局监听器
    if (buffers.size === 0) {
      stopGlobalListener()
    }
  }

  /** 清理所有 buffer */
  function clearAllBuffers() {
    buffers.clear()
    realtimeHandlers.clear()
    stopGlobalListener()
  }

  // ==================== Realtime Handler ====================

  /** 注册实时输出回调（TerminalView onMounted 时调用） */
  function registerRealtimeHandler(sessionId: string, handler: RealtimeHandler) {
    realtimeHandlers.set(sessionId, handler)
  }

  /** 注销实时输出回调（TerminalView onUnmounted 时调用） */
  function unregisterRealtimeHandler(sessionId: string) {
    realtimeHandlers.delete(sessionId)
  }

  // ==================== Utility ====================

  /** Base64 解码为 Uint8Array */
  function decodeBase64(base64: string): Uint8Array {
    const binary = atob(base64)
    const bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i)
    }
    return bytes
  }

  return {
    buffers,
    realtimeHandlers,
    ensureBuffer,
    getBuffer,
    appendToBuffer,
    markSubscribed,
    markUnsubscribed,
    markAllUnsubscribed,
    markSessionStopped,
    clearBuffer,
    clearAllBuffers,
    registerRealtimeHandler,
    unregisterRealtimeHandler,
    startGlobalListener,
  }
})
```

- [ ] **Step 2: Verify build compiles**

Run: `cd bedcode-mobile && npx vue-tsc --noEmit`
Expected: PASS (no type errors)

- [ ] **Step 3: Commit**

```bash
git add bedcode-mobile/src/stores/terminalBuffer.ts
git commit -m "feat(mobile): add terminalBuffer Pinia store — lightweight JS buffer for background sessions"
```

---

### Task 2: Create useTerminalBuffer Composable

**Files:**
- Create: `bedcode-mobile/src/composables/useTerminalBuffer.ts`

- [ ] **Step 1: Write the composable file**

```typescript
/**
 * Terminal Buffer Composable
 *
 * TerminalView 用的 composable — 从 store 读取 buffer、注册实时 handler、写入历史到 xterm
 */

import { useTerminalBufferStore, type OutputPayload } from '@/stores/terminalBuffer'
import {
  wsSubscribeSession,
  wsLeaveSession,
} from '@/composables/useMobileCommands'
import type { Terminal } from '@xterm/xterm'

// ==================== Types ====================

export type { OutputPayload } from '@/stores/terminalBuffer'

// ==================== Composable ====================

export function useTerminalBuffer() {
  const store = useTerminalBufferStore()

  /**
   * 写入 buffer 中的历史数据到 xterm
   *
   * @param sessionId - 会话 ID
   * @param terminal - xterm Terminal 实例
   */
  function writeBufferHistoryToTerminal(sessionId: string, terminal: Terminal) {
    const buffer = store.getBuffer(sessionId)
    if (!buffer || buffer.chunks.length === 0) return

    // 逐 chunk 写入，xterm.write() 内部异步处理但调用同步
    for (const chunk of buffer.chunks) {
      terminal.write(chunk)
    }
  }

  /**
   * 注册实时输出 handler — 新数据同时写 buffer（store 已处理）和 xterm
   *
   * @param sessionId - 会话 ID
   * @param terminal - xterm Terminal 实例
   */
  function registerRealtimeHandler(sessionId: string, terminal: Terminal) {
    store.registerRealtimeHandler(sessionId, {
      onOutput: (data: Uint8Array, payload: OutputPayload) => {
        if (terminal) {
          terminal.write(data)
        }
      },
    })
  }

  /**
   * 注销实时输出 handler
   *
   * @param sessionId - 会话 ID
   */
  function unregisterRealtimeHandler(sessionId: string) {
    store.unregisterRealtimeHandler(sessionId)
  }

  /**
   * 订阅会话 — 如果 buffer 已标记 subscribed 则跳过
   *
   * @param sessionId - 会话 ID
   * @returns SubscribeResult 或 null（已订阅时跳过）
   */
  async function subscribeSession(sessionId: string): Promise<{ minSeq: number; maxSeq: number; historyCount: number } | null> {
    const buffer = store.getBuffer(sessionId)
    if (buffer?.subscribed) return null // 已订阅，跳过

    // 增量同步：有 lastIndex 时从断点继续
    const startSeq = buffer && buffer.lastEndIndex >= 0 ? buffer.lastEndIndex + 1 : undefined

    // 全量回放时重置 buffer 去重游标
    if (startSeq === undefined) {
      if (buffer) {
        buffer.lastIndex = -1
        buffer.lastEndIndex = -1
      }
    }

    // 先确保 buffer 存在 + 监听器启动，再订阅后端
    store.ensureBuffer(sessionId)

    const result = await wsSubscribeSession(sessionId, startSeq)

    // 增量同步回退检测：后端 minSeq > startSeq，说明旧数据已被覆盖
    if (startSeq !== undefined && result && result.minSeq > startSeq) {
      console.warn(
        `[useTerminalBuffer] Incremental sync gap: minSeq=${result.minSeq} > startSeq=${startSeq}, clearing buffer for fresh replay`
      )
      // 清空 buffer 避免显示不完整的拼接内容
      const buf = store.getBuffer(sessionId)
      if (buf) {
        buf.chunks = []
        buf.totalBytes = 0
        buf.lastIndex = -1
        buf.lastEndIndex = -1
        buf.hasGap = true
      }
    }

    store.markSubscribed(sessionId)
    return result
  }

  /**
   * 取消订阅会话（会话停止/删除时调用）
   *
   * @param sessionId - 会话 ID
   */
  async function unsubscribeSession(sessionId: string) {
    store.unregisterRealtimeHandler(sessionId)
    store.markUnsubscribed(sessionId)
    try {
      await wsLeaveSession(sessionId)
    } catch (e) {
      console.warn('[useTerminalBuffer] Leave session failed:', e)
    }
  }

  /**
   * 连接断开时 — 标记所有 buffer 未订阅 + hasGap
   */
  function handleDisconnect() {
    store.markAllUnsubscribed()
  }

  /**
   * 连接恢复时 — 重新订阅所有有 buffer 且未停止的会话
   */
  async function handleReconnect() {
    const sessionIds: string[] = []
    for (const [sessionId, buffer] of store.buffers.entries()) {
      if (!buffer.sessionStopped) {
        sessionIds.push(sessionId)
      }
    }

    for (const sessionId of sessionIds) {
      try {
        await subscribeSession(sessionId)
      } catch (e) {
        console.warn(`[useTerminalBuffer] Resubscribe failed for ${sessionId}:`, e)
      }
    }
  }

  /**
   * 会话停止时 — 标记 buffer + 取消后端订阅
   */
  async function handleSessionStopped(sessionId: string) {
    store.markSessionStopped(sessionId)
    store.unregisterRealtimeHandler(sessionId)
    try {
      await wsLeaveSession(sessionId)
    } catch (e) {
      console.warn('[useTerminalBuffer] Leave stopped session failed:', e)
    }
  }

  /**
   * 会话删除时 — 清理 buffer + 取消后端订阅
   */
  async function handleSessionRemoved(sessionId: string) {
    store.unregisterRealtimeHandler(sessionId)
    try {
      await wsLeaveSession(sessionId)
    } catch (e) {
      console.warn('[useTerminalBuffer] Leave removed session failed:', e)
    }
    store.clearBuffer(sessionId)
  }

  return {
    store,
    writeBufferHistoryToTerminal,
    registerRealtimeHandler,
    unregisterRealtimeHandler,
    subscribeSession,
    unsubscribeSession,
    handleDisconnect,
    handleReconnect,
    handleSessionStopped,
    handleSessionRemoved,
  }
}
```

- [ ] **Step 2: Verify build compiles**

Run: `cd bedcode-mobile && npx vue-tsc --noEmit`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add bedcode-mobile/src/composables/useTerminalBuffer.ts
git commit -m "feat(mobile): add useTerminalBuffer composable — TerminalView interface to buffer store"
```

---

### Task 3: Refactor TerminalView — Remove keep-alive dependency and simplify lifecycle

**Files:**
- Modify: `bedcode-mobile/src/views/TerminalView.vue`

This is the largest task. The changes are:

**Imports to change:**
- Remove: `onActivated, onDeactivated` from vue imports
- Remove: `useTerminalOutput, type OutputPayload` import
- Add: `useTerminalBuffer, type OutputPayload` from `useTerminalBuffer`
- Add: `useTerminalBufferStore` from stores
- Remove: `wsJoinSession, wsLeaveSession` imports (moved to composable)

**State to remove:**
- `isActive` ref (v-show guard)
- `webglAddonRef` ref
- `outputListenerRef` ref
- `lastIndexRef` ref
- `subscribedSessionIdRef` ref
- `isSubscribing` ref

**State to keep (unchanged):**
- `xtermContainer`, `scrollContainer` refs
- `isTerminalReady` ref
- `terminalRef`, `fitAddonRef`, `resizeObserverRef` refs
- `touchState` reactive
- All settings/UI state refs
- `session`, `sessionName`, `isSessionActive`, etc. computed

**Functions to remove:**
- `loadWebglRenderer()`
- `disposeWebglRenderer()`
- `createOutputListener()`
- `createFrontendListener()`
- `clearFrontendListener()`
- `subscribeSession()` (replaced by composable)
- `unsubscribeSession()` (replaced by composable)

**Functions to modify:**
- `initTerminal()` — remove WebGL await, add buffer history write + realtime handler register + subscription
- `disposeTerminal()` — remove WebGL dispose, add realtime handler unregister

**Lifecycle hooks to rewrite:**
- `onMounted` — simplified: initTerminal + buffer history + subscribe
- `onUnmounted` — simplified: unregister handler + dispose terminal
- Remove: `onActivated`, `onDeactivated` entirely

**Watch handlers to simplify:**
- `watch(isSessionActive)` — use composable's `handleSessionStopped`
- `watch(isConnected)` — use composable's `handleDisconnect` / `handleReconnect`
- Remove: `watch(sessionId)` entirely (no longer needed — each route entry is a fresh component)

**Template changes:**
- Remove `v-show="isActive"` from `.terminal-view` div (component existence = visibility)

- [ ] **Step 1: Update imports**

Replace the import block:

```typescript
// Old imports to REMOVE:
import { useTerminalOutput, type OutputPayload } from '@/composables/useTerminalOutput'
import { onActivated, onDeactivated } from 'vue'  // (remove these from the vue import line)
import { wsJoinSession, wsLeaveSession, wsResizeTerminal } from '@/composables/useMobileCommands'

// New imports to ADD:
import { useTerminalBuffer, type OutputPayload } from '@/composables/useTerminalBuffer'
import { useTerminalBufferStore } from '@/stores/terminalBuffer'

// Keep wsResizeTerminal but remove wsJoinSession/wsLeaveSession:
import { wsResizeTerminal } from '@/composables/useMobileCommands'
```

The vue import line becomes:
```typescript
import { ref, reactive, computed, inject, type Ref, onMounted, onUnmounted, watch, nextTick } from 'vue'
```

- [ ] **Step 2: Replace `useTerminalOutput` with `useTerminalBuffer`**

Replace:
```typescript
const { registerHandler, unregisterHandler } = useTerminalOutput()
```
With:
```typescript
const { store: bufferStore, writeBufferHistoryToTerminal, registerRealtimeHandler, unregisterRealtimeHandler, subscribeSession, unsubscribeSession, handleDisconnect, handleReconnect, handleSessionStopped } = useTerminalBuffer()
```

- [ ] **Step 3: Remove deprecated state refs**

Remove these ref declarations:
```typescript
// REMOVE these:
const isActive = ref(true)
const webglAddonRef = ref<any>(null)
const outputListenerRef = ref(false)
const lastIndexRef = ref(-1)
const subscribedSessionIdRef = ref<string | null>(null)
const isSubscribing = ref(false)
```

- [ ] **Step 4: Rewrite `initTerminal()`**

Replace the entire `initTerminal()` function body with:

```typescript
async function initTerminal() {
  if (!xtermContainer.value) return

  const theme = TERMINAL_THEMES[terminalSettings.value.theme]
  const term = new Terminal({
    theme: theme,
    fontFamily: '"Courier New", Courier, "Lucida Console", monospace',
    fontSize: terminalSettings.value.fontSize,
    lineHeight: 1.2,
    cursorBlink: true,
    cursorStyle: 'block',
    allowProposedApi: true,
    scrollback: 5000,
    convertEol: true,
    disableStdin: true,
    scrollSensitivity: 0.8,
  })

  terminalRef.value = term
  term.open(xtermContainer.value)

  // Load addons
  const addon = new FitAddon()
  fitAddonRef.value = addon
  term.loadAddon(addon)
  term.loadAddon(new WebLinksAddon())

  // WebGL renderer — 后台加载，不阻塞显示
  try {
    const { WebglAddon } = await import('@xterm/addon-webgl')
    const webglAddon = new WebglAddon()
    term.loadAddon(webglAddon)
    webglAddon.onContextLoss(() => {
      // WebGL 上下文丢失不影响数据，切换回来时重建终端即可
    })
  } catch {
    // WebGL 不可用时回退到 canvas 渲染器
  }

  // 从 buffer 写入历史数据
  writeBufferHistoryToTerminal(sessionId.value, term)

  // 注册实时 handler — 新数据同时写 buffer（store 处理）和 xterm
  registerRealtimeHandler(sessionId.value, term)

  // Fit terminal — delay to ensure container is rendered
  setTimeout(() => {
    fitTerminal()
    setupViewportScroll()
  }, 100)

  // Resize observer
  const observer = new ResizeObserver(() => {
    requestAnimationFrame(fitTerminal)
  })
  resizeObserverRef.value = observer
  observer.observe(xtermContainer.value)

  window.addEventListener('resize', handleWindowResize)

  // Terminal resize 事件：通知桌面端调整 PTY 大小
  term.onResize(({ cols, rows }) => {
    if (isConnected.value && isSessionActive.value && sessionId.value) {
      wsResizeTerminal(sessionId.value, cols, rows).catch((e: Error) => {
        console.warn('[TerminalView] Resize failed:', e)
      })
    }
  })
}
```

- [ ] **Step 5: Rewrite `disposeTerminal()`**

Replace `disposeTerminal()` — remove WebGL dispose and outputListenerRef cleanup, add realtime handler unregister:

```typescript
function disposeTerminal() {
  if (resizeObserverRef.value) {
    resizeObserverRef.value.disconnect()
    resizeObserverRef.value = null
  }
  window.removeEventListener('resize', handleWindowResize)
  // 注销实时 handler
  if (sessionId.value) {
    unregisterRealtimeHandler(sessionId.value)
  }
  if (terminalRef.value) {
    terminalRef.value.dispose()
    terminalRef.value = null
    fitAddonRef.value = null
  }
  isTerminalReady.value = false
  // 清理伪滚动容器状态
  isUserScrolling.value = false
  scrollbarVisible.value = false
  if (touchState.hideTimer) {
    clearTimeout(touchState.hideTimer)
    touchState.hideTimer = null
  }
  if (touchState.inertiaRafId) {
    cancelAnimationFrame(touchState.inertiaRafId)
    touchState.inertiaRafId = 0
  }
  // 清理触摸事件监听器
  if (scrollContainer.value) {
    scrollContainer.value.removeEventListener('touchstart', onTouchStart, { capture: true } as EventListenerOptions)
    scrollContainer.value.removeEventListener('touchmove', onTouchMove, { capture: true } as EventListenerOptions)
    scrollContainer.value.removeEventListener('touchend', onTouchEnd, { capture: true } as EventListenerOptions)
  }
  currentLine.value = 0
  cellHeight.value = 0
}
```

- [ ] **Step 6: Remove old helper functions**

Delete these functions entirely:
- `loadWebglRenderer()`
- `disposeWebglRenderer()`
- `createOutputListener()`
- `createFrontendListener()`
- `clearFrontendListener()`
- `subscribeSession()` (the old one in TerminalView)
- `unsubscribeSession()` (the old one in TerminalView)

- [ ] **Step 7: Rewrite lifecycle hooks**

Replace `onMounted`, `onUnmounted`. Remove `onActivated`, `onDeactivated` entirely.

```typescript
onMounted(async () => {
  await nextTick()
  initTerminal()

  // 订阅后端（如果未订阅）
  if (isSessionActive.value && isConnected.value) {
    await subscribeSession(sessionId.value)
  }

  isTerminalReady.value = true

  // 监听任务状态变更
  taskStatusListenerRef.value = await listen<{ session_id: string; task_status: string; task_reason?: string; task_questions?: Array<{ header: string; options: Array<{ label: string }> }> }>('ws_sync_task_status_changed', (event) => {
    if (event.payload.session_id !== sessionId.value) return
    handleTaskStatusChanged(event.payload.task_status, event.payload.task_questions)
  })

  // 监听会话模式变更
  sessionModeListenerRef.value = await listen<{ session_id: string; auto_approve: boolean }>('ws_sync_session_mode_changed', (event) => {
    if (event.payload.session_id !== sessionId.value) return
    handleSessionModeChanged(event.payload.auto_approve)
  })
})

onUnmounted(async () => {
  // 注销实时 handler + 释放终端
  disposeTerminal()

  // 如果会话已停止，取消后端订阅
  if (!isSessionActive.value) {
    await unsubscribeSession(sessionId.value)
  }

  // 清理事件监听器
  if (taskStatusListenerRef.value) {
    taskStatusListenerRef.value()
    taskStatusListenerRef.value = null
  }
  if (sessionModeListenerRef.value) {
    sessionModeListenerRef.value()
    sessionModeListenerRef.value = null
  }
  autoCleanup()
})
```

**DELETE** the entire `onActivated` and `onDeactivated` blocks.

- [ ] **Step 8: Simplify watch handlers**

Replace `watch(isSessionActive)`:
```typescript
watch(isSessionActive, async (active, prevActive) => {
  if (!sessionId.value) return

  if (active && !prevActive) {
    // Session became active — subscribe backend
    await subscribeSession(sessionId.value)
  } else if (!active && prevActive) {
    // Session stopped — mark buffer + unsubscribe
    await handleSessionStopped(sessionId.value)
  }
})
```

Replace `watch(isConnected)`:
```typescript
watch(isConnected, async (connected) => {
  if (!sessionId.value) return

  if (!connected) {
    handleDisconnect()
  } else if (connected && isSessionActive.value) {
    // Connection restored — resubscribe active session
    await subscribeSession(sessionId.value)
  }
})
```

**DELETE** the entire `watch(sessionId)` block — no longer needed since each route entry creates a fresh component.

- [ ] **Step 9: Remove `v-show="isActive"` from template**

In the template, change:
```html
<div v-show="isActive" class="terminal-view" :style="terminalViewStyle">
```
To:
```html
<div class="terminal-view" :style="terminalViewStyle">
```

- [ ] **Step 10: Verify build compiles**

Run: `cd bedcode-mobile && npx vue-tsc --noEmit && npx vite build`
Expected: PASS (no type errors, build succeeds)

- [ ] **Step 11: Commit**

```bash
git add bedcode-mobile/src/views/TerminalView.vue
git commit -m "refactor(mobile): TerminalView — remove keep-alive, use buffer store, simplify lifecycle"
```

---

### Task 4: Delete useTerminalOutput composable

**Files:**
- Delete: `bedcode-mobile/src/composables/useTerminalOutput.ts`

- [ ] **Step 1: Delete the file**

```bash
rm bedcode-mobile/src/composables/useTerminalOutput.ts
```

- [ ] **Step 2: Verify build compiles (no remaining references)**

Run: `cd bedcode-mobile && npx vue-tsc --noEmit`
Expected: PASS (all references should be gone after Task 3 refactor)

If it fails due to remaining imports, search and fix:
```bash
grep -r "useTerminalOutput" bedcode-mobile/src/
```

- [ ] **Step 3: Commit**

```bash
git add -u bedcode-mobile/src/composables/useTerminalOutput.ts
git commit -m "refactor(mobile): delete useTerminalOutput — replaced by terminalBuffer store + composable"
```

---

### Task 5: Remove keep-alive from MobileLayout and router

**Files:**
- Modify: `bedcode-mobile/src/components/MobileLayout.vue`
- Modify: `bedcode-mobile/src/router/index.ts`

- [ ] **Step 1: Modify MobileLayout.vue**

Replace the `<router-view>` section. The current code:
```html
<router-view v-slot="{ Component, route }">
  <keep-alive :include="cachedMobileRoutes" :max="maxCachedTerminals">
    <component :is="Component" :key="getKey(route)" />
  </keep-alive>
</router-view>
```

Change to:
```html
<router-view v-slot="{ Component, route }">
  <!-- TerminalView 不使用 keep-alive：buffer store 持有数据，组件正常销毁/重建 -->
  <!-- MobileSwipeContainer 保持 keep-alive 缓存 -->
  <keep-alive v-if="route.name !== 'mobile-terminal'" :include="['MobileSwipeContainer']">
    <component :is="Component" />
  </keep-alive>
  <component v-else :is="Component" :key="route.fullPath" />
</router-view>
```

Also simplify the `<script setup>` — remove `cachedMobileRoutes`, `maxCachedTerminals`, `getKey` function:
```typescript
// Remove these:
const cachedMobileRoutes = ['TerminalView', 'MobileSwipeContainer']
const maxCachedTerminals = computed(() => settingsStore.settings.ui.max_cached_terminals || 10)
function getKey(route: any): string { ... }

// Keep only:
const isTerminalRoute = computed(() => route.name === 'mobile-terminal')
```

The import line can also remove `settingsStore` if it was only used for `maxCachedTerminals`. Check if it's used elsewhere in this component — if not, remove it.

- [ ] **Step 2: Modify router/index.ts**

Remove `meta: { keepAlive: true }` from the terminal route:
```typescript
{
  path: '/mobile/terminal/:id',
  name: 'mobile-terminal',
  component: () => import('@/views/TerminalView.vue'),
  // Removed: meta: { keepAlive: true } — no longer using keep-alive
},
```

- [ ] **Step 3: Verify build compiles**

Run: `cd bedcode-mobile && npx vue-tsc --noEmit && npx vite build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add bedcode-mobile/src/components/MobileLayout.vue bedcode-mobile/src/router/index.ts
git commit -m "refactor(mobile): remove keep-alive from terminal route — buffer store replaces cached instances"
```

---

### Task 6: Integration — hook buffer store into connection lifecycle

**Files:**
- Modify: `bedcode-mobile/src/composables/useMobileConnection.ts`

The buffer store needs to react to connection disconnect/reconnect. Currently, TerminalView watches `isConnected` and calls composable methods, but for background sessions (no TerminalView mounted), the store must also handle reconnection.

- [ ] **Step 1: Add buffer store reconnection in useMobileConnection**

In `useMobileConnection.ts`, find the `onConnected` callback in `initMobileEventListeners` and add buffer store resubscribe logic.

Add import at top:
```typescript
import { useTerminalBufferStore } from '@/stores/terminalBuffer'
```

In the `onConnected` callback (inside `initConnectionListeners`), add after existing logic:
```typescript
onConnected: () => {
  // ... existing connection logic ...

  // 连接恢复时重新订阅所有后台会话的终端输出
  const bufferStore = useTerminalBufferStore()
  bufferStore.startGlobalListener()
  // 使用 composable 的 handleReconnect 逻辑
  const sessionIds: string[] = []
  for (const [sid, buffer] of bufferStore.buffers.entries()) {
    if (!buffer.sessionStopped) {
      sessionIds.push(sid)
    }
  }
  for (const sid of sessionIds) {
    wsSubscribeSession(sid, bufferStore.getBuffer(sid)?.lastEndIndex >= 0 ? bufferStore.getBuffer(sid)!.lastEndIndex + 1 : undefined)
      .then((result) => {
        if (result) {
          const buf = bufferStore.getBuffer(sid)
          if (buf && result.minSeq > (buf.lastEndIndex + 1)) {
            // 增量同步回退 — 清空 buffer，标记 hasGap
            buf.chunks = []
            buf.totalBytes = 0
            buf.lastIndex = -1
            buf.lastEndIndex = -1
            buf.hasGap = true
          }
        }
        bufferStore.markSubscribed(sid)
      })
      .catch((e) => console.warn(`[useMobileConnection] Resubscribe ${sid} failed:`, e))
  }
},
```

Also in the `onDisconnected` callback, add:
```typescript
onDisconnected: () => {
  // ... existing disconnect logic ...

  // 标记所有 buffer 未订阅 + hasGap
  const bufferStore = useTerminalBufferStore()
  bufferStore.markAllUnsubscribed()
},
```

- [ ] **Step 2: Verify build compiles**

Run: `cd bedcode-mobile && npx vue-tsc --noEmit`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add bedcode-mobile/src/composables/useMobileConnection.ts
git commit -m "feat(mobile): hook terminalBuffer store into connection lifecycle — resubscribe on reconnect"
```

---

### Task 7: Handle session sync events — stopped/removed sessions cleanup

**Files:**
- Modify: `bedcode-mobile/src/composables/useMobileConnection.ts`

The sync events for session stopped/removed need to update buffer store.

- [ ] **Step 1: Add buffer cleanup in sync event handlers**

In `useMobileConnection.ts`, find the `onSyncSessionStopped` and `onSyncSessionRemoved` callbacks in `initMobileEventListeners`.

In `onSyncSessionStopped`:
```typescript
onSyncSessionStopped?: (data) => {
  // ... existing logic (remove from activeSessions) ...

  // 标记 buffer 会话停止
  const bufferStore = useTerminalBufferStore()
  bufferStore.markSessionStopped(data.session_id)
},
```

In `onSyncSessionRemoved`:
```typescript
onSyncSessionRemoved?: (data) => {
  // ... existing logic (remove from activeSessions) ...

  // 清理 buffer
  const bufferStore = useTerminalBufferStore()
  bufferStore.clearBuffer(data.session_id)
},
```

- [ ] **Step 2: Verify build compiles**

Run: `cd bedcode-mobile && npx vue-tsc --noEmit`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add bedcode-mobile/src/composables/useMobileConnection.ts
git commit -m "feat(mobile): handle session stopped/removed in terminalBuffer store"
```

---

## Self-Review Checklist

1. **Spec coverage:** Every section in the spec maps to a task:
   - SessionBuffer data structure → Task 1
   - Capacity strategy → Task 1 (MAX_BUFFER_BYTES + hasGap)
   - TerminalBufferStore (Pinia) → Task 1
   - useTerminalBuffer composable → Task 2
   - TerminalView lifecycle rewrite → Task 3
   - Delete useTerminalOutput → Task 4
   - Remove keep-alive → Task 5
   - Connection lifecycle hooks → Task 6
   - Session sync cleanup → Task 7

2. **Placeholder scan:** No TBD, TODO, or vague instructions. All code blocks contain complete implementation code.

3. **Type consistency:**
   - `OutputPayload` defined in Task 1 store, re-exported from Task 2 composable — matches what TerminalView uses
   - `SessionBuffer` interface defined in store, used by composable methods
   - `RealtimeHandler.onOutput(data: Uint8Array, payload: OutputPayload)` — matches store's invocation
   - `wsSubscribeSession` / `wsLeaveSession` — used in composable (Task 2) and connection (Task 6) with same signatures as existing `useMobileCommands.ts`
