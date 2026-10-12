# 票 04 · 桌面绑定归位（common 壳 + WASI + 桌面扩展域 → 桌面宿主）

> 输入：`spec.md` §5 T-04 + §2 路径 B 先例。前置：T-03（交集绑定定稿）。
> **依赖**：T-03 → **本票**；**本票 ∥ T-05 可并行**（桌面宿主与移动宿主改动集不重叠；并行前确认两票在 wasm-core 的文件切面无交集——由 T-03 交付物切开）。
> **测试节奏**：针对性自验（桌面宿主 `cargo check` + 绑定相关单测 + 锁自检）；桌面全量 `cargo test` 留 T-06 收尾。
> 目的：wasm-core 内的桌面侧绑定与装配全部迁出到 `bedcode-desktop/src-tauri/src/plugin/`，wasm-core 不再引用桌面生成物路径。

## 1. 迁移对象（common 侧）

| # | 现位置（wasm-core） | 迁移内容 | 落点（桌面宿主） |
| --- | --- | --- | --- |
| D1 | `manager/runtime/common.rs`（约 545 行，T-01 后） | 运行时壳：Engine/Linker/Store 生命周期装配 + WASI p2/p3 preopen + fs_auth 授权过滤 + `expand_preopen_declarations` / `resolve_preopen_dirs` + `LoadedWasmPlugin` 使用面 | `src-tauri/src/plugin/` 相应模块（既有 `bindings.rs` 承接绑定） |
| D2 | `manager/runtime/common/component.rs` 中**桌面扩展** Host impl | `host_events_desktop` / `host_fs_desktop` / `host_platform_desktop`（lib.rs 防回接锁已注明迁宿主，本票闭环） | `src-tauri/src/plugin/{events,fs,platform}.rs`（已有同名文件，核对是否已有 impl——避免 defined twice，见下） |
| D3 | `common/component.rs` 中**交集 9 域** 的桌面绑定形态 | 若 T-03 选 Option C：连同交集绑定全上移；Option B：交集留在 core，仅把桌面**装配接线**移出 | 按 T-03 裁决 |
| D4 | ~~WASI 装配 + `wasmtime-wasi` 依赖~~ | ~~wasip3 preopen、fs_auth 过滤链~~ | **不迁**（用户裁决 2026-10-12）：WASI 面是双端共享的核心机制——`manager/runtime/wasi.rs` + `CoreConfig::wasi`（默认全开），见 §6.6-2 |
| D5 | `abi-form` 消费（桌面独有必选导出） | verify_abi 对 abi-form 的检查 | 宿主 verify_abi 流程 |
| D6 | common 侧测试 + fixture（`fixture_build` / `fixture_target` / tests 15 文件） | 装配面测试 | `src-tauri/tests/`（迁移后改造，不整套搬） |
| D7 | common 侧 `host_api` 14 域模块中**纯装配词汇** | 随绑定迁出；机制核 / sqlite_ports / auth_center / unit_executor 等**留 core**（lib.rs 防回接锁合法残余清单为凭） | 核对 lib.rs 各锁注释的「合法残余」列表，逐一确认去留 |

## 2. 关键核对（开工前）

- **defined twice 风险**：桌面宿主 `plugin/events.rs`、`plugin/fs.rs`、`plugin/platform.rs` 是否已含这些 interface 的 `Host` impl？若已有，wasm-core 侧必须**删**而非迁（防回接锁注释确认「WIT impl 落宿主」——即宿主**已持有**，wasm-core 的 common 侧 impl 可能是测试替身或同步残留，逐行确认）。
- `host_module_whitelist()`（common/component.rs:328）与宿主 `pty_wiring.rs` 双向比对的合并结果，随迁移改为宿主单点持有。
- 桌面 `server/crate_boundary_lock.rs` 的分裂产物登记表（wasm-core 与宿主单向引用）——若绑定层迁宿主后产生新 crate 拆分或路径变化，同步登记表。

## 3. 依赖面

