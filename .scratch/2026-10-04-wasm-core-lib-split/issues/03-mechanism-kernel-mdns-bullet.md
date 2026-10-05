# 03: 机制内核 crate + mdns 域全链路（tracer bullet）

**What to build:** 一个能力域（mdns，5 条原语）**完全搬出 `wasm_core`**，由一个新建的独立 crate 提供；宿主不再有任何针对该域的硬编码装配代码 —— 它被自动发现、自动注册。同时把宿主 server 的端口层从「反向依赖 wasm_core 的插件绑定模块」纠正为「依赖平台无关引擎 crate」。

完成后可以演示：新 crate 被加进宿主依赖 + 白名单常量，其接口就出现在插件的可用 import 集里，全程不改宿主装配代码；且 `Ports` 的 shared-daemon 出口不再经由 wasm_core。

本票同时交付后续 7 票依赖的两块地基：

- **`packages/bedcode-host-kit`**（机制内核锚点，~250 行）：`HostModule` trait、`inventory::collect!` 提交类型、从运行时搬出的插件实例状态类型、以及一个把已有 13 个窄端口 scope trait 聚合起来的 supertrait（**聚合 trait 的实现在宿主侧一行，不动那 13 个 trait 本身**）。
- **宿主侧唯一装配入口**：`add_to_linker` 从 22 行逐接口硬编码，改为「core 接口本地表 + 自动收集的能力模块」两段遍历。

**为什么锚点 crate 必须独立（两条已实测的硬约束）**：

- `inventory` 的注册靠 linker-section 静态，**未被引用的 rlib 不进最终二进制 ⇒ submit 不执行**。已实测：不引用能力 crate 任何 item 时收集结果为空，加一行强制引用后才有值。
- 能力 crate 必须能命名「collect 声明的类型」与「插件实例状态类型」（wit-bindgen 的 `add_to_linker::<S, D>` 是单态的）。若这两样住在宿主 bin crate 内，能力 crate 就得依赖宿主、而宿主又必须依赖能力 crate ⇒ **Cargo 硬拒循环依赖**（已实测报错退出码 101，`crate-type` 含 `rlib` 亦然）。

**约束**：descriptor 只描述机制（接口路径 / 权限位 / ABI 下界），**禁带任何产品名词**，否则越过「宿主侧无业务代码」的 B1/B5 红线。WIT 的 `world plugin` 的 22 个 import **一个不动**（本票零 ABI 变更）。

**Blocked by:** 02（认领 wasm_core 在途改动）— **已解除**（票 02 结案：那批改动自行回滚，无等待）

**Status:** done（除「共享守护创建点」一项外全部完成；该项 HEAD 即不成立，须单独立票）

- [x] 新 crate 建在 `bedcode-desktop/packages/`（**偏离 spec §4 的仓库根**，依据见第三段：spec D8 的前提不成立——能力 crate 必须自带 provider 侧 `bindgen!`，故绑死桌面 WIT），target 落点 `target/host-kits` 已用 `cargo metadata` 核验
- [x] `add_to_linker` 中不存在任何逐接口硬编码；core 接口走本地表，迁出的能力域经自动收集注册（mdns 已迁出并验证）
- [x] 强制引用行（`use bedcode_discovery_engine as _;`）与白名单常量 `HOST_MODULES` **同处**
- [x] 新 crate 加进宿主依赖 + 白名单后其接口即生效，**宿主装配代码零改动**（`capability_registry_matches_whitelist` 实测 `discovery` 已被收集）
- [x] `Ports` 的 shared-daemon 出口改为指向新 crate（方向倒置已终结）
- [ ] ⚠️ **全仓共享守护创建点仍只有一处** —— 实测 **HEAD 即有两处**（本次未新增亦未修复，见末段，须单独立票）
- [x] mdns 域的属主隔离、事件按属主定向投递、停用时双表回收语义逐字保留（14 条用例，含跨插件拒绝 / host 与插件共存 / 双表回收只碰本人）
- [x] 桌面全部回归绿（含集成 target）——见下方「最终回归台账」
      ⬜ 移动端：**用户裁定本期不改移动端**，故未跑（本期零移动端改动，§10 该项按用户口径豁免）
