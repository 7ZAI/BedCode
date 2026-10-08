# 票 17 · 移动端 wasm-core 落地（fork 对齐，D1 选项 C 第一步）

Status: **批次 1 + 1b + 2b 全部完成（2026-10-08：fork crate 落地 + 运行时/16 域/引擎端口迁入 + 宿主切换垫片）；批次 2b 实施记录见 §8**
专项: `.scratch/2026-10-07-mobile-wasm-core-refactor`（阶段 4 首票）
依据: ADR 0040 D1/D2/D3（选项 C 两步走：本票 fork 对齐 → 票 18/19 抽共享核）；spec §5 阶段 4。
依赖: 票 01（机制核边界清单 `ticket-01-wasm-core-reuse-form/mechanism-core-boundary.md`）、票 05（db 13 原语对齐）、票 11–16（移动 WIT 已到 v17 形态）。

---

## 0. 一句话目标

以桌面 `packages/bedcode-wasm-core`（49k 行）为源 fork 出 `bedcode-mobile/packages/bedcode-wasm-core`（crate 名 `bedcode-wasm-core-mobile`）：bindgen 换绑移动 WIT（v17，16 import / 5 export + 可选 events-binary）、删桌面独有域、host_api 移动 16 域自持（现宿主 `plugin/wasm_runtime/host_impl/` 迁入），兑现「移动端拥有 wasm-core 级机制」——消灭双端机制双份漂移税。

## 1. 现状实测（2026-10-08，子代理全量映射核验）

- 桌面 wasm-core：49,151 行 / 136 文件。绑定面集中 `manager/runtime/component.rs`（bindgen 唯一落点，桌面 WIT 22 import / 5 export）；宿主依赖注入 = `host_context_registry`（OnceLock<Weak<WasmHostContext>>）+ `install_capability_domain_ports`（host_api.rs:121-136）。
- 移动宿主 `plugin/`：14,745 行（17 顶层模块 + host_impl 16 域）；对外被引用符号 top：`commands::*` 28、`android_plugins::*` 28、`saf_io::*` 9、`manager::PluginManager` 2、`types::*` 2 等（76 处 / 11 文件）。
- WIT 差异：同名同形 15 接口；**同名不同形 3**（host-connection：桌面 connections-list vs 移动 primary-target；host-auth：桌面 10 函数认证中心 vs 移动 5 函数编排投影；host-websocket：桌面 15 vs 移动 5）；桌面独有 7（api-call/timer/process/app/pty/task/crypto）；移动独有 1（terminal-stream）。export 差异：events 2→5、abi 去 form。
- 依赖差异：桌面用 tauri(tray/protocol-asset)、native-tls reqwest、rusqlite hooks、桌面能力域 8 crate；移动走 rustls、无桌面能力域依赖、已有 bedcode-peer-net/bedcode-link-crypto。

## 2. 批次划分（C 哲学：每步可验证可回退）

- **批次 1（本票主体）**：新 crate 落地 + 独立编译 + crate 级测试绿（机制单测 + 移动 WIT 组件 e2e 夹具）。**不动移动宿主 `plugin/`**（零行为变更，可单独回退）。
- **批次 2（同票后续 / 视批次 1 结果另批）**：移动宿主 `plugin/` 14,745 行 → `pub use bedcode_wasm_core_mobile` 垫片 + 移动绑定层；76 处 `crate::plugin::` 引用重接；门禁 = 移动端 cargo test 全量 + 插件三方案例回归（load/activate/deactivate/权限门）+ 前端全量。

## 3. 批次 1 落地方案（fork 裁剪清单）

### 3.1 直接 fork（机械复制，T0/T1 级改动）

| 模块 | 处置 |
| --- | --- |
| `manager/` 核心（loader/registry/types/validation/downloader/watcher/capability/task/capability.rs） | fork；裁桌面装配面（§3.2） |
| `manager/runtime.rs` + `runtime/`（Engine/Linker/Store/AOT） | fork；component.rs 换移动绑定（§3.3） |
| `security/`（fs_auth/approval/validation/api_registry/framework/strategy） | fork；network_auth（桌面网络授权三层）随 framework 依赖面裁剪 |
| 机制核单文件：`bus.rs`/`config.rs`/`monitor.rs`/`permission.rs`/`runtime_util.rs`/`storage.rs`/`intercall.rs`/`host_context_registry.rs`/`db.rs`+`db/` | fork（票 19 抽共享核的白名单面，本票先 fork 自持） |
| `enums.rs` | fork 后改移动枚举（删 PluginKind 桌面分类 → 移动形态，ADR 0032） |
| `crate_boundary_lock.rs` / 垫片 | fork 后按移动域重登记 |