- wasm-core `Cargo.toml`：删除 `wasmtime-wasi`（若 move）、桌面 SDK `bedcode-plugin-api` 的绑定引用（机制面已在 T-02 剥离）；`bedcode-server-{http,websocket,peer-net}/common-host` 联动 feature 引用确认（这些是能力域 crate 自身 feature，**保留**——T-02 已登记例外）。
- `bedcode-desktop/src-tauri/Cargo.toml`：按需补 `wasmtime-wasi` 等。

## 4. 门禁

- 桌面 `cargo test`（src-tauri 全量）绿；桌面宿主 `cargo check --lib` 0 error。
- wasm-core（无桌面 feature）`cargo check --lib` + `cargo test --lib` 全绿——**证明 wasm-core 已不依赖桌面绑定**。
- `pnpm run check:wit` 通过。
- 锁：`sliced_interfaces_must_not_return_to_wasm_core` 等防回接锁更新为「wasm-core 禁一切桌面扩展词汇」最终形态；变异自检。

## 5. 交付物

- 桌面绑定迁移落地（D1-D7 处置表结果）+ 宿主绑定层单点持有；
- 防回接锁最终化 + 门禁结果附本票。

---

## 6. 状态：进行中（2026-10-12 会话：夹具 e2e 批量迁宿主 → **4 替身 + `TestGuestEvents` + 全 world `bindgen!` 已删**）

> **本会话产物未提交**（工作区待认领）：桌面 4 个新测试二进制 + wasm-core 侧删除与
> test_support 转发面。并行线 4 文件（`bedcode-mobile/.../peer_net.rs`、
> `db/database.rs`、`host_api/context.rs`、`manager/types.rs`）**未触碰**。
> 继续认领基线 = **HEAD `8c71aebdc` + 本会话工作区改动**。

### 6.1 已迁移（桌面宿主，两段合计 34 用例全绿）

| 宿主文件 | 用例 | 来源（wasm-core） | 改造点 |
| --- | --- | --- | --- |
| `tests/component_e2e.rs`（接在途） | 2 | `common/tests/component_e2e.rs` | **修复**：直连运行时用例的属主 id 必须 `com.bedcode.test`（`setup_wasm_runtime` 权限授予面固定按它装配；误用 `com.bedcode.sdk-test` 会让 guest 内 host-storage 退化为 `{"error":...}`） |
| `tests/sdk_e2e.rs` | 3 | `common/tests/sdk_e2e.rs` | `mod tests` → `mod support`；构建 → 只读 `sdk_fixture_artifact_bytes("sdk")` |
| `tests/http_e2e.rs` | 1 | `common/tests/http_e2e.rs` | 同上；`host_ctx.permission` → `host_ctx.permission()`（`PermissionScope`） |
| `tests/engine_e2e.rs`（新） | 16 | `engine_limits.rs` 15 条夹具用例 + `engine_config.rs` 逃生舱端到端 | `raw_store`/`aot_cache_key`/`engine()` 经 test_support 转发；导出直调绑定改宿主 `plugin::bindings::Plugin` |
| `tests/owner_e2e.rs`（新） | 5 | `common/tests/owner_e2e.rs` | 属主转发面（见 §6.4）；I4 结构锁改读 wasm-core 真源路径 |
| `tests/wasm_flow_e2e.rs`（新） | 7 | `wasm_flow_test.rs` 4 条（degraded / 持久化意图 / 激活-调用-停用 / 热重载）+ `instance_call_model_test.rs` 3 条（两模型对照） | `setup_host_shared` → 直接 `setup_host()`（`PluginHost::new` 已 bind owner_sink）；`entry.{call_model,shutdown}` / `owner_cleanup_skipped` 经转发面 |

