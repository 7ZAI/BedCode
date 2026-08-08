# 14 — 遗留修复：WASM 权限校验 + 冗余清理

Type: task
Status: resolved
Blocked by: 13

## 问题

1. **WASM 插件无权限校验**（安全缺口）：`wasm_runtime.rs` 注释明确"无权限校验"，
   插件即使 manifest 未声明权限也能调用 storage/db/terminal/http/fs/bus 全部 host functions。
2. **冗余依赖**：SDK 已移除 wasm-bindgen，但 auto-task 仍声明。
3. **死代码 / 未用项**：`validatePermissions` + `VALID_PERMISSIONS`（前端无引用）、
   `WasmRuntime::new` 未用参数、`loader.rs` 未用 import。

## Answer

### WASM 权限校验（重点）

- `WasmPluginState` 增加 `granted_permissions: HashSet<String>`（实例化时注入）。
- `WasmRuntime::instantiate` 增加 `granted_permissions` 参数；`verify_abi` 临时 state 用空集。
- `loader.rs` 注入 manifest.permissions，并**自动补 `storage`**（与 SDK
  `PermissionManager::grant_permissions` 默认授予语义一致）。
- 新增 `has_permission` helper，13 个敏感 host fn 调用前校验（与 SDK permission.rs 常量对应）：
  - storage_get/set/delete、db_execute/db_query → `storage`
  - terminal_send → `terminal:input`
  - http_fetch → `network:http`
  - fs_read → `fs:read`；fs_write → `fs:write`；fs_copy → 两者
  - bus_publish/subscribe/unsubscribe → `bus`
- 不校验（设计内）：emit_event / notify / log_* / mark_plugin_error（通用或插件自身状态）、
  session_list/get（noop 空操作）。
- 校验失败返回 -1 + warn 日志。
- ai-chatbox manifest 补 `network:http`（其 ai_client 用 http_fetch 但未声明）。

### 冗余清理

- auto-task Cargo.toml：移除 wasm-bindgen 依赖与 feature 引用。
- `WasmRuntime::new()`：删除未用的 db/storage/app_handle 参数（调用点同步）。
- `loader.rs`：删除未用 `PluginStorage` import。
- 前端 `permission.ts`：删除无引用的 `validatePermissions` 与 `VALID_PERMISSIONS`。

### 验证

- 宿主 `cargo check`：plugin/ 相关警告清零（剩余 19 个均为既有非 plugin 警告）。
- 宿主 `cargo test`：46 passed；SDK `cargo test --features wasm`：2 passed。
- 前端 `vue-tsc --noEmit` / `vitest run`（12 passed）/ `npm run build`：通过。
- auto-task / ai-chatbox `cargo build --features wasm --target wasm32-unknown-unknown`：通过。
