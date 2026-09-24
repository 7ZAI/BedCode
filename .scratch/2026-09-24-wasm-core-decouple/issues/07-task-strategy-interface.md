# 07 — C3 策略化：host_api/task.rs 反向依赖破除（TaskEngine + UnitExecutor）

**Type:** task
**Blocked by:** 04, 06
**Status:** ready-for-agent

**What to build:** 解除 `host_api/task.rs` 对 `manager::task`（core_task）的双向依赖，并用「消费方定义接口」把 core-task 改为只依赖接口不依赖具体域函数。

两条反向边逐一处理：

1. **`host_api/task.rs → manager::task`**：core-task 是执行引擎（TaskRegistry + 线程池），host_api/task.rs 是权限门 + 解析/配额盖在其上。定义 `TaskEngine` trait（`host_api/task.rs` 内或 `host_api/context.rs` 同层），方法 = 现状 core_task 对外的 execute_batch / submit / status / cancel / list_jobs / purge_for_plugin 签名；`manager::task` 实现 `TaskEngine`；host_api/task.rs 经 trait 调用。
   - **注入路径**：与 PluginServices 同构的两阶段注入（WasmHostContext 的 engine 槽位，PluginHost 构造时 set）——这是 04 已迁入的既有先例，直接复用，不新增全局状态。
   - host_api/task.rs 的权限门 / plan 解析 / 配额仲裁逻辑**留原地**（它做的是门禁不是引擎），只把「调 core_task::X」换成「engine.X」。核心里程碑：host_api/task.rs 顶部不再 `use crate::wasm_core::manager::task`。

2. **`manager/task.rs → host_api::{fs,http,process}`**（execute_unit 的字符串 match）：定义 `UnitExecutor` trait（`host_api/` 同层新文件或 context.rs），`fn matches(&self, kind_prefix)` + `fn execute(&self, ctx: &dyn …, owner, unit) -> Result<Option<String>, String>`；fs / http / process 三个实现方在自己的 domain 文件内注册执行器（构造期 `register_executor(Arc<dyn UnitExecutor>)`）；core-task 执行单元时查注册表分发，不再 import 具体域函数。
   - `ensure_unit_path_granted`（fs 单元的授权预检）属于 fs 执行器的契约，随注册逻辑归位（fs executor 内部先做判据再执行——判据同源纪律不变：fs_auth 只做已授权校验、绝不从池线程触发弹窗）。
   - `PlanUnit` / `Plan` / `JobPhase` 等 DTO 的归属：若 trait 方法签名需引用它们，则 DTO 下沉到 host_api（或 trait 签名用 JSON 字符串透传，以编译为准——**优先保持 wire 不变量**，DTO 类型跨层可见性以最小改动为准）。

**行为零变化（硬约束）**：task 的 spec（§6 双门结构 / 权限门 / 配额 / 事件回调 / cancel 协作语义 / purge）与服务行为逐字不变；`task_e2e` 集成测试是这条链的主回归，必须全绿。

**验收：**

- [ ] `rg "use crate::wasm_core::manager::task" host_api/` 零命中（host_api/task.rs 不再 import core_task）
- [ ] `rg "host_api::fs|host_api::http|host_api::process" manager/task.rs` 零命中（execute_unit 经 UnitExecutor 注册表分发）
- [ ] host_api/task.rs 的权限门/解析/配额逻辑不变（diff 审查应只见调用方与 trait 接线）
- [ ] `cargo test` 的 task_e2e / session_e2e（若涉 task 链）/ host_api/task 单测满绿
- [ ] 无 wire / 权限词汇变更