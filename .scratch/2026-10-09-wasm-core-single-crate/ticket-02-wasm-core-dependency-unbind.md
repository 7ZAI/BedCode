# 票 02 · wasm-core 依赖图脱桌面：非核心域迁宿主 + 内核只留核心机制

Status: **in-progress**（批次 01 ✅；批次 02 第 1–5 步已落地未提交 2026-10-09——宿主三个新测试二进制的**实跑**受磁盘限制待补，见实施记录「未跑」段）
spec：`.scratch/2026-10-09-wasm-core-single-crate/spec.md`（票序表）
前置：票 01 POC（P3 实测：`cargo check --no-default-features` 绿，但 `cargo tree` 命中 **28 处**桌面依赖）

## 0. 用户指令（本票方向来源，2026-10-09）

> 「wasm-core 中不应该包含 pty 或者其非核心机制的业务代码，请拆分到宿主，然后在通过在宿主中添加到 wit」
> 「包括 加密 认证 ws mdns 等，通过上面验证的、在宿主端组合的方式完成能力实现」

口径：**wasm-core 只留核心机制**；一切非核心域（pty / http / ws / peer / mdns / crypto / auth / process / app / timer / task / api-call / connection）的实现搬到**宿主与能力域 crate**，由**宿主端组合**完成装配与 WIT 接入。内核不得点名任何具体能力域。

## 1. 现状（2026-10-09 实测）

### 1.1 crate 引用面（`packages/bedcode-wasm-core/src` 内 grep 计数）

| 被引用 crate | 命中 | 性质 |
| --- | --- | --- |
| `bedcode_plugin_api`（桌面 SDK） | 66 | WIT 绑定 + wire 类型 |
| `bedcode_server_websocket` | 29 | adapter + 强制引用 + 测试 |
| `bedcode_pty_engine` | 26 | adapter + 强制引用 + 生命周期钩子 + 测试 |
| `bedcode_server_base` | 21 | 通用基础层（错误/常量/身份）——**保留**（双端共享，非桌面独有） |
| `bedcode_server_http` | 13 | adapter + 强制引用 |
| `bedcode_server_peer_net` | 13 | adapter + 强制引用 |
| `bedcode_discovery_engine` | 5 | adapter + 强制引用 |
| `bedcode_crypto_engine` | 2 | 算法注册表 |

### 1.2 内核自持的 `Host` impl 全表（全部集中在 `manager/runtime/component.rs:68-478`）

| 行 | interface | 核心判定（spec §3 D3） | 处置 |
| --- | --- | --- | --- |
| 68 | `host_storage` | 核心（双端全等 3 函数） | 留 |
| 85 | `host_plugin_database` | 核心（双端全等 5 函数） | 留 |
| 113 | `host_auth` | **交集 0**（移动 5 配对/生物 vs 桌面 10 secret+认证中心） | **迁宿主** |
| 179 | `host_task` | 桌面独有 | **迁宿主** |
| 203 | `host_crypto` | 桌面独有 | **迁宿主** |
| 289 | `host_log` | 核心（5 全等） | 留 |
| 311 | `host_config` | 核心（1 全等） | 留 |
| 324 | `host_connection` | **交集 0** | **迁宿主** |
| 336 | `host_process` | 桌面独有 | **迁宿主** |
| 357 | `host_app` | 桌面独有 | **迁宿主** |
| 371 | `host_timer` | 桌面独有 | **迁宿主** |
| 383 | `host_events` | 交集 1 + 桌面 `notify` | **切片**：核心留 / `notify` 迁 |
| 396 | `host_fs` | 交集 6 + 桌面 3 + WSL | **切片**：核心留 / 桌面扩展迁 |
| 435 | `host_bus` | 核心（5 全等） | 留 |
| 459 | `host_api_call` | 桌面独有 | **迁宿主** |
| 478 | `host_platform` | 交集 2 + 桌面 4 | **切片**：核心留 / 桌面扩展迁 |

`host-pty` / `host-http` / `host-websocket`（服务端部分）/ `host-peer` / `host-mdns` 的 `Host` impl **不在内核**（已在各自能力域 crate，ADR 0035/0039）⇒ 内核里只剩 adapter（`host_api/{pty,http,ws,peer,mdns}.rs`）、强制引用行与生命周期钩子。