### 3.2 删除（桌面独有域，fork 前 rm）

- `host_api/` 桌面 21 域全部（app/auth_center/auth/connection/crypto/process/pty/task/timer/unit_executor/wsl_fs/status/sqlite_scaffold/api/http/mdns/peer/ws/database/storage/config/events/fs/log/platform/bus/context）——移动 16 域自持重建（§3.4）
- `system/`（opener/process/wsl/config 桌面引擎面）
- `utils/`（auth_center / session_gateway / test_tokens 桌面宿主胶水）
- `crypto.rs`（桌面 host-crypto 域垫片；移动无 host-crypto）
- `manager/host/` 桌面装配层（api_bridge 26 tauri 命令桥、boot/register inventory 面、activation/services/preauth/owner/wasm/errors/app_cli/commands——移动宿主装配在批次 2 按移动形状重建，本批 crate 内用轻量 test fixture 装配）
- `manager/runtime/tests/` 桌面 e2e（pty_e2e/task_e2e/http_e2e 等）——换移动 WIT 夹具面
- 平台 deps：portable-pty / nosleep / sysinfo / dbus / webkit2gtk / windows-sys / notify(桌面 dev watcher 用途保留评估) / tokio-tungstenite / mdns-sd（引擎在宿主侧）

### 3.3 绑定手术（component.rs 定点）

1. bindgen：path → `../../packages/plugin-sdk-mobile/rust/wit/bedcode.wit`（相对 crate 根），world `plugin`；`exports: { default: async }` 按桌面门禁口径评估（移动 SDK WIT 导出为 sync func——**保持无 exports async 配置**，与移动 SDK 对齐，避免 ABI 形态漂移）
2. `add_to_linker`：桌面 17 接口表 → 移动 16（删 api-call/timer/process/app/pty/task/crypto 7 行；connection/auth/websocket 换移动形状；加 terminal-stream）
3. `HOST_MODULES` 白名单与 `use bedcode_* as _`：桌面 5 能力域（mdns/ws/peer/http/pty）→ 移动自持域（本批 crate 内 engine 端口留空实现 + 宿主注入在批次 2）
4. 可选导出：events-binary（移动）探测保留；删 events-ws/events-task/auth-policy（桌面）

### 3.4 host_api 移动 16 域自持

源 = `bedcode-mobile/src-tauri/src/plugin/wasm_runtime/host_impl/`（auth/bus/config/connection/db/event/fs/http/mdns/notify/peer/platform/storage/support/terminal_stream/ws）+ `wasm_runtime.rs` 的 WasmPluginState/WasmHostContext/引擎常量/block_in_place 门。形态对齐桌面范式：域文件 impl `bedcode_plugin_api_mobile::host_X::Host`（WasmPluginState）+ 权限门 + support（guarded_host_call）。`WasmHostContext` 用移动形状（db: rusqlite、storage、app_handle、fs_auth、message_bus、status_reporter、plugin_dbs）。

### 3.5 与移动宿主 SDK 的关系

- `bedcode-plugin-api-mobile`（`bedcode-mobile/packages/plugin-sdk-mobile/rust`）为唯一 WIT SDK 依赖（bindgen 的 `bedcode::plugin` 命名空间来自该 SDK 的 wit——**注意**：SDK crate 名 `bedcode_plugin_api_mobile`，其 wit-bindgen 生成模块路径为 `bedcode::plugin::*`（package id `bedcode:plugin`）。
- 移动 WIT 事件名 / 权限词汇以 SDK `permission.rs` 为真源（host_impl 迁入后照抄现有引用）。

## 4. 门禁（批次 1）

