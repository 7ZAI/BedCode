# @binblink/bedcode-plugin-sdk-desktop

The BedCode Desktop WASM app development kit — provides everything an app needs: **type definitions**, **runtime proxies**, a **Vite build plugin**, and **shared UI components**. Apps only need this package; no host source code required.

> Terminology: this repository calls "plugins" WASM apps; the package name, CLI name and identifiers keep the original `plugin` words (`plugin.json`, `bedcode-plugin-api`).
>
> Full development guide: [`wasm-app-dev-desktop.md`](../../wasm-app-dev-desktop.md) (Chinese); API & extension-point reference [`wasm-app-dev-guide.md`](../../../bedcode-desktop/docs/knowledge/wasm-app-dev-guide.md).

English | [简体中文](README.md)

## Install

```bash
pnpm add -D @binblink/bedcode-plugin-sdk-desktop
```

Peer dependencies (provided by the host at runtime; installed in the plugin for building & type-checking):

| Dependency | Version |
|------------|---------|
| vite | ^5.0.0 |
| vue | ^3.4.0 |

## Quick Start (recommended: scaffold)

```bash
# Create a WASM app project (id and name are positional args, not interactive; --rust adds a WASM backend)
bedcode-plugin-desktop create com.example.my-app "My App" --author "you" --rust

cd my-app
pnpm install
ppnpm run dev        # Browser dev environment (Dev Shell: empty host shell + HMR)
ppnpm run build      # Build dist/ (vite frontend + cargo wasm32 for rust-ts)
ppnpm run manifest   # Auto-fill plugin.json contributes / permissions from source
```

More commands (run the CLI directly inside the app project, not as npm scripts):

```bash
bedcode-plugin-desktop build --resources-dir ../bedcode-desktop/src-tauri/resources/plugins/desktop
# copies the build output into src-tauri/resources/plugins/desktop/{id}/ (how built-in WASM apps ship)

bedcode-plugin-desktop build --frontend-only / --rust-only   # build only half (rust-ts)
bedcode-plugin-desktop dev [pluginDir] [--port <port>] [--host] [--open]
bedcode-plugin-desktop manifest --check   # check only, don't write (CI)
bedcode-plugin-desktop validate [--dir]   # validate plugin.json (id format / required fields / pluginType / permission allowlist / artifact presence)
bedcode-plugin-desktop doctor             # environment self-check (Node / Rust / wasm32 target / dev-shell / SDK)
```

Copy the build output into the host's `src-tauri/resources/plugins/desktop/{id}/` directory, then enable it on the host's app management page.
User-side install goes through a zip package (desktop "Plugins" page → pick zip, unpacked into `app_data_dir/plugins/{id}/`).

## Creating a Plugin Manually

A minimal plugin needs just two files:

**`plugin.json`** — plugin manifest (id / permissions / extension point declarations):

```json
{
  "id": "com.example.my-plugin",
  "name": "My Plugin",
  "version": "0.1.0",
  "platform": "desktop",
  "description": "Example plugin",
  "author": "you",
  "main": "index.js",
  "pluginType": "ts-only",
  "permissions": ["storage", "ui:sidebar"],
  "contributes": {
    "commands": [{ "id": "my-plugin.hello", "title": "Hello" }],
    "views": [{ "id": "my-plugin.sidebar", "type": "sidebar", "title": "My Plugin", "component": "MyPanel" }]
  }
}
```

Required fields: `name` / `version` / `platform` / `main` / `pluginType` / `permissions` / `contributes`
(validation source of truth `bin/manifest-validate.js`; the host deserializes with no defaults, so a missing field rejects the install).
`pluginType` accepts `rust` / `rust-ts` / `ts-only`; `rust-ts` requires `rustLibrary` (must match the Cargo.toml package name).

> `sandbox` has been retired from the manifest (audit ticket 06 ruling 2: the frontend is not sandboxed) — the Rust types ignore and drop the field, so declaring it has no effect. Do not declare it.

**`src/index.ts`** — plugin entry (must export `activate`):

