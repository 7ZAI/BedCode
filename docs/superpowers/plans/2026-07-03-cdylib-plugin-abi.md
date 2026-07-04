# cdylib Plugin ABI + ai-chatbox Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add cdylib dynamic loading to the plugin system and rewrite ai-chatbox as the first cdylib plugin with Rust AI client, custom SQLite tables, Tauri commands, and terminal handlers.

**Architecture:** C ABI + JSON bridge — cdylib plugins export `#[no_mangle]` C functions, host injects capability via `HostContext` function pointer table. All complex data serialized as JSON across FFI boundary. Table names prefixed with `plugin_{id}_` for isolation.

**Tech Stack:** Rust (libloading, reqwest, tokio, serde_json), TypeScript (Vue 3), SQLite

**Spec:** `docs/superpowers/specs/2026-07-03-cdylib-plugin-abi-design.md`

---

## File Structure

### New Files (Rust — plugin system)
- `bedcode-desktop/src-tauri/src/plugin/cdylib_loader.rs` — cdylib loading via libloading
- `bedcode-desktop/src-tauri/src/plugin/host_context.rs` — HostContext FFI function implementations

### New Files (Rust — ai-chatbox cdylib)
- `bedcode-desktop/src-tauri/plugins/ai-chatbox/src/ai_client.rs` — reqwest SSE AI client
- `bedcode-desktop/src-tauri/plugins/ai-chatbox/src/db.rs` — custom SQLite table operations
- `bedcode-desktop/src-tauri/plugins/ai-chatbox/src/commands.rs` — command handlers
- `bedcode-desktop/src-tauri/plugins/ai-chatbox/src/terminal.rs` — terminal handlers (MVP no-op)
- `bedcode-desktop/src-tauri/plugins/ai-chatbox/src/host_api.rs` — HostContext FFI type definitions

### Modified Files (Rust — plugin system)
- `bedcode-desktop/src-tauri/Cargo.toml` — remove ai-chatbox dep, add libloading
- `bedcode-desktop/src-tauri/src/plugin.rs` — export new modules
- `bedcode-desktop/src-tauri/src/plugin/host.rs` — cdylib lifecycle + command dispatch
- `bedcode-desktop/src-tauri/src/plugin/types.rs` — PluginSource::Cdylib
- `bedcode-desktop/src-tauri/src/plugin/loader.rs` — detect rustLibrary field
- `bedcode-desktop/src-tauri/src/plugin/api_bridge.rs` — route plugin_invoke to cdylib
- `bedcode-desktop/src-tauri/bedcode-plugin-api/src/types.rs` — rust_library field

### Modified Files (Rust — ai-chatbox cdylib)
- `bedcode-desktop/src-tauri/plugins/ai-chatbox/Cargo.toml` — cdylib, remove bedcode-plugin-api
- `bedcode-desktop/src-tauri/plugins/ai-chatbox/src/lib.rs` — rewrite as cdylib exports

### Modified Files (Frontend)
- `bedcode-desktop/src/plugin/types.ts` — rustLibrary field
- `bedcode-desktop/src/plugin/loader.ts` — recognize cdylib plugins
- `bedcode-desktop/src/plugin/components/PluginViewHost.vue` — provide PluginContext
- `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/plugin.json` — add rustLibrary
- `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/index.ts` — remove window hack
- `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/components/ChatView.vue` — inject context
- `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/composables/useAiChat.ts` — Rust commands
- `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/composables/usePromptOptimizer.ts` — Rust commands

### Deleted Files
- `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/services/openaiClient.ts`
- `bedcode-desktop/src-tauri/resources/plugins/desktop/com.bedcode.ai-chatbox/services/openaiClient.ts`
- `bedcode-desktop/src-tauri/resources/plugins/desktop/com.bedcode.ai-chatbox/index.js` (regenerated)

---

## Task 1: Add rust_library field to PluginManifest types