- [x] 新 crate 自身有测试；`cargo fmt` / `cargo clippy` 干净

> ⚠️ **开工前置（本票剩余工作的每个文件都要做）**：`bedcode-desktop/src-tauri/` 下存在
> **反复活动的并发会话**（内联测试目录化拆分，17:00 再次启动并扫过 `wasm_core/`）。
> 本票要改的 `component.rs` / `config.rs` / `monitor.rs` / `runtime.rs` /
> `server/ports_impl.rs` 均可能正被它改。**动手前逐文件 `git status`；同文件双写违反
> AGENTS §11，须等它落定。** 详见 `issues/02-claim-inflight-changes.md` 末节。

## Comments

### 开工前实测到的硬阻塞：spec §5 的「机制内核 crate」少算了一层宿主端口边界

**结论先行**：spec §5.2 把内核 crate 估成「≈250 行」，但要成立就必须把 `WasmPluginState` 搬进 crate，而搬它就得处理一条 spec 未预算的**宿主端口边界**。这不是实现细节，是会重塑后续 6 票形态的取舍，故停下确认。

**取证（均为本会话实测）**：

1. **约束一反过来咬人。** spec §5.1 已实测「能力 crate 必须能命名 `collect!` 声明的类型 + `WasmPluginState`（`add_to_linker::<S,D>` 单态）」⇒ 这两个类型**必须在** `bedcode-host-kit`。这一步无异议。
2. **但 `WasmPluginState` 持有具体宿主上下文。** `manager/runtime.rs:134` = `host_ctx: Arc<WasmHostContext>`；`host_api/context.rs` 的 `WasmHostContext` 15 字段里挂满宿主 bin crate 类型。逐个数过 13 个 scope trait 的签名：

   | scope trait | 返回类型 | 类型住在哪 | kit 能否引用 |
   | --- | --- | --- | --- |
   | `PermissionScope` | `&Arc<PermissionManager>` | ✅ **SDK `bedcode-plugin-api`**（`wasm_core/permission.rs` 只是 `pub use` 再导出，词源真源在 SDK） | 能 |
   | `SecretsScope` / `ServicesScope` / `ProcessScope` / `CapabilityScope` | std 类型 / 纯 trait | 可搬 | 能 |
   | `DbScope` | `&Arc<Database>` | ❌ 宿主 bin（`src/db/`，票 07 才搬） | **不能** |
   | `StorageScope` | `&Arc<PluginStorage>` | ❌ 宿主 bin（`wasm_core/storage.rs` 233 行） | **不能** |
   | `FsAuthScope` / `NetworkAuthScope` | `FsAuthChecker` / `NetworkAuthChecker` | ❌ 宿主 bin | **不能** |
   | `BusScope` | `&Arc<MessageBus>` | ❌ 宿主 bin（`wasm_core/bus.rs` 838 行） | **不能** |
   | `ApiRegistryScope` / `SecurityScope` | `ApiRegistry` / `SecurityFramework` | ❌ 宿主 bin | **不能** |
   | `AppHandleScope` | `Option<&tauri::AppHandle>` | ❌ 外部 crate tauri | **不能**（且不该） |

   ⇒ **13 个里有 8 个的签名引用 kit 拿不到的类型**。所以「13 个 scope trait 原地不动 + kit 声明聚合 `HostPorts`」在票 03 这一步**不可实现**（票 07 把 db 搬走也只解决 1 个）。
3. **mdns 域实际需要的端口只有 3 个**（这让方案可行）：`AppHandleScope`（emit）+ `PermissionScope`（已在 SDK ✅）+ `MessageBus::publish`（`mdns.rs:498` 一处）。
4. **仓库已有现成先例可循**（不必自创架构）：`bedcode-server-base/src/ports.rs` 就是「**消费方 crate 声明端口 trait（14 个，243 行）→ 宿主 `src/server/ports_impl.rs` 逐个实现**」。本票应照此办理，而非新发明。

**三个选项（实测后 B 已被排除）**：

