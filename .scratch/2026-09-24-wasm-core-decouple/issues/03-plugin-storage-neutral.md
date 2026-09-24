# 03 — PluginStorage 中立化（manager/storage.rs 下沉）

**Type:** task（wide refactor）
**Blocked by:** None — can start immediately.
**Status:** ready-for-agent

**What to build:** 把 `PluginStorage`（现 `manager/storage.rs`，纯 SQLite plugin_storage 表封装，只依赖 `crate::db`）下沉到中立层（如 `wasm_core/storage.rs`），使 `security/{framework,approval,fs_auth}` 与 `host_api/auth` 不再为用它而 import manager。

- **迁移对象**：`PluginStorage` 结构体及全部方法；`SYSTEM_PLUGIN_ID`/`ACTIVATION_STATE_KEY` 常量随行。文件整体搬移，语义零变化。
- **引用方改路径**（已枚举）：`security/framework.rs`、`security/approval.rs`、`security/fs_auth.rs`（生产 + 测试两处）、`host_api/auth.rs`、`manager/host.rs`、`manager/runtime.rs`；`host_api.rs::tests::build_host_ctx` 同为消费方。
- **模块声明调整**：`wasm_core.rs` facade 增导出；`manager.rs` 移除 `pub mod storage`；外部消费方（其余 crate）如引用 `manager::storage` 需改路径（当前全仓消费方已枚举，无可疑遗漏）。
- **行为零变化**：PluginStorage 方法签名、SQL、错误语义原样保留；浏览器/存储隔离语义（按 plugin_id 隔离、`__system__` 系统级）逐字不动。

**验收：**

- [ ] `rg "crate::wasm_core::manager::storage"` 全仓零命中（不含注释/文档）——security 从此零 manager 依赖
- [ ] `cargo check` 通过；related 单测（storage 域、fs_auth 三层校验、approval）满绿
- [ ] 生产路径与测试路径全部改完（含 `tests` 内引用，`build_host_ctx` 构造不破）