**Files:**
- Modify: `bedcode-desktop/src-tauri/bedcode-plugin-api/src/types.rs:8-39`
- Modify: `bedcode-desktop/src/plugin/types.ts:16-27`

- [ ] **Step 1: Add `rust_library` to Rust PluginManifest**

In `bedcode-plugin-api/src/types.rs`, add the `rust_library` field to `PluginManifest`:

```rust
/// cdylib 动态库文件名（不含路径，相对于插件目录）
/// 仅 rust-ts 类型插件使用
#[serde(default)]
pub rust_library: String,
```

- [ ] **Step 2: Add `rustLibrary` to TypeScript PluginManifest**

In `bedcode-desktop/src/plugin/types.ts`, add to `PluginManifest` interface:

```typescript
rustLibrary?: string
```

And add to `PluginInfo` interface:

```typescript
rustLibrary?: string
```

- [ ] **Step 3: Update DesktopPluginInfo to include rust_library**

In `bedcode-desktop/src-tauri/src/plugin/types.rs`, add `rust_library` field to `DesktopPluginInfo` and update the `From<&LoadedPlugin>` impl to map it.

- [ ] **Step 4: Commit**

```bash
git add bedcode-desktop/src-tauri/bedcode-plugin-api/src/types.rs bedcode-desktop/src/plugin/types.ts bedcode-desktop/src-tauri/src/plugin/types.rs
git commit -m "feat(plugin): add rust_library field to PluginManifest for cdylib support"
```

---

## Task 2: Add PluginSource::Cdylib variant

**Files:**
- Modify: `bedcode-desktop/src-tauri/src/plugin/types.rs:17-22`

- [ ] **Step 1: Add Cdylib variant to PluginSource**

In `plugin/types.rs`, update `PluginSource`:

```rust
pub enum PluginSource {
    /// 静态注册的 Rust 插件（通过 inventory::collect）
    StaticRegistry,
    /// 文件系统扫描的 TS-only 插件
    FileScan,
    /// cdylib 动态库加载的 Rust+TS 插件
    Cdylib,
}
```

- [ ] **Step 2: Commit**

```bash
git add bedcode-desktop/src-tauri/src/plugin/types.rs
git commit -m "feat(plugin): add PluginSource::Cdylib variant"
```

---

## Task 3: Implement HostContext FFI types and host_context.rs

**Files:**
- Create: `bedcode-desktop/src-tauri/src/plugin/host_context.rs`

- [ ] **Step 1: Write HostContext FFI struct definition**

Create `bedcode-desktop/src-tauri/src/plugin/host_context.rs` with:

1. `#[repr(C)]` `HostContext` struct matching the spec (plugin_id, free_string, storage_get/set/delete, db_execute, db_query, terminal_send_input, session_list, session_get, emit_event)
2. `HostContextFns` struct holding Arc references to Database, PluginStorage, SessionManager, AppHandle, PermissionManager
3. `impl HostContextFns` with `build_host_context(&self, plugin_id: &str) -> HostContext` method that creates the C function pointer struct
4. Each function pointer is an `extern "C"` fn that:
   - Converts raw pointers to Rust strings (with null checks)
   - Performs permission checks where needed
   - Calls the corresponding subsystem
   - Returns results as C strings or i32

Key implementation details for each function:

- **free_string**: drops the CString from raw pointer
- **storage_get/set/delete**: call `PluginStorage` methods, serialize/deserialize JSON
- **db_execute/db_query**: call `Database`, validate table name prefix `plugin_{sanitized_id}_`, execute SQL
- **terminal_send_input**: call `SessionManager::write_input`
- **session_list/get**: call `SessionManager` methods, serialize to JSON
- **emit_event**: call `app_handle.emit`

- [ ] **Step 2: Implement SQL table name validation**

