# 13 — 遗留处理：权限对齐 + ABI v3 升级

Type: task
Status: resolved
Blocked by: 12

## 问题

两项遗留：
1. 前端 `permission.ts` 与 SDK Rust `permission.rs` 权限名不一致
   （`ui:navTab` vs `ui:navtab`、`ui:terminalToolbar` vs `ui:input`），且前端缺
   `network:http` / `fs:read` / `fs:write` / `bus` / `terminal.onInput`。
2. 移动端 v1 ABI 元组返回的 FFI-safe 警告 + 线性内存泄漏（`wasm_alloc_string`
   用 `mem::forget` 永不回收）。

## Answer

### 权限对齐

- 前端 `src/plugin/permission.ts`：统一为 SDK Rust 权限名（`ui:navtab` / `ui:input`），
  补全 5 个缺失权限，API map 对齐 Rust（含 `terminal.onInput`）。
- auto-task `plugin.json` + rust manifest：`ui:terminalToolbar` → `ui:input`。
- ai-chatbox 已用 `ui:navtab`，无需改。

### ABI v3 升级（out_ptr + deallocate + abi_version）

- SDK `abi.rs`：`ABI_VERSION = 3`，`RESULT_PAIR_SIZE = 8`，import 签名表 6 个结果函数
  改 out_ptr（+1 参、返回 i32），export 新增 `__bedcode_abi_version` / `__bedcode_deallocate`，
  manifest/invoke_command/on_terminal_input/on_terminal_output 改 out_ptr；修正
  on_bus_message 签名表为 (7, 1)（含 timestamp: u64）。
- SDK `wasm_host.rs`：6 个结果方法改 out_ptr + `read_and_free_string`（读后回收）；
  `wasm_alloc_string` 改 `std::alloc` Layout 分配（与 `__bedcode_deallocate` 配对）；
  新增 `wasm_dealloc_string` / `read_and_free_string` / `wasm_write_result_to_out_ptr`。
- SDK `wasm.rs`（宏）：新增 `__bedcode_abi_version` / `__bedcode_deallocate`；
  `__bedcode_allocate` 改 std::alloc；4 个导出函数改 out_ptr；全部参数读取后 dealloc。
- 宿主 `wasm_runtime.rs`：6 个 host fn 改 out_ptr 写 + `write_result_to_out_ptr` helper；
  `LoadedWasmPlugin` 新增 `allocate_memory` / `read_result_from_out_ptr` /
  `dealloc_plugin_memory`（旧插件无 deallocate 时跳过，退化 v1）；invoke_command /
  on_terminal_input / on_terminal_output / get_manifest / call_lifecycle_event 改 out_ptr +
  结果回收；instantiate 时 ABI 版本协商（插件要求 > 宿主支持则拒绝加载）；
  顶部注释更新。

### 验证

- 两个插件的 wasm32 导出签名用脚本逐一比对 `PLUGIN_EXPORT_SIGNATURES`：OK。
- SDK `cargo test --features wasm`：2 passed；宿主 `cargo test`：46 passed。
- ai-chatbox / auto-task `cargo build --features wasm --target wasm32-unknown-unknown`：通过。
- 宿主 `cargo check` / 前端 `npm run build`（vue-tsc + vite）/ `vitest run`（12 passed）：通过。
- FFI-safe 警告消除（编译无 improper_ctypes 警告）。