```ts
import { defineComponent, h } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'

export async function activate(context: PluginContext): Promise<void> {
  // Register a sidebar panel (components can re-fetch context via inject('pluginContext'))
  context.ui.registerSidebarPanel({
    id: 'my-plugin.sidebar',
    title: 'My Plugin',
    order: 600,
    component: defineComponent({
      render: () => h('div', 'Hello BedCode!'),
    }),
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
| `context.terminal` | Send input to sessions, subscribe to output | `terminal:input` / `terminal:output` / `terminal:observe` |
| `context.session` | Session list, status change subscription | `session:read` / `session:write` |
| `context.ui` | Sidebar / toolbox / statusbar / settings section / input extensions / terminal toolbar / titlebar / file handlers (`registerFileHandler`) | `ui:sidebar` / `ui:toolbox` / `ui:statusbar` / `ui:pageToolbar` / `ui:settings` / `ui:input` / `ui:fileHandler` |
| `context.events` | Host event subscription & publishing (`on` / `emit`) | subscribe: default; publish: `broadcast` |
| `context.storage` | Key-value storage (`get` / `set` / `delete` / `flush`) | `storage` (**no default grant on desktop — must be declared explicitly**) |
| `context.http` | Register HTTP endpoints (`registerEndpoint`, e.g. for Agent CLI hooks) | `network:http` |
| `context.i18n` | Host i18n: register app translations, `t()` helper, `getI18n()` for the instance | granted by default |
| `context._disposables` | List of Disposables collected by the host on deactivation (internal) | — |
| `context.id` / `context.extensionPath` | App id / install directory path (read-only) | — |

Permission example: `"permissions": ["storage", "ui:sidebar", "terminal:output", "network:http"]`.

All `register*` / `on*` calls return a `Disposable`; the host collects them automatically on plugin deactivation — no manual `dispose()` needed.

## Shared Modules (avoid bundling vue etc.)

During plugin builds, `vue` / `vue-i18n` / `pinia` / `vue-sonner` are externalized and read at runtime from the host global `window.__BEDCODE_SHARED__`. **Always access them via the SDK proxy functions — never touch the global directly**:

> Inclusion rule: modules holding process-wide singleton state — a second instance desynchronizes state. `vue-sonner` is the host toast queue itself, so a plugin's `import { toast } from 'vue-sonner'` is rewritten to the host instance (identical queue and look; a bundled second copy would render nothing).

```ts
import { getVue, getI18n, getPinia, getRouter, getPluginContext } from '@binblink/bedcode-plugin-sdk-desktop'

const { ref, computed } = getVue()      // Host Vue instance (components can import vue directly — externalized at build time)
const i18n = getI18n()                  // Host vue-i18n instance (module-level code; components use useI18n())
const context = getPluginContext()      // PluginContext via inject inside component setup
```

## Vite Build Integration

Add `bedcodePlugin()` to the plugin's `vite.config.ts`; it marks shared modules as external and rewrites them to read from the host global:

```ts
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { bedcodePlugin } from '@binblink/bedcode-plugin-sdk-desktop/vite'

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

