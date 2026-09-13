# 06: core-plugin-manager：系统组件类型与能力装配框架

**What to build:** 插件分两种类型并可装配闭环：manifest 声明 `type: system | application`（缺省 application，旧插件零迁移）与 `dependencies`（应用插件声明依赖的系统组件能力）。能力注册表以能力名为键、提供者为值（宿主 Rust 原语或 WASM 系统组件实例，二选一装配）；系统组件内置、默认启用、只停不删、先于应用插件加载激活；应用插件的 import 由 Linker 按注册表路由到宿主原语或系统组件实例（host-side 转发，组件间不共享内存）；依赖缺失则激活失败报明确错误。系统组件与应用插件走同一条 Store/安全/监控管线，trap 隔离不扩散。能力不携带业务语义（ADR 0022 裁剪线）。

**Blocked by:** 01（五模块骨架归位）

**Status:** resolved

- [x] manifest 解析：`type`/`dependencies` 缺省兼容旧插件（现有插件 manifest 无需修改即可加载）
- [x] 加载顺序：系统组件先于应用插件激活（集成测试断言顺序）
- [x] 最小 WASM fixture 闭环：fake 系统组件注册能力 + fake 应用插件消费，调用经 Linker 路由到达组件实例并返回正确结果
- [x] 依赖缺失：应用插件激活失败且错误信息指明缺失的能力名
- [x] 系统组件 trap 不影响其他插件（隔离测试）
- [x] `cargo test` 全绿

## Comments

