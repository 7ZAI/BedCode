# cdylib 插件动态加载 + ai-chatbox 重写设计

> 插件系统新增 cdylib 动态加载机制，ai-chatbox 作为首个 cdylib 插件重写为 Rust+TS 双端实现。

## 目标

1. 插件系统支持 Rust 端以 cdylib 动态库形式加载，独立于主应用编译
2. ai-chatbox 重写为 cdylib 插件，Rust 端提供 AI API 客户端、自定义 SQLite 表、自定义 Tauri commands、终端处理器
3. 保持现有 inventory 静态注册和 TS-only 文件扫描两种加载方式不变

---

## 1. C ABI + JSON 桥接规范

### 1.1 设计原则

- **C ABI 边界**：所有跨动态库的调用只使用 C 兼容类型（`*const c_char`、`i32`、`*const void`）
- **JSON 序列化**：复杂数据（manifest、command args/result、storage values）通过 JSON 字符串跨 FFI
- **宿主注入**：宿主通过 `HostContext` 指针向插件提供能力，插件不直接访问宿主内存
- **内存所有权**：JSON 字符串由分配方拥有，对方只读不释放；宿主提供的 `free_string` 统一释放

### 1.2 插件导出函数

每个 cdylib 插件必须导出以下 `#[no_mangle]` C 函数：

```rust
/// 返回插件 manifest JSON 字符串
/// 调用方必须通过 host.free_string() 释放返回值
#[no_mangle]
pub extern "C" fn bedcode_plugin_manifest() -> *mut c_char;

/// 激活插件，传入 HostContext 指针
/// 返回 0 表示成功，非 0 表示失败
#[no_mangle]
pub extern "C" fn bedcode_plugin_activate(host: *const HostContext) -> i32;

/// 停用插件
/// 返回 0 表示成功
#[no_mangle]
pub extern "C" fn bedcode_plugin_deactivate() -> i32;

/// 调用插件注册的自定义 command
/// args_json: 命令参数 JSON 字符串
/// 返回结果 JSON 字符串（调用方通过 host.free_string 释放）
/// 返回 null 表示命令不存在或执行失败
#[no_mangle]
pub extern "C" fn bedcode_plugin_invoke_command(
    command_name: *const c_char,
    args_json: *const c_char,
) -> *mut c_char;

/// 调用终端输入处理器
/// input_json: { "session_id": "...", "text": "..." }
/// 返回修改后的文本 JSON 字符串（{ "text": "..." }），或 null 表示不修改
#[no_mangle]
pub extern "C" fn bedcode_plugin_on_terminal_input(
    input_json: *const c_char,
) -> *mut c_char;

/// 调用终端输出处理器
/// output_json: { "session_id": "...", "data": "..." }
/// 返回修改后的数据 JSON 字符串（{ "data": "..." }），或 null 表示不修改
#[no_mangle]
pub extern "C" fn bedcode_plugin_on_terminal_output(
    output_json: *const c_char,
) -> *mut c_char;
```

### 1.3 HostContext（宿主注入插件的能力上下文）

```rust
/// 宿主向插件注入的能力上下文
/// 所有函数指针由宿主实现，插件通过调用这些函数访问宿主能力
#[repr(C)]
pub struct HostContext {
    /// 插件 ID（宿主设置，插件只读）
    pub plugin_id: *const c_char,

    /// 释放由宿主或插件分配的 JSON 字符串
    pub free_string: extern "C" fn(*mut c_char),

    // ==================== Storage API ====================

    /// storage.get(plugin_id, key) → JSON value string or null
    pub storage_get: extern "C" fn(*const c_char, *const c_char) -> *mut c_char,

    /// storage.set(plugin_id, key, value_json) → 0 success
    pub storage_set: extern "C" fn(*const c_char, *const c_char, *const c_char) -> i32,

    /// storage.delete(plugin_id, key) → 0 success
    pub storage_delete: extern "C" fn(*const c_char, *const c_char) -> i32,

    // ==================== Database API ====================

    /// 执行 SQL 语句（CREATE TABLE, INSERT, UPDATE, DELETE）
    /// sql: SQL 语句字符串
    /// params_json: 参数数组 JSON（如 ["value1", 42]），可为 null
    /// 返回受影响行数（负数表示错误）
    pub db_execute: extern "C" fn(*const c_char, *const c_char) -> i32,

    /// 执行 SQL 查询（SELECT）
    /// sql: SQL 查询语句
    /// params_json: 参数数组 JSON，可为 null
    /// 返回结果行数组 JSON（[{ "col1": val1, ... }, ...]），或 null
    pub db_query: extern "C" fn(*const c_char, *const c_char) -> *mut c_char,

    // ==================== Terminal API ====================

    /// 向指定会话的 PTY 发送输入
    /// 返回 0 成功
    pub terminal_send_input: extern "C" fn(*const c_char, *const c_char) -> i32,

    // ==================== Session API ====================

    /// 获取会话列表 JSON
    pub session_list: extern "C" fn() -> *mut c_char,

    /// 获取指定会话 JSON
    pub session_get: extern "C" fn(*const c_char) -> *mut c_char,

    // ==================== Event API ====================

    /// 向前端发送事件
    /// event_name: 事件名，payload_json: 事件数据 JSON
    pub emit_event: extern "C" fn(*const c_char, *const c_char),
}
```

