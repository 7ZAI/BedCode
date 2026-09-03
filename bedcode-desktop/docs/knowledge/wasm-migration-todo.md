# WASM 插件系统迁移待办

> cdylib + FFI → WASM (wasmtime) 迁移已完成核心框架，以下为剩余待办事项

## 1. ai-chatbox HTTP 客户端迁移

**状态**: 已完成（OpenAI 格式） / 未开始（Anthropic/Gemini/Ollama）
**优先级**: 高（阻塞 WASM 模式运行）

### 问题

`ai_client.rs` 仍使用 `reqwest` 直接发 HTTP 请求。在 WASM 沙箱中无法直接发起网络请求，
所有 HTTP 调用必须通过宿主代理（`WasmHost::http_fetch()`）。

### 涉及函数

- `chat_stream_openai()` — SSE 流式请求
- `chat_complete_openai()` — 非流式请求
- `chat_stream_anthropic()` — SSE 流式请求
- `chat_complete_anthropic()` — 非流式请求
- `chat_stream_gemini()` — SSE 流式请求
- `chat_complete_gemini()` — 非流式请求
- `chat_stream_ollama()` — NDJSON 流式请求
- `chat_complete_ollama()` — 非流式请求

### 迁移方案

**流式请求**：构造 HTTP 请求 JSON，通过 `WasmHost::http_fetch()` 传入 `stream: true`，
宿主执行请求并逐 chunk 通过 `emit_event` 推送。插件监听 `streamEvent` 事件，解析 SSE 数据。

**非流式请求**：构造 HTTP 请求 JSON，通过 `WasmHost::http_fetch()` 传入 `stream: false`，
宿主同步执行请求并返回完整响应。

### 请求 JSON 格式

```json
{
  "method": "POST",
  "url": "https://api.openai.com/v1/chat/completions",
  "headers": {
    "Authorization": "Bearer sk-xxx",
    "Content-Type": "application/json"
  },
  "body": "{\"model\":\"gpt-4\",\"messages\":[...],\"stream\":true}",
  "stream": true,
  "streamEvent": "ai-chatbox:stream:uuid"
}
```

### 难点

- SSE 解析逻辑目前内联在 `ai_client.rs`，迁移到宿主代理后需要调整：
  流式模式下宿主逐 chunk 推送原始字节，插件需解析 SSE 事件
- 多种 API 格式（OpenAI/Anthropic/Gemini/Ollama）各有不同的 SSE 协议
- `commands.rs` 中 `chat_stream` 使用 `tokio::spawn` 启动异步任务，
  WASM 中需要改为宿主侧异步（http_fetch 的流式模式已由宿主 spawn）

### 建议

1. 创建 `ai_client_wasm.rs` 作为 WASM 版本的 HTTP 客户端实现
2. 使用 `#[cfg(feature = "wasm")]` 条件编译切换实现
3. 流式模式：调用 `http_fetch({ stream: true })` 后立即返回，
   由宿主逐 chunk emit 事件，前端直接消费（无需插件解析 SSE）
4. 非流式模式：调用 `http_fetch({ stream: false })` 获取完整响应

### 已完成的实现

- **Cargo.toml**: 添加 `native`/`wasm` feature flags，reqwest/tokio/futures-util 改为 optional
- **ai_client.rs**: `#[cfg(feature = "wasm")]` 条件编译，WASM 模式通过 `WasmHost::http_fetch()` 代理 OpenAI 格式请求
- **commands.rs**: WASM 模式同步调用（无 tokio::spawn/block_in_place），native 模式保留异步
- **wasm_host.rs (宿主侧)**: `execute_streaming_http` 支持 `sseFormat` 字段，宿主解析 OpenAI SSE 并提取 content delta；非 2xx 响应改为 emit 错误事件
- **events.ts (前端)**: `pluginEvents.on()` 同时注册 Tauri `listen()`，桥接 Rust → 前端事件

### 待完成

- Anthropic/Gemini/Ollama 格式的 WASM 模式支持（需在宿主 `parse_and_emit_sse` 中添加对应格式解析）

---

## 2. WASM 编译验证

**状态**: 已验证（ai-chatbox 插件 OpenAI 格式）
**优先级**: 高

### 任务

```bash
# 安装 WASM 目标
rustup target add wasm32-unknown-unknown

# 编译 ai-chatbox 为 WASM
cd bedcode-desktop/src-tauri/plugins/ai-chatbox
cargo build --target wasm32-unknown-unknown --release

# 验证产物
ls target/wasm32-unknown-unknown/release/bedcode_plugin_ai_chatbox.wasm
```

### 注意事项