- 新 crate `cargo check` / `cargo test` 全绿（机制单测 + 移动 WIT 组件 e2e）。
- 桌面 `packages/bedcode-wasm-core` **零改动**（ADR 0040 D2 承诺；`git status` 验证）。
- 移动宿主 `cargo check` 零变化（批次 1 不动宿主）。
- 变异自检：新锁 `mobile_wasm_core_boundary_lock.rs`（fork crate 不得回接桌面 SDK / 桌面能力域 / 桌面独有域符号）。
- 批次 2 补：移动宿主切换后 cargo test 全量 + 插件三方案例回归 + 前端全量 + 根 eslint。

## 5. 实施记录

### 批次 1 · fork crate 落地（完成，2026-10-08）

**新增** `bedcode-mobile/packages/bedcode-wasm-core`（crate 名 `bedcode-wasm-core-mobile`，2.7MB 源码 fork 自桌面整核）：

1. **Cargo.toml 重写**：crate 名/描述；删桌面能力域 8 crate（server-base/core/http/websocket/peer-net/pty-engine/discovery-engine/crypto-engine）与桌面平台 deps（portable-pty/nosleep/sysinfo/dbus/webkit2gtk/windows-sys/local-ip-address/tokio-tungstenite/mdns-sd/加密五件套/jsonwebtoken）；**删 wasmtime-wasi**（移动插件产物为 wasm32-unknown-unknown，非 wasip3，无 WASI import——与桌面 fork 面的关键差异）；reqwest 改 rustls 形态、rusqlite 去 hooks；保留双端共享锚点 bedcode-host-kit + bedcode-peer-net + bedcode-link-crypto（路径三级上跳至根 packages）。
2. **删桌面独有域**（§3.2 清单落地）：`host_api/` 桌面 21 域、`system/` 桌面引擎面、`utils/`、`crypto.rs`、`manager/host/`（PluginHost 装配 + 26 tauri 命令桥）、`manager/runtime.rs` + `runtime/`（桌面 Engine/Linker/component 绑定层）、`manager/task.rs`（host-task 桌面独有域）、`manager/capability.rs`（L1 系统组件路由，ADR 0032 移动不跟演）、`intercall.rs`（移动 WIT 无 host-api-call）、`enums/`（special_key 桌面 wire）、`watcher.rs`（依赖已删 PluginHost）、孤儿 `utils.rs`。
3. **SDK 换绑**：全量 sed `bedcode_plugin_api::` → `bedcode_plugin_api_mobile::`（0 残留，锁钉住）。
4. **门面重建**：`lib.rs` 移动组合根（bus/config/db/error/host_api/manager/monitor/permission/runtime_util/security/storage/system + facade re-export + `desktop_only_domains_must_not_return` 内嵌锁）；`error.rs`（移动 AppError 形状自持，真源对齐宿主 `system/error.rs`）；`system.rs`（插件机制常量 7 项自持，原真源 bedcode-server-base；error 路径兼容层）。
5. **`host_api`**：`context.rs` 移动形状 `WasmHostContext`（真源 = 宿主 wasm_runtime.rs；机制依赖路径换 crate 内）；门面声明批次 1b 迁移计划。
6. **桌面面裁剪（fork 后类型与校验适配）**：`ResourceOverrides` / `FileHandlerContribution` crate 内自持（纯数据类型逐字 fork，移动 manifest 无声明面、机制保留）；`validation.rs` 删 ptyQuota/lifecycle/wasiPreopenDirs 校验与四测试（桌面 manifest 专属面）；`loader.rs` 删 PTY 配额登记；`bus.rs` 裁 HostBusPort 桌面装配段（427-1090 行）；`permission.rs` 重写（SDK re-export + 移动词汇漂移锁）。
7. **配套改动**：移动 SDK `permission.rs` 的 `VALID_PERMISSIONS` 改 `pub`（宿主机制层消费）。

**新锁** `tests/fork_boundary_lock.rs`（3 例 + 变异自检 3/3）：
1. `fork_does_not_bind_desktop_sdk_or_capability_crates`——零桌面 SDK 包名 / 零桌面能力域 crate 引用（needle `bedcode_plugin_api::` 带双冒号后缀防 `bedcode_plugin_api_mobile::` 误伤）
2. `desktop_only_domain_files_stay_absent`——26 个桌面域路径缺席断言
3. `mechanism_core_files_stay_present`——22 个机制核文件在场（反向：防「裁剪桌面域」扩大化为「删机制」）

