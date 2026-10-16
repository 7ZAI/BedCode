# @binblink/bedcode-plugin-sdk-mobile

The BedCode Mobile WASM app development kit — provides **type definitions**, **runtime proxies**, a **Vite build plugin**, and **shared UI components**. Compared to the desktop SDK, it additionally wraps mobile-only capabilities: SAF storage access, dialogs / system notifications, dynamic routing, lifecycle hooks, and the dev-shell mock data protocol.

> Terminology: this repository calls "plugins" WASM apps; the package name, CLI name and identifiers keep the original `plugin` words (`plugin.json`, `bedcode-plugin-api-mobile`).
>
> Full development guide: [`wasm-app-dev-mobile.md`](../../wasm-app-dev-mobile.md) (Chinese); development red-line checklist [`wasm-app-development-checklist.md`](../../../docs/knowledge/wasm-app-development-checklist.md).

English | [简体中文](README.md)

## Install

```bash
pnpm add -D @binblink/bedcode-plugin-sdk-mobile
```

Peer dependencies (provided by the host at runtime; installed in the plugin for building & type-checking):

| Dependency | Version |
|------------|---------|
| vite | ^5.0.0 |
| vue | ^3.4.0 |

## Quick Start (recommended: scaffold)

```bash
# Create a WASM app project (id and name are positional args, not interactive; a WASM backend is included by default, --ts-only for frontend-only)
bedcode-plugin create com.example.my-app "My App" --author "you"

cd my-app
pnpm install
ppnpm run dev        # Browser dev environment (Dev Shell: empty host shell + mobile skeleton, HMR)
ppnpm run build      # Build output dist/
ppnpm run package    # Pack the dist/{id}.zip app bundle
```

More commands (run the CLI directly inside the app project, not as npm scripts):

```bash
bedcode-plugin manifest [--check]   # auto-fill plugin.json contributes / permissions from source
bedcode-plugin validate [--dir]     # validate plugin.json
bedcode-plugin doctor               # environment self-check
bedcode-plugin build --watch        # rebuild on change
bedcode-plugin dev --host           # bind the LAN so a phone browser can open http://<pc-ip>:5173
```

> [!NOTE]
> `ppnpm run dev` auto-installs dev-shell dependencies on first run and opens http://localhost:5173 in the browser; WASM backends and mobile-only capabilities (SAF, system back key, ...) still need verification on a real device.

**Installing on a phone**: transfer `dist/{id}.zip` to the phone → BedCode Mobile → Plugin Management → Install from file (remote download with SHA256 verification is also supported).

## Creating a Plugin Manually

A minimal plugin needs just two files:

**`plugin.json`** — plugin manifest (id / permissions / extension point declarations):