| 选项 | 做法 | 代价 | 结论 |
| --- | --- | --- | --- |
| **B** | 状态类型留在宿主 bin，只把 `HostModule`/`ModuleEntry`/注册表放 kit | — | ❌ **直接违反 spec §5.1 约束一**：`register` 必须命名状态类型，命名不了就不是 auto-registry，退回 22 行硬编码（= 没做本票） |
| **A**（推荐） | 照 `bedcode-server-base` 先例：kit 拥有 `WasmPluginState{ host: Arc<dyn HostPorts>, … }`；kit 声明机制面端口；**每个迁出域自声明窄端口**（mdns → 3 个）；宿主 adapter 实现它们 | ① `component.rs` **168 处** `self.host_ctx` 机械改写；② 每个迁出域一个窄端口 trait + 一处 adapter；③ core 侧 15 个接口需要一个向下转型出口（或 kit 端口方法） | ✅ 架构干净、双端可共享、能力 crate 零反向依赖 |
| **C** | 先把 8 个宿主类型全搬进 crate，kit 拥有完整 13 scope | 把票 07 提前 + 搬 8 个类型 ≈ 重写整个 `wasm_core` | ❌ 不成比例 |

**A 的关键设计细节（待确认）**：`WasmPluginState.host` 存 `Arc<dyn HostPorts>`；core 侧 15 个接口的 `impl Host for WasmPluginState` 留在宿主，经一个扩展 trait 拿回 `&WasmHostContext`（向下转型，`as_any()`，TypeId 比较，相对一次 host 调用可忽略）；迁出域则用自己的窄端口 trait，**不碰**向下转型。

**影响面**：A 一旦采纳，票 04/05/06/08 的验收清单都应补一条「本域窄端口 trait 已声明 + 宿主 adapter 已实现」，且票 08 的 db 域（`DbScope` → `Database`）会同时消掉票 07 的一部分必要性。

**待用户裁定**：采纳 A？若采纳，是否同意 `WasmPluginState.host` 走 `Arc<dyn HostPorts> + as_any()` 向下转型（而非把 core 15 接口也改成走 kit 端口）？

### 2026-10-04 实施记录 · 第一段：机制内核 crate 已建成并自测绿

**用户裁定**：采纳方案 **A**（照 `bedcode-server-base::ports` 先例：kit 拥有
`WasmPluginState`，`host` 字段存 `Arc<dyn HostPorts>`；core 侧 15 接口留在宿主、
经向下转型拿回 `&WasmHostContext`；迁出域自声明窄端口 trait，不用向下转型）。

**已交付（`packages/bedcode-host-kit/`，11 条测试全绿）**：

| 文件 | 内容 | 来源 |
| --- | --- | --- |
| `src/limits.rs` | `StoreLimits` + `defaults` + `plugin_debug_mode` | 从 `wasm_core/config.rs` 搬（`apply_overrides` **刻意留在宿主**——它的输入是 SDK manifest 类型，属配置面而非机制面，见下） |
| `src/metrics.rs` | `PluginMetrics` / `LifecycleEvent` / `AuthzDecisionKind` / `CallTimer` / `PluginMetricsSnapshot` | 从 `wasm_core/monitor.rs` 搬（纯原子值对象，无宿主依赖）；`MetricsRegistry` / `MetricsSource` / JSON 拼装留宿主 |
| `src/state.rs` | `WasmPluginState` + `StoreSpec` + `WasiView` + `ResourceLimiter` | 从 `wasm_core/manager/runtime.rs` 搬，`host_ctx: Arc<WasmHostContext>` → `host: Arc<dyn HostPorts>` |
| `src/ports.rs` | `HostPorts`（marker + `as_any`/`as_any_mut`）+ `downcast_host{,_mut}` | 新增 |
| `src/module.rs` | `HostModuleDesc` / `HostModule` / `ModuleEntry` / `inventory::collect!` / `submit_module!` | 新增 |
| `src/registry.rs` | `ModuleRegistry`：收集 → 按名排序 → 白名单**双向**校验 → `install_all` | 新增 |

**target 落点**：`packages/bedcode-host-kit/.cargo/config.toml` → `../../target/host-kits`
（仓库根新桶，与 `target/server-libs` 同族）。`cargo metadata` 实测
`target_directory = .../packages/bedcode-host-kit/../../target/host-kits` ✅