Add `validate_sql_table_prefix(plugin_id: &str, sql: &str) -> Result<()>`:
- Sanitize plugin_id: replace `.` and `-` with `_`
- Extract table names from SQL using regex (CREATE TABLE, INSERT INTO, UPDATE, DELETE FROM, SELECT FROM)
- Verify each table name starts with `plugin_{sanitized_id}_`
- Return error if any table name doesn't match

- [ ] **Step 3: Commit**

```bash
git add bedcode-desktop/src-tauri/src/plugin/host_context.rs
git commit -m "feat(plugin): implement HostContext FFI for cdylib plugins"
```

---

## Task 4: Implement CdylibLoader

**Files:**
- Create: `bedcode-desktop/src-tauri/src/plugin/cdylib_loader.rs`
- Modify: `bedcode-desktop/src-tauri/Cargo.toml` — add `libloading = "0.8"`

- [ ] **Step 1: Add libloading dependency**

In `Cargo.toml`, add under `[dependencies]`:

```toml
libloading = "0.8"
```

- [ ] **Step 2: Write CdylibLoader**

Create `bedcode-desktop/src-tauri/src/plugin/cdylib_loader.rs` with:

1. `LoadedCdylibPlugin` struct: holds `Library` handle + `CdylibExports` function pointers
2. `CdylibExports` struct: typed function pointers for all 5 export functions
3. `CdylibLoader::load(plugin_dir: &Path, rust_library: &str) -> Result<LoadedCdylibPlugin>`:
   - Validate `rust_library` contains no path separators (security)
   - Resolve full path: `plugin_dir.join(rust_library_with_platform_suffix)`
   - Platform suffix: Windows `.dll`, macOS `.dylib`, Linux `.so`
   - `unsafe { Library::new(full_path) }?`
   - Load each export symbol via `library.get(b"bedcode_plugin_xxx")`
   - Return `LoadedCdylibPlugin`

- [ ] **Step 3: Commit**

```bash
git add bedcode-desktop/src-tauri/src/plugin/cdylib_loader.rs bedcode-desktop/src-tauri/Cargo.toml
git commit -m "feat(plugin): implement CdylibLoader for dynamic library loading"
```

---

## Task 5: Integrate cdylib loading into PluginHost and PluginLoader

**Files:**
- Modify: `bedcode-desktop/src-tauri/src/plugin/host.rs`
- Modify: `bedcode-desktop/src-tauri/src/plugin/loader.rs`
- Modify: `bedcode-desktop/src-tauri/src/plugin/api_bridge.rs`
- Modify: `bedcode-desktop/src-tauri/src/plugin.rs`

- [ ] **Step 1: Add cdylib_plugins and host_context_fns to PluginHost**

In `host.rs`:
- Add `cdylib_plugins: Arc<RwLock<HashMap<String, LoadedCdylibPlugin>>>` field
- Add `host_context_fns: Arc<HostContextFns>` field
- In `PluginHost::new()`: after file scan plugins, iterate and load cdylib plugins for any manifest with non-empty `rust_library`
- Store loaded cdylib handles in `cdylib_plugins`

- [ ] **Step 2: Update activate_plugin for cdylib**

In `host.rs` `activate_plugin()`:
- If plugin source is `Cdylib`, get `LoadedCdylibPlugin` from `cdylib_plugins`
- Build `HostContext` via `host_context_fns.build_host_context(plugin_id)`
- Call `exports.activate(&host_context)` with `catch_unwind` protection
- Update state to Activated

- [ ] **Step 3: Update deactivate_plugin for cdylib**

In `host.rs` `deactivate_plugin()`:
- If plugin source is `Cdylib`, call `exports.deactivate()` with `catch_unwind`
- Continue with existing cleanup (unregister, revoke permissions)

- [ ] **Step 4: Update invoke_rust_command for cdylib**