## 2. 处置口径（每类一个答案）

| 类 | 去向 | 理由 |
| --- | --- | --- |
| adapter（端口实现） | **宿主** | 端口实现属于宿主（ADR 0035 D2 边界）；内核不持有能力域类型 |
| 强制引用行 + 白名单 | **宿主**（两者同处，ADR 0035 D3） | inventory 静态在被引用 crate 内；宿主引用即被链接 |
| 生命周期钩子（装载/回收） | **机制化**：host-kit 钩子注册表，内核只遍历回调 | 否则内核永远依赖能力域 |
| 跨 crate 集成测试 | **宿主 `src-tauri/tests/`** | 纪律：跨 crate 集成测试一律住宿主；`capability_crates_unit_tests_only` 锁禁止 `packages/bedcode-*` 的 dev-deps 含 bedcode* 内部 crate |
| `bedcode_plugin_api` | 批次 07：内核 bindgen 换核心 WIT（`wit/core.wit`） | 内核不该绑端 WIT（票 01 P2 已证组合等价） |

## 3. 全域搬迁清单

**I 类 · 有能力域 crate，impl 已在 crate**（只需搬走内核侧残留）：

| 域 | 能力域 crate | 内核残留 | 去向 |
| --- | --- | --- | --- |
| host-pty | `bedcode-pty-engine` | `host_api/pty.rs`(164) + 强制引用 + 钩子 | adapter/强制引用/白名单 → 宿主；钩子 → 自报 |
| host-http | `bedcode-server-http` | `host_api/http.rs`(198) + 强制引用 | 同上 |
| host-websocket | `bedcode-server-websocket` | `host_api/ws.rs`(163) + 强制引用 | 同上 |
| host-peer | `bedcode-server-peer-net` | `host_api/peer.rs`(104) + 强制引用 | 同上 |
| host-mdns | `bedcode-discovery-engine` | `host_api/mdns.rs`(141) + 强制引用 | 同上 |

**II 类 · 无能力域 crate，impl 在内核**（需新建落点）：

| 域 | 内核文件 | 新建落点形态 |
| --- | --- | --- |
| host-auth | `host_api/auth.rs`(761) + `auth_center.rs`(400) + `utils/auth*` | 宿主 `src-tauri/src/plugin/auth/`（自带 bindgen + impl + 自报） |
| host-crypto | `host_api/crypto.rs`(191)（`bedcode-crypto-engine` 提供算法） | 宿主 `src-tauri/src/plugin/crypto/`（同上；算法仍由 crypto-engine 供） |
| host-task | `host_api/task.rs`(117) + `unit_executor.rs`(35) | 宿主 `src-tauri/src/plugin/task/` |
| host-process | `host_api/process.rs`(539) | 宿主 `src-tauri/src/plugin/process/` |
| host-app | `host_api/app.rs`(130) | 宿主 `src-tauri/src/plugin/app/` |
| host-timer | `host_api/timer.rs`(80) | 宿主 `src-tauri/src/plugin/timer/` |
| host-api-call | `host_api/api.rs`(559) | 宿主 `src-tauri/src/plugin/api_call/` |
| host-connection | `host_api/connection.rs`(130) | 宿主（桌面侧；移动侧自带） |

**III 类 · 切片域的桌面扩展**（核心子集留内核，扩展部分迁宿主）：

| 域 | 核心子集（留） | 桌面扩展（迁宿主，新增 interface 名） |
| --- | --- | --- |
| host-fs | 交集 6 函数 | `host-fs-desktop`（canonicalize / read-dir / stat）+ `system/wsl.rs`(276) |
| host-platform | 交集 2 函数 | `host-platform-desktop`（local-ipv4 / pick-folders / reveal-in-dir / wsl-distros） |
| host-events | 交集 1 | `host-events-desktop`（notify） |
| host-http | 出站 1 | `host-http-endpoint`（register/unregister-endpoint，随 I 类 http 一并走） |
| abi | `version` | `abi-form`（桌面） |