**三条实测发现（都不是设计而是事实，记账于此避免重复踩）**：

1. **`HostModule` 首版不可 dyn 兼容**——`fn desc() -> HostModuleDesc`（无 receiver）
   使 trait 无法 `dyn`。改为 `fn desc(&self)` 后正常。`ModuleEntry.module` 存
   `&'static dyn HostModule` 的前提就是这一条。
2. **`HostModule::register` 的返回类型必须是 `wasmtime::Result<()>`**（即
   `add_to_linker` 的原生错误），不能是 `crate::Result<()>`——后者与
   `HostKitError::Register { source: wasmtime::Error }` 字段类型冲突。
3. **重复注册护栏不在 `Linker::instance(name)`**：wasmtime 48 的 `into_instance`
   **有意允许**对同一名字重复开启（源码注释：“explicitly allow re-opening an
   instance multiple times over separate API calls”）。真正的报错来自**同一 instance
   内同名函数**（`instance.func_new` / wit-bindgen 的 `add_to_linker` 会碰的那条）。
   ⇒ kit 的测试替身必须真绑一个函数，否则「重复注册报错」这条用例测的是空气。

**环境事故（已处理，记账以防重演）**：首次 `cargo test` 链接阶段 rust-lld 报
**Bus error（signal 7）**。根因不是代码：**根分区 100% 满（157G 用满，仅剩 237MiB）**，
linker 写 mmap 输出文件失败→SIGBUS。已 `cargo clean` 掉 `cross-end-tests/target`
（18.6GiB，纯构建产物、零 `.rs` 源文件、且**本票链无任何一票需要它**——§10 只在改
跨端协议时才要求跑它）。清理后可用 18GiB，测试随即全绿。
**注意**：`bedcode-desktop/src-tauri/target` 15G + `bedcode-mobile/src-tauri/target` 15G
仍在，AGENTS §3 的 15GB 阈值已到；跑全量回归前建议先处理，否则宿主测试会以同样的
Bus error 失败（且失败信息完全不指向真实原因，极易误判成代码问题）。

**剩余工作（下一段）**：
1. 宿主接线：`WasmHostContext: HostPorts` + 向下转型扩展 trait +
   `component.rs` **168 处** `self.host_ctx` 机械改写 + 从 `config.rs`/`monitor.rs`/`runtime.rs`
   删除已搬走的部分（保留 `pub use` 转发，票 07/08 的 expand–contract 模式同理）
2. `add_to_linker` 22 行硬编码 → core 15 接口本地表 + `ModuleRegistry::install_all`
3. mdns 域 → `packages/bedcode-discovery-engine`（窄端口 trait `MdnsPorts`：AppHandle /
   Permission / Bus publish 三项）+ 宿主 adapter + `Ports` shared-daemon 改指
4. 强制引用行与白名单常量同处 + 全量回归

### 2026-10-04 18:5x 实施记录 · 第二段：宿主接线完成，装配改两段式

**验证口径**：桌面宿主 `cargo test --lib` = **918 passed / 1 failed**。
基线是 **914 passed / 1 failed**（唯一失败即 `session_e2e::test_session_task_domain_closed_loop`，
既有的断言陈旧：插件已多发一个 `queue-retrying-check` 域，测试仍期望 3 个）。
**914 + 本票新增 4 条锁测试 = 918 ⇒ 搬迁零行为变更，逐项对齐基线。**

#### 已完成

