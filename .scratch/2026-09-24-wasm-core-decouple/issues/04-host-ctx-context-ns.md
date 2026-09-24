# 04 — host_api/context.rs 立项（WasmHostContext 迁入 + capability trait 化）

**Type:** task（expand 半场）
**Blocked by:** 01, 03
**Status:** done（2026-09-24 执行完成，提交 `1ff010825`）

**What to build:** 创建 `host_api/context.rs`，把宿主上下文从 manager 迁到 host_api（消费方家园），并让 WasmHostContext 对 manager 的唯一剩余类型依赖（CapabilityRegistry）改为经 trait 消除。**expand 阶段**：新增 context.rs 承载全部内容，`manager/runtime.rs` 旧定义改为 re-export 新股（保持旧路径编译绿），随后迭代清理 re-export。

- **迁入内容**（自 `manager/runtime.rs` 逐字搬移）：`WasmHostContext`（14 字段 + new() + set_plugin_db_root/services 等访问器）、`PluginServices` trait、`ProcessRegistry` + `RunningProcess`、`kill_process_group`；字段注释、两阶段注入注释、pub(crate) 可见性逐字保留。
- **capability trait 化**：`host_api/context.rs` 定义 `CapabilityProvider` trait（只暴露 host_api 侧真正用到的能力查询方法，以 `host_api/storage.rs` 与 `manager/runtime.rs`/`component.rs` 的消费点为准抽取）；`manager::capability::CapabilityRegistry` 实现之；WasmHostContext 持有 `Arc<dyn CapabilityProvider>` 而非具体类型。反过来 manager 侧继续可直接用具体类型（方向合法）。
- **消费方改路径**：`component.rs`（含 Host 绑定 22 域 + WasmPluginState 的 host_ctx 字段声明）、`runtime.rs`、`host.rs`（`WasmHostContext::new(...)` 构造处，参数顺序不许变）、`activation.rs`、`manager/task.rs`、`host_api/*.rs`（21 处类型引用）、各 `tests`（含 `build_host_ctx` 同级使用方）。
- **注意**：`WasmPluginState` 留在 `manager/runtime.rs`（它是 wasmtime Store state，属运行时装配域，不迁）；`StoreSpec`/`StoreLimits` 同理。context.rs 引用它们**不行**——若 WasmHostContext 的方法签名涉 StoreSpec/Runtime 类型，需用 trait 或保持对该类型的引入；以实际编译报错为准，原则是 context.rs 只依赖中立层 + 自身 + 纯数据。
- `build_host_ctx`（host_api.rs::tests）改为构造新 ctx，所有测试共用路径一处改。

**验收：**

- [x] `rg "pub struct WasmHostContext" manager/` 零命中（定义已迁 host_api；manager 只 import）
- [x] `rg "capability::CapabilityRegistry" host_api/` 零命中（host_api 只经 `&dyn CapabilityProvider` 消费）
- [x] `manager/runtime.rs` 无 WasmHostContext 定义体（仅 `pub use` 再导出；manager→host_api 合法方向，保留作历史路径过渡）
- [x] `cargo check` + 针对性测试（host_api 各域单测 + `build_host_ctx` 消费方）满绿
- [x] 无 WIT / ABI / wire 变更

## 实施记录（2026-09-24）

- 新模块 `host_api/context.rs`（527 行）：WasmHostContext（14 字段 + new + 全访问器
  含 get_or_create_plugin_db/call_plugin_api_host）、PluginServices trait、CapabilityProvider
  trait（新，7 方法）、RunningProcess/ProcessRegistry/kill_process_group——自 runtime.rs
  逐字搬移（python 按行区间提取保真，仅拼装；两处手改见下）
- 手改两处：① new() 增第 8 参 `capabilities: Arc<dyn CapabilityProvider>`（DIP 注入，
  #[allow(clippy::too_many_arguments)] + Builder 留后续票）；② capabilities 字段/访问器
  类型改 trait 对象
- CapabilityRegistry 实现 trait：manager 侧具体类型直连（方向合法）；**注意同文件已有
  内部枚举 `CapabilityProvider`**（提供者状态），trait 只能全路径引用；provider_kind
  去 `#[cfg(test)]`（非 test 构建的 trait impl 委托它）
- 消费方：host_api 21 域 import 改 `host_api::context::WasmHostContext`（sed 批量 +
  status.rs 全限定内联）；host_api.rs 增 `pub mod context`；build_host_ctx 经新增
  `manager::capability::test_registry()`（cfg(test) 测试辅助，host_api 不命名具体类型）；
  pty.rs 能力清单断言改经 `ctx.capabilities().is_available(...)`（&dyn）；manager 侧
  host.rs/scaffold/runtime 测试构造注入 `Arc::new(CapabilityRegistry::new())`
- 收尾坑：① context.rs 缺 `use tauri::Manager`（get_or_create_plugin_db 的
  `app_handle.path()`）→ 补；② runtime.rs 测试子模块经 `use super::*` 继承的名字
  （Mutex/RwLock/HashMap/Pin/Database/PermissionManager）随生产 import 删除而丢失
  → cfg(test) 门控恢复；③ api.rs:33 混合行尾（HEAD 既有 LF 行，CRLF 文件）→ 归一 CRLF
- 事件域 hunk 隔离：host_api/events.rs 与 session-events 在途改动交织，只暂存票 04
  import 行 hunk（python 提取 hunk[0] + git apply --cached）
- 验证（绿窗承载）：cargo check --all-targets 全绿；host_api 261 / capability 4 /
  security 47 / storage 2 / manager::host 71 / engine_limits 17 / task_e2e 6 满绿；
  rustfmt 净化 context.rs（装配时的双空行）+ 其它改动文件零差异
- **遗留**：runtime.rs 的 `pub use` 再导出为过渡（`随后迭代清理 re-export`，票据原文）——
  manager 侧消费方（component/host/activation/task）仍走历史路径，迭代时统一改
  host_api::context；context.rs 的 LoadedWasmPlugin 单点 manager 引用随装配域下沉处置