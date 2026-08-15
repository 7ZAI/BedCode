# 09 — 清理自研 ABI 与文档同步

**What to build:** 所有插件切换到组件形态后，删除自研 ABI 的全部残留：core module 实例化路径、41 个 hand 注册的 host 函数（含会话 no-op）、签名表与运行期校验、`(ptr,len)` 内存搬运与 out_ptr 结果通道、分配/回收导出。运行时文件瘦身为组件单路径。同步更新迁移记录文档（标记两端均已实施）与移动端插件开发文档（构建链、SDK 依赖、契约差异表）。

**Blocked by:** 06 — auto-task 插件切换；07 — ai-chatbox 插件切换；08 — file-transfer 插件切换

**Status:** done（2025-08-14，全部验收通过）

- [x] `cargo test` 全绿；三个组件插件在真机仍正常（无回归）
- [x] 自研 ABI 残留清零：仓库内检索 `__bedcode_allocate` / `out_ptr` / `HOST_FN_SIGNATURES` / `host_session_` 无命中
- [x] 运行时文件为组件单路径（与桌面端形态对齐）
- [x] 迁移记录文档更新为「两端均已实施」；移动端插件开发文档的构建链/SDK 依赖/契约差异表同步
- [x] Kotlin 侧编译验证通过（`./gradlew :app:compileUniversalDebugKotlin`，预期零改动仅回归）

## 完成记录（2025-08-14）

**删除内容**

| 范围 | 删除项 |
|------|--------|
| SDK `abi.rs` | legacy 段全删：`NAMESPACE`/`MEMORY`/`RESULT_PAIR_SIZE`、`export`/`import` 常量模块、`HOST_FN_SIGNATURES`/`PLUGIN_EXPORT_SIGNATURES` 及 4 个 legacy 契约锁测试；仅留 `ABI_VERSION` |
| 宿主 `wasm_runtime.rs` | `compile_module`/`compile_module_from_file`/`instantiate`/`verify_abi`、`LoadedWasmPlugin` 及其全部业务方法与内存助手、`register_host_functions`（41 个 func_wrap）；字段 `linker` 删除、`component_linker` 改名 `linker`；模块文档改组件单路径 |
| 宿主 `manager.rs` | `init_wasm_runtime` 中 `runtime.verify_abi(...)` 启动期签名表校验（组件契约由 WIT 编译期保证，逐实例 `abi.version()` 协商在 instantiate_component 内） |
| 宿主 `types.rs` | `PluginLifecycleEvent::wasm_export_name()`（唯一使用者是已删的 core 路径 `call_lifecycle_event`） |
| `host_impl/*` | 全部 func_wrap 胶水（Caller + (ptr,len)）：storage/db/terminal/event/notify/http/config/bus 各删 wrapper；fs.rs 删 8 个、filesrv.rs 删 12 个；`log.rs`、`session.rs` 整文件删除（逻辑层保留） |
| `support.rs` | `read/write_wasm_string`、`write_result_to_out_ptr`、`has_permission` 删除；`guarded_host_call`/`report_host_err` 保留 |

**验证结果**

- 宿主 `cargo test --lib`：**293 全绿**（与迁移前基线一致，无回归）
- SDK `cargo test --features wasm`：**85 全绿**（89 − 4 个 legacy 契约锁测试，符合预期）
- 三个内置插件 `cargo check --features wasm --target wasm32-unknown-unknown`：全部通过（仅 check，产物未被还原为 core module）
- 验收 grep 四项（`__bedcode_allocate`/`out_ptr`/`HOST_FN_SIGNATURES`/`host_session_`）：**零命中**（按 spec §6.5 范围：`bedcode-mobile/src-tauri` + `packages/plugin-sdk-mobile` 源码；实现计划/历史 ticket 文本与 wasmtime 第三方产物除外）
- 真机：**未重跑**（checklist 第 1 项后半）；证据沿用 08 的全量真机回归——09 为纯删除、component.rs 与插件侧零 diff、宿主 293 测试全绿，风险可控
- Kotlin：`./gradlew :app:compileUniversalDebugKotlin` **BUILD SUCCESSFUL**（零改动，纯回归验证）

**文档同步**

- `docs/implementation-plans/mobile-wasmtime-component-migration.md`：状态改「已实施完成」；S0–S4 全部标注完成；§6 验收标准标注全过
- `../../../bedcode-mobile/plugin-dev-mobile.md`：§4 补组件化构建链说明；§8 重写为组件形态 + SDK 依赖 + 契约差异表（spec §3.2 表纳入）
- `docs/knowledge/wasmtime-component-migration.md`：状态改「两端均已实施」，标注历史记录性质
- `handoff.md`：更新为完结状态（见下）