| 项 | 内容 |
| --- | --- |
| 依赖 | `src-tauri/Cargo.toml` 加 `bedcode-host-kit = { path = "../../packages/bedcode-host-kit" }`（与 `peer-net` 同深度，`src-tauri` 距仓库根两层——首次写成 `../../../` 直接编不过，是路径基准问题） |
| `config.rs` | `StoreLimits` 结构 + `impl`（fuel_budget/clamped_within）+ `Default` 搬走，留 `pub use` 转发；`defaults` 与 `plugin_debug_mode` 转 `pub(crate) use`；`apply_overrides` **降为自由函数 `apply_store_overrides(base, request)`**（`StoreLimits` 已是外来类型，不能再写 inherent impl），调用点 `security/framework.rs:308` + 4 处单测同步 |
| `monitor.rs` | 值对象层搬走（721 → 445 行），`pub use bedcode_host_kit::metrics::{…}` 转发；`MetricsRegistry`/`MetricsSource`/JSON 拼装留宿主 |
| `manager/runtime.rs` | `WasmPluginState` + `StoreSpec` + `WasiView` + `ResourceLimiter` 搬走（1400 → 1283 行），`pub use` 转发 |
| `host_api/context.rs` | `impl HostPorts for WasmHostContext`（`as_any`/`as_any_mut`）+ `HostCtxOf` 扩展 trait（向下转型出口，类型不符 panic 并点名期望/实际） |
| `manager/runtime/component.rs` | **180 处** `self.host_ctx.as_ref()` / `&self.host_ctx` → `self.host_ctx()` |
| 装配 | `add_to_linker` 改**两段式**：core 本地表（留内核的 interface）+ `ModuleRegistry::install_all` 自动收集；白名单 `HOST_MODULES` + `host_module_registry()` 双向校验接入运行期 |
| 锁测试（新增 4 条） | `capability_registry_matches_whitelist`（双向）、`host_module_whitelist_has_no_duplicates`、`collected_module_names_are_unique`、`capability_module_descriptors_carry_no_product_nouns`（AGENTS §5.1 B1/B5 词汇红线） |
| kit 可见性修正 | `PluginMetrics::record_call_duration` 由私有改 `pub`——宿主 monitor 单测要**不 sleep** 地钉死直方图分桶边界（1ms→桶0 / 1ms→桶1 / 10s→末桶），只能直接喂耗时 |

#### 一个连带的设计变更：`TaskEngine` 不再收 `host_ctx`

任务登记表需要**拥有**一份 `Arc<WasmHostContext>` 强引用（任务在异步执行期间须独立于
插件实例存活），而状态字段搬进内核后是 `Arc<dyn HostPorts>`，拿不回具体 Arc。
处置：`CoreTaskEngine` 在**注入时捕获**该 Arc（`set_task_engine` 本就是在同一个 Arc
上调用的，捕获到的与调用方持有的必然同一对象），`TaskEngine::execute_batch/submit`
去掉 `host_ctx` 形参，`host_api/task.rs` 5 个域函数改收 `&WasmHostContext`。

#### 遗留（下一段：mdns 域迁出）

已把 mdns 域对宿主的真实依赖逐条查清（**这三条是规格没写的、必须先定的**）：

1. **`tauri::async_runtime::spawn`**：浏览事件循环与 re-announce 续期都靠它派生任务，
   返回的 `JoinHandle` 存在句柄表里。**实测两张表的 `task` / `reannounce_task`
   字段从不被 abort 也不被 await**（`stop_browser` 只退订 channel 让循环自然退出；
   `register_host_service` 甚至塞的是 `spawn(async {})` 假句柄）。
   ⇒ 端口可设计成 `fn spawn(&self, fut: BoxFuture)` 不返回句柄，但**这属语义取舍**
   （丢句柄 = 丢「表内生命周期可见性」），需明确裁定。
2. **`publish_mdns`** 走 `AppContext::try_global()` 取总线（已是全局取用，非经 ctx
   字段）⇒ 天然可做成 ZST adapter，`Arc<Adapter>` 零成本克隆进任务。
3. **`is_self_broadcast`** 需从 `AppHandle` 取本机节点 ID（`bedcode_server_peer_net::
   current_node_id`）⇒ 端口方法 `fn local_node_id(&self) -> Option<String>`，
   无句柄时返回 None 即「不拦截」，与现状一致。

⇒ 拟定端口面 4 个方法：`check_permission` / `local_node_id` / `publish_owned` /
`spawn`。**未定前不动手**（AGENTS §0「不确定的设计取舍先问用户」）。

#### 环境提示（写入以防重演）

- 并发会话仍在改宿主文件（`pty_process.rs` / `wsl.rs` 等，实测 diff 只是 import 重排 +
  空行，语义中性）。本段我未触碰其文件。