**保留在内核**：`host_storage` / `host_plugin_database` / `host_log` / `host_config` / `host_bus` + 与之同侧的 `db/`、`sqlite*`、`storage.rs`、`bus.rs`、`config.rs`、`log.rs` 机制面（ADR 0036「机制与真源同侧」）。

## 4. 宿主端组合形态（两条路径）

**路径 A（I 类域，已有能力域 crate）**

```text
能力域 crate：wit/<domain>.wit 分片 + bindgen!(world cap-<domain>) + impl Host + HostModule 自报 + 生命周期钩子自报
宿主        ：① 端口 adapter（实现能力域的窄端口 trait）② `use <crate> as _;` 强制引用 ③ 白名单 ④ WIT 分片拼装进端 world
内核        ：只做 ModuleRegistry/DomainHooksRegistry 的收集 + 校验 + 遍历
```

**路径 B（II 类域，无 crate）**：宿主 `src-tauri/src/plugin/<domain>/` 自建 `bindgen!` + `impl Host for WasmPluginState` + `submit_module!`（与能力域 crate 同形，只是住所在宿主）。

> **孤儿规则说明（决定为什么必须自带 bindgen）**：`impl Trait for WasmPluginState` 要求 `Trait` 或 `WasmPluginState` 之一为本地类型。`WasmPluginState` 属 host-kit（对任何消费方都是外部类型）⇒ 装配方必须**自己生成 trait**（自带 `bindgen!`）才能 impl。因此「把 impl 搬到宿主」= 宿主自带 bindgen；这也是 ADR 0035 D5 的既有结论。

## 5. 批次

| 批次 | 内容 | 状态 |
| --- | --- | --- |
| **01** | host-kit 生命周期钩子注册表（`src/lifecycle.rs`） | ✅ 已绿（4/4 单测，2026-10-09） |
| **02** | **pty 样板**（I 类全流程跑通）：pty-engine 自报钩子 → 内核改遍历 → adapter/强制引用/白名单迁宿主 → 测试迁 `src-tauri/tests/` → 从内核去掉 `bedcode-pty-engine` 依赖 | ✅ 第 1–5 步已落地（2026-10-09；宿主测试待磁盘补跑） |
| 03 | http / ws / peer / mdns 四域按 02 样板搬出 | 待 02 |
| 04 | auth（`host-api/auth.rs` + `auth_center.rs` + `utils/auth*`）+ crypto 迁宿主（路径 B） | 待 02 |
| 05 | task / process / app / timer / api_call / connection 迁宿主（路径 B） | 待 04 |
| 06 | 切片域桌面扩展外迁：fs（+wsl）/ platform / events / abi-form | 待 02 |
| 07 | 内核 bindgen 换核心 WIT（`wit/core.wit` 真源）+ 端 WIT 由拼装脚本生成（票 03） | 待 06 |
| 08 | 平台依赖脱桌面：`webkit2gtk` / `dbus` 的 `cfg(target_os="linux")` 加 `cfg(not(target_os="android"))`；`tauri` features / `tauri-plugin-dialog` / `portable-pty` 按端裁剪 | 待 07 |
| 09 | 门禁收口：`cargo tree --no-default-features` 桌面命中归零 + Android target 实证 + 桌面零回归 | 待 08 |

## 6. 机制设计（批次 01，已落）

```rust
// packages/bedcode-host-kit/src/lifecycle.rs
pub struct DomainHooks {
    pub name: &'static str,                          // 与 HostModuleDesc::name 同源（白名单键）
    pub on_manifest_load: Option<fn(&str, &str)>,    // (plugin_id, manifest_json) —— 域配额自解析，内核零语义
    pub on_plugin_purge: Option<fn(&str)>,           // (plugin_id) —— 停用回收
}
```

- 内核侧改为遍历 `DomainHooksRegistry`，**不再出现任何 `bedcode_pty_engine::` 路径**。
- 能力域侧（`desktop-host` feature 下）自报；`on_manifest_load` 内部解析自己的配额字段（内核不解释 manifest，避免 B6）。
- **不开** `has_live` / `on_shutdown_reclaim`：关停回收与活跃计数由宿主直接调能力域（宿主是装配方，允许点名）——内核不解释产品事件。