- 2026-09-13 完成。核心设计：

  **manifest（SDK 契约）**：`PluginKind`（`type` 字段，`system`/`application`，缺省 application，`skip_serializing_if` 保证缺省值不落盘）与 `dependencies`（缺省空数组）；与既有 `pluginType`（产物形态 rust/rust-ts/ts-only）正交——新字段描述装配角色。Rust（`plugin-sdk-desktop/rust/src/types.rs`）与 TS（`src/types.ts`）双侧类型同步；旧 manifest 无字段零迁移（serde default）。

  **能力注册表（`manager/capability.rs`）**：能力名 → 提供者，二选一装配——`HostPrimitive`（宿主 Rust 原语，现状直连）或 `SystemComponent { plugin_id, instance }`（WASM 系统组件实例）。`CapabilityRegistry::new()` 预登记 18 组 host-* 宿主原语能力（依赖检查对宿主能力恒可用）；`ROUTABLE_CAPABILITIES` 声明「可被系统组件接管」的能力及其要求的全部导出函数（当前 `host-storage` 的 get/set/delete，**全命中才认定提供**）。注册表内 `std RwLock`（临界区仅 map 读写、不跨 await），宿主同步调用栈可直接读。

  **Linker 装配（host-side 转发）**：应用插件的 host-* import 仍进入宿主函数（`host_impl/storage.rs`），函数内先做权限校验，再查注册表——命中系统组件提供者则 `forward_storage_*` 转发到该实例的同形导出，否则走宿主原语。转发经 `LoadedWasmPlugin::call_capability_export`（燃料续费语义同 `exports()`，计入导出调用计时），组件间不共享内存、载荷在 WIT 类型边界序列化。自调用（提供者 == 调用方）回落宿主原语，避免实例互斥锁重入死锁。

  **导出探测**：实例化时 `probe_exported_capabilities` 按 `ItemName` 路径语法（`pkg:ns/iface.func`，见下）探测可路由能力接口导出 → `LoadedWasmPlugin::exported_capabilities`；激活 `type=system` 后 `PluginHost::register_system_capabilities` 据此注册为提供者（未导出任何可路由能力则告警不阻断）。

  **加载编排**：`PluginHost::new` 在持久化自动激活**之前**执行 `activate_system_components()`（按插件 ID 排序保证确定性；单个失败不阻断其余，失败组件落 Error 态由其消费方的依赖检查如实报错）。系统组件启停**不持久化**（持久化真源是「内置」而非用户状态，只停不删、默认启用）：`get_activated_state` / `auto_activate_from_persisted_state` 均排除 `kind=System`。停用即 `revert_all_from(plugin_id)` 撤销其能力提供，能力回落宿主原语。

  **依赖检查**：激活阶段 1.5（无 map 锁）对 `dependencies` 逐项查注册表，缺失即 `mark_error` + 返回 `Err`（错误信息含缺失能力名，激活失败不留悬挂 Activating）。

  **故障自愈 / trap 隔离**：转发调用 trap 或传输出错（外层 `Err`）由 `unwrap_forward_result` 隔离为调用方 `Err(string)`（调用方实例不中毒、可继续调用），同时 `revert_to_host(capability, provider_id)` 使能力回落宿主原语；guest 自报的 `Err(string)`（WIT result 内层）原样透传不触发回落。实例重建时 `revert_all_from` 后按新实例重新装配。

  **WIT 契约**：桌面 `bedcode.wit` 新增 `plugin-system` world（guest 绑定 world，供系统组件构建时导出 host-* 同形接口；宿主仍按 `plugin` world 实例化 + 动态探测，与 `plugin-binary` 同策略）。`plugin` world 与 `abi.version()` **未变**（无 ABI bump）；**移动端不同步**——移动端运行时无系统组件/能力注册表概念（spec 明确移动端运行时模块化另行立项），该 world 属桌面宿主专属增量，`plugin` world 契约未变故无需双端同步。

  **fixture**：新包 `packages/plugin-system-test/`（cdylib，`plugin-system` world）——导出 host-storage 同形接口（组件实例私有内存 KV，与宿主 SQLite 隔离，用于区分路由是否命中）+ `sys-test.panic` key 触发故意 panic（trap 隔离测试）。

  **顺带修复（二进制 guest 回调全链缺陷，票据 05 残留）**：v11 二进制链路此前有三处断点，本轮全部修掉并补端到端覆盖——
  1. **探测语法**（`component.rs`）：`events-binary#on-message-binary` 的 str 查找恒不命中（wasmtime 47 组件接口导出为嵌套实例形态），改用 `ItemName` 路径语法（`bedcode:plugin/events-binary.on-message-binary`）；同因 `capability.rs` 的能力探测与转发亦用 ItemName。
  2. **SDK 契约缺口**（双端 `plugin-sdk-*/rust/src/host/bus.rs` + `wasm_host.rs`）：`HostBus` trait 只有 `bus_publish`/`bus_subscribe`，**没有** `bus_publish_binary`/`bus_subscribe_binary`——而 `WasmPlugin::on_message_binary` 的文档明确要求用 `host-bus.subscribe-binary` 声明二进制偏好，即 SDK 侧根本无从声明，二进制订阅不可达。补齐两方法（桌面 v11 / 移动 v9 语义一致）。
  3. **端到端覆盖缺失**：新增 `test_sdk_plugin_binary_bus_roundtrip`（`wasm_runtime.rs`）——同一 SDK 组件双实例：发布方经 `bus_publish_binary` 发非 UTF-8 字节列，订阅方 activate 内 `bus_subscribe_binary` 声明偏好，断言 guest `on_message_binary` 回调收到的 topic/sender/字节列与发布逐字节一致；fixture `plugin-sdk-test` 增记录 + `test_binary_publish`/`test_binary_received` 命令。另在 `test_sdk_plugin_component_roundtrip` 增加「宏产物必须暴露 events-binary 导出」断言（探测命中 + 调用成功）。

  **测试**：`host.rs` 新增 5 用例——装配闭环（系统组件私有 KV 值经路由读到、区别于宿主 SQLite 对照值）、依赖缺失报错含能力名 + 落 Error 态、trap 隔离 + 能力回落 + 回落后续调用正常、停用回落 + 启停不持久化 + 重激活再装配、`PluginHost::new` 全路径启动顺序（系统组件先于应用插件、`activated_at` 时序佐证）；`capability.rs` 4 单测（宿主动词预登记、依赖缺失列举、可路由性门禁、条件回落）；SDK `types.rs` 2 单测（system 解析 + 缺省兼容）；`wasm_runtime.rs` 新增 `test_sdk_plugin_binary_bus_roundtrip`（SDK 二进制发布/订阅/回调全链，见上）。**修正**：启动顺序集成测试的 manifest 拼接此前多了一个前导 `"`（拼出非法 JSON → 插件被加载器跳过），已改正。

  **验证**：桌面 `cargo test` 644 单测 + 全部集成 + doc 全绿；移动端 `cargo test` 282 + 集成全绿；两端 SDK `cargo test` 全绿（桌面 77 / 移动 63）；桌面 SDK 改动经 `cargo check --target wasm32-unknown-unknown --features wasm` 编译校验（两端）；根目录 `pnpm exec eslint .` 0 error；桌面 `pnpm run test:run` 614 全绿；改动/新增文件 `rustfmt --check` 干净（既存文件基线漂移不在本次范围）。code-map 同步（manager/capability 模块、新 fixture 包、config/monitor/security 描述修正）。