- 全仓 45 个文件有 `cargo fmt` 差异（含并发会话那批）。我只对自己的 4 个文件跑
  `rustfmt --edition 2021`（遵守仓库 `rustfmt.toml` 的 `max_width=120`），
  **未格式化他人文件**。
- `pty::output_notify_is_rate_limited_and_owner_scoped` 与
  `a03_p1b_wasip3_artifact_full_closed_loop` 在全量并行下**偶发**失败，
  单独跑 3/3 绿 ⇒ 时序 flake，非本次改动引入。

### 2026-10-04 19:xx 实施记录 · 第三段：mdns 域迁出完成（用户裁定方案 a）

**用户裁定**：后台任务用 **可取消句柄**方案——但实测发现**不能直接丢句柄**，改为
`DiscoveryTask::cancel()` 端口（详见「实测修正」）。

#### 新 crate：`bedcode-discovery-engine`（14 测试全绿）

| 文件 | 内容 |
| --- | --- |
| `src/engine.rs` | 共享守护单例 + BROWSERS/ADVERTISERS 双句柄表 + 5 条原语 + 宿主身份广播登记 + 双表回收 + 14 条测试 |
| `src/ports.rs` | `DiscoveryPorts`（4 方法）+ `DiscoveryTask`（1 方法）+ `install_ports`/`ports` 进程级装配 |
| `src/lib.rs` | `bindgen!` provider 侧绑定 + `impl Host for WasmPluginState` + `HostModule` 自报 |

**宿主侧只剩 `host_api/mdns.rs` 一个 adapter**（795 行 → ~100 行）：`HostDiscoveryPorts`
（零大小，四方法经 `AppContext::try_global()` 取用）+ `install()` + `purge_for_plugin` 转发。

#### 实测修正：方案 (a) 不能「丢句柄」

我原以为两张表的 `task` / `reannounce_task` 从不被用。**实际 `stop_advertise` 有
`entry.reannounce_task.abort()`** —— 不中止的话，已注销的服务会被续期循环重新注册回去，
是**真实功能回归**。故按 (a) 的方向做，但端口给的是**可取消句柄**（`DiscoveryTask::cancel`）
而非丢弃：`BrowserEntry.task` 确实从不被取消（保持 `#[allow(dead_code)]`），
`AdvertiserEntry.reannounce_task` 在 stop 时 `cancel()`。语义与 HEAD 逐字等价。

#### 两处偏离 spec，各有实测依据

1. **crate 落点：`bedcode-desktop/packages/` 而非 spec §4 的仓库根 `packages/`**。
   依据：spec D8「能力 crate 不自带 `generate!`」的**前提在本 crate 不成立**——
   宿主自己的 `bedcode` 模块是 **guest 视角**（import 是调用函数，不是 `Host` trait +
   `add_to_linker`），能力 crate 要自己装配就必须跑 provider 侧 `bindgen!`，
   而 WIT 在 `bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/`。
   ⇒ 本 crate 当前**绑死桌面 WIT**，放根 `packages/` 反而制造「看起来是双端共享 crate」
   的误导（放根的成本不为 0：移动端复用它仍需自己的 provider 绑定）。
   连带硬约束已写进 `lib.rs` 注释：两侧生成的 `Host` trait **同名但不同类型**，
   故宿主必须**同时**删掉自己的 mdns `Host` impl 与 `add_to_linker` 行（否则同一
   interface 注册两次 → `defined twice`）——本次两处都删了。
2. **`publish` 收完整 topic 而非 `owner + suffix`**：避免在能力域里做字符串手术。
   `owned_topic(owner, event)` 是 SDK 的纯函数，留在能力域；总线订阅方隔离留宿主。

#### 方向倒置已终结（spec §1.2 病灶 3）

`server/ports_impl.rs:319` 由 `crate::wasm_core::host_api::mdns::shared_daemon()`
改指 `bedcode_discovery_engine::engine::shared_daemon()`——宿主 server 的端口层不再
依赖 wasm_core 的插件绑定模块。

#### ⚠️ 验收项「全仓共享守护创建点仍只有一处」——**HEAD 即不成立，非本次引入**