### 1.4 manifest JSON 格式

`bedcode_plugin_manifest()` 返回的 JSON 与现有 `plugin.json` 格式完全一致，增加 `plugin_type: "rust-ts"` 和 `rust_library` 字段：

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
      { "id": "ai-chatbox.optimize-prompt", "title": "Optimize Prompt" }
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
        "apiProviders": { "type": "string", "title": "API Providers (JSON)", "default": "[]" },
        "activeProvider": { "type": "string", "title": "Active Provider Name", "default": "" }
      }
    }
  }
}
```

`rustLibrary` 字段指定动态库文件名（相对于插件目录），宿主根据平台自动添加后缀：
- Windows: `.dll`
- macOS: `.dylib`
- Linux: `.so`

**manifest 来源优先级**：磁盘上的 `plugin.json` 是权威来源，`bedcode_plugin_manifest()` 返回的 JSON 仅用于校验（验证 id/version 一致）。若两者冲突，以 `plugin.json` 为准。

### 1.5 数据流

```
┌─────────────────────────────────────────────────────────────────┐
│                        宿主进程 (bedcode-desktop)                │
│                                                                 │
│  PluginHost                                                     │
│    │                                                            │
│    ├─ dlopen(rust_library)                                      │
│    │   ├─ bedcode_plugin_manifest() → JSON → PluginManifest     │
│    │   ├─ bedcode_plugin_activate(&HostContext) → i32           │
│    │   ├─ bedcode_plugin_invoke_command(name, args_json)        │
│    │   │   → result_json                                        │
│    │   └─ bedcode_plugin_on_terminal_input/output(json)         │
│    │       → modified_json / null                               │
│    │                                                            │
│    ├─ HostContext 函数实现                                       │
│    │   ├─ storage_get/set/delete → PluginStorage (SQLite)       │
│    │   ├─ db_execute/db_query → Database (SQLite)               │
│    │   ├─ terminal_send_input → SessionManager.write_input       │
│    │   ├─ session_list/get → SessionManager                     │
│    │   └─ emit_event → Tauri app_handle.emit                    │
│    │                                                            │
│    └─ api_bridge (Tauri commands)                               │
│        └─ plugin_invoke → PluginHost.invoke_rust_command        │
│            → bedcode_plugin_invoke_command()                    │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. 插件系统改动

### 2.1 新增 PluginSource 变体

`plugin/types.rs` 的 `PluginSource` 枚举新增 `Cdylib`：

```rust
pub enum PluginSource {
    StaticRegistry,  // inventory 静态注册
    FileScan,        // TS-only 文件扫描
    Cdylib,          // cdylib 动态库加载
}
```

### 2.2 新增 CdylibLoader

`plugin/cdylib_loader.rs` — 负责加载 cdylib 动态库：

```rust
pub struct CdylibLoader;

impl CdylibLoader {
    /// 扫描插件目录，对包含 rustLibrary 字段的 plugin.json 加载对应动态库
    ///
    /// 流程：
    /// 1. PluginLoader 扫描 plugin.json（已有逻辑）
    /// 2. 若 manifest.rust_library 非空，CdylibLoader 加载动态库
    /// 3. 调用 bedcode_plugin_manifest() 获取 Rust 端 manifest（与 plugin.json 合并）
    /// 4. 返回 LoadedCdylibPlugin（含 Library 句柄和导出函数指针）
    pub fn load(plugin_dir: &Path, manifest: &PluginManifest) -> Result<LoadedCdylibPlugin>
}
```

