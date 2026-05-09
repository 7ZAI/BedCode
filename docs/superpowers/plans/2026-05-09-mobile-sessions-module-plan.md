# Mobile Sessions Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a mobile Sessions tab showing active sessions from the connected desktop, with click-to-monitor flow, and remove auto-popup/auto-navigation behavior.

**Architecture:** Add `SessionsView.vue` as a new mobile page, update bottom nav and router. Modify `DevicesView` to fetch both session configs AND active sessions after pairing, and stop auto-navigating to terminal. Modify `TerminalView` to accept `?sessionId` query param and directly join the specified session. Extend backend `SessionSummary` with `created_at`/`started_at` for runtime display.

**Tech Stack:** Vue 3 + TypeScript (frontend), Rust + Tokio (backend)

---

### Task 1: Backend — Add time fields to SessionSummary

**Files:**
- Modify: `src-tauri/src/websocket/message.rs:319-324`
- Modify: `src-tauri/src/websocket/server.rs:881-889`

- [ ] **Step 1: Add `created_at` and `started_at` to SessionSummary**

In `src-tauri/src/websocket/message.rs`, replace the SessionSummary struct:

```rust
/// 会话摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
}
```

- [ ] **Step 2: Update ListSessions handler to map new fields**

In `src-tauri/src/websocket/server.rs`, replace the ListSessions mapping block (lines 881-889):

```rust
        ControlAction::ListSessions => {
            let sessions = session_manager.list_sessions().await;
            let summaries = sessions
                .into_iter()
                .map(|s| super::message::SessionSummary {
                    id: s.id,
                    name: s.name,
                    status: format!("{:?}", s.status),
                    created_at: s.created_at.to_rfc3339(),
                    started_at: s.started_at.map(|t| t.to_rfc3339()),
                })
                .collect();

            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: super::message::ControlPayload {
                    action: ControlAction::SessionList { sessions: summaries },
                },
            }))
```

- [ ] **Step 3: Build check**

```bash
cd src-tauri && cargo check 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/websocket/message.rs src-tauri/src/websocket/server.rs
git commit -m "feat(ws): add created_at/started_at to SessionSummary"
```

---

### Task 2: Frontend — Update RemoteSession type

**Files:**
- Modify: `src/composables/useRemoteTerminal.ts:4-8` (interface)
- Modify: `src/composables/useRemoteTerminal.ts:98-104` (handleControlMessage mapping)

- [ ] **Step 1: Add time fields to RemoteSession interface**

In `src/composables/useRemoteTerminal.ts`, update the interface:

```typescript
export interface RemoteSession {
  id: string
  name: string
  status: 'running' | 'waiting_input' | 'stopped'
  createdAt: string
  startedAt?: string
}
```

- [ ] **Step 2: Update handleControlMessage to pass through time fields**

In the same file, inside `handleControlMessage`, update the `session_list` handler (replace the existing mapping at ~line 99):

```typescript
    if (action.type === 'session_list') {
      sessions.value = action.sessions.map((s: any) => ({
        id: s.id,
        name: s.name,
        status: mapSessionStatus(s.status),
        createdAt: s.created_at,
        startedAt: s.started_at || undefined,
      }))
    }
```

- [ ] **Step 3: Update loadSessions to pass through time fields**

In the same file, in `loadSessions()`, update the mapping (replace the existing ~line 151):

```typescript
        sessions.value = response.payload.action.sessions.map((s: any) => ({
          id: s.id,
          name: s.name,
          status: mapSessionStatus(s.status),
          createdAt: s.created_at,
          startedAt: s.started_at || undefined,
        }))
```

- [ ] **Step 4: Commit**

```bash
git add src/composables/useRemoteTerminal.ts
git commit -m "feat(useRemoteTerminal): add createdAt/startedAt to RemoteSession type"
```

---

### Task 3: Frontend — Add /mobile/sessions route

**Files:**
- Modify: `src/router/index.ts`

- [ ] **Step 1: Add the sessions route**

Insert after the `/mobile/devices` route block (after line 36), before the terminal route:

```typescript
    {
      path: '/mobile/sessions',
      name: 'mobile-sessions',
      component: () => import('@/views/mobile/SessionsView.vue'),
      meta: { platform: 'mobile' },
    },
```

- [ ] **Step 2: Commit**