- 需确认 `bedcode-plugin-api` 的 wasm feature 在 `wasm32-unknown-unknown` 下编译通过
- `reqwest` 不能在 WASM 目标下编译，必须先完成 HTTP 客户端迁移（#1）或条件排除
- `tokio` 在 WASM 下的支持有限，`tokio::spawn` / `block_in_place` 不可用，
  需要条件编译或替换为同步逻辑
- `futures_util::StreamExt` 在 WASM 中可能需要不同的实现

### WASM 不兼容的依赖

| 依赖 | WASM 兼容性 | 替代方案 |
|------|------------|---------|
| `reqwest` | 不兼容 | WasmHost::http_fetch() |
| `tokio::spawn` | 不兼容 | 移除，由宿主处理异步 |
| `tokio::task::block_in_place` | 不兼容 | 移除，WASM 中同步执行 |
| `futures_util::StreamExt` | 不兼容 | 移除，流式由宿主处理 |

---

## 3. 端到端集成测试

**状态**: 未开始
**优先级**: 高（在 #1 和 #2 完成后）

### 测试场景

1. **基础加载**: 宿主启动 → 扫描插件目录 → 编译 .wasm → 实例化 → 激活
2. **Command 调用**: 前端 invoke `plugin_invoke` → PluginHost 路由到 WASM invoke_command → 返回结果
3. **DB 操作**: 激活时创建自定义表 → 列出对话 → 保存/删除对话
4. **Storage 操作**: storage_get/set/delete
5. **流式聊天**: chat-stream → 宿主 HTTP 代理 → SSE 逐 chunk 推送 → 前端接收
6. **热重载**: 替换 .wasm 文件 → watcher 检测 → 重新 instantiate → 功能恢复
7. **TS-only 插件**: 确保不受影响

---

## 4. 插件开发文档更新

**状态**: 未开始
**优先级**: 中

### 需要更新的文档

- `bedcode-desktop/docs/knowledge/plugin-system.md` — 架构图和说明从 cdylib 改为 WASM
- `bedcode-desktop/docs/knowledge/plugin-dev-guide.md` — 开发指南增加 WASM 编译步骤
- `bedcode-desktop/docs/code-map.md` — 模块列表更新

### 新增文档

- WASM 插件开发快速入门
- WasmPlugin trait + wasm_entry! 宏使用说明
- Host Functions API 参考
- 第三方开发者发布流程（编译 .wasm → 放入插件目录）

---

## 5. 生产构建配置

**状态**: 未开始
**优先级**: 中

### 任务

- [ ] 配置 ai-chatbox 的 WASM release 编译优化（`-C opt-level=s`）
- [ ] 自动化 WASM 编译流程（CI 或 build script）
- [ ] 将 .wasm 产物复制到 `resources/plugins/desktop/` 目录
- [ ] 验证 release 构建的 .wasm 体积合理（目标 < 2MB）
- [ ] 更新 `tauri.conf.json` 的 resources 配置包含 .wasm 文件

---

## 6. 安全加固

**状态**: 未开始
**优先级**: 中

### 任务

- [ ] WASM 模块文件签名验证（防止篡改的 .wasm 被加载）
- [ ] Host Function 调用频率限制（防止插件 DoS 宿主）
- [ ] WASM 内存使用上限配置（`wasmtime::Store::limit_memory_size()`）
- [ ] HTTP 代理 URL 白名单（限制插件可访问的域名）
- [ ] 插件 .wasm 文件完整性校验（SHA256）

---

## 已完成的变更

### 新增文件

| 文件 | 职责 |
|------|------|
| `src/plugin/wasm_runtime.rs` | wasmtime 运行时：Engine/Linker/Store/Instance 管理，Host Function 注册和实现 |
| `src/plugin/wasm_host.rs` | SQL 表名校验、DB 列转换、HTTP 代理执行（流式+非流式） |
| `packages/plugin-sdk-desktop/rust/src/wasm.rs` | WasmPlugin trait + wasm_entry! 宏 |
| `packages/plugin-sdk-desktop/rust/src/wasm_host.rs` | 插件侧宿主 API 绑定（WASM import 声明 + 内存辅助函数） |

### 修改文件

| 文件 | 变更 |
|------|------|
| `Cargo.toml` | +wasmtime, +reqwest, -libloading |
| `plugin.rs` | +wasm_runtime, +wasm_host, -cdylib_loader, -host_context |
| `host.rs` | cdylib → wasm 全面重构 |
| `types.rs` | PluginSource::Cdylib → Wasm |
| `loader.rs` | Cdylib → Wasm 来源判定 |
| `watcher.rs` | .dll/.dylib/.so → .wasm 监听 |
| `api_bridge.rs` | reload_cdylib_plugin → reload_wasm_plugin |
| `sdk/Cargo.toml` | +wasm feature |
| `sdk/lib.rs` | 条件导出 wasm/wasm_host 模块 |
| `ai-chatbox/Cargo.toml` | +bedcode-plugin-api (wasm feature) |
| `ai-chatbox/src/lib.rs` | WasmPlugin trait 实现 + wasm_entry! 宏 |
| `ai-chatbox/src/db.rs` | HOST_CONTEXT → WasmHost |
| `ai-chatbox/src/commands.rs` | 通过 WasmHost 访问 DB |
| `ai-chatbox/src/ai_client.rs` | HOST_CONTEXT.emit → WasmHost.emit_event |