## 7. 门禁

- 每批次：内核 `cargo check --no-default-features` + 相关 crate `cargo test` + 桌面宿主 `cargo check`。
- 批次 09 收口：`cargo tree --no-default-features | rg -c 'bedcode-plugin-api|bedcode-server-*|bedcode-pty-engine|bedcode-discovery-engine|portable-pty|actix-web|webkit2gtk|dbus'` **= 0**；Android target `cargo check --target aarch64-linux-android --no-default-features`（跑不了须写明原因）。
- 桌面零回归：桌面 `src-tauri` `cargo test` 全量 + 前端 `pnpm run test:run` + 根 `pnpm exec eslint .` 0 error。
- **行为不变硬要求**：能力域自报 / 装配 / 回收行为逐字不变（ADR 0035「开 feature 前后行为不变」同款）。

## 8. 风险

| 风险 | 处置 |
| --- | --- |
| 孤儿规则：`impl Host for WasmPluginState` 必须与 `bindgen!` 同 crate | 装配方自带 bindgen（路径 A 已有、路径 B 按同形新建） |
| inventory 强制引用必须与白名单同处，搬错 ⇒ 注册静默丢失 | 白名单双向校验 + 跨 crate 实证 |
| 治理锁：`packages/bedcode-*` 的 crate 根不得有 `tests/`、dev-deps 不得含 bedcode* 内部 crate | 跨 crate 集成测试迁宿主 `src-tauri/tests/` |
| 内核有票 18/19 在途未提交改动（component.rs / host_api/{database,events,log} 等） | 仅用精确 edit 改本票相关片段，**不回滚他人改动**（AGENTS §11）；每批改前 `git diff` 复核 |
| 切片域需拆 interface（fs/platform/events/abi）⇒ 双端 ABI bump + 产物重建 | 随批次 06/07 走 `stale_artifact_rebuild_hint` 三形态 |

## 9. 实施记录

### 批次 01 · host-kit 生命周期钩子（2026-10-09，✅）

- 新增 `packages/bedcode-host-kit/src/lifecycle.rs`（`DomainHooks` / `DomainHooksEntry` / `submit_hooks!` / `DomainHooksRegistry` + 4 条单测），`lib.rs` 登记模块与再导出。
- `cargo test --lib` **4 passed, 0 failed**；`cargo test` 全量 11 项注册表测试仍全绿（零回归）。
- 设计取舍：只开 `on_manifest_load` / `on_plugin_purge` 两个时点，关停回收留给宿主直调（内核不解释产品事件）。

### 批次 02 · pty 样板（2026-10-09，进行中：第 1–2 步已验证）

**已落地**

1. `packages/bedcode-pty-engine/src/plugin_binding.rs`（`desktop-host` 下）：
   - `on_manifest_load(plugin_id, manifest_json)`：从**原文**解析 `ptyQuota` 后调 `registry::register_quota`（解析失败/缺字段 ⇒ 落默认档，与「未声明」同形）；
   - `on_plugin_purge(plugin_id)`：适配 `registry::purge_for_plugin`（域内返回回收计数，钩子契约为 `fn(&str)` ⇒ 就地记 `debug!` 吸收返回值，不让形状反向污染通用契约）；
   - `pub static HOOKS: DomainHooks` + `bedcode_host_kit::submit_hooks!(HOOKS)`。
2. `packages/bedcode-wasm-core/src/manager/loader.rs`（原 `bedcode_pty_engine::…::register_quota` 处）：改为重读 `plugin.json` 原文 → `DomainHooksRegistry::collected().on_manifest_load(&plugin_id, &raw)`；原文重读失败不阻断装载，`warn!` 带 `plugin_id` / `error` 字段留痕。
3. `packages/bedcode-wasm-core/src/manager/host/activation.rs`（原 `bedcode_pty_engine::…::purge_for_plugin` 处）：改为 `DomainHooksRegistry::collected().on_plugin_purge(plugin_id)`。

**验证**