```bash
git add src/router/index.ts
git commit -m "feat(router): add /mobile/sessions route"
```

---

### Task 4: Frontend — Update MobileNav

**Files:**
- Modify: `src/components/mobile/MobileNav.vue`

- [ ] **Step 1: Replace "历史" entry with "会话" and reorder**

Replace the entire `navItems` array. The new order is: 连接 | 会话 | 快捷 | 设置.

```typescript
const navItems = [
  {
    path: '/mobile/devices',
    label: '连接',
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z'
        })
      ])
    }
  },
  {
    path: '/mobile/sessions',
    label: '会话',
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z'
        })
      ])
    }
  },
  {
    path: '/mobile/quick-actions',
    label: '快捷',
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M13 10V3L4 14h7v7l9-11h-7z'
        })
      ])
    }
  },
  {
    path: '/mobile/settings',
    label: '设置',
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z'
        }),
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M15 12a3 3 0 11-6 0 3 3 0 016 0z'
        })
      ])
    }
  }
]
```

- [ ] **Step 2: Commit**

```bash
git add src/components/mobile/MobileNav.vue
git commit -m "feat(MobileNav): replace history tab with sessions, reorder"
```

---

### Task 5: Frontend — Create SessionsView.vue

**Files:**
- Create: `src/views/mobile/SessionsView.vue`

- [ ] **Step 1: Write SessionsView.vue**

```vue
<template>
  <div class="h-full flex flex-col bg-dark-900">
    <!-- Header -->
    <header class="bg-dark-800 border-b border-dark-700 px-4 py-3" style="padding-top: calc(var(--safe-area-inset-top, 0px) + 12px);">
      <div class="flex items-center justify-between">
        <h1 class="text-lg font-semibold">会话</h1>
        <button
          v-if="connection.isConnected.value"
          class="text-dark-500 text-xs"
          :class="{ 'opacity-50': isLoading }"
          :disabled="isLoading"
          @click="refreshSessions"
        >
          {{ isLoading ? '刷新中...' : '刷新' }}
        </button>
      </div>
    </header>

    <!-- Not Connected -->
    <div v-if="!connection.isConnected.value" class="flex-1 flex items-center justify-center p-8">
      <div class="text-center">
        <svg class="w-16 h-16 mx-auto text-dark-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
        </svg>
        <p class="text-dark-300 font-medium mb-2">未连接设备</p>
        <p class="text-dark-500 text-sm mb-4">请先在"连接"页面连接到桌面端</p>
        <button
          class="bg-primary-600 text-white px-6 py-2.5 rounded-xl text-sm font-medium active:bg-primary-700"
          @click="$router.push({ name: 'mobile-devices' })"
        >
          前往连接
        </button>
      </div>
    </div>

    <!-- Connected: Loading -->
    <div v-else-if="isLoading && sessions.length === 0" class="flex-1 flex items-center justify-center">
      <div class="text-center">
        <div class="w-8 h-8 border-2 border-primary-400 border-t-transparent rounded-full animate-spin mx-auto mb-3" />
        <p class="text-dark-500 text-sm">加载会话中...</p>
      </div>
    </div>

    <!-- Connected: Empty -->
    <div v-else-if="sessions.length === 0" class="flex-1 flex items-center justify-center p-8">
      <div class="text-center">
        <svg class="w-16 h-16 mx-auto text-dark-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
        <p class="text-dark-300 font-medium mb-2">暂无活跃会话</p>
        <p class="text-dark-500 text-sm">前往"连接"页面启动新会话</p>
      </div>
    </div>

    <!-- Connected: Session List -->
    <div v-else class="flex-1 overflow-auto p-4">
      <!-- Connection info bar -->
      <div class="flex items-center gap-2 mb-3">
        <div class="w-2 h-2 rounded-full bg-green-500"></div>
        <span class="text-dark-400 text-xs">{{ connection.currentDevice.value?.name || '已连接' }} · {{ sessions.length }} 个会话</span>
      </div>

      <div class="space-y-2">
        <div
          v-for="session in sessions"
          :key="session.id"
          class="bg-dark-800 rounded-xl active:bg-dark-700 transition-colors overflow-hidden"
          :class="{ 'opacity-60': session.status === 'stopped' }"
        >
          <div class="flex">
            <!-- Status color bar -->
            <div
              :class="[
                'w-1 shrink-0',
                session.status === 'running' ? 'bg-green-500' :
                session.status === 'waiting_input' ? 'bg-yellow-500' : 'bg-red-500'
              ]"
            ></div>

            <!-- Content -->
            <div class="flex-1 p-4 min-w-0" @click="handleSessionClick(session)">
              <div class="flex items-start justify-between">
                <div class="flex-1 min-w-0">
                  <p class="font-medium truncate">{{ session.name }}</p>
                  <div class="flex items-center gap-2 mt-1">
                    <span
                      :class="[
                        'text-xs px-1.5 py-0.5 rounded-full',
                        session.status === 'running' ? 'bg-green-900/50 text-green-400' :
                        session.status === 'waiting_input' ? 'bg-yellow-900/50 text-yellow-400' : 'bg-red-900/50 text-red-400'
                      ]"
                    >
                      {{ statusLabel(session.status) }}
                    </span>
                    <span class="text-dark-500 text-xs">{{ elapsedTime(session) }}</span>
                  </div>
                </div>
                <svg class="w-5 h-5 text-dark-400 shrink-0 ml-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                </svg>
              </div>
            </div>

            <!-- Stop button (running/waiting sessions only) -->
            <button
              v-if="session.status !== 'stopped'"
              class="px-3 flex items-center justify-center active:bg-dark-700"
              @click.stop="handleStopSession(session)"
            >
              <svg class="w-5 h-5 text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 6h12v12H6z" />
              </svg>
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useRemoteConnection } from '@/composables/useRemoteConnection'
import { useRemoteTerminal, type RemoteSession } from '@/composables/useRemoteTerminal'

const router = useRouter()
const connection = useRemoteConnection()
const terminal = useRemoteTerminal({
  state: connection.state,
  isConnected: connection.isConnected,
  lastMessage: connection.lastMessage,
  sendMessage: connection.sendMessage,
  sendMessageWithResponse: connection.sendMessageWithResponse,
  setReconnectCallback: connection.setReconnectCallback,
})

const sessions = computed(() => terminal.sessions.value)
const isLoading = computed(() => terminal.isLoading.value)

function statusLabel(status: string): string {
  switch (status) {
    case 'running': return '运行中'
    case 'waiting_input': return '等待输入'
    case 'stopped': return '已停止'
    default: return status
  }
}

function elapsedTime(session: RemoteSession): string {
  const start = session.startedAt || session.createdAt
  if (!start) return ''
  const elapsed = Date.now() - new Date(start).getTime()
  const seconds = Math.floor(elapsed / 1000)
  if (seconds < 60) return `${seconds}s`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`
  const hours = Math.floor(minutes / 60)
  return `${hours}h ${minutes % 60}m`
}