### 删除文件

| 文件 | 原因 |
|------|------|
| `src/plugin/cdylib_loader.rs` | WASM 替代 cdylib |
| `src/plugin/host_context.rs` | WASM host functions 替代 FFI HostContext |
| `ai-chatbox/src/host_api.rs` | SDK 的 wasm_host.rs 替代 |

## 5. WASI 预打开文件访问（插件自身 fs）

### 目标

让 WASM 插件经 WASI preview2 预打开目录直接读写文件（`std::fs`），不再仅依赖宿主 `host_fs` 函数。

### 状态：**宿主接线已完成 + ai-chatbox 构建链已切 wasip2（2026-08-19）**

**宿主侧（已完成）**：
- `src-tauri/Cargo.toml` + `wasmtime-wasi = "47"`
- `wasm_runtime.rs`：`WasmPluginState` 增 `wasi_ctx` + `wasi_table`（`WasiView` impl，linker 共享 / ctx 每实例）；`AMBIENT_RT` 全局收益运行时 + `block_on_ambient`（无 handle 线程/宿主函数阻塞执行兜底）
- `wasm_runtime/component.rs`：`p2::add_to_linker_sync(linker)` 注册 WASI preview2 全接口（惰性，未导入 wasi 的既有插件不受影响）；`build_wasi_ctx` + `resolve_preopen_dir`（读插件 config `useSelfFileAccess`+`fileAccessDir`，经 `fs_auth.is_granted()` 无弹窗校验后 preopen `/data`，未授权/异常回退 host_fs）
- `fs_auth.rs`：新增 `is_granted()`（白名单/受信任/持久化授权查询，不弹窗）——WASI preopen 授权校验用，与授权弹窗同源
- `host/commands.rs`：`run_guest_call` 把所有 guest 调用搬到 `spawn_blocking` 无 handle 线程（wasi 同步绑定 `in_tokio` 要求），宿主函数经 `block_on_async` ambient 兜底；`with_wasm_plugin_call` / activate / deactivate / on_startup / on_shutdown / hooks / 命令 / 生命周期回调全部迁移
- 测试：`test_wasi_preopen_std_fs_e2e`（preopen → guest std::fs 直写 → 宿主落盘 → 读回 → 列目录 → 根外沙箱边界）+ `resolve_preopen_dir` 三例 + `fs_auth.is_granted` 两例；`plugin-wasi-test`（wasip2 目标测试插件)

**插件构建链（已完成，ai-chatbox）**：`scripts/build.js` cargo 目标 `wasm32-unknown-unknown` → `wasm32-wasip2`（产物直接是组件，删除 componentize 步骤；watch/copy 路径同步）。wasip2 下既有宿主接口（host_fs 等）不受影响。

### 配置契约（已落位，2026-08-19）

插件 storage key `config` 内的三个字段（plugin.json `contributes.configuration` + 前端 `types.ts PluginConfig` + Rust `store::parse_file_access_config` 三端对齐）：

| 字段 | 类型 | 语义 |
|------|------|------|
| `useSelfFileAccess` | boolean | 是否由插件自身实现文件访问（WASI 预打开目录模式） |
| `fileAccessDir` | string | WASI 预打开目录绝对路径；激活时已随 fs_request_auth 批量预授权 |
| `defaultDir` | string | 数据目录（ai-chatbox：对话 JSONL）；空 = 默认 `{home}/.bedcode/ai-chatbox` |

宿主 WASI 接线时：activate 阶段读取插件 `config` → `useSelfFileAccess=true` 时按 `fileAccessDir` preopen → 插件侧 `std::fs` 写入路径以预打开目录为根（guest 映射 `/data` 前缀）。

### 注意

- `fs_request_auth` 授权记录与 WASI preopen 权限建议共用同一张授权表（FsAuthChecker），避免两套授权口径——已实现：preopen 前 `is_granted()` 复用 fs_auth 授权（持久化 fs_granted_paths / 白名单 / 受信任插件），`config` 可由插件自身写入也不能绕过授权弹窗
- 旧 `host_fs` 路径保留：`useSelfFileAccess=false`（默认）插件行为不变，host_fs 仍是唯一通道
- 后续：ai-chatbox 存储层可在 `useSelfFileAccess=true` 时改用 `std::fs` 直写（当前仍走 host_fs，功能等价）