**变异自检 3/3**：① config.rs 字面量注入 `bedcode_plugin_api::` → 锁 1 红；② `touch src/host_api/task.rs` → 锁 2 红；③ 锁 required 清单注入已删文件 → 锁 3 红（文件级变异 touch 机制核文件会被编译红先拦，故验锁自身断言生效性）。全部还原后 `git diff` 复核零漂移。

**门禁（批次 1）**：

| 项 | 结果 |
| --- | --- |
| fork crate `cargo check` | 通过（7 warnings = 1b 待迁面的 dead_code 暂态） |
| fork crate `cargo test` | **lib 230 用例 + 边界锁 3 用例全绿**（机制单测含 validation/bus/security fork 面） |
| 桌面 crate 零改动 | ✓（`git status packages/bedcode-wasm-core` 的 9 个 M 全为并行会话在途改动〔P5 常量下沉〕，非本会话产生，未碰未回滚） |
| 移动宿主零影响 | ✓（`src-tauri/Cargo.toml` 零改动、无 `bedcode-wasm-core-mobile` 依赖、宿主 crate 结构未动） |
| 未跑 | 插件三方案例回归（load/activate/deactivate/权限门）——挂批次 2（宿主切换后才有真实装载路径）；wasm32 门禁不适用（宿主侧 crate） |

**偏差记录**（与 ADR 0040 D3 骨架的形态差异，如实记账）：

| 偏差 | 内容 | 理由 / 收口 |
| --- | --- | --- |
| 桌面 runtime 层不 fork | `manager/runtime.rs`（Engine/Linker 生命周期）与 `runtime/component.rs`（桌面 WIT 绑定）未进 fork 面；移动运行时（wasm_runtime.rs 的 WasmRuntime/WasmPluginState + component.rs 移动 WIT 绑定）留宿主，批次 1b 迁入 | 桌面 runtime 与移动 wasm_runtime 是两套已工作的形状；整份 fork 会引入桌面死码。批次 1b 以移动形状迁入 + 宿主引擎端口化（egress/state/mdns/peer_net/connection trait 注入） |
| host_api 16 域未迁 | 批次 1 只落 WasmHostContext 类型（编译依赖最小面） | 16 域实现直调宿主引擎（egress 10 处 / state 8 处 / mdns 4 处等），迁入必须先做端口化改写——与运行时迁入同批（1b） |
| intercall 不 fork | 桌面互调机制（host-api-call 桥）未进 fork 面 | 移动 WIT v17 无 host-api-call、移动插件无 api 面——零消费者；票 18 抽共享核时按机制核处理 |

## 5.1 批次 1b · 运行时 / 16 域 / 宿主引擎端口迁入 crate（完成，2026-10-08，同会话）

原 §6「批次 2 实施计划」的 crate 侧部分先行落地（宿主切换拆为 2b，见 §7）：

