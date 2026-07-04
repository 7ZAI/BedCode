# TerminalView Refactoring Design

## Problem

`bedcode-mobile/src/views/TerminalView.vue` is 2248 lines — too large to maintain effectively. Template (~266 lines), script (~1170 lines), and style (~810 lines) are all in one file, mixing UI, complex scroll logic, modal state, and terminal management.

## Goal

Reduce TerminalView.vue to ~550 lines by extracting self-contained pieces into components, composables, and a CSS file. All functionality and visual behavior remain identical.

## Architecture

```
TerminalView.vue (~550 lines) — orchestrator
├── TerminalHeader.vue (~200 lines) — header + toolbar + overflow menu
├── TerminalSettingsModal.vue (~280 lines) — settings modal
├── TerminalConfirmModal.vue (~80 lines) — generic confirm dialog
├── useTerminalScroll.ts (~380 lines) — touch scroll + selection composable
└── styles/terminal.css (~450 lines) — terminal/xterm/scrollbar/selection styles
```

## Extracted Components

### TerminalHeader.vue

**File**: `src/components/TerminalHeader.vue`

**Props**:
| Prop | Type | Description |
|------|------|-------------|
| `sessionName` | `string` | Header title |
| `visibleItems` | `ToolbarItemConfig[]` | Configured visible toolbar items |
| `allItems` | `ToolbarItemConfig[]` | All available items (for overflow) |
| `showSidebar` | `boolean` | Folder button active state |

**Emits**:
| Event | Payload | Description |
|-------|---------|-------------|
| `back` | — | Back button clicked |
| `action` | `key: string` | Toolbar item action (task/shortcut/clear/refresh/settings/folder) |

**Contains**:
- Back button + session name
- Loop over `visibleItems` rendering each toolbar button
- Overflow menu with transition for remaining items
- Overflow backdrop + click-outside handling

**Internal state**: `showOverflowMenu` (local to header)

**CSS**: Header layout, button styles, overflow menu styles — all scoped.

### TerminalSettingsModal.vue

**File**: `src/components/TerminalSettingsModal.vue`

**Props**:
| Prop | Type | Description |
|------|------|-------------|
| `visible` | `boolean` | v-if control |
| `fontSize` | `number` | Current font size |
| `theme` | `string` | Current theme name |
| `isThemeUserSet` | `boolean` | Whether theme was manually set |
| `quickBarCount` | `number` | Current quick bar count |
| `toolbarItems` | `string[]` | Current toolbar items |
| `safeAreaStyle` | `Record<string, string>` | Safe area padding |

**Emits**:
| Event | Payload | Description |
|-------|---------|-------------|
| `confirm` | `TerminalSettings` | User confirms with all new values |
| `cancel` | — | User cancels |

**Interface**:
```typescript
interface TerminalSettings {
  fontSize: number
  theme: string
  isThemeUserSet: boolean
  quickBarCount: number
  toolbarItems: string[]
}
```

**Internal state**: All `temp*` values managed locally, only emitted on confirm.

**CSS**: Modal layout, theme grid, font size control, toolbar toggle grid, footer — all scoped.

### TerminalConfirmModal.vue

**File**: `src/components/TerminalConfirmModal.vue`

**Props**:
| Prop | Type | Description |
|------|------|-------------|
| `visible` | `boolean` | v-if control |
| `message` | `string` | Confirm message |
| `safeAreaStyle` | `Record<string, string>` | Safe area padding |

**Emits**:
| Event | Payload | Description |
|-------|---------|-------------|
| `confirm` | — | User confirms |
| `cancel` | — | User cancels |

**CSS**: Confirm modal overlay, modal card, buttons — all scoped.

## Extracted Composable

### useTerminalScroll

**File**: `src/composables/useTerminalScroll.ts`

**Parameters**:
| Param | Type | Description |
|-------|------|-------------|
| `terminalRef` | `Ref<Terminal \| null>` | xterm Terminal instance |
| `scrollContainerRef` | `Ref<HTMLDivElement \| null>` | Touch scroll container |
| `sessionId` | `Ref<string>` | Current session ID (for toast messages) |

**Returns**:
```typescript
{
  // Reactive state
  currentLine: Ref<number>
  isSelectionMode: Ref<boolean>
  hasSelection: Ref<boolean>
  scrollbarVisible: Ref<boolean>
  scrollbarThumbStyle: ComputedRef<{ top: string; height: string }>
  xtermContainerStyle: ComputedRef<Record<string, string>>
  shortcutsPanelHeight: Ref<number>
  isUserScrolling: Ref<boolean>
  cellHeight: Ref<number>

  // Methods
  scrollToBottom: () => void
  fitTerminal: (fitAddon: FitAddon | null) => void
  setupViewportScroll: (fitAddon: FitAddon) => void
  exitSelectionMode: () => void
  copySelection: () => Promise<void>
  selectAllText: () => void
  handleShortcutsPanelToggle: (height: number) => void
  applySettings: (theme: string, fontSize: number, fitAddon: FitAddon | null) => void

  // Lifecycle
  dispose: () => void
}
```