`LoadedCdylibPlugin` 结构体：

```rust
pub struct LoadedCdylibPlugin {
    /// 动态库句柄（libloading::Library）
    library: Library,
    /// 导出函数指针缓存
    exports: CdylibExports,
}

pub struct CdylibExports {
    pub activate: unsafe extern "C" fn(*const HostContext) -> i32,
    pub deactivate: unsafe extern "C" fn() -> i32,
    pub invoke_command: unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_char,
    pub on_terminal_input: unsafe extern "C" fn(*const c_char) -> *mut c_char,
    pub on_terminal_output: unsafe extern "C" fn(*const c_char) -> *mut c_char,
}
```

### 2.3 PluginHost 改动

`PluginHost` 新增 cdylib 插件管理：

```rust
pub struct PluginHost {
    // ... 现有字段 ...
    /// cdylib 插件句柄（plugin_id → LoadedCdylibPlugin）
    cdylib_plugins: Arc<RwLock<HashMap<String, LoadedCdylibPlugin>>>,
    /// HostContext 函数实现（共享引用，所有 cdylib 插件共用）
    host_context_fns: Arc<HostContextFns>,
}
```

**activate_plugin 改动**：对 cdylib 插件，构造 `HostContext` 并调用 `bedcode_plugin_activate(&host_context)`。

**invoke_rust_command 改动**：对 cdylib 插件，调用 `bedcode_plugin_invoke_command()` 而非内存中的 handler。

**终端处理器路由**：PTY 输出监听器中，遍历所有已激活的 cdylib 插件，调用 `on_terminal_input/output`。

### 2.4 HostContextFns 实现

`plugin/host_context.rs` — 实现 `HostContext` 中的函数指针：

```rust
pub struct HostContextFns {
    db: Arc<Mutex<Database>>,
    storage: Arc<PluginStorage>,
    session_manager: Arc<SessionManager>,
    app_handle: Arc<tauri::AppHandle>,
    permission: Arc<PermissionManager>,
}
```

每个函数指针对应一个 `extern "C"` 函数，内部：
1. 从原始指针恢复 Rust 类型
2. 执行权限校验
3. 调用对应的宿主子系统
4. 返回结果

**关键安全措施**：
- 所有 `*const c_char` 参数在进入时立即转为 `CString`，空指针检查
- `db_execute`/`db_query` 限制只能操作以 `plugin_{plugin_id}_` 为前缀的表，防止跨插件数据访问
- `storage_get/set/delete` 复用现有 `PluginStorage`，天然按 plugin_id 隔离

### 2.5 自定义 SQLite 表隔离

cdylib 插件通过 `db_execute`/`db_query` 创建和操作自己的表。**表名必须以 `plugin_{plugin_id}_` 为前缀**，宿主在执行前校验：

```rust
fn validate_table_name(plugin_id: &str, sql: &str) -> Result<()> {
    // 解析 SQL 中的表名，校验前缀
    // 例如 plugin_id = "com.bedcode.ai-chatbox"
    // 允许的表名模式: plugin_com_bedcode_ai_chatbox_xxx
    // （将 plugin_id 中的 . 和 - 替换为 _）
}
```

### 2.6 前端 PluginLoader 改动

`src/plugin/loader.ts` 的 `loadAll()` 中，对 `pluginType === 'rust-ts'` 且 `rustLibrary` 非空的插件：
- 走 `loadFrontendOnly` 路径（Rust 端已由 PluginHost 通过 cdylib 激活）
- 前端只加载 TS 入口文件，创建 PluginContext，调用 `activate(context)`

### 2.7 前端 context.ts 改动

`commands.execute()` 中，对 Rust command 的调用走 `plugin_invoke`（已有），无需改动。

---

## 3. ai-chatbox 插件重写

### 3.1 目录结构