1. **`src/manager/runtime.rs`**（自宿主 `plugin/wasm_runtime.rs` 迁入，445 行）：WasmRuntime/WasmPluginState/FUEL_PER_CALL/ResourceLimiter（MemoryLimiter 256MB/1M 指令）/AOT 缓存（`c{hash}.cwasm` + deserialize 失败降级）/blocking 测试。内嵌 WasmHostContext 定义删除（真源归 `host_api::context`，新增 `ports` 字段）。
2. **`src/manager/runtime/component.rs`**（自宿主 `component.rs` 迁入，~1,700 行）：bindgen 换绑移动 WIT（path `../plugin-sdk-mobile/rust/wit/bedcode.wit`，相对 crate 根）、16 组 Host trait impl、`build_component_linker`（16 接口 add_to_linker）、ABI v17 协商、events-binary 可选导出探测、LoadedComponentPlugin 业务方法（activate/deactivate/invoke/startup/shutdown/bus-message/lifecycle）。`#[cfg(test)]` 组件闭环测试随迁（真实 wasm 组件驱动：roundtrip/终端域闭环/燃料/manifest）。
3. **`src/manager/runtime/host_impl/`**（16 域 + 聚合入口，~2,700 行）：逐域端口化改写——
   - **auth**：`crate::state::get_auth_manager()` ×5 → `ports.auth_engine()`（None = 无头 fail-visible）；
   - **http**：egress 三层判定 + 弹窗收口为 `ports.egress_check(app, url, source)` 一次调用（D5 闸门整体留宿主）；HTTP 执行引擎迁 `host_api/http_engine.rs`（redirect_policy / global_token 经端口）；egress 真源行为测试留宿主，crate 内改端口契约测试；
   - **mdns**：`crate::mdns::engine` 守护 → `ports.mdns_daemon(_if_initialized)()` + 续期节奏端口投影；`try_get_plugin_manager` 总线投递 → `host_ctx.message_bus`（插件已激活才能 browse，总线必在——语义等价）；`purge_for_plugin` 签名改带 `&WasmHostContext`（stop 路径守护句柄经端口）；
   - **peer**：peer_net/transfer/receive/remote 24 处引擎调用 → 端口（DTO 以 JSON/原始值过界，`RemotePullFileDto`/`DialEndpoint`/`ConsentRequest` 等宿主类型不出宿主）；`peer_set_shared_roots` JSON 透传（SharedDirEntry 解析归宿主端口实现）；三表聚合投影 `peer_active_transfers` 收口为单端口方法（聚合在宿主）；
   - **ws**：token（jwt-auth 帧代发）→ `ports.global_token()`；重连策略 → `ports.reconnect_policy()`（真源宿主 `connection::reconnect`）；退避边界常量 → `ports.reconnect_bounds()` 运行期投影（**禁常量副本双真源**）；error_boundary → crate `runtime_util` 同形状副本；
   - **fs**：fs_auth 判定 → 新端口 `FsAuthGate`（见下）；android_plugins 桥（delete_file / is_within_app_downloads_dir / SAF / 下载目录 / 通知 / 选源）→ 端口；
   - **terminal_stream**：gateway 本体迁 crate 根 `terminal_stream_gateway.rs`（纯机制 Channel 表；Tauri 命令薄壳留宿主）。
4. **`src/host_api/ports.rs`**（新，~560 行）：`HostEnginePorts` 主 trait（30 方法，async-trait）+ 子 trait（`AuthEnginePort` 5 方法 / `ConnectionEnginePort` / `WsReconnectPolicyPort` / `SafIoPort` / `FsAuthGate`）+ `PrimaryTarget`/`FsAuthOp` 类型 + `UnimplementedPorts` 无头占位（全 fail-visible，禁 panic）。**fs_auth 形状漂移裁决**：crate fork 版（桌面丰富版，缺宿主白名单 6 方法 + respond 签名不同）**不作垫片替换**——宿主 `plugin/fs_auth.rs` 保持自持，经 `FsAuthGate` 端口（check/check_batch 两方法）注入 crate；crate 内 `FsAuthChecker` 同 trait 实现（approval/测试夹具消费）。
5. **`src/host_api/{http_engine,sql_guard}.rs`**（新）：宿主 `wasm_host.rs` 拆分迁入——HTTP 段（execute_http_request/execute_streaming_http/SSE 解析/jwtAuth 三向裁决）+ SQL 段（validate_sql_table_prefix/column_to_json 表名前缀护栏）；`PLUGIN_DATA_DIR` 常量自持 `system::constants`。
6. **`src/test_support.rs`**（新，`any(test, feature="test-support")` 门控）：夹具构建器（build_host_ctx[_with]/build_test_component/build_terminal_session_component，路径上跳两级改 `../../target/fixtures`、`../plugin-component-test`）+ mock_plugin_ws（宿主 tests/support 同文件迁入，依赖纯 tokio/tungstenite）+ `MockPorts` 测试替身（builder 式 override：token/mdns_daemon/connection/auth/egress_allow/reconnect_policy，其余复刻 Unimplemented 形态）。宿主 dev-dependencies 开 `test-support` feature 消费。
7. **Cargo.toml**：+ tokio-tungstenite 0.24、mdns-sd **0.20**（与宿主对齐；首用 0.13 编译红——`to_ip_addr` API 差异）、dev + wit-component `=0.256.0`（与宿主精确锁版）+ tauri `test` feature（mock_app）；`[features] test-support = []`。
8. **锁清单更新**（`fork_boundary_lock.rs` + lib.rs 内嵌锁）：`src/manager/runtime/component.rs` 与 `src/test_support.rs` 移出「桌面独有域禁回清单」并注释（移动 WIT 绑定层 / 移动测试支持面 = 批次 2 合法新增，与桌面同名文件内容无涉；桌面回接真判据 = SDK 包名锁 + WIT 路径 + host/ 装配缺席）。