In `host.rs` `invoke_rust_command()`:
- If plugin source is `Cdylib`, get `LoadedCdylibPlugin`
- Convert `command_name` and `args` to `CString`
- Call `exports.invoke_command(name_ptr, args_ptr)` with `catch_unwind`
- Parse result JSON string, free via `free_string`
- Return parsed `serde_json::Value`

- [ ] **Step 5: Update PluginLoader to set source=Cdylib**

In `loader.rs` `load_all()`:
- After loading manifest, if `manifest.rust_library` is non-empty, set `source: PluginSource::Cdylib` in `LoadedPlugin`
- The actual dlopen happens in PluginHost::new() after all manifests are collected

- [ ] **Step 6: Export new modules in plugin.rs**

Add `pub mod cdylib_loader;` and `pub mod host_context;` to `plugin.rs`.

- [ ] **Step 7: Pass HostContextFns dependencies to PluginHost::new()**

Update `PluginHost::new()` signature to accept the dependencies needed for `HostContextFns`:

```rust
pub async fn new(
    db: Arc<Mutex<Database>>,
    plugins_dir: &Path,
    session_manager: Arc<SessionManager>,
    app_handle: Arc<tauri::AppHandle>,
) -> Self
```

In `PluginHost::new()`, construct `HostContextFns` from these parameters before loading cdylib plugins.

Update the call site in `lib.rs` (around line 253):

```rust
let plugin_host = Arc::new(
    tauri::async_runtime::block_on(
        plugin::PluginHost::new(
            db.clone(),
            &plugins_dir,
            session_manager.clone(),
            app_handle_arc.clone(),
        )
    )
);
```

- [ ] **Step 8: Commit**

```bash
git add bedcode-desktop/src-tauri/src/plugin/ bedcode-desktop/src-tauri/src/lib.rs
git commit -m "feat(plugin): integrate cdylib loading into PluginHost lifecycle"
```

---

## Task 6: Remove ai-chatbox from main Cargo.toml and rewrite as cdylib

**Files:**
- Modify: `bedcode-desktop/src-tauri/Cargo.toml` — remove `bedcode-plugin-ai-chatbox` dep
- Rewrite: `bedcode-desktop/src-tauri/plugins/ai-chatbox/Cargo.toml`
- Rewrite: `bedcode-desktop/src-tauri/plugins/ai-chatbox/src/lib.rs`

- [ ] **Step 1: Remove from main Cargo.toml**

Delete the line:
```toml
bedcode-plugin-ai-chatbox = { path = "plugins/ai-chatbox" }
```

- [ ] **Step 2: Rewrite ai-chatbox Cargo.toml as cdylib**

```toml
[package]
name = "bedcode-plugin-ai-chatbox"
version = "1.0.0"
description = "AI Chatbox Plugin — AI chat and terminal prompt optimization"
authors = ["BedCode"]
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["stream", "json"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync"] }
futures-util = "0.3"
tracing = "0.1"
anyhow = "1"
chrono = { version = "0.4", features = ["serde"] }
```

- [ ] **Step 3: Write host_api.rs — HostContext FFI type definitions**