```json
{
  "id": "com.example.my-plugin",
  "name": "My Plugin",
  "version": "0.1.0",
  "platform": "mobile",
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
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'

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
| `context.commands` | Register / execute commands (`register` / `execute`) — including the app's own business commands (e.g. `file-transfer.list-remote`) | granted by default |
| `context.session` | Session list, single-session lookup, status change subscription | `session:read` / `session:write` |
| `context.ui` | Host-shell surface / home quick cards / capsule menu / platform settings entries / dynamic routes (`registerSurface` / `registerSlot` / `registerCapsuleItem` / `registerSettingsEntry` / `registerRoute`), navigation (`openPage` / `goBack`), system back-key interception (`onBackPressed`), global dialogs (`showDialog`) | `ui:route` / `ui:back` / `ui:dialog` / `ui:settings` |
| `context.events` | Host event subscription & publishing (`on` / `emit`) | granted by default |
| `context.storage` | Key-value storage (`get` / `set` / `delete`) | `storage` (**always granted on mobile** — see note below) |
| `context.lifecycle` | App lifecycle hooks (`onAppStartup` / `onAppShutdown` / `onAuthSuccess` / `onDisconnect` / `onSessionCreated` / `onSessionStopped`) | granted by default |
| `context.dialogs` | Dialogs (`showDialog` / `showConfirm` / `showPrompt` / `showToast`) | granted by default |
| `context.notifications` | System notifications (`notify(title, body?)`) | granted by default |
| `context.status` | Lifecycle status reporting (`reportReady` / `reportError`) | granted by default |
| `context.logger` | Leveled logging (`info` / `debug` / `warn` / `error`) | granted by default |
| `context.i18n` | Register app translations, `t()` helper | granted by default |
| `context.system` | System-level operations (`openFile` / `revealInDir` / `revealReceivedFileLocation` / `requestAllFilesAccess` / `openDownloadDir`) | `system:open` |
| `context._disposables` | List of Disposables collected by the host on deactivation (internal) | — |

> Note: mobile has **no** `context.terminal` / `context.fileService` / `context.http` (those three are desktop-only).
> On mobile, terminal output is subscribed through the host-shell run surface (the app owns the protocol), file access goes through the `system` domain APIs (gated by `system:open` and SAF authorization), and cross-device transfer goes through app-owned `context.commands` (the underlying peer-network primitives live in the business core crate).

> Permissions are declared **per risk domain** — no default grants except `storage`: the host grants the effective set from the manifest `permissions`, and anything undeclared is denied.
> The exception is `storage` — mobile treats it as **always granted** (reading/writing the app's own configuration space; turning it off would make the app unusable), so there is no toggle: the host's `src-tauri/src/plugin/approval.rs::effective_permissions` unconditionally `insert("storage")`, and the shell marks it as a locked entry in `src/shell/permissions.ts::SHELL_LOCKED_PERMISSIONS`.

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
context.lifecycle.onAppStartup(() => { /* app start (foreground launches only) */ })
context.lifecycle.onAuthSuccess(() => { /* refresh peer data after pairing */ })
context.lifecycle.onDisconnect((reason) => { /* disconnected from desktop */ })
context.lifecycle.onSessionCreated((sessionId) => { /* new session established */ })
context.lifecycle.onSessionStopped((sessionId) => { /* session ended */ })
context.lifecycle.onAppShutdown(() => { /* clean up before shutdown */ })
```

> `onTerminalInput` / `onTerminalOutput` were retired in ticket 15 phase B — terminal I/O no longer flows through lifecycle hooks; the app owns its subscription channel (host-shell run surface).

### Peer Desktop HTTP Capabilities (MobileHostApi)

`getMobileApi()` provides reactive connection state plus a peer REST client (task queue, session auto mode, task history, scheduled jobs — the HTTP paths are owned by each business app):

```ts
import { getMobileApi } from '@binblink/bedcode-plugin-sdk-mobile'

const api = getMobileApi()
console.log(api.isConnected.value, api.activeSessionId.value, api.isPaired.value)

// Generic REST client (path and payload shapes are app-owned)
const res = await api.httpRequest<{ tasks: unknown[]; queue_count: number }>(
  `/task-queue/${api.activeSessionId}`, { method: 'GET' },
)

await api.loadActiveSessions()          // refresh active sessions
api.onSessionEvent((evt) => { /* session event push */ })
const settings = await api.readAllSettings()   // read all settings (flat key/value table)
await api.writeSetting('my-key', 'my-value')   // write a single setting key
```

### File & System Access

Mobile has **no** `context.fileService`. File-related capabilities split into two paths:

- **`context.system`** (`system:open` permission): `openFile(path, displayName?)` opens with the system viewer, `revealInDir(path)` / `revealReceivedFileLocation(fileName)` locate a file in the file manager, `openDownloadDir()` opens the shared downloads directory, `requestAllFilesAccess()` guides the user to grant MANAGE_EXTERNAL_STORAGE (unauthorized access to the shared downloads directory rejects with a message starting with `needs_all_files_access` — catch it and show the guidance dialog instead of failing silently).
- **Cross-device file transfer**: goes through app-owned `context.commands` (e.g. `file-transfer.list-remote`); the underlying peer-network / resumable transfer lives in the business core crate (`packages/bedcode-file-transfer-core`) and is not exposed as a shared SDK API.

### dev-shell Mock Data (devMock)

Apps can export a `devMock` field with business demo data for the browser dev environment (queue seeds, SAF tree, ...); the real host ignores it — no conditional compilation needed:

```ts
export const devMock: PluginDevMock = {
  queueSeed: [{ id: '1', prompt: 'Example task', position: 0, status: 'pending', created_at: '' }],
}
```

> `PluginDevMock` is typed as `Record<string, unknown>` — key names are a convention between the app and the dev-shell, with no compile-time validation.

## Shared Modules (avoid bundling vue etc.)

During plugin builds, `vue` / `vue-i18n` / `pinia` are externalized and read at runtime from the host global `window.__BEDCODE_SHARED__`. **Always access them via the SDK proxy functions — never touch the global directly**:

```ts
import { getVue, getPinia, getRouter, getPresetTasks, getMobileApi, getPluginContext } from '@binblink/bedcode-plugin-sdk-mobile'

const { ref, computed } = getVue()      // Host Vue instance (components can import vue directly — externalized at build time)
const presetTasks = getPresetTasks()    // Host preset-tasks composable
const context = getPluginContext()      // PluginContext via inject inside component setup
```

### App run and stores (Pinia)

The host scopes plugin stores to a single **app run**: everything between `activate` and `deactivate`
(including swipe-away / × / "disable" in the app detail / uninstall) belongs to that run. **Stopping
the app disposes every store created in it** — the next launch starts from the setup initial state and
never sees leftovers from the previous run.

Two contracts follow:

1. **Define stores at the top level of your plugin module** (`const useX = defineStore(...)`). The host
   records ownership while the module is loaded; because the module is cached and evaluated only once,
   that is still enough for the next run to get a fresh scope. Stores defined in async chunks fall back
   to the host Pinia with a warning (not guaranteed to be disposed on app stop).
2. **Module-level mutable singletons must be cleared in `deactivate`** (or tied to a `context`
   disposable). The host does not re-evaluate your module; state that survives across activations is a
   plugin defect. Data that must outlive a run belongs in storage / plugin-database.

## Vite Build Integration

Add `bedcodePlugin()` to the plugin's `vite.config.ts`; it marks shared modules as external and rewrites them to read from the host global:

```ts
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { bedcodePlugin } from '@binblink/bedcode-plugin-sdk-mobile/vite'

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
import Select from '@binblink/bedcode-plugin-sdk-mobile/ui'

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

Currently provided (each subpath is an independent export):

| Subpath | Contents | Purpose |
|---------|----------|---------|
| `./ui` | `Select` component | Dropdown (`update:modelValue` / `open` events, `md` / `sm` sizes) |
| `./ui/plugin-global-dialog` | `PluginGlobalDialog` component | Mobile host global dialog (mounted once at the app root; rendering driven by the global dialog controller, same shape as desktop) |
| `./ui/swipe-tabs` | `useSwipeTabs` composable | Horizontal swipe switching for dialog/tab content areas (a dominant-horizontal swipe past the threshold fires `onSwitch`; dominant-vertical swipes record no offset; `shouldSkip` excludes touch origins that must not act as page-swipe gestures, e.g. input controls) |
| `./global-dialog` | global dialog controller | Publish-subscribe dialog queue (FIFO, at most one open at a time; `update()` builds a fresh entry object before broadcasting), shared by the host and the dev-shell |

## Subpath Exports

| Subpath | Contents |
|---------|----------|
| `@binblink/bedcode-plugin-sdk-mobile` | Main API: types + runtime proxies |
| `@binblink/bedcode-plugin-sdk-mobile/vite` | `bedcodePlugin()` Vite build plugin |
| `@binblink/bedcode-plugin-sdk-mobile/types` | Pure types (types only, no runtime) |
| `@binblink/bedcode-plugin-sdk-mobile/ui` | Shared Vue component: `Select` |
| `@binblink/bedcode-plugin-sdk-mobile/ui/plugin-global-dialog` | `PluginGlobalDialog` |
| `@binblink/bedcode-plugin-sdk-mobile/ui/swipe-tabs` | `useSwipeTabs` |
| `@binblink/bedcode-plugin-sdk-mobile/global-dialog` | global dialog controller |

## Developing this SDK

```bash
pnpm run build      # tsup build → dist (ESM + d.ts)
pnpm run test:run   # vitest run
```