```
bedcode-desktop/src-tauri/plugins/ai-chatbox/
├── Cargo.toml                    # 独立 crate，编译为 cdylib
├── src/
│   ├── lib.rs                    # cdylib 导出函数 + 插件主逻辑
│   ├── ai_client.rs              # AI API 客户端（reqwest + SSE 流式）
│   ├── db.rs                     # 自定义 SQLite 表操作
│   ├── commands.rs               # 自定义 Tauri command 处理函数
│   └── terminal.rs               # 终端处理器实现
└── build.rs                      # 无需特殊构建脚本

bedcode-desktop/src/plugins/com.bedcode.ai-chatbox/
├── plugin.json                   # 插件描述（含 rustLibrary 字段）
├── index.ts                      # TS 入口（注册 UI 组件）
├── vite.config.ts                # 编译配置
├── types.ts
├── components/
│   ├── ChatView.vue
│   ├── ChatMessage.vue
│   ├── ChatInput.vue
│   ├── ProviderManager.vue
│   └── PromptOptimizeDialog.vue
├── composables/
│   ├── useAiChat.ts              # 改为调用 Rust command
│   ├── useAiConfig.ts
│   └── usePromptOptimizer.ts     # 改为调用 Rust command
└── services/
    └── openaiClient.ts           # 删除，AI 调用移到 Rust 端
```

### 3.2 Rust 端实现

#### 3.2.1 Cargo.toml

```toml
[package]
name = "bedcode-plugin-ai-chatbox"
version = "1.0.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["stream", "json"] }
tokio = { version = "1", features = ["rt", "macros", "sync"] }
futures-util = "0.3"
tracing = "0.1"
anyhow = "1"
chrono = { version = "0.4", features = ["serde"] }
```

**关键**：`crate-type = ["cdylib"]`，不依赖 `bedcode-plugin-api`（避免编译期耦合），不依赖 `inventory`。

#### 3.2.2 lib.rs — cdylib 导出

```rust
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::sync::OnceLock;

// 宿主注入的上下文（activate 时设置）
static HOST_CONTEXT: OnceLock<HostContext> = OnceLock::new();

#[no_mangle]
pub extern "C" fn bedcode_plugin_manifest() -> *mut c_char {
    let json = serde_json::json!({
        "id": "com.bedcode.ai-chatbox",
        "name": "AI Chatbox",
        "version": "1.0.0",
        // ... 完整 manifest
    });
    CString::new(json.to_string()).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn bedcode_plugin_activate(host: *const HostContext) -> c_int {
    if host.is_null() { return 1; }
    let ctx = unsafe { &*host };
    HOST_CONTEXT.set(ctx.clone()).map_err(|_| 1).err().map_or(0, |_| 1)
    // 初始化自定义数据库表
    // db_init()
}

#[no_mangle]
pub extern "C" fn bedcode_plugin_deactivate() -> c_int {
    0
}

#[no_mangle]
pub extern "C" fn bedcode_plugin_invoke_command(
    command_name: *const c_char,
    args_json: *const c_char,
) -> *mut c_char {
    let name = unsafe { CStr::from_ptr(command_name) }.to_str().unwrap_or("");
    let args = unsafe { CStr::from_ptr(args_json) }.to_str().unwrap_or("{}");

    let result = match name {
        "ai-chatbox.chat-stream" => commands::chat_stream(args),
        "ai-chatbox.chat-complete" => commands::chat_complete(args),
        "ai-chatbox.optimize-prompt" => commands::optimize_prompt(args),
        _ => Err(anyhow::anyhow!("Unknown command: {}", name)),
    };

    match result {
        Ok(val) => CString::new(val.to_string()).unwrap().into_raw(),
        Err(_) => ptr::null_mut(),
    }
}
```

#### 3.2.3 ai_client.rs — AI API 客户端

```rust
/// 流式聊天请求
/// 返回 SSE 事件流，宿主通过 emit_event 逐 chunk 推送到前端
pub async fn chat_stream(
    provider: &ApiProvider,
    messages: Vec<ChatMessage>,
    event_name: &str,
) -> anyhow::Result<()> {
    let host = HOST_CONTEXT.get().ok_or_else(|| anyhow::anyhow!("Not activated"))?;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/chat/completions", provider.base_url))
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .json(&serde_json::json!({
            "model": provider.model,
            "messages": messages,
            "stream": true,
        }))
        .send()
        .await?;

    // SSE 流式解析
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // 解析 SSE 行
        for line in buffer.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" { continue; }
                if let Ok(parsed) = serde_json::from_str::<SseResponse>(data) {
                    if let Some(content) = parsed.choices[0].delta.content.as_ref() {
                        // 通过宿主事件推送到前端
                        let payload = serde_json::json!({ "chunk": content });
                        let payload_cstr = CString::new(payload.to_string()).unwrap();
                        (host.emit_event)(
                            CString::new(event_name).unwrap().as_ptr(),
                            payload_cstr.as_ptr(),
                        );
                    }
                }
            }
        }
        buffer.clear(); // 简化处理，实际需保留不完整行
    }

    Ok(())
}
```

