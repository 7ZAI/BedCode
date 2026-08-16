# @bedcode/plugin-sdk-mobile

The BedCode Mobile plugin development kit — provides **type definitions**, **runtime proxies**, a **Vite build plugin**, and **shared UI components**. Compared to the desktop SDK, it additionally wraps mobile-only capabilities: SAF storage access, dialogs / system notifications, dynamic routing, lifecycle hooks, and the dev-shell mock data protocol.

> Full development guide: [`plugin-dev-mobile.md`](../../plugin-dev-mobile.md) (Chinese).

English | [简体中文](README.md)

## Install

```bash
npm install --save-dev @bedcode/plugin-sdk-mobile
```

Peer dependencies (provided by the host at runtime; installed in the plugin for building & type-checking):

| Dependency | Version |
|------------|---------|
| vite | ^5.0.0 |
| vue | ^3.4.0 |

## Quick Start (recommended: scaffold)

```bash
# Create a plugin project (interactive name / type selection; --rust adds a WASM backend)
npx bedcode-plugin create my-plugin --rust

cd my-plugin
npm install
npm run dev        # Browser dev environment (Dev Shell: mock host + mobile skeleton, HMR)
npm run build      # Build output dist/index.js
npm run package    # Pack dist/{id}.zip plugin bundle
npm run manifest   # Auto-fill plugin.json contributes / permissions from source
npm run validate   # Validate plugin.json
npm run doctor     # Environment self-check
```

> [!NOTE]
> `npm run dev` auto-installs dev-shell dependencies on first run and opens http://localhost:5173 in the browser; WASM backends and mobile-only capabilities (SAF, system back key, ...) still need verification on a real device.

**Installing on a phone**: transfer `dist/{id}.zip` to the phone → BedCode Mobile → Plugin Management → Install from file.

## Creating a Plugin Manually

A minimal plugin needs just two files:

**`plugin.json`** — plugin manifest (id / permissions / extension point declarations):

```json
{
  "id": "com.example.my-plugin",
  "name": "My Plugin",
  "version": "0.1.0",
  "description": "Example plugin",
  "author": "you",
  "icon": "🧩",
  "main": "index.js",
  "pluginType": "ts-only",
  "permissions": ["storage", "ui:input"],
  "contributes": {
    "commands": [{ "id": "my-plugin.hello", "title": "Hello" }],
    "terminal": {
      "toolbarItems": [{ "id": "my-plugin.toolbar", "title": "My Plugin", "icon": "🧩" }]
    }
  }
}
```

**`src/index.ts`** — plugin entry (must export `activate`):

```ts
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'

export async function activate(context: PluginContext): Promise<void> {
  // Register a terminal toolbar button
  context.ui.registerTerminalToolbarItem({
    id: 'my-plugin.toolbar',
    label: 'My Plugin',
    icon: '🧩',
    onClick: () => {
      context.dialogs.showToast('Hello BedCode!', 'success')
    },
  })
}

export async function deactivate(): Promise<void> {
  // Clean up (all Disposables registered via context are collected by the host automatically)
}
```

## PluginContext API

The `PluginContext` received by `activate(context)` is the **single channel** through which a plugin accesses host capabilities, grouped by permission:

| API | Description | Required permission |
|-----|-------------|---------------------|
| `context.commands` | Register / execute commands | granted by default |
| `context.terminal` | Send input to sessions, subscribe to output | `terminal:input` / `terminal:output` |
| `context.session` | Session list, status change subscription | `session:read` / `session:write` |
| `context.ui` | Toolbox pages, bottom nav tabs, terminal toolbar, settings sections, dynamic routes | `ui:toolbox` / `ui:navtab` / `ui:settings` / `ui:input` / `ui:route` / `ui:back` |
| `context.events` | Host event subscription & publishing (`on` / `emit`) | granted by default |
| `context.storage` | Key-value storage (`get` / `set` / `delete`) | `storage` (default) |
| `context.fileService` | File service mounts, SAF storage access, system pickers | `fileservice` |
| `context.lifecycle` | App lifecycle hooks (start / connect / session / terminal I/O) | granted by default |
| `context.dialogs` | Dialogs (`showDialog` / `showConfirm` / `showPrompt` / `showToast`) | granted by default |
| `context.notifications` | System notifications | granted by default |
| `context.status` | Lifecycle status reporting (`reportReady` / `reportError`) | granted by default |
| `context.logger` | Leveled logging (`info` / `debug` / `warn` / `error`) | granted by default |
| `context.i18n` | Register plugin translations, `t()` helper | granted by default |
| `context.system` | System-level operations (e.g. open file with system viewer) | `system:open` |

All `register*` / `on*` calls return a `Disposable`; the host collects them automatically on plugin deactivation — no manual `dispose()` needed.

## Mobile-Only Capabilities

### Dynamic Routing

Plugins can register routes at any depth; the host mounts them under `/mobile/plugins/{pluginId}/{id}`:

```ts
context.ui.registerRoute({
  id: 'settings',                    // may contain '/' for deep paths
  title: 'Plugin Settings',          // host header title (omitted when header: false)
  component: SettingsPage,           // any Vue component
})

context.ui.openPage('settings')      // full navigation
context.ui.goBack()                  // go back
```

> [!NOTE]
> `context.ui.onBackPressed()` can intercept the Android system back key (Android devices only); on iOS / dev-shell it silently downgrades to never firing.

### Dialogs & Notifications

```ts
const ok = await context.dialogs.showConfirm({ title: 'Confirm', message: 'Delete?', variant: 'danger' })
const name = await context.dialogs.showPrompt({ title: 'Name', inputPlaceholder: 'Enter name' })
context.dialogs.showToast('Saved', 'success')

await context.notifications.notify('Task done', 'Session #3 finished')
```