- `packages/bedcode-pty-engine`：`cargo test --features desktop-host` **99 passed / 1 failed**；失败项 `plugin_binding::tests::output_notify_is_rate_limited_and_owner_scoped` 单跑**连续两次通过** ⇒ 判定为高负载下的时序 flaky（限频装饰器断言依赖读块节奏，与本次改动无因果关系；已记入 flaky 清单）。
- `packages/bedcode-wasm-core`：`cargo check` **通过**（0 error；仅票 18 在途改动遗留的 unused import 警告）。

### 批次 02 · 第三~五步（2026-10-09，已落地未提交）

**设计裁决（关键，批次 03 沿用）：白名单与端口装配都走「宿主自报」，不加构造参数**

内核不能再点名能力域，但白名单值（哪些模块该在二进制里）与端口装配（域 adapter 何时装）只有宿主知道，且后者**必须早于任何插件激活**（激活发生在 `PluginHost::new` 内部 ⇒ 返回后再装就晚了）。两条都走 **host-kit 装配自报面**（与批次 01 的 `DomainHooks` 同范式），**零签名改动**（否则要动 `PluginHost::new` / `WasmRuntime::new` / `EngineSetup` 及全部测试夹具）：

- `bedcode_host_kit::expect_host_module!(<MODULE_NAME>)` → 白名单的宿主侧来源；
- `bedcode_host_kit::submit_domain_ports_installer!(<静态>)` → 端口装配回调，内核装配链 `install_capability_domain_ports` 遍历调用（`Arc<dyn HostPorts>` 类型擦除，具体类型由实现侧 `downcast_host` 还原——内核不认识宿主上下文类型）。

**已落地（第三~五步）**

1. 新增 `packages/bedcode-host-kit/src/assembly.rs`（+ `lib.rs` 登记与再导出）：`ExpectedModuleEntry` / `expected_host_modules()` / `expect_host_module!` 与 `DomainPortsInstaller` / `install_domain_ports()` / `submit_domain_ports_installer!`；6 条单测（收集 / 去重 / 字典序 / 装配器被调用 / 无自报返回空）。
2. 新宿主落点 `bedcode-desktop/src-tauri/src/plugin.rs` + `src/plugin/pty.rs`（`lib.rs` 登记 `pub mod plugin;`）：**四件同处**——`HostPtyPorts` 五方法端口实现（整文件迁自 wasm-core `host_api/pty.rs`，`ctx` 字段改 `Arc<dyn HostPorts>` + 每次调用 `downcast_host`）、`install`（双登记）、`expect_host_module!(MODULE_NAME)` + `use bedcode_pty_engine as _;`、`submit_domain_ports_installer!`；两个单测（白名单声明/接口/权限三件一致 + 配置快照）。
3. wasm-core：删 `host_api/pty.rs` 与 `pub mod pty;`；`check_permission` 提 `pub`（宿主 adapter 复用同一条拒绝路径，不另起第二份判定）；`install_capability_domain_ports` 末段改为遍历宿主自报装配器；`component.rs` 的 `HOST_MODULES` → `IN_CRATE_HOST_MODULES`（去 pty）+ 新增 `merge_host_module_whitelist`（纯函数）与 `pub fn host_module_whitelist()`（内建在册 ∪ 宿主自报；宿主侧集成锁要同一份合并结果）；删 pty 强制引用行。
4. 测试迁移（第四步）：`host_api/tests/pty_wiring.rs`（**孤儿 + 三条针脚全 stale**）重写为宿主 `tests/pty_wiring.rs` 六例（权限五同步点 / 关停回收 / 停用钩子链 / 加载漏斗钩子链 / **白名单双向 == 收集集** / 域端口装配可取回）；`pty_e2e.rs`（783 行）与 `terminal_output_perf.rs`（426 行）经脚本逐字迁入宿主 `tests/`（只替换头部导入、adapter 路径、`pub(crate)` 字段 → `PermissionScope` trait 访问器、stale 接线锁改钉 DomainHooks 链），共享脚手架进 `tests/support/mod.rs`（`lock_pty_fixture_e2e` / `build_pty_test_component`（只读产物）/ `pty_*` 断言助手一族 + 类型再导出）。
5. 依赖摘除（第五步）：`packages/bedcode-wasm-core/Cargo.toml` 删 `bedcode-pty-engine`（含 `desktop-host` feature 条目）与**已无消费者的 `portable-pty`**；宿主 `Cargo.toml` 注释改为「宿主直接消费」；边界锁两侧同步：宿主 `crate_boundary_lock.rs` 摘 `wasm-core → pty-engine`（ALLOWED + REQUIRED 两表）、wasm-core `crate_boundary_lock.rs` 注释、pty-engine 四处 doc 指针改指新落点。
6. fail-visible 补强：wasm-core `lib.rs::pty_module_must_not_return_to_wasm_core` 增补「`src/host_api/pty.rs` 不得回接」断言；host-kit `tests/registry.rs` 增补白名单差异 **Display 文案**两方向修法锁（原 wasm-core 的 unlisted 用例随收集集收缩会失效，判据上移）；wasm-core `component.rs` 删除依赖「收集集非空」的 unlisted 用例并写明覆盖去向。

