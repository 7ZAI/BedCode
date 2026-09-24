# 03 — PluginStorage 中立化（manager/storage.rs 下沉）

**Type:** task（wide refactor）
**Blocked by:** None — can start immediately.
**Status:** done（2026-09-24 执行完成，提交 `554e86758`）

**What to build:** 把 `PluginStorage`（现 `manager/storage.rs`，纯 SQLite plugin_storage 表封装，只依赖 `crate::db`）下沉到中立层（如 `wasm_core/storage.rs`），使 `security/{framework,approval,fs_auth}` 与 `host_api/auth` 不再为用它而 import manager。

- **迁移对象**：`PluginStorage` 结构体及全部方法；`SYSTEM_PLUGIN_ID`/`ACTIVATION_STATE_KEY` 常量随行。文件整体搬移，语义零变化。
- **引用方改路径**（已枚举）：`security/framework.rs`、`security/approval.rs`、`security/fs_auth.rs`（生产 + 测试两处）、`host_api/auth.rs`、`manager/host.rs`、`manager/runtime.rs`；`host_api.rs::tests::build_host_ctx` 同为消费方。
- **模块声明调整**：`wasm_core.rs` facade 增导出；`manager.rs` 移除 `pub mod storage`；外部消费方（其余 crate）如引用 `manager::storage` 需改路径（当前全仓消费方已枚举，无可疑遗漏）。
- **行为零变化**：PluginStorage 方法签名、SQL、错误语义原样保留；浏览器/存储隔离语义（按 plugin_id 隔离、`__system__` 系统级）逐字不动。

**验收：**

- [x] `rg "crate::wasm_core::manager::storage"` 全仓零命中（不含注释/文档）——security 从此零 manager 依赖（连带达成 spec 判据 3：`rg "crate::wasm_core::manager" security/` 零命中，票 01 联合效果）
- [x] `cargo check` 通过；related 单测（storage 域、fs_auth 三层校验、approval）满绿
- [x] 生产路径与测试路径全部改完（含 `tests` 内引用，`build_host_ctx` 构造不破）

## 实施记录（2026-09-24）

- `git mv` 整体搬移 `manager/storage.rs` → `wasm_core/storage.rs`（历史保留，rename 90%）：`PluginStorage` + `SYSTEM_PLUGIN_ID` / `ACTIVATION_STATE_KEY` 常量随行，方法签名 / SQL / 错误语义 / 按 plugin_id 隔离逐字不动；文件头补位置纪律注释（中立层只依赖 `crate::db`，零兄弟依赖）
- 模块声明：`manager.rs` 移除 `pub mod storage`（含模块 doc 条目同步删除 + 指向新家的说明行）；`wasm_core.rs` facade 增 `pub mod storage` + 再导出改 `pub use storage::PluginStorage`
- 10 处消费方改路径（`crate::wasm_core::manager::storage` → `crate::wasm_core::storage`）：`security/fs_auth.rs`×2、`security/approval.rs`×1、`security/framework.rs`×2（测试全限定）、`manager/runtime.rs`×2（生产 import + 测试 import，测试处不在票据枚举内但已覆盖）、`manager/host.rs`×1、`host_api.rs`×1（build_host_ctx）、`host_api/auth.rs`×1
- 验证（工作区一致性窗口内）：cargo check --lib 全绿；storage 5 / security 47 / approval 15 / host_api 261 / manager::host 71 / engine_limits 17 满绿；rustfmt --check 对我编辑行零差异（FMT-DIFF 报告均属既有递归漂移，未触碰）
- **安全里程碑**：security 至此零 manager import（block_on_async 走 runtime_util 票 01 + storage 票 03），spec 目标架构中 security 一层已就位