Create `bedcode-desktop/src-tauri/plugins/ai-chatbox/src/host_api.rs` with:
- `#[repr(C)]` `HostContext` struct (mirror of host's definition)
- Helper methods: `plugin_id_str()`, `storage_get_json()`, `storage_set_json()`, `storage_delete_json()`, `db_execute_sql()`, `db_query_sql()`, `terminal_send()`, `emit()`
- These helpers wrap unsafe FFI calls with safe Rust interfaces

- [ ] **Step 4: Rewrite lib.rs as cdylib exports**

Rewrite `bedcode-desktop/src-tauri/plugins/ai-chatbox/src/lib.rs`:
- `static HOST_CONTEXT: OnceLock<HostContext> = OnceLock::new();`
- `bedcode_plugin_manifest()` — returns manifest JSON
- `bedcode_plugin_activate(host)` — stores HostContext, calls `db::init()`
- `bedcode_plugin_deactivate()` — no-op
- `bedcode_plugin_invoke_command(name, args_json)` — dispatches to commands module
- `bedcode_plugin_on_terminal_input()` — returns null (MVP)
- `bedcode_plugin_on_terminal_output()` — returns null (MVP)

- [ ] **Step 5: Commit**

```bash
git add bedcode-desktop/src-tauri/Cargo.toml bedcode-desktop/src-tauri/plugins/ai-chatbox/
git commit -m "refactor(ai-chatbox): rewrite as independent cdylib plugin"
```

---

## Task 7: Implement ai-chatbox Rust backend modules

**Files:**
- Create: `bedcode-desktop/src-tauri/plugins/ai-chatbox/src/ai_client.rs`
- Create: `bedcode-desktop/src-tauri/plugins/ai-chatbox/src/db.rs`
- Create: `bedcode-desktop/src-tauri/plugins/ai-chatbox/src/commands.rs`
- Create: `bedcode-desktop/src-tauri/plugins/ai-chatbox/src/terminal.rs`

- [ ] **Step 1: Write ai_client.rs — AI API client**

Implement:
- `ApiProvider` struct (name, api_key, base_url, model) with Deserialize
- `ChatMessage` struct (role, content) with Serialize
- `chat_complete(provider, messages) -> Result<String>` — non-streaming reqwest POST
- `chat_stream(provider, messages, event_name) -> Result<()>` — streaming SSE with reqwest, each chunk emitted via `host.emit_event()`
- SSE parsing: split on `\n\n`, parse `data: ` lines, extract `choices[0].delta.content`
- Handle `data: [DONE]` sentinel
- Error handling: HTTP status, network errors, JSON parse errors

- [ ] **Step 2: Write db.rs — custom SQLite table operations**

Implement:
- `init()` — CREATE TABLE IF NOT EXISTS for `plugin_com_bedcode_ai_chatbox_conversations` and `plugin_com_bedcode_ai_chatbox_messages`
- `list_conversations() -> Result<Vec<ConversationMeta>>` — SELECT from conversations table
- `get_messages(conv_id) -> Result<Vec<ChatMessage>>` — SELECT from messages table
- `save_conversation(conv)` — INSERT OR REPLACE
- `save_message(conv_id, msg)` — INSERT
- `delete_conversation(conv_id)` — DELETE from both tables
- All operations use `host.db_query()` and `host.db_execute()`

- [ ] **Step 3: Write commands.rs — command handlers**

Implement:
- `chat_stream(args_json) -> Result<Value>` — parse args, spawn tokio task for streaming, return stream_id immediately
- `chat_complete(args_json) -> Result<Value>` — parse args, call `ai_client::chat_complete()`, return result
- `optimize_prompt(args_json) -> Result<Value>` — parse args, construct system prompt, call `ai_client::chat_complete()`, return `{ original, optimized }`
- `list_conversations(args_json) -> Result<Value>` — call `db::list_conversations()`
- `get_messages(args_json) -> Result<Value>` — call `db::get_messages()`
- `save_conversation(args_json) -> Result<Value>` — call `db::save_conversation()`
- `save_message(args_json) -> Result<Value>` — call `db::save_message()`
- `delete_conversation(args_json) -> Result<Value>` — call `db::delete_conversation()`
- Update `lib.rs` invoke_command dispatch to include new commands

- [ ] **Step 4: Write terminal.rs — terminal handlers (MVP no-op)**

```rust
//! Terminal handlers (MVP: no-op)
//!
//! 预留终端输入/输出拦截扩展点，当前不做任何修改

// 终端处理器逻辑在 lib.rs 的 bedcode_plugin_on_terminal_input/output 中实现
// 此模块为未来扩展预留
```

- [ ] **Step 5: Update lib.rs module declarations**

Add `mod host_api; mod ai_client; mod db; mod commands; mod terminal;` and update invoke_command dispatch table.

- [ ] **Step 6: Commit**

```bash
git add bedcode-desktop/src-tauri/plugins/ai-chatbox/src/
git commit -m "feat(ai-chatbox): implement Rust backend — AI client, DB, commands, terminal"
```

---

## Task 8: Update ai-chatbox plugin.json and frontend TS

**Files:**
- Modify: `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/plugin.json`
- Modify: `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/index.ts`
- Modify: `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/components/ChatView.vue`
- Modify: `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/composables/useAiChat.ts`
- Modify: `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/composables/usePromptOptimizer.ts`
- Delete: `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/services/openaiClient.ts`

- [ ] **Step 1: Update plugin.json**

Add `rustLibrary` field and update contributes:

```json
{
  "id": "com.bedcode.ai-chatbox",
  "name": "AI Chatbox",
  "version": "1.0.0",
  "description": "AI 大模型对话与终端提示词优化",
  "author": "BedCode",
  "main": "index.js",
  "sandbox": "inline",
  "pluginType": "rust-ts",
  "rustLibrary": "bedcode_plugin_ai_chatbox.dll",
  "permissions": ["ui:sidebar", "ui:input", "storage", "terminal:input", "terminal:output", "session:read"],
  "contributes": {
    "commands": [
      { "id": "ai-chatbox.chat-stream", "title": "AI Chat Stream" },
      { "id": "ai-chatbox.chat-complete", "title": "AI Chat Complete" },
      { "id": "ai-chatbox.optimize-prompt", "title": "Optimize Prompt" },
      { "id": "ai-chatbox.list-conversations", "title": "List Conversations" },
      { "id": "ai-chatbox.get-messages", "title": "Get Messages" },
      { "id": "ai-chatbox.save-conversation", "title": "Save Conversation" },
      { "id": "ai-chatbox.save-message", "title": "Save Message" },
      { "id": "ai-chatbox.delete-conversation", "title": "Delete Conversation" }
    ],
    "views": [
      { "id": "ai-chatbox.sidebar", "type": "sidebar", "title": "AI 对话", "component": "ChatView" }
    ],
    "terminal": {
      "inputHandlers": ["on_terminal_input"],
      "outputParsers": []
    },
    "configuration": {
      "title": "AI Chatbox Settings",
      "properties": {
        "apiProviders": { "type": "string", "title": "API Providers (JSON)", "description": "JSON array of API provider configs", "default": "[]" },
        "activeProvider": { "type": "string", "title": "Active Provider Name", "default": "" }
      }
    }
  }
}
```

- [ ] **Step 2: Rewrite index.ts — remove window hack**

Remove all `(window as any).__ai_chatbox_context__` and `__ai_chatbox_optimizer__` references. Pass `context` directly to composables. Use `provide/inject` pattern instead.

```typescript
import ChatView from './components/ChatView.vue'
import { usePromptOptimizer } from './composables/usePromptOptimizer'
import type { PluginContext } from '../../plugin/types'

export async function activate(context: PluginContext): Promise<void> {
  const optimizer = usePromptOptimizer(context)

  context.ui.registerSidebarPanel({
    id: 'ai-chatbox.sidebar',
    title: 'AI 对话',
    component: ChatView,
  })

  context.ui.registerTerminalToolbarItem({
    id: 'ai-optimize-prompt',
    label: 'AI 优化',
    icon: '✨',
    onClick: () => optimizer.optimizePrompt(),
  })
}

export async function deactivate(): Promise<void> {}
```

- [ ] **Step 3: Update ChatView.vue — use inject for context**

Replace `window.__ai_chatbox_context__` with `inject('pluginContext')`. Pass context to composables.

- [ ] **Step 4: Rewrite useAiChat.ts — call Rust commands**

Replace `chatStream()` from `openaiClient.ts` with:
- `context.commands.execute('ai-chatbox.chat-stream', { streamId, provider, messages })` to start stream
- `context.events.on('ai-chatbox:stream:' + streamId, handler)` to receive chunks
- `context.commands.execute('ai-chatbox.list-conversations', {})` for history loading
- `context.commands.execute('ai-chatbox.save-message', { convId, msg })` for persistence

- [ ] **Step 5: Rewrite usePromptOptimizer.ts — call Rust command**

Replace `chat()` from `openaiClient.ts` with:
- `context.commands.execute('ai-chatbox.optimize-prompt', { provider, prompt })`

- [ ] **Step 6: Delete openaiClient.ts**

Delete `bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/services/openaiClient.ts`.

- [ ] **Step 7: Update PluginViewHost.vue — provide PluginContext**

In `PluginViewHost.vue`, add `provide('pluginContext', context)` when rendering plugin components, so they can `inject` it.

- [ ] **Step 8: Commit**

```bash
git add bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/ bedcode-desktop/src/plugin/components/PluginViewHost.vue
git commit -m "refactor(ai-chatbox): frontend uses Rust commands, remove window hack and openaiClient"
```

---

## Task 9: Update resources plugin.json and rebuild index.js

**Files:**
- Modify: `bedcode-desktop/src-tauri/resources/plugins/desktop/com.bedcode.ai-chatbox/plugin.json`
- Regenerate: `bedcode-desktop/src-tauri/resources/plugins/desktop/com.bedcode.ai-chatbox/index.js`

- [ ] **Step 1: Update resources plugin.json**

Copy the updated `plugin.json` from `src/plugins/` to `src-tauri/resources/plugins/desktop/com.bedcode.ai-chatbox/plugin.json`.

- [ ] **Step 2: Rebuild index.js**

Run the vite build for the plugin:

```bash
cd bedcode-desktop/src/plugins/com.bedcode.ai-chatbox
npx vite build
```

This regenerates `bedcode-desktop/src-tauri/resources/plugins/desktop/com.bedcode.ai-chatbox/index.js`.

- [ ] **Step 3: Delete resources openaiClient.ts**

Delete `bedcode-desktop/src-tauri/resources/plugins/desktop/com.bedcode.ai-chatbox/services/openaiClient.ts` (if it exists as a separate file in resources).

- [ ] **Step 4: Commit**

```bash
git add bedcode-desktop/src-tauri/resources/plugins/desktop/com.bedcode.ai-chatbox/
git commit -m "build(ai-chatbox): update resources with new plugin.json and rebuilt index.js"
```

---

## Task 10: Build and verify

**Files:** None (verification only)

- [ ] **Step 1: Build ai-chatbox cdylib**

```bash
cd bedcode-desktop/src-tauri/plugins/ai-chatbox
cargo build --release
```

Copy the output DLL to the resources directory:

```bash
cp target/release/bedcode_plugin_ai_chatbox.dll ../resources/plugins/desktop/com.bedcode.ai-chatbox/
```

- [ ] **Step 2: Build main application**

```bash
cd bedcode-desktop
npm run tauri:dev
```

- [ ] **Step 3: Verify plugin loads**

Check console output for:
- `[PluginHost] Static plugin loaded: ...` (existing plugins)
- `[PluginHost] Cdylib plugin loaded: com.bedcode.ai-chatbox`
- `[PluginLoader] Rust+TS plugin frontend loaded: com.bedcode.ai-chatbox`

- [ ] **Step 4: Verify AI chat functionality**

1. Open sidebar → click "AI 对话"
2. Configure a provider (e.g., DeepSeek with API key)
3. Send a message → verify streaming response
4. Click terminal toolbar "✨ AI 优化" → verify prompt optimization dialog

- [ ] **Step 5: Commit final state**

```bash
git add -A
git commit -m "feat(plugin): cdylib dynamic loading + ai-chatbox Rust backend complete"
```