**Responsibilities**:
- Touch scroll handlers (start/move/end with capture phase)
- Inertia scrolling with friction decay
- Custom scrollbar position calculation
- Selection mode (long press detection, extend selection, copy, select all)
- GPU hint management (will-change on xterm-screen)
- rAF-throttled viewport sync
- FitAddon.fit() wrapper with error handling

**Key design**: Composable doesn't own Terminal/FitAddon instances. TerminalView creates them and passes refs. `setupViewportScroll()` is called after terminal init; `dispose()` on unmount.

## CSS Extraction

### styles/terminal.css (~450 lines, unscoped)

Moves from `<style scoped>` to an imported stylesheet:

| Category | Classes | Reason |
|----------|---------|--------|
| Terminal layout | `.terminal-view`, `.main-content`, `.terminal-output-area`, `.terminal-scroll-container`, `.xterm-container` | Core layout, no scoping needed |
| xterm overrides | `:deep(.xterm*)`, `:deep(.xterm-viewport*)`, `:deep(.xterm-screen*)`, `:deep(.xterm-scroll-area*)` | Already pierce scoped boundaries |
| Scrollbar | `.scrollbar-track`, `.scrollbar-thumb`, `.scrollbar-thumb.visible` | Part of terminal area |
| Selection mode | `.selection-mode`, `.selection-action-bar`, `.selection-action-btn` | Terminal-specific |
| Loading overlay | `.loading-overlay`, `.loading-spinner`, `.loading-text`, `@keyframes spin` | Terminal-specific |
| Sidebar overlay | `.sidebar-overlay`, `.sidebar-hidden`, `.sidebar-backdrop` | Layout positioning |
| Transitions | `loading-fade-*`, `overflow-menu-*`, `selection-bar-*` | Animation keyframes |

**Import**: `import '@/styles/terminal.css'` in TerminalView.vue

### Component-scoped CSS

| Component | Scoped CSS | Lines |
|-----------|-----------|-------|
| `TerminalHeader.vue` | Header layout, button styles, overflow menu, backdrop | ~150 |
| `TerminalSettingsModal.vue` | Modal overlay, content, theme grid, font control, toolbar toggle, footer | ~200 |
| `TerminalConfirmModal.vue` | Confirm overlay, modal card, buttons | ~50 |
| `TerminalView.vue` | Minimal: padding transition, container positioning | ~20 |

## Data Flow

```
TerminalView.vue
  │
  ├─ owns: terminalRef, fitAddonRef, resizeObserverRef
  ├─ owns: terminalSettings, showSettings, showClearConfirm, showSidebar
  ├─ owns: showTaskPicker, showShortcutConfig
  ├─ owns: session/connection computed props
  │
  ├─ uses: useTerminalScroll(terminalRef, scrollContainerRef, sessionId)
  │   └─ returns: scroll state + handlers → bound to template
  │
  ├─ <TerminalHeader>
  │   ├─ props: sessionName, visibleItems, allItems, showSidebar
  │   └─ emits: @back, @action(key)
  │       └─ TerminalView maps actions to modal visibility toggles
  │
  ├─ <TerminalSettingsModal>
  │   ├─ props: visible, fontSize, theme, isThemeUserSet, quickBarCount, toolbarItems, safeAreaStyle
  │   └─ emits: @confirm(settings), @cancel
  │       └─ TerminalView applies settings + calls scroll.applySettings()
  │
  └─ <TerminalConfirmModal>
      ├─ props: visible, message, safeAreaStyle
      └─ emits: @confirm, @cancel
          └─ TerminalView handles clearTerminal()
```

## TerminalView.vue After Refactoring (~550 lines)

**Template (~80 lines)**:
- Loading overlay
- `<TerminalHeader>` with bound props/events
- Main content: terminal-scroll-container with xterm container, scrollbar, selection bar
- FileSidebar + backdrop
- TerminalInputBar
- `<TerminalSettingsModal>`
- `<TerminalConfirmModal>`
- TaskPickerModal, ShortcutConfigModal

**Script (~250 lines)**:
- Route, router, connection, settings, stores setup
- Terminal init/dispose lifecycle
- Input handlers (submit, execute, special key)
- Settings confirm/cancel handlers
- Toolbar action dispatcher
- Session/connection watchers

**Style (~20 lines)**:
- Minimal scoped CSS (padding transition)

## Non-Goals

- No visual or behavioral changes — purely structural refactoring
- No new features or bug fixes
- No changes to existing composables (useTerminalBuffer, useMobileConnection, etc.)
- No changes to TerminalInputBar, FileSidebar, TaskPickerModal, ShortcutConfigModal

## Validation

After refactoring:
1. `npm run tauri:android:dev` — app starts, terminal renders
2. Touch scrolling + inertia works
3. Long press → selection mode → copy/select all works
4. Settings modal: font, theme, quick bar, toolbar config all save correctly
5. Clear screen confirm dialog works
6. Header toolbar + overflow menu works
7. Sidebar open/close works
8. Keyboard avoid + safe area works
9. All i18n keys resolve correctly