**wasm-core 侧删除**：`common/tests/{component_e2e,sdk_e2e,http_e2e,owner_e2e}.rs` 整文件
+ `mod` 声明；`engine_limits.rs` 18→3（留缓存 key 纯函数 / 配置校验 / block_on_async 重入）；
`engine_config.rs` 8→7；`component/tests/linker.rs` 删往返用例（覆盖等价迁移：往返本体 →
`component_e2e::test_component_roundtrip`；activate 燃料消耗 →
`engine_e2e::test_component_fuel_watchdog`）；`wasm_flow_test.rs` 留定时器/限频用例 +
4 张源码扫描锁、`instance_call_model_test.rs` 留 `instance_slot_structural_lock`；
连带清理 ws/http 断言助手、`parse_rfc3339_for_test`、`build_sdk_test_component`、
`positive_auth_needs_dedicated_binary`（宿主 support 有同形副本）、`setup_host_shared`、
`TEST_WASM_PLUGIN_ID`、`build_test_component` 与失效 cfg(test) 导入。

**内核支撑改动（test_support 转发面，未放宽生产 API）**：`test_support::common` 增
`plugin_raw_store` / `aot_cache_key` / `runtime_engine` / `pub use host_api::log::capture`
（`host_api::log` 是 `pub(super)`）；去掉 `WasmRuntime::engine()` 与
`LoadedWasmPlugin::raw_store()` 的 `#[cfg(test)]` 门控（**宿主测试链的是不带 cfg(test)
的 lib**，cfg(test) 项对宿主等价于不可达——仍是 `pub(crate)`，跨 crate 生产不可达）；
宿主 `tests/support/mod.rs` 扩 5 个 re-export。

### 6.2 判决性探针（本会话，关键事实）

把 `common/component.rs::add_to_linker` 的 4 个替身注册**临时注释**后跑 wasm-core
`cargo test --lib`（随后以 `cp` 还原 + sha256 校验）：

- 第一轮（迁移前）：**7 条用例依赖替身** —— `manager/host/tests/{wasm_flow_test(4),
  instance_call_model_test(3)}`；其余（a03_probe / preopen_e2e / p3 / engine_* /
  component/tests / fixture_keeper）全绿。
- **结论（推翻了原 §6.3-2/3 的迁移判断）**：`preopen_e2e`（wasip2 夹具只 import
  `bedcode:plugin/host-log`）与 `a03_probe`（wasip3 夹具只 import `host-log`）的
  import 表 ⊆ core 8 域 ⇒ **不需要迁，留 wasm-core**（字符串级实证：`strings <产品>.wasm
  | grep 'bedcode:plugin/host-'` 只有 host-log；与「夹具 import 全 world」的旧记述不符
  ——SDK 组件的 import 是**按使用集合**生成的）。sdk 夹具则确实 import host-api-call +
  host-events-desktop/fs-desktop/platform-desktop（= 替身存在的原因）。
- 第二轮（本会话迁移完成后）：**0 条依赖** ⇒ 替身可删。

### 6.3 已完成：4 替身 + `TestGuestEvents` + 全 world `bindgen!` 全删（本会话）

- **新增迁移**：`owner_e2e.rs`（5 用例）→ 宿主 `tests/owner_e2e.rs`；`wasm_flow_test` 的
  4 条夹具用例 + `instance_call_model_test` 的 3 条两模型对照 → 宿主
  `tests/wasm_flow_e2e.rs`（7 用例）。两原文件**留 wasm-core** 的部分：`PluginServices`
  定时器 / 重载限频、`instance_slot_structural_lock`、4 张退役面源码扫描锁（都不实例化夹具）。
- **owner_e2e 走法（用户指定先定方案：取 ②，见 §6.4）**：test_support 加属主转发面
  （`spawn_test_owner` / `TestOwnerHandle` / `owner_queue_cap` / re-export
  `OwnerFailureSink` / `OwnerStatsSnapshot` / `ShutdownReport`）；owner 三类型提为 crate
  内 `pub`（模块仍 `pub(crate)`，生产跨 crate 不可达）；`GuestCallFailure::panic_message()`
  访问器；装配条目转发 `instance_call_model` / `instance_shutdown` /
  `host_owner_cleanup_skipped`。