The `--rust` scaffold includes the Rust-side SDK crate **`bedcode-plugin-api`** (in this package's `rust/` directory, MIT) for backend logic compiled into a WASM component that runs inside the host's wasmtime sandbox.

### Enable

```toml
[dependencies]
bedcode-plugin-api = { path = "<host>/packages/plugin-sdk-desktop/rust", features = ["wasm"] }
```

The `wasm` feature provides:

- `WasmPlugin` trait + `wasm_entry!` macro — generates all component exports defined by the WIT contract (`rust/wit/bedcode.wit`, single source of truth)
- `WasmHost` — host API bindings (import backend), calling host capabilities via bindgen
- `#[plugin_api]` attribute macro (from `bedcode-plugin-api-macros`, the `rust-macros/` crate) — inter-plugin call IDL: trait definition → JSON-RPC dispatch + client generation + drift detection

### Minimal Example

```rust
use bedcode_plugin_api::{CommandArgs, WasmHost, WasmPlugin};

struct MyPlugin;

impl WasmPlugin for MyPlugin {
    fn activate() -> anyhow::Result<()> {
        // Access host capabilities via WasmHost (events, storage, HTTP endpoints, ...)
        Ok(())
    }
}

bedcode_plugin_api::wasm_entry!(MyPlugin);
```

### Build

```bash
cargo build --target wasm32-unknown-unknown --no-default-features --features wasm --release
```

The output is a WASM component (Component Model) loaded by the host inside the wasmtime sandbox — a crashing plugin never affects the host. Without the `wasm` feature it works as a regular Rust crate (static registration / library dependency).

## Configuration Declaration

Declare plugin config in `plugin.json` under `contributes.configuration`; the host renders a config page from it. At runtime, read/write via `context.storage` (unified key `PLUGIN_CONFIG_STORAGE_KEY = 'config'`). The SDK provides a declarative helper to keep both ends consistent:

```ts
import { defineConfiguration, PLUGIN_CONFIG_STORAGE_KEY } from '@binblink/bedcode-plugin-sdk-desktop'

const config = defineConfiguration('My Plugin Settings', {
  apiKey: { type: 'string', title: 'API Key' },
  maxRetries: { type: 'number', title: 'Max Retries', default: 3 },
  debugMode: { type: 'boolean', title: 'Debug Mode', default: false },
})

// Read user config at runtime
const saved = await context.storage.get<typeof config.properties>(PLUGIN_CONFIG_STORAGE_KEY)
```

## Event Constants

Event topics follow the `{domain}:{action}` convention; each domain owns its event names in its own WASM app package (single source of truth). The SDK ships no business-specific event names. For example, the terminal-session app (`wasm-apps/terminal-session`) subscribes to its task status event:

```ts
// Inside an app, subscribe to its domain event (string literal or a local constant)
context.events.on('task:status-changed', (payload) => {
  // Task status change (e.g. Agent CLI idle → in_progress)
})
```

## Shared UI Components

Components exported via the `./ui` subpath follow the host design tokens so plugin UI matches the host look & feel (**native `<select>` etc. system controls are banned**):

```vue
<script setup lang="ts">
import { ref } from 'vue'
import Select from '@binblink/bedcode-plugin-sdk-desktop/ui'

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

Currently provided (each subpath is an independent export that default-exports the component):

| Subpath | Component | Purpose |
|---------|-----------|---------|
| `./ui` | `Select` | Dropdown (`update:modelValue` / `open` events, `md` / `sm` sizes) |
| `./ui/markdown-editor` | `MarkdownEditor` | Markdown editing area (syntax toolbar + preview toggle, `v-model`) |
| `./ui/terminal-input-rail` | `TerminalInputRail` | Terminal input navigation rail (floating on the right of the terminal; one line = one user input, position mapped to the buffer) |
| `./ui/plugin-global-dialog` | `PluginGlobalDialog` | Host global dialog (mounted once at the app root; rendering driven by the global dialog controller, apps drive it via `context.ui.showDialog`, no props on mount) |
| `./ui/zoom-compensation` | (no component) | Coordinate conversion for fixed-position overlays under root CSS zoom (self-calibrating; currently F=1, kept as a defensive no-op) |

## Subpath Exports

| Subpath | Contents |
|---------|----------|
| `@binblink/bedcode-plugin-sdk-desktop` | Main API: types + runtime proxies + config helper + event constants |
| `@binblink/bedcode-plugin-sdk-desktop/vite` | `bedcodePlugin()` Vite build plugin |
| `@binblink/bedcode-plugin-sdk-desktop/types` | Pure types (types only, no runtime) |
| `@binblink/bedcode-plugin-sdk-desktop/ui` | Shared Vue component: `Select` |
| `@binblink/bedcode-plugin-sdk-desktop/ui/markdown-editor` | `MarkdownEditor` |
| `@binblink/bedcode-plugin-sdk-desktop/ui/terminal-input-rail` | `TerminalInputRail` |
| `@binblink/bedcode-plugin-sdk-desktop/ui/plugin-global-dialog` | `PluginGlobalDialog` |
| `@binblink/bedcode-plugin-sdk-desktop/ui/zoom-compensation` | zoom coordinate conversion helper |

## Developing this SDK

```bash
pnpm run build      # tsup build → dist (ESM + d.ts)
pnpm run test:run   # vitest run
```