实测 `ServiceDaemon::new()` 的桌面生产面有**两处**：

1. `bedcode-discovery-engine/src/engine.rs:42`（本次从 `host_api/mdns.rs` 搬来）——
   host-mdns + peer-net 共用的共享守护
2. `src/mdns/advertiser.rs:54` —— `MdnsAdvertiser::new()` 的工厂，**HEAD 即存在**
   （该文件本次未改）；经 `ports_impl.rs:239` 的 `HostMdnsAdvertiserPort::advertise`
   被 `bedcode-server-core::supervisor::start_mdns_advertisement` 驱动，属活路径

⇒ 该验收项在改造前就不成立，**本次既未新增也未修复**。这是真实的「双守护争抢同一组播
端口」隐患（有真机实证），但与本票的 crate 化无关，**应单独立票**（让
`MdnsAdvertiser` 改用共享守护，`start_mdns_advertisement` 走 `MdnsPort`）。
本票不动它——AGENTS §0 最小改动原则。

#### 验证

| 套件 | 结果 |
| --- | --- |
| `bedcode-discovery-engine` | **14 passed**（12 条从宿主逐条移植 + 2 条新增：`self_broadcast_filter_matches_local_node_id` 正例、`uninstalled_ports_panic_loudly` fail-visible） |
| 桌面宿主 `cargo test --lib` | **906 passed / 1 failed**（唯一失败 = 既有 `session_e2e`）；906 = 918 − 12（迁出的 mdns 用例），账目对齐 |
| `capabilities_lock` | **7 passed** |
| `hot_path_logging_lock` | **3 passed** |
| fmt / clippy | 两 crate 均干净 |

**一处自造 bug（已修，值得记）**：移植 `advertise_state_flip_roundtrip` 时，我把
`is_advertising(...)` 断言放进了已持有 `ADVERTISERS` 那把 `Mutex` 的作用域里 ——
`Mutex` 不可重入 ⇒ **该测试死锁并连带 6 个碰同一张表的测试一起挂**（跑满 60s 超时）。
现象极具迷惑性：只有「碰表」的测试挂、纯逻辑测试全绿。**教训：跨函数边界的锁断言必须
先确认被调方是否自己取同一把锁。**

### 最终回归台账（2026-10-04 收口）

| 套件 | 结果 |
| --- | --- |
| 桌面 `cargo test --lib` | **906 passed / 1 failed**（唯一失败 = 既有 `session_e2e`，断言陈旧，非本次引入） |
| `capabilities_lock` | 7 passed |
| `hot_path_logging_lock` | 3 passed |
| `server_integration` | 1 passed |
| `ws_auth_rules` | 1 passed |
| `broadcast_shutdown` | 1 passed |
| `http_auth_biometric` | 1 passed |
| `pty_session_chain` | 1 passed |
| `build_manifest_smoke` | 1 passed |
| `wasm_bridge_bench`（真实 WASM 插件装载 → 桥接往返） | **2/2 数量级门禁 PASS**：nop 往返 71.2µs < 3000µs；总线二进制吞吐 100.5 MiB/s > 1 MiB/s |
| `bedcode-discovery-engine` | 14 passed |
| `bedcode-host-kit` | 11 passed |
| fmt / clippy | 两个新 crate **0 问题**；宿主侧仅格式化自己的文件 |

**合计 936 绿 / 1 既有失败**。`wasm_bridge_bench` 是本票最强的端到端证据：真实 wasm
插件经**新的两段式装配**（core 本地表 + 自动收集的 `discovery` 模块）装载并完成
往返与总线投递。

**测试后清理**：无残留 cargo/插件进程，无残留监听端口（已核 `ps` + `ss`），
`/tmp/bedcode_bridge_bench_*` 临时根已删。

**磁盘事故第二次**（同第一次，根因同一）：集成 target 链接 Tauri 全量二进制时
再次 `Bus error (signal 7)`，根分区又 100% 满。已 `cargo clean`
`bedcode-mobile/src-tauri/target`（16.6GiB，依据：用户裁定本期不动移动端 +
AGENTS §3 的 15GB 阈值）。清理后可用 38GiB（会话开始时仅 237MiB）。
