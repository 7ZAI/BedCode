# 04 — host_api/context.rs 立项（WasmHostContext 迁入 + capability trait 化）

**Type:** task（expand 半场）
**Blocked by:** 01, 03
**Status:** ready-for-agent

**What to build:** 创建 `host_api/context.rs`，把宿主上下文从 manager 迁到 host_api（消费方家园），并让 WasmHostContext 对 manager 的唯一剩余类型依赖（CapabilityRegistry）改为经 trait 消除。**expand 阶段**：新增 context.rs 承载全部内容，`manager/runtime.rs` 旧定义改为 re-export 新股（保持旧路径编译绿），随后迭代清理 re-export。

- **迁入内容**（自 `manager/runtime.rs` 逐字搬移）：`WasmHostContext`（14 字段 + new() + set_plugin_db_root/services 等访问器）、`PluginServices` trait、`ProcessRegistry` + `RunningProcess`、`kill_process_group`；字段注释、两阶段注入注释、pub(crate) 可见性逐字保留。
- **capability trait 化**：`host_api/context.rs` 定义 `CapabilityProvider` trait（只暴露 host_api 侧真正用到的能力查询方法，以 `host_api/storage.rs` 与 `manager/runtime.rs`/`component.rs` 的消费点为准抽取）；`manager::capability::CapabilityRegistry` 实现之；WasmHostContext 持有 `Arc<dyn CapabilityProvider>` 而非具体类型。反过来 manager 侧继续可直接用具体类型（方向合法）。
- **消费方改路径**：`component.rs`（含 Host 绑定 22 域 + WasmPluginState 的 host_ctx 字段声明）、`runtime.rs`、`host.rs`（`WasmHostContext::new(...)` 构造处，参数顺序不许变）、`activation.rs`、`manager/task.rs`、`host_api/*.rs`（21 处类型引用）、各 `tests`（含 `build_host_ctx` 同级使用方）。
- **注意**：`WasmPluginState` 留在 `manager/runtime.rs`（它是 wasmtime Store state，属运行时装配域，不迁）；`StoreSpec`/`StoreLimits` 同理。context.rs 引用它们**不行**——若 WasmHostContext 的方法签名涉 StoreSpec/Runtime 类型，需用 trait 或保持对该类型的引入；以实际编译报错为准，原则是 context.rs 只依赖中立层 + 自身 + 纯数据。
- `build_host_ctx`（host_api.rs::tests）改为构造新 ctx，所有测试共用路径一处改。

**验收：**

- [ ] `rg "pub struct WasmHostContext" manager/` 零命中（定义已迁 host_api；manager 只 import）
- [ ] `rg "capability::CapabilityRegistry" host_api/` 零命中（host_api 只经 `&dyn CapabilityProvider` 消费）
- [ ] `manager/runtime.rs` 无 WasmHostContext 定义体（仅 re-export 过渡，且最终 удалить re-export 或保留也是 manager→host_api 合法方向）
- [ ] `cargo check` + 针对性测试（host_api 各域单测 + `build_host_ctx` 消费方）满绿
- [ ] 无 WIT / ABI / wire 变更