function handleSessionClick(session: RemoteSession) {
  const deviceId = connection.currentDevice.value?.id
  if (!deviceId) return

  router.push({
    name: 'mobile-terminal',
    params: { id: deviceId },
    query: { sessionId: session.id },
  })
}

async function handleStopSession(session: RemoteSession) {
  try {
    await terminal.stopSession(session.id)
  } catch (e) {
    console.error('Failed to stop session:', e)
  }
}

async function refreshSessions() {
  if (!connection.isConnected.value) return
  await terminal.loadSessions()
}

onMounted(() => {
  if (connection.isConnected.value) {
    terminal.loadSessions()
  }
})
</script>
```

- [ ] **Step 2: Commit**

```bash
git add src/views/mobile/SessionsView.vue
git commit -m "feat(mobile): add SessionsView with session list and monitoring"
```

---

### Task 6: Frontend — Modify DevicesView (load sessions, remove auto-nav)

**Files:**
- Modify: `src/views/mobile/DevicesView.vue`

- [ ] **Step 1: Import useRemoteTerminal composable**

Add import after the existing imports (after line 230):

```typescript
import { useRemoteTerminal } from '@/composables/useRemoteTerminal'
```

- [ ] **Step 2: Initialize terminal composable**

Add after the `const connection = useRemoteConnection()` line (~line 232):

```typescript
const terminal = useRemoteTerminal({
  state: connection.state,
  isConnected: connection.isConnected,
  lastMessage: connection.lastMessage,
  sendMessage: connection.sendMessage,
  sendMessageWithResponse: connection.sendMessageWithResponse,
  setReconnectCallback: connection.setReconnectCallback,
})
```

- [ ] **Step 3: Update handlePairingSubmit to also load sessions**

In `handlePairingSubmit`, after the `await loadSessionConfigs()` line (~line 484), add:

```typescript
      // Also fetch active sessions for the Sessions tab
      await terminal.loadSessions()