#### 3.2.4 db.rs — 自定义 SQLite 表

```rust
/// 初始化插件自定义表
fn db_init() -> anyhow::Result<()> {
    let host = HOST_CONTEXT.get().ok_or_else(|| anyhow::anyhow!("Not activated"))?;
    let plugin_id_cstr = CString::new(host.plugin_id_str()).unwrap();

    let sql = CString::new(
        "CREATE TABLE IF NOT EXISTS plugin_com_bedcode_ai_chatbox_conversations (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            provider_name TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS plugin_com_bedcode_ai_chatbox_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            FOREIGN KEY (conversation_id) REFERENCES plugin_com_bedcode_ai_chatbox_conversations(id)
        );"
    ).unwrap();

    let result = (host.db_execute)(sql.as_ptr(), ptr::null());
    if result < 0 {
        return Err(anyhow::anyhow!("Failed to create tables: {}", result));
    }
    Ok(())
}
```

**优势**：对话历史使用结构化表存储，支持高效查询（如按时间排序、分页加载），而非 JSON key-value 的全量读写。

#### 3.2.5 commands.rs — 自定义 Tauri commands

```rust
/// 非流式聊天（用于提示词优化等短回复场景）
pub fn chat_complete(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: ChatCompleteArgs = serde_json::from_str(args_json)?;
    // 同步调用 reqwest（在 tokio runtime 中）
    let rt = tokio::runtime::Handle::current();
    let result = rt.block_on(async {
        ai_client::chat_complete(&args.provider, &args.messages).await
    })?;
    Ok(serde_json::json!({ "content": result }))
}

/// 流式聊天（启动异步任务，通过事件推送 chunks）
pub fn chat_stream(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: ChatStreamArgs = serde_json::from_str(args_json)?;
    let event_name = format!("ai-chatbox:stream:{}", args.stream_id);

    // spawn 异步任务，立即返回 stream_id
    tokio::spawn(async move {
        match ai_client::chat_stream(&args.provider, &args.messages, &event_name).await {
            Ok(()) => { /* emit done event */ }
            Err(e) => { /* emit error event */ }
        }
    });

    Ok(serde_json::json!({ "streamId": args.stream_id }))
}

/// 提示词优化
pub fn optimize_prompt(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: OptimizePromptArgs = serde_json::from_str(args_json)?;
    let rt = tokio::runtime::Handle::current();
    let optimized = rt.block_on(async {
        ai_client::chat_complete(&args.provider, &[
            ChatMessage { role: "system".into(), content: OPTIMIZE_SYSTEM_PROMPT.into() },
            ChatMessage { role: "user".into(), content: args.prompt.clone() },
        ]).await
    })?;
    Ok(serde_json::json!({ "original": args.prompt, "optimized": optimized }))
}
```

#### 3.2.6 terminal.rs — 终端处理器

```rust
/// 终端输入处理器（MVP：不做修改，返回 null）
#[no_mangle]
pub extern "C" fn bedcode_plugin_on_terminal_input(
    input_json: *const c_char,
) -> *mut c_char {
    // MVP 阶段不拦截终端输入
    ptr::null_mut()
}

/// 终端输出处理器（MVP：不做修改，返回 null）
#[no_mangle]
pub extern "C" fn bedcode_plugin_on_terminal_output(
    output_json: *const c_char,
) -> *mut c_char {
    // MVP 阶段不拦截终端输出
    ptr::null_mut()
}
```

终端处理器在 MVP 阶段为空实现，但架构已预留。未来可扩展为：
- 检测终端输出中的错误信息，自动建议修复
- 拦截特定命令输入，提供 AI 辅助

### 3.3 前端 TS 端改动

#### 3.3.1 index.ts — 去掉 window 全局变量 hack

