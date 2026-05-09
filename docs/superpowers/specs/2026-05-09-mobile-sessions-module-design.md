# Mobile Sessions Module Design

## Overview

Add a "Sessions" tab to the mobile bottom navigation, allowing users to view and monitor
active sessions on a connected desktop. Replace the "History" tab position, place it next to
"Devices" (连接), and change the flow so WebSocket connection no longer auto-navigates to
terminal or pops up session selection modals.

## Navigation Change

**Bottom nav order:** 连接 | 会话 | 快捷 | 设置

- Remove "历史" from bottom nav (route `/mobile/history` preserved, HistoryView unchanged)
- Add `MobileNav` entry for `/mobile/sessions` with a terminal/monitor icon

## Route Changes

| Action | Route | Component |
|--------|-------|-----------|
| New | `/mobile/sessions` | `SessionsView.vue` |
| Keep | `/mobile/history` | `HistoryView.vue` (nav entry removed) |
| Modify | `/mobile/terminal/:id` | Add `?sessionId=xxx` query param |

## SessionsView.vue — New Component

Route: `/mobile/sessions`

### States

1. **Not connected:** Empty state with message "未连接设备" and a button navigating to `/mobile/devices`
2. **Connected, loading:** Spinner while fetching sessions
3. **Connected, no sessions:** Empty state "暂无活跃会话"
4. **Connected, has sessions:** Session card list

### Session Card

Each card displays:
- **Left color bar:** green (running), yellow (waiting_input), red (stopped)
- **Session name** (from `session.name`)
- **Status + runtime** — "运行中 · 3m 24s" computed from `createdAt`/`startedAt`
- **Last output time** — "最后输出 2 秒前" (if available)
- **Quick action:** stopped sessions show "重新运行", running sessions show stop button
- **Tap:** navigates to `/mobile/terminal/{deviceId}?sessionId={sessionId}`

### Data Source

Uses `useRemoteConnection` singleton for connection state and `useRemoteTerminal` for
`loadSessions()` / `sessions` / `stopSession()`.

### Refresh

Pull-to-refresh or a "刷新" button to reload the session list.

## DevicesView.vue Changes

### After successful pairing (handlePairingSubmit)

After pairing succeeds, call **both**:
1. `loadSessionConfigs()` — existing, shows session config templates
2. `terminal.loadSessions()` — **new**, populates active sessions for the Sessions tab

### After starting a session (handleStartSession)

- **Remove:** `router.push('/mobile/terminal/...')` auto-navigation
- **Replace with:** Toast notification "会话已启动" + refresh active sessions list
- User manually switches to "会话" tab to see the new session

## TerminalView.vue Changes

### Route param change

Route `params.id` remains the device ID. New query param `sessionId` specifies which session
to join directly.

### onMounted behavior change

- **Remove:** auto-join first session, `showSessionSelect` modal popup logic
- **New:** read `route.query.sessionId`, if present → directly `joinSession(sessionId)`
- If not connected yet → connect to device first, then join
- **Keep:** header session-switcher button (for switching sessions after connected)

## Backend Changes

### SessionSummary (message.rs)

Add time fields:
```rust
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: String,       // new — ISO 8601
    pub started_at: Option<String>, // new — ISO 8601, None if not started
}
```

### server.rs — ListSessions handler

Map `SessionInfo.created_at` and `SessionInfo.started_at` into `SessionSummary` fields.

## Frontend Type Changes

### useRemoteTerminal.ts — RemoteSession

```typescript
export interface RemoteSession {
  id: string
  name: string
  status: 'running' | 'waiting_input' | 'stopped'
  createdAt: string   // new
  startedAt?: string  // new
}
```

Update `mapSessionStatus` and `handleControlMessage` to pass through the new fields.

## File Manifest

| # | File | Action | Description |
|---|------|--------|-------------|
| 1 | `src/views/mobile/SessionsView.vue` | New | Session list page |
| 2 | `src/router/index.ts` | Edit | Add `/mobile/sessions` route |
| 3 | `src/components/mobile/MobileNav.vue` | Edit | Replace history→sessions, reorder |
| 4 | `src/views/mobile/DevicesView.vue` | Edit | Load sessions after connect, remove auto-nav |
| 5 | `src/views/mobile/TerminalView.vue` | Edit | Remove auto-popup, support `?sessionId` |
| 6 | `src/composables/useRemoteTerminal.ts` | Edit | Add `createdAt`/`startedAt` to RemoteSession |
| 7 | `src-tauri/src/websocket/message.rs` | Edit | Add time fields to SessionSummary |
| 8 | `src-tauri/src/websocket/server.rs` | Edit | Map new fields in ListSessions handler |

## Out of Scope

- Multi-device session aggregation (single device only)
- Session output preview/snapshots on cards
- HistoryView changes (only removed from nav, view unchanged)
