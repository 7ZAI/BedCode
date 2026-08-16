# @binblink/plugin-sdk-desktop

The BedCode Desktop plugin development kit — provides everything a plugin needs: **type definitions**, **runtime proxies**, a **Vite build plugin**, and **shared UI components**. Plugins only need this package; no host source code required.

> Full development guide: [`plugin-dev-desktop.md`](../../plugin-dev-desktop.md) (Chinese).

English | [简体中文](README.md)

## Install

```bash
npm install --save-dev @binblink/plugin-sdk-desktop
```

Peer dependencies (provided by the host at runtime; installed in the plugin for building & type-checking):

| Dependency | Version |
|------------|---------|
| vite | ^5.0.0 |
| vue | ^3.4.0 |

## Quick Start (recommended: scaffold)

```bash
# Create a plugin project (interactive name / type selection; --rust adds a WASM backend)
npx bedcode-plugin-desktop create my-plugin --rust

cd my-plugin
npm install
npm run dev        # Browser dev environment (Dev Shell: mock host + HMR)
npm run build      # Build output dist/index.js (optional: -- --resources-dir <dir> copies into the host)
npm run manifest   # Auto-fill plugin.json contributes / permissions from source
npm run validate   # Validate plugin.json
npm run doctor     # Environment self-check
```

Copy the build output into the host's `plugins/desktop/{plugin-id}/` directory, then enable it on the host's plugin management page.

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
  "main": "index.js",
  "sandbox": "inline",
  "pluginType": "ts-only",
  "permissions": ["storage", "ui:sidebar"],
  "contributes": {
    "commands": [{ "id": "my-plugin.hello", "title": "Hello" }],
    "views": [{ "id": "my-plugin.sidebar", "type": "sidebar", "title": "My Plugin", "component": "MyPanel" }]
  }
}
```

**`src/index.ts`** — plugin entry (must export `activate`):

```ts
import { defineComponent, h } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'

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
| `context.ui` | Sidebar / toolbox / statusbar / input extensions / terminal toolbar / titlebar / file viewer | `ui:sidebar` / `ui:toolbox` / `ui:statusbar` / `ui:pageToolbar` / `ui:input` / `ui:fileHandler` |
| `context.events` | Host event subscription & publishing (`on` / `emit`) | subscribe: default; publish: `broadcast` |
| `context.storage` | Key-value storage (`get` / `set` / `delete` / `flush`) | `storage` (default) |
| `context.http` | Register HTTP endpoints (e.g. for Agent CLI hooks) | `network:http` |
| `context.fileService` | File service mount, peer info, system dir / file pickers | `fileservice` |
| `context.i18n` | Host i18n: register plugin translations, `t()` helper | granted by default |
| `context.system` | System-level operations (e.g. reveal in file manager) | `system:open` |

Permission example: `"permissions": ["storage", "ui:sidebar", "terminal:output", "network:http"]`.

All `register*` / `on*` calls return a `Disposable`; the host collects them automatically on plugin deactivation — no manual `dispose()` needed.

## Shared Modules (avoid bundling vue etc.)

During plugin builds, `vue` / `vue-i18n` / `pinia` are externalized and read at runtime from the host global `window.__BEDCODE_SHARED__`. **Always access them via the SDK proxy functions — never touch the global directly**:

```ts
import { getVue, getI18n, getPinia, getRouter, getPluginContext } from '@binblink/plugin-sdk-desktop'

const { ref, computed } = getVue()      // Host Vue instance (components can import vue directly — externalized at build time)
const i18n = getI18n()                  // Host vue-i18n instance (module-level code; components use useI18n())
const context = getPluginContext()      // PluginContext via inject inside component setup
```

## Vite Build Integration

Add `bedcodePlugin()` to the plugin's `vite.config.ts`; it marks shared modules as external and rewrites them to read from the host global:

```ts
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { bedcodePlugin } from '@binblink/plugin-sdk-desktop/vite'

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
import { defineConfiguration, PLUGIN_CONFIG_STORAGE_KEY } from '@binblink/plugin-sdk-desktop'

const config = defineConfiguration('My Plugin Settings', {
  apiKey: { type: 'string', title: 'API Key' },
  maxRetries: { type: 'number', title: 'Max Retries', default: 3 },
  debugMode: { type: 'boolean', title: 'Debug Mode', default: false },
})

// Read user config at runtime
const saved = await context.storage.get<typeof config.properties>(PLUGIN_CONFIG_STORAGE_KEY)
```

## Event Constants

Host event names are exported as constants (kept in sync with the Rust SDK — single source of truth):

```ts
import { EVENT_TASK_STATUS_CHANGED } from '@binblink/plugin-sdk-desktop'

context.events.on(EVENT_TASK_STATUS_CHANGED, (payload) => {
  // Task status change (e.g. Agent CLI idle → in_progress)
})
```

## Shared UI Components

Components exported via the `./ui` subpath follow the host design tokens so plugin UI matches the host look & feel (**native `<select>` etc. system controls are banned**):

```vue
<script setup lang="ts">
import { ref } from 'vue'
import Select from '@binblink/plugin-sdk-desktop/ui'

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
| `@binblink/plugin-sdk-desktop` | Main API: types + runtime proxies + config helper + event constants |
| `@binblink/plugin-sdk-desktop/vite` | `bedcodePlugin()` Vite build plugin |
| `@binblink/plugin-sdk-desktop/types` | Pure types (types only, no runtime) |
| `@binblink/plugin-sdk-desktop/ui` | Shared Vue components |

## Developing this SDK

```bash
npm run build      # tsup build → dist (ESM + d.ts)
npm run test:run   # vitest run
```