```typescript
export async function activate(context: PluginContext): Promise<void> {
  // 注册侧边栏面板（ChatView 通过 context 传递，不再用 window 全局变量）
  context.ui.registerSidebarPanel({
    id: 'ai-chatbox.sidebar',
    title: 'AI 对话',
    component: ChatView,
  })

  // 终端提示词优化
  const optimizer = usePromptOptimizer(context)

  // 注册终端工具栏按钮
  context.ui.registerTerminalToolbarItem({
    id: 'ai-optimize-prompt',
    label: 'AI 优化',
    icon: '✨',
    onClick: () => optimizer.optimizePrompt(),
  })
}
```

#### 3.3.2 ChatView.vue — 通过 props 接收 context

不再从 `window.__ai_chatbox_context__` 读取，改为通过 Vue provide/inject 或组件 props 传递 PluginContext。

**方案**：在 `PluginViewHost.vue` 渲染插件组件时，通过 `provide('pluginContext', context)` 注入，插件组件用 `inject('pluginContext')` 获取。

#### 3.3.3 useAiChat.ts — 改为调用 Rust command

```typescript
// 之前：直接 fetch 调用 AI API
// 之后：通过 context.commands.execute 调用 Rust 端 command

async function sendMessage(content: string): Promise<void> {
  const streamId = Date.now().toString(36)

  // 调用 Rust 端 chat-stream command
  await context.commands.execute('ai-chatbox.chat-stream', {
    streamId,
    provider: activeProvider.value,
    messages: requestMessages,
  })

  // 监听流式事件
  const disposable = context.events.on(`ai-chatbox:stream:${streamId}`, (data: any) => {
    if (data.chunk) {
      streamingContent.value += data.chunk
      // 更新消息
    } else if (data.done) {
      sending.value = false
      disposable.dispose()
    } else if (data.error) {
      // 错误处理
    }
  })
}
```

#### 3.3.4 usePromptOptimizer.ts — 改为调用 Rust command

```typescript
async function optimizePrompt(): Promise<void> {
  // 获取终端当前输入（同之前逻辑）
  const input = await getCurrentInput()

  // 调用 Rust 端 optimize-prompt command
  const result = await context.commands.execute('ai-chatbox.optimize-prompt', {
    provider: await getActiveProvider(),
    prompt: input.text,
  })

  optimizedText.value = result.optimized
}
```

#### 3.3.5 删除 openaiClient.ts

AI API 调用完全移到 Rust 端，前端不再直接 fetch。

---

## 4. 编译与部署

### 4.1 插件独立编译

```bash
# 编译 ai-chatbox cdylib
cd bedcode-desktop/src-tauri/plugins/ai-chatbox
cargo build --release

# 输出：target/release/bedcode_plugin_ai_chatbox.dll (Windows)
#       target/release/libbedcode_plugin_ai_chatbox.so (Linux)
#       target/release/libbedcode_plugin_ai_chatbox.dylib (macOS)
```

### 4.2 部署位置

编译产物复制到插件目录：

```
resources/plugins/desktop/com.bedcode.ai-chatbox/
├── plugin.json
├── index.js                       # TS 前端编译产物
├── bedcode_plugin_ai_chatbox.dll  # Rust cdylib 编译产物
└── components/                    # (仅开发时存在，编译后打包进 index.js)
```

### 4.3 主应用 Cargo.toml 改动

**移除** `bedcode-plugin-ai-chatbox` 依赖：

```toml
# 删除这行
# bedcode-plugin-ai-chatbox = { path = "plugins/ai-chatbox" }
```

**新增** `libloading` 依赖（用于 dlopen）：

```toml
libloading = "0.8"
```

### 4.4 构建脚本

新增 `scripts/build-plugin.sh`（或集成到 npm scripts）：

```bash
#!/bin/bash
# 编译指定插件的 cdylib 并复制到 resources 目录
PLUGIN_NAME=$1
cd "src-tauri/plugins/$PLUGIN_NAME"
cargo build --release
# 复制到 resources 目录（根据平台选择文件）
cp target/release/${LIB_PREFIX}bedcode_plugin_${PLUGIN_NAME}.${EXT} \
   ../resources/plugins/desktop/com.bedcode.${PLUGIN_NAME}/
```

---

## 5. 安全考量