```

- [ ] **Step 4: Update handleStartSession to not auto-navigate**

Replace the `handleStartSession` function (lines 359-383). Remove the `router.push` and replace with toast + session refresh:

```typescript
async function handleStartSession(config: SessionConfigSummary) {
  if (!isConnected.value || startingConfigId.value) return

  startingConfigId.value = config.id
  try {
    const response = await connection.sendMessageWithResponse('control', {
      action: { type: 'start_session', config_id: config.id },
    })

    const sessionId = response?.session_id
    if (sessionId) {
      // Refresh active sessions for the Sessions tab
      await terminal.loadSessions()
      // Toast notification instead of auto-navigation
      connection.activeSessionId.value = sessionId
    } else {
      console.error('Failed to start session: no session_id in response')
    }
  } catch (e) {
    console.error('Failed to start session:', e)
  } finally {
    startingConfigId.value = null
  }
}
```

- [ ] **Step 5: Update onMounted to also load sessions when already connected**

In `onMounted`, after the existing `await loadSessionConfigs()` line (~line 390), add:

```typescript
    await terminal.loadSessions()
```

- [ ] **Step 6: Commit**

```bash
git add src/views/mobile/DevicesView.vue
git commit -m "feat(DevicesView): load sessions on connect, remove auto-navigation to terminal"
```

---

### Task 7: Frontend — Modify TerminalView (remove auto-popup, support ?sessionId)

**Files:**
- Modify: `src/views/mobile/TerminalView.vue`

- [ ] **Step 1: Remove the Session Select Modal from template**

Delete lines 54-83 (the entire `<Teleport to="body">` block with `<Transition name="fade">` containing the session select modal).

- [ ] **Step 2: Remove showSessionSelect state**

Delete line 121:
```typescript
// REMOVE: const showSessionSelect = ref(false)
```

- [ ] **Step 3: Rewrite onMounted to use query.sessionId instead of auto-select**

Replace the `onMounted` block (lines 140-170):

```typescript
onMounted(async () => {
  const deviceId = route.params.id as string
  const sessionId = route.query.sessionId as string | undefined

  terminal.enableAutoReconnect()

  // Connect to device if not already connected
  if (connection.state.value.status !== 'connected' && connection.state.value.status !== 'paired') {
    const device = connection.pairedDevices.value.find(d => d.id === deviceId)
    if (device) {
      try {
        await connection.connect(device)
      } catch (error) {
        console.error('Failed to connect:', error)
        return
      }
    }
  }

  // Load sessions and join the specified one
  await terminal.loadSessions()

  if (sessionId) {
    await terminal.joinSession(sessionId)
    connection.activeSessionId.value = terminal.currentSessionId.value
  } else if (terminal.sessions.value.length > 0) {
    // Fallback: join first session if no specific session requested
    await terminal.joinSession(terminal.sessions.value[0].id)
    connection.activeSessionId.value = terminal.currentSessionId.value
  }
})
```

- [ ] **Step 4: Clean up unused imports if any**

The `watch` for `showSessionSelect` is no longer needed since the modal was removed. There are no other references to `showSessionSelect` in the script.

- [ ] **Step 5: Commit**

```bash
git add src/views/mobile/TerminalView.vue
git commit -m "feat(TerminalView): remove auto-popup, support ?sessionId query param"
```

---

### Task 8: Build verification

- [ ] **Step 1: Check TypeScript compilation**

```bash
cd /d/tauriProject/BedCode && npx vue-tsc --noEmit 2>&1 | tail -20
```

Expected: No type errors.

- [ ] **Step 2: Check Rust compilation**

```bash
cd /d/tauriProject/BedCode/src-tauri && cargo check 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Step 3: Run frontend tests**

```bash
cd /d/tauriProject/BedCode && npm run test:run 2>&1 | tail -20
```

Expected: All tests pass.