- **内核删除**：`component.rs` 的 `bindgen!`（world plugin）+ 4 个 cfg(test) `Host` impl +
  4 处替身注册 + `TestGuestEvents` + `HostCtxOf`/`D` 类型别名；
  `TestGuestEvents` 的两处转出（common.rs / runtime.rs）与 `fixture_build` crate 级转出
  （消费者已迁走）；`scaffold.rs::setup_host_shared`、`host.rs::TEST_WASM_PLUGIN_ID`。
- **派发器注入改单路**：`test_support::setup_wasm_runtime` 不再注入（缺省
  `UnwiredGuestEvents` 派发即 fail-visible）；宿主 `tests/support/mod.rs` 自己的
  `setup_wasm_runtime` 注入 `DesktopGuestEvents`（与 `PluginHost::new` 同一实现）。
- **a03_probe 适配**（唯一被「缺省 fail-visible」暴露的用例）：P1-a 的
  「`on_message` 在 async store 下可用」改探原生探测路径的 `on_message_binary`
  （同属「可选导出在 async store 下可调」性质；`on_message` 的 async 覆盖由宿主
  sdk_e2e / pty / ws 一族持有）。

### 6.4 owner_e2e 走法裁决（用户 2026-10-12：② test_support 转发面）

- ① owner 机制 API 提 `pub` 被否：`spawn_owner` 跨 crate 可达即打破 I1「宿主侧唯一入口
  是装配表」；③ 经 `PluginHost` 重写被否：与原用例主旨（不经门面、直接驱动属主）相悖，
  且 queue_full / stats / is_alive / 自定义 sink 四条断言无等价观测。② 与既有
  `TestInstanceDispatcher` / `build_host_ctx_at` / `kernel_test_domain_ports` 同章程：
  暴露面收窄到测试所需，生产 API 零变化。

### 6.5 门禁实证（本会话实跑，两段合计）

- 迁移段：wasm-core `cargo test --lib` **574/0**（原 597，差 23 = 迁走 22 + 删 1）；
- 收口段（替身/bindgen 删除后）：wasm-core `cargo test --lib` **562/0**；
  `cargo check --lib`（common-host / `--no-default-features`）0 error；
- 宿主实跑：`component_e2e` 2、`engine_e2e` 16、`http_e2e` 1、`owner_e2e` 5、
  `sdk_e2e` 3、`wasm_flow_e2e` 7、`pty_e2e` 5、`task_e2e` 6、`host_crypto_e2e` 1 = **46 全绿**。

### 6.6 剩余与基线缺口（T-04 余项 + 本次查明）

1. **锁收口（已完成，本会话）**：新增 `desktop_binding_surface_must_not_return_to_wasm_core`
   （终局形态：全域禁 `TestGuestEvents` + 四条替身/扩展 interface 模块路径；桌面壳
   `common.rs` / `common/component.rs` 禁 `bindgen!`；自排除 lib.rs + 两条正面锚点），
   并修正既有两锁（`api_call_wit_impl…` / `sliced_interfaces…`）里已过期的「合法残余」
   记述（替身已删）。**变异自检 2/2**（注入 needle → 红；注入壳内 `bindgen!` → 红；
   `cp` 还原后 sha256 一致 → 全量 563/0）。