| 风险 | 缓解措施 |
|------|----------|
| cdylib 代码与主进程同地址空间，crash 会影响主进程 | 插件 activate/deactivate/command 均包裹 `catch_unwind`，防止 panic 传播 |
| 恶意插件通过 db_execute 操作其他插件的表 | 表名前缀校验，拒绝不匹配前缀的 SQL |
| 恶意插件通过 HostContext 访问未授权能力 | 每个 HostContext 函数内部做权限校验（与 api_bridge 一致） |
| cdylib 路径遍历攻击 | rustLibrary 字段只允许文件名（不含路径分隔符） |
| Rust 版本不兼容导致 ABI 崩溃 | C ABI 只用原语类型，避免 Rust 特有类型跨边界；文档要求插件与主应用使用相同 Rust 版本编译 |

---

## 6. 修改文件清单

### Rust 端

| 文件 | 变更 |
|------|------|
| `src-tauri/Cargo.toml` | 移除 `bedcode-plugin-ai-chatbox` 依赖，新增 `libloading` |
| `src-tauri/src/plugin/host.rs` | 新增 cdylib 插件加载/激活/停用/command 调用逻辑 |
| `src-tauri/src/plugin/cdylib_loader.rs` | **新增**：cdylib 加载器 |
| `src-tauri/src/plugin/host_context.rs` | **新增**：HostContext 函数实现 |
| `src-tauri/src/plugin/types.rs` | `PluginSource` 新增 `Cdylib` 变体 |
| `src-tauri/src/plugin/loader.rs` | 扫描时识别 `rustLibrary` 字段，触发 CdylibLoader |
| `src-tauri/src/plugin/api_bridge.rs` | `plugin_invoke` 路由到 cdylib 的 `invoke_command` |
| `src-tauri/src/plugin.rs` | 导出新模块 |
| `src-tauri/bedcode-plugin-api/src/types.rs` | `PluginManifest` 新增 `rust_library` 字段 |

### 前端

| 文件 | 变更 |
|------|------|
| `src/plugin/types.ts` | `PluginManifest` 新增 `rustLibrary` 字段 |
| `src/plugin/loader.ts` | 识别 `rustLibrary`，走 `loadFrontendOnly` 路径 |
| `src/plugin/components/PluginViewHost.vue` | 通过 `provide` 注入 PluginContext |
| `src/plugins/com.bedcode.ai-chatbox/index.ts` | 去掉 window 全局变量 hack |
| `src/plugins/com.bedcode.ai-chatbox/components/ChatView.vue` | 用 `inject` 获取 context |
| `src/plugins/com.bedcode.ai-chatbox/composables/useAiChat.ts` | 改为调用 Rust command |
| `src/plugins/com.bedcode.ai-chatbox/composables/usePromptOptimizer.ts` | 改为调用 Rust command |
| `src/plugins/com.bedcode.ai-chatbox/services/openaiClient.ts` | **删除** |
| `src/plugins/com.bedcode.ai-chatbox/plugin.json` | 新增 `rustLibrary` 字段 |

### ai-chatbox Rust crate（重写）

| 文件 | 变更 |
|------|------|
| `plugins/ai-chatbox/Cargo.toml` | 改为 `crate-type = ["cdylib"]`，移除 `bedcode-plugin-api` 依赖 |
| `plugins/ai-chatbox/src/lib.rs` | **重写**：cdylib 导出函数 |
| `plugins/ai-chatbox/src/ai_client.rs` | **新增**：reqwest + SSE 流式 AI 客户端 |
| `plugins/ai-chatbox/src/db.rs` | **新增**：自定义 SQLite 表操作 |
| `plugins/ai-chatbox/src/commands.rs` | **新增**：自定义 command 处理函数 |
| `plugins/ai-chatbox/src/terminal.rs` | **新增**：终端处理器（MVP 空实现） |

---

## 7. 不变文件

- `src-tauri/src/plugin/storage.rs` — 复用现有 PluginStorage
- `src-tauri/src/plugin/registry.rs` — 复用现有 PluginRegistry
- `src-tauri/src/plugin/permission.rs` — 复用现有 PermissionManager
- `src-tauri/src/plugin/manager.rs` — 不涉及
- `src-tauri/src/plugin/setup.rs` — 不涉及
- 前端 `src/plugin/context.ts` — commands.execute 已支持 plugin_invoke
- 前端 `src/plugin/commands.ts` — 已有 pluginInvoke
- 前端 `src/plugin/events.ts` — 已有事件总线
- 前端 `src/plugin/registry.ts` — 已有 UI 注册表