### Lifecycle Hooks

```ts
context.lifecycle.onAuthSuccess(() => { /* refresh peer data after pairing */ })
context.lifecycle.onDisconnect((reason) => { /* disconnected from desktop */ })
context.lifecycle.onTerminalOutput((sessionId, data) => { /* live terminal output */ })
```

### Peer Desktop HTTP Capabilities (MobileHostApi)

`getMobileApi()` provides reactive connection state plus the peer REST APIs (task queue, session auto mode, task history, scheduled jobs, ...):

```ts
import { getMobileApi } from '@bedcode/plugin-sdk-mobile'

const api = getMobileApi()
console.log(api.isConnected.value, api.activeSessionId.value)

const res = await api.httpTaskQueueList(sessionId)
// → { code, message, data: { tasks, queue_count } }
```

### SAF Storage Access (Android)

`fileService.saf` provides directory-tree traversal and relay copy under scoped storage (`listTree` / `copyStart` / `copyStatus` / `copyCancel`); `pickSharedDirectory` opens the system directory-tree picker and persists authorization, `requestAllFilesAccess` guides to grant full file access.

> [!NOTE]
> SAF APIs are Android-only; they reject on other platforms. `pickDirectory` / `pickFile` also reject on unsupported providers (cloud drives, iOS, ...) — plugins should catch and fall back to manual path input.

### dev-shell Mock Data (devMock)

Plugins can export a `devMock` field with business demo data for the browser dev environment (queue seeds, SAF tree, ...); the real host ignores it — no conditional compilation needed:

```ts
export const devMock: PluginDevMock = {
  queueSeed: [{ id: '1', prompt: 'Example task', position: 0, status: 'pending', created_at: '' }],
}
```

## Shared Modules (avoid bundling vue etc.)

During plugin builds, `vue` / `vue-i18n` / `pinia` are externalized and read at runtime from the host global `window.__BEDCODE_SHARED__`. **Always access them via the SDK proxy functions — never touch the global directly**:

```ts
import { getVue, getPinia, getRouter, getPresetTasks, getMobileApi, getPluginContext } from '@bedcode/plugin-sdk-mobile'

const { ref, computed } = getVue()      // Host Vue instance (components can import vue directly — externalized at build time)
const presetTasks = getPresetTasks()    // Host preset-tasks composable
const context = getPluginContext()      // PluginContext via inject inside component setup
```

## Vite Build Integration

Add `bedcodePlugin()` to the plugin's `vite.config.ts`; it marks shared modules as external and rewrites them to read from the host global:

```ts
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { bedcodePlugin } from '@bedcode/plugin-sdk-mobile/vite'

export default defineConfig({
  plugins: [vue(), bedcodePlugin()],
  build: {
    lib: {
      entry: 'src/index.ts',
      formats: ['es'],
      fileName: () => 'index.js',
    },
    outDir: 'dist',
  },
})
```

The output filename must be `index.js` (matching `main` in `plugin.json`).

## Rust WASM Backend (`--rust` plugins)

The `--rust` scaffold includes the Rust-side SDK crate **`bedcode-plugin-api-mobile`** (in this package's `rust/` directory, MIT) for backend logic compiled into a WASM component that runs inside the host's wasmtime sandbox.

### Enable

```toml
[dependencies]
bedcode-plugin-api-mobile = { path = "<host>/packages/plugin-sdk-mobile/rust", features = ["wasm"] }
```

The `wasm` feature provides:

- `WasmPlugin` trait + `wasm_entry!` macro — generates all component exports defined by the WIT contract (`rust/wit/bedcode.wit`, single source of truth)
- `WasmHost` — host API bindings (import backend), calling host capabilities via bindgen

### Minimal Example

```rust
use bedcode_plugin_api_mobile::{CommandArgs, WasmHost, WasmPlugin};

struct MyPlugin;

impl WasmPlugin for MyPlugin {
    fn activate() -> anyhow::Result<()> {
        // Access host capabilities via WasmHost (events, storage, file service, ...)
        Ok(())
    }
}

bedcode_plugin_api_mobile::wasm_entry!(MyPlugin);
```

### Build

```bash
cargo build --target wasm32-unknown-unknown --no-default-features --features wasm --release
```

The output is a WASM component (Component Model) loaded by the host inside the wasmtime sandbox — a crashing plugin never affects the host. Mobile-only capabilities (SAF, system back key, ...) need verification on a real Android device.

## Shared UI Components

Components exported via the `./ui` subpath follow the host design tokens so plugin UI matches the host look & feel (**native `<select>` etc. system controls are banned**):

```vue
<script setup lang="ts">
import { ref } from 'vue'
import Select from '@bedcode/plugin-sdk-mobile/ui'

const value = ref('a')
const options = [
  { value: 'a', label: 'Option A' },
  { value: 'b', label: 'Option B' },
]
</script>

<template>
  <Select v-model="value" :options="options" size="sm" />
</template>
```

Currently provided: `Select` (dropdown, supports `update:modelValue` / `open` events, `md` / `sm` sizes).

## Subpath Exports

| Subpath | Contents |
|---------|----------|
| `@bedcode/plugin-sdk-mobile` | Main API: types + runtime proxies |
| `@bedcode/plugin-sdk-mobile/vite` | `bedcodePlugin()` Vite build plugin |
| `@bedcode/plugin-sdk-mobile/types` | Pure types (types only, no runtime) |
| `@bedcode/plugin-sdk-mobile/ui` | Shared Vue components |

## Developing this SDK

```bash
npm run build      # tsup build → dist (ESM + d.ts)
npm run test:run   # vitest run
```