**门禁（实跑）**

- `packages/bedcode-host-kit`：`cargo test` **26 全绿**（9 lib + 3 forced_link + 2 forced_link_absent + 12 registry，含新增装配面 6 例与文案锁）。
- `packages/bedcode-pty-engine`：`cargo test --features desktop-host` **99 passed / 1 failed**；失败项即批次 01 已记录的 flaky `output_notify_is_rate_limited_and_owner_scoped`，单跑**连续两次通过**。
- `packages/bedcode-wasm-core`：`cargo check` 与 `cargo check --tests` **0 error**；针对性测试 9 例全绿（`capability_registry_matches_whitelist` / `merge_whitelist_unions_and_dedups_both_sources` / `in_crate_host_module_list_has_no_duplicates` / `missing_capability_module_fails_loudly_with_remediation` / `collected_module_names_are_unique` / `pty_module_must_not_return_to_wasm_core` + 3 条同名过滤命中）。
- 宿主：`cargo check` + `cargo check --tests` 三个新测试目标（`pty_wiring` / `pty_e2e` / `terminal_output_perf`）**0 error**（编译级证据）。
- 边界锁等价预检：`.scratch/2026-10-09-wasm-core-single-crate/_tools/precheck-boundary-lock.py` 复刻断言①②③ → **PASS**（10 拆分产物；wasm-core↔pty-engine 边与标识符零残留）；预检自身已证非空转（首版被注释里的字面量命中而报红，已修）。

**未跑（如实上报 + 原因）**

- 宿主 `cargo test` 实跑三个新测试二进制：**磁盘不可行**（host 测试二进制需 ~10G+ 全量链接，本机根分区长期 96–100%；本会话已 `cargo clean` 释放 4.7G + `cargo clean -p bedcode-wasm-core` 释放 7.2G，仍不足以建 host 测试二进制）。残余风险 = 端到端行为未在本机复跑（编译级 + 等价预检兜底）；**下次磁盘宽裕时必须补跑**：`cd bedcode-desktop/src-tauri && cargo test --test pty_wiring && cargo test --test pty_e2e && cargo test --test terminal_output_perf`。
- **在途/基线红（非本票引入，未修）**：宿主 `tests/ws_e2e.rs`（3 处）与 `tests/ws_output_perf.rs`（1 处）在 HEAD 即编译红——它们 `use bedcode_plugin_api::EndpointAuth`，而 `bedcode_server_websocket::endpoint::register` 收的是该 crate 自己的 `wire::EndpointAuth`（两个类型同名不同源；`packages/bedcode-server-websocket/` 无本地改动）。修法是一行 import 换源，按「非本任务不碰」留给对应会话。

**遗留（批次 03 起处理）**

- wasm-core 仍有 pty 词汇残留：`manager/validation.rs::validate_pty_quota`（含 `system/constants.rs` 的 `PLUGIN_PTY_*`）与 `manager/capability.rs` 的 `"host-pty"` 能力名——本批次只摘依赖与 adapter，未动校验/闭表（不属第 3–5 步范围，避免顺手重构）。
- `install_capability_domain_ports` 仍在内核（内建四域逐行 + 宿主自报遍历）；批次 03 起逐域搬出，直至该函数只剩宿主自报一条来源。
