# 01 — 异步基础设施中立化（block_on_async / AMBIENT_RT / ambient_handle）

**Type:** task
**Blocked by:** None — can start immediately.
**Status:** done

**What to build:** 把 `manager::runtime` 中三个纯异步基础设施符号下沉到中立层（新模块 `wasm_core/runtime_util.rs` 或 `utils/` 等），让 host_api 与 security 不再因「要调 block_on_async 就得 import manager」。

- **迁移对象**：`block_on_async`、`AMBIENT_RT`（LazyLock tokio runtime）、`ambient_handle`，及其私有助手 `BlockInPlaceGuard`、`IN_BLOCK_IN_PLACE` 线程局部标志。语义、重入保护、ambient runtime 行为**逐字保留**（这是 CER：actix current_thread 互调自锁 / wasi ambient runtime 共存的先例，注释一并搬走，不得改写）。
- **36 个使用方改 import**：`crate::wasm_core::manager::runtime::block_on_async` → 新路径。用 `rg` 全量枚举后机械替换，逐文件核对；`manager/runtime.rs` 不再定义这些符号（内部使用方同样改新路径）。
- **行为零变化**：任务 e2e（task_e2e / session_e2e / ws_e2e 等依赖 fixture 的用例）必须逐字通过——它们验证的正是这条 async 桥的宿主语义。

**验收（针对性地毯后全量回归跑票 08，但本票至少跑一次带 fixture 的 task/server 相关用例）：**

- [x] `rg "manager::runtime::block_on_async|manager::runtime::ambient_handle"` 全仓零命中（不含注释/文档链接）——代码层（`src/` `tests/`）零命中；`.scratch/**` 规划文档保留描述属预期
- [x] 新模块无任何 `crate::wasm_core` 兄弟模块 import（真中立）——`runtime_util.rs` 只引用 `std` / `tokio`，可被任意兄弟引用
- [x] `cargo check` 通过；相关针对性测试（`cargo test` 过滤到 host_api/security/task/bus 各域）满绿
- [x] 无测试逻辑改动（只动 import 路径）

**实施记录（2026-09-24）：**

- 新模块 `bedcode-desktop/src-tauri/src/wasm_core/runtime_util.rs`：`pub(crate)` 暴露 `block_on_async` / `block_on_ambient` / `ambient_handle` 三件套，私有 `AMBIENT_RT` / `BlockInPlaceGuard` / `IN_BLOCK_IN_PLACE` 逐字搬入；`wasm_core.rs` 声明 `pub(crate) mod runtime_util;`（与 `manager` 同级，零兄弟依赖）。
- 机械替换：13 个文件的 `crate::wasm_core::manager::runtime::{block_on_async, ambient_handle, block_on_ambient}` 全路径调用点改走 `crate::wasm_core::runtime_util::*`；复合 import（host_api 各域、capability / component / task 等）拆分：`WasmHostContext` 等留 `manager::runtime`，桥工具走 `runtime_util`（rustfmt 排序：manager < runtime_util）。
- 注意：`runtime.rs` 删块时误删 `WASIP3_NIGHTLY` 常量定义，已即时回补（注释与 `#[cfg_attr(not(test), allow(dead_code))]` 保留）；该常量仍在 `manager::runtime`，fixture 构建引用不变。
- 验证：`cargo check --all-targets` 通过；针对性 `cargo test` 滤波 `engine_limits / manager::task / host_api / security / capability / bus / utils::auth / server::websocket` 全绿；依赖 fixture 的 `task_e2e`（22 用例）+ `ws_e2e` + `session_e2e` 全绿（含 actix arbiter 路径依赖 `ambient_handle`，及重入保护 `block_on_async_reentrant_nested_call_no_panic`）。
- 共享 worktree 注意：`enums/*`、`monitor.rs`、`manager/task.rs` 含另一并行 agent 的在途改动（票 02 monitor 去环），本票只动其中 import 路径行；**未提交**（避免把对侧在途改动并入本票提交），交票由用户统一收口。
- **收口提交（2026-09-24）**：`e2366dae4`（30 文件，纯迁移 hunk 暂存：17 host_api + security/framework + manager 5 文件 + utils/auth 2 + server/websocket 1 + wasm_core.rs + runtime_util.rs 新增；runtime.rs/task.rs 与票 02 已提交内容共存，仅取票 01 hunk）。收口时补一处剧尾：`manager/runtime.rs` 残留的 `runtime_util::block_on_async` import 仅测试代码使用（当时 `cargo check --lib` 报 unused import）→ 加 `#[cfg(test)]` 门控。验证：`cargo check --all-targets` 全绿（含 session-events 在途状态下）；针对性测试 capability 4 / security 47 / host_api 260 / bus 17 / utils::auth 15 满绿。