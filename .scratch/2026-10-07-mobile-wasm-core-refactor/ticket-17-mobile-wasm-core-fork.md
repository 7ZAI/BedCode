# 票 17 · 移动端 wasm-core 落地（fork 对齐，D1 选项 C 第一步）

Status: **批次 1 完成（2026-10-08：fork crate 落地 + 编译/测试全绿 + 边界锁变异 3/3）；批次 2 待实施（宿主切换）**
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

## 6. 风险

| 风险 | 吸收 |
| --- | --- |
| fork 面 49k 行裁剪遗漏 → 编译红暴露 | 删面后 `cargo check` 迭代；死码引用逐个清 |
| host_impl 迁入后与 crate 范式（HostContextRegistry）不匹配 | 本批保持移动现有范式（WasmPluginState 直依赖），不强行套桌面 registry——范式统一留票 18 |
| 并行会话碰 plugin/（票 13/15 在途） | 批次 1 零宿主改动；开工前 git status 认领 |
| SDK WIT 变更双份漂移 | 票 19 对照锁 + ADR 0040 fork 面收缩路线 |
