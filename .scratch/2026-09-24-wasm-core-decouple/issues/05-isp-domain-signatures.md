# 05 — 接口隔离：host_api 域签名角色接口化

**Type:** task（contract 半场，wide blast radius）
**Blocked by:** 04
**Status:** ready-for-agent

**What to build:** 把 22 个 host_api 能力域的**函数签名**从「人手一个 `&WasmHostContext` 上帝对象」收敛为「各自需要的窄角色接口」（ISP）。这是用户明确要求的「接口隔离彻底化」落地票。

- **角色接口定义**（放 `host_api/context.rs` 或同层新文件，trait 命名与现有字段一一对应，均为 `Send + Sync` 视图）：
  - `DbScope`（db / plugin_dbs / plugin_db_root）
  - `PermissionScope`（permission + 便利的 check 方法——现有 `host_api.rs::check_permission` 收窄为 `PermissionScope::check` 或保留顶层自由函数但签名改 `&dyn PermissionScope`）
  - `StorageScope`（storage）
  - `FsAuthScope`（fs_auth）
  - `BusScope`（message_bus）
  - `AppHandleScope`（app_handle）
  - `ServicesScope`（services：`Arc<dyn PluginServices>`）
  - `ProcessScope`（process_registry）
  - `ApiRegistryScope`（api_registry）
  - `SecurityScope`（security）
  - `CapabilityScope`（capabilities）
  - `SecretsScope`（secrets_cache）
- `WasmHostContext` 加上 `impl` 全部角色接口（各方法就是字段访问/既有访问器）。
- **每个域按「该域用到的字段」改签名**：如 `database.rs` 只收 `&dyn DbScope`、`fs.rs` 收 `&dyn FsAuthScope + &dyn PermissionScope`（以现有 `host_ctx.X` 消费点为准，逐域核对，禁止顺手多收没用到的作用域——这是本票的核心纪律）。fs 内嵌的 monitor 埋点不经 context（走独立注册句柄，已在 02 解环）。
- **调用链联动**：host_api 内部域间互调、`component.rs` 的 Host 绑定（`域函数(&self.host_ctx, …)` → `域函数(self.host_ctx.as_ref(), …)` 或经 `&*self.host_ctx` 自动 coerce）、`manager/task.rs`、`activation.rs`、tests 全部随签名联动。
- 每个域改完即跑该域针对性单测（几十域分批，**红则当场修**，禁止一口气全改完再编）；`build_host_ctx` 返回 `Arc<WasmHostContext>` 不变，域测试直接 `&*ctx` 传参。
- `check_permission`（host_api.rs 顶层，约 30 处调用点）若改签名为 `&dyn PermissionScope`，调用点传 `&*host_ctx` 即可——机械替换，注意 `crate::wasm_core::host_api::check_permission` 的所有引用路径统一。

**验收：**

- [ ] `rg "\&WasmHostContext" host_api/*.rs`（生产源码）零命中——域函数签名不再出现上帝对象；只允许 `context.rs` 内定义与 `Arc<WasmHostContext>` 值传递（task 等需 clone 的上下文载体）
- [ ] 每个角色接口只有一个域用不到的字段即视为「没收窄干净」，接受代码审查逐域核对
- [ ] 全 host_api 域单测 + `component.rs` 相关 + `manager/task.rs` 相关满绿
- [ ] `build_host_ctx` 构造不变（仍返回完整 ctx，供测试与需要整体上下文的调用方）