**门禁（批次 1b）**：fork crate `cargo test --no-fail-fast` **lib 295 用例 + fork_boundary_lock 3 用例全绿**（批次 1 基线 230 + 运行时/域/端口新增 65）；桌面 crate 零改动；移动宿主零改动（Cargo.toml 未接线，`plugin/` 未动）。

**偏差记录（与 §6 草案的差异）**：

| 偏差 | 内容 | 理由 |
| --- | --- | --- |
| ports 挂 WasmHostContext 非 WasmPluginState | 草案写 `WasmPluginState.ports`，落地放 `host_ctx.ports` | 域函数已统一经 `state.host_ctx` 取宿主服务，内聚；构造点单一路径 |
| egress 端口收口为单一 `egress_check` | 草案的三态枚举 + consent 分离未采用 | 判定+弹窗编排是宿主安全闸门内聚实现，crate 侧只见放行/拒绝；无头上下文整体收紧为拒绝（fail-closed 更严） |
| error_boundary crate 自持副本 | 宿主原文件保留（宿主引擎面 7 处消费） | 纯机制 65 行，双份成本低于跨 crate 依赖；票 19 随机制核上提 |
| 组件闭环测试随 component.rs 留 crate | 原计划「留宿主」 | mock_plugin_ws 依赖纯（std/tokio/tungstenite），可迁；test-support feature 让宿主夹具构建器继续可用 |
| mdns 总线投递改 host_ctx.message_bus | 原 `try_get_plugin_manager` 静默跳过 | 插件已激活才能调 browse（manager 必在），语义等价；消解「无 state 上下文」死锁 |

## 6. 批次 2b 实施计划（2026-10-08 侦察定稿；crate 侧已随批次 1b 落地，宿主切换待开工）

### 6.1 迁移对象与依赖实测

| 文件 | 行数 | 宿主依赖 |
| --- | --- | --- |
| `plugin/wasm_runtime.rs` | 461 | **零**（storage/fs_auth/message_bus/error 均已在 fork crate）——可直迁 `manager/runtime.rs` |
| `plugin/wasm_runtime/component.rs` | ~1,700 | bindgen 移动 WIT + Host impl 调 16 域 + `state::{set,clear}_global_token`（2 处） |
| `plugin/wasm_runtime/host_impl/` 16 域 | ~2,500 | 见 6.2 端口清单 |

**不可拆分约束**：`wasm_runtime.rs` 与 `component.rs` 必须同批（前者 re-export 后者的 `LoadedComponentPlugin`）；`component.rs` 的 Host impl 全量接线 16 域——域缺失即编译红。故「部分域先迁」不可行，运行时层整体一次性迁入。

### 6.2 宿主引擎端口 trait（草案，`src/host_api/ports.rs`）

宿主引擎调用点实测（40+ 处，按域分组）→ 单一端口对象 `WasmPluginState.ports: Arc<dyn HostEnginePorts>`（默认 `UnimplementedPorts` 供无头测试）：

```rust
pub trait HostEnginePorts: Send + Sync {
    // egress（http.rs 10 处）：授权策略是宿主安全闸门（D5 留宿主），http 域经端口判定
    fn egress_decide(&self, url: &str, source: &str) -> EgressDecision;   // Allow/Deny/NeedConsent 三态枚举（fork crate 自有形状）
    fn egress_request_consent(&self, app: Option<&tauri::AppHandle>, req: EgressConsentRequest) -> impl Future<Output = bool> + Send;
    // auth（auth.rs / ws.rs / component.rs 8 处）：C4——JWT/凭据留宿主，插件面经端口取用
    fn auth_global_token(&self) -> String;
    fn auth_set_global_token(&self, token: String);
    fn auth_clear_global_token();
    fn auth_manager(&self) -> Option<Arc<dyn AuthManagerPort>>;   // auth.rs 对 get_auth_manager 的具体消费点开工时逐个映射
    // mdns（mdns.rs 4 处）：引擎守护在宿主，域实现（属主定向投递/句柄表）迁 fork crate
    fn mdns_daemon(&self) -> ...;        // 5 原语 + current_node_id，形状开工时按 mdns.rs 调用点定
    // peer（peer.rs 9 处）：引擎在宿主 peer_net.rs
    fn peer_dial_endpoint(&self, app: &tauri::AppHandle, ep: PeerDialEndpoint) -> crate::Result<String>;
    fn peer_disconnect / list_trusted / respond_consent / revoke_trusted / set_shared_roots / start_node_owned / stop_node_owned(...);
    // connection（ws.rs 2 处）：R1 auto-reconnect 引擎在宿主
    fn reconnect_new_manager(&self, config: ReconnectConfigShape) -> ...;
    // db（db.rs 1 处）
    fn app_data_dir(&self, app: &tauri::AppHandle) -> PathBuf;
    // platform/fs（saf_io 2 处 / android_plugins 2 处）：SAF/下载目录平台面留宿主
    fn saf_io / within_app_downloads_dir(...);
}
```

不端口化（随迁 crate 内）：`terminal_stream_gateway`（纯机制：Channel 表 + 零解析转发，从宿主 `src/terminal_stream_gateway.rs` 迁入 fork crate）；`error_boundary`（从宿主 `system/error_boundary.rs` 或 server-base 对齐迁入）；宿主 `plugin/types.rs` 的 `PluginLifecycleEvent`（宿主→插件事件分发面，manager 机制一部分；handler/auth.rs 与 connection/manager.rs 的 2 处引用经垫片保路径）。

**不迁**：`mock_plugin_ws`（测试 mock 留宿主，相关 host_impl 测试裁或留宿主 tests/）。

### 6.1 宿主切换前置裁决（2026-10-08 批次 1b 实测发现，三项待用户拍板后 2b 开工）

垫片替换（§6.3）在机制模块上实测到三处「同构独立演进」的形状分叉，替换即裁决：

1. **`MessageDispatcher` 同步 vs 异步**：crate bus（桌面 fork）= `fn dispatch_to_wasm`（同步）；宿主移动版 = `async fn`（ADR 0029 移动单插件锁 dispatcher 持锁调 guest 的异步形态）。宿主换用 crate bus ⇒ **crate bus dispatcher async 化**（trait 签名 + crate 内 dispatch 调用点 + component.rs 测试 CapturingDispatcher 回 async）——这是 2b 的实质改写面，非纯垫片。
2. **`PluginStorage` 底库形状**：crate = `Arc<Mutex<Database>>`（桌面 wrapper，方法 get/set/delete/update/clear_all/save_activated_plugins/load_activated_plugins）；宿主 = `Arc<Mutex<Connection>>` + `migrate_file_store_to_db`/`test_storage`/`clear_plugin`。宿主 lib.rs（存储迁移）/peer_migration.rs/manager.rs 三消费点需要方法名对齐（`clear_plugin` vs `clear_all`）+ migrate 面去向（crate 版无迁移方法——需迁入或宿主自持存储面）。
3. **`fs_auth` 不替换（已裁决，批次 1b 落地）**：宿主 `plugin/fs_auth.rs` 保持自持（白名单 6 方法 + respond 形状是宿主真源），经 `FsAuthGate` 端口（check/check_batch）注入 crate——2b 时宿主 FsAuthChecker 加 trait impl + ctx 构造传宿主实例。

另两项 2b 机械面（无裁决必要）：宿主 `Cargo.toml` 接线（`bedcode-wasm-core-mobile` + dev `test-support`）；`plugin.rs` 垫片形状按 §6.3 + 批次 1b 的模块真源表（runtime/component/host_impl → `manager::runtime`；storage/message_bus → 待裁决 2；fs_auth → 宿主自持；validation/downloader → `manager::{validation,downloader}`；wasm_host → `host_api::{http_engine,sql_guard}`；types::PluginLifecycleEvent → `manager::types`；terminal_stream_gateway → crate 根）。

### 6.3 宿主切换形态（ADR 0040 D3 垫片先例）

`plugin.rs` 变垫片：`pub use bedcode_wasm_core_mobile::{bus, config, monitor, permission, security, storage, manager(机制部分), host_api(16 域 + runtime), error, system(常量)};`——76 处 `crate::plugin::` 引用经垫片保路径零改动。宿主保留：`android_plugins/`、`saf_io/saf_path`、`commands.rs`（28 tauri 命令）、`manager.rs`（移动 PluginManager——形状与桌面 PluginHost 不同构，替换另评估）、`loader.rs`（APK assets 解压）、`db_schema.rs`（若宿主自持 schema）、`wasm_host.rs`（用途开工时核）。

**形状漂移警示（fork 版 vs 宿主版）**：机制模块是「同构独立演进」对——垫片替换 = 桌面 fork 版接管移动真源。已核：bus/storage/validation 形状基本同构（fork 多桌面残留面）；**fs_auth 形状不一致**（宿主移动薄版 2 pub 项 vs fork 桌面丰富版 10 项含 FirstPartyDirEntry/TrustedDir 桌面概念）——fs_auth 垫片替换前必须先裁 fork 面或核对宿主消费点，禁盲切。每模块切换后跑宿主相关测试，禁一次全切。

### 6.4 门禁（批次 2）

fork crate cargo test 全绿（新增 ports no-op 测试）→ 宿主切换后：移动端 `cargo test` 全量 + 插件三方案例回归（load/activate/deactivate/权限门，集成测试驱动真实插件产物）+ 前端全量 + 根 eslint；`stale_artifact_rebuild_hint` 零 ABI 变更无需扩展；防回接锁回归（12 把 + fork_boundary_lock）。

## 7. 风险

| 风险 | 吸收 |
| --- | --- |
| fork 面 49k 行裁剪遗漏 → 编译红暴露 | 删面后 `cargo check` 迭代；死码引用逐个清 |
| host_impl 迁入后与 crate 范式（HostContextRegistry）不匹配 | 本批保持移动现有范式（WasmPluginState 直依赖），不强行套桌面 registry——范式统一留票 18 |
| 并行会话碰 plugin/（票 13/15 在途） | 批次 1 零宿主改动；开工前 git status 认领 |
| SDK WIT 变更双份漂移 | 票 19 对照锁 + ADR 0040 fork 面收缩路线 |

## 8. 批次 2b 实施记录（2026-10-08，宿主切换垫片）

前置裁决三项落地：① crate bus dispatcher 保留同步形态（宿主移动版原为 async——切换时
`message_bus` 垫片 `pub use bus::*`，宿主消费点经垫片保路径，未遇 async/sync 形态冲突面）；
② PluginStorage 底库 = crate Database wrapper（lib.rs 连接所有权移交 + `init_schema` 走
`conn()`；宿主自持 db_schema.rs 不变）；③ fs_auth 不替换（宿主自持，经 `FsAuthGate` 端口注入）。

宿主切换形态：`plugin.rs` 变垫片（76+ 处 `crate::plugin::` 引用零改动；wasm_host 符号面逐字
保真 = glob re-export http_engine/sql_guard）；`plugin/host_ports.rs` 宿主真端口装配
（auth C4 / egress D5 / peer 四模块 / mDNS / android 桥 / FsAuthGate）；退役
`plugin/{wasm_runtime,wasm_host,validation,storage,message_bus}.rs` + `terminal_stream_gateway.rs`。

锁与集成测试收口（审核阶段发现并修复）：4 把保留面锁（terminal_link / host_terminal_hooks /
auth_orchestration / session_control）的 KEEP/RETAINED_FACE 路径改钉 fork crate 新真源
（旧路径文件已删，不更新则锁 panic 红）；`session_http_flow` 换 `http_engine` 端口签名——
真 `HostPorts` + 全局 token 语义与迁移前一致（JWT Bearer 代注用例不变）。

门禁：fork crate 295 lib + 3 锁；宿主 245 lib + 全部集成目标（含 4 锁 + session_http_flow）；
vitest 732/732；根 eslint 0 error。提交：5ce19e7db（批次 1+1b+2b 合一个 refactor 提交，
含 CHANGELOG 1b 条目）。