2. **WASI 装配（D1 的 WASI 部分 / D4）—— 用户裁决：不迁宿主，留核心**（2026-10-12）：
   > 「不；双端都可以开启，默认开启；wasip2 p3 就放在核心——为了以后 worker wasm 使用。
   > 或者通过 config 让外部去配置开启或者关闭，默认全部开启。」

   **裁决内容**：WASI 面（p2/p3 注册 + preopen/fs_auth 过滤链）**住核心**、双端形态都编译
   且**默认注册**；开 / 关由**外部配置** `CoreConfig::wasi`（`enabled` / `p2` / `p3`，默认全真）
   决定。理由：worker 类别插件（ADR 0034 L3.b）将来双端都要跑 wasm，接线不能挂在桌面形态上。
   备选 A/B/C（端口注入 / 自报 / 移入 host-kit）**均作废**——它们的前提「WASI 是桌面契约事实」
   已被推翻。

   **落实（本会话，代码已落地）**：
   - 新增 `manager/runtime/wasi.rs`：p2/p3 注册（`add_to_linker`，按配置）+ `build_ctx`
     （preopen 装配，配置关时空上下文）+ `empty_ctx`（无声明面，移动实例化用）+
     `expand_preopen_declarations` / `resolve_preopen_dirs`（自 `common/component.rs` 整块迁入）。
   - `CoreConfig.wasi`（`WasiConfig`，默认 `enabled/p2/p3 = true`）：外部配置文件
     （`wasm-core.json`）与 `set_config` 都能改；注册面构建期固化（`set_config` 对该段 warn，
     与 Engine 参数同性质），preopen 装配每次实例化读配置 → 即时生效。
   - host-kit `wasi-store` 形态 feature **退役**：`wasi_ctx`/`wasi_table` →
     `wasi: Mutex<WasiSlot>` 常编译（`Mutex` 包裹理由 = `WasiCtx` 含非 Sync 的
     `dyn StdinStream/…`，而移动端口 adapter 要求 `&WasmPluginState: Send + Sync`）；
     `wasmtime-wasi` 改无条件依赖（host-kit 与 wasm-core 两侧）。
   - 移动形态：`WasmRuntime::with_config`（外部配置入口，`new` 即默认配置 → 行为不变）+
     linker 也注册 p2/p3（当前移动产物是 wasm32-unknown-unknown ⇒ 惰性无效；worker 启用即用）+
     实例化用 `wasi::empty_ctx()`（移动无 manifest preopen 声明面）。
   - 用例：`common/tests/wasi_config.rs` 4 条（默认开可用 / `enabled=false` 断开 /
     `p2=false` 断开 / `p3=false` 不影响 p2 产物）、`preopen::build_ctx_yields_empty_context_when_wasi_disabled`、
     bare 侧 `test_component_linker_honors_wasi_disabled_config` + 默认全开下「重复注册 p2 必红」实证、
     config 默认全真 + 部分 JSON 保默认两项。
   - 文档：spec §裁决 3（并作废 §2 的 WASI 差异行与「WASI 归桌面宿主」结论）、
     `engine.rs` 机制表、`runtime.rs` 头部、host-kit/wasm-core Cargo 注释同步。
   - `feature 收敛`（`common-host`/`bare-host` 退役）仍依赖 T-05，不能先收；
     `wasi` 模块的形态 cfg 门控只排除「无宿主壳」的 `--no-default-features` 配置
     （两端 feature 都编译它）。
3. **feature 收敛**（`common-host`/`bare-host` 退役、`compile_error!` 守卫删除、
   双 SDK 依赖退出）——依赖 T-05（移动半），本票内不可能先收。
4. **基线缺口（非本会话引入，T-06 前需处理）**：
   - `system_component_test`：`bedcode_plugin_system_test.wasm` **全仓已无构建者**
     （原构建器随 05b/05c 迁移消亡，只余只读读取面）⇒ 该二进制缺产物即红；需补
     fixture_keeper 条目。
   - `session_e2e` 12 条 closed-loop 失败 = **桌面插件产物过期**：产物（10月10 构建）
     仍 import `bedcode:plugin/host-connection`，而当前 WIT 已改名
     `host-connection-registry`（cap-desktop.wit:282-286）⇒ fail-visible 的
     「请用当前 SDK 重建插件产物」按设计触发；T-06 的「app 重建」步骤覆盖。
5. **flaky 候选（新记）**：`engine_e2e::test_component_trap_emits_host_error_log`
   （并发运行下 1/10 级偶发）——`capture` 为**线程局部**订阅者，而
   `block_on_async` 在 ambient / 新线程桥分支下事件可能不在捕获线程；已把「trap 本身
   成立」拆成显式断言（`expect_err` + backtrace 断言）以区分「无 trap」与「日志缺失」，
   T-06 全量回归时复核。