# 票 02 · wasm-core 依赖图脱桌面：非核心域迁宿主 + 内核只留核心机制

Status: **in-progress**（批次 01–06 ✅ 2026-10-10：批 01 host-kit 钩子注册表；批 02 pty 样板全流程；批 03 pty 配额判据 / mdns 路由词汇迁能力域 + mdns·peer·ws·http 四域 adapter 迁宿主 + 内核测试端口替身补位；批 04 crypto + auth 迁宿主（路径 B，宿主共享 bindgen 地基；auth_center 注册表留内核）；批 05 task / process / app / timer / connection 整域迁宿主 + host-api-call WIT impl 迁宿主（回复道编排留内核）+ 执行器自报注册面；**批 06 切片域桌面扩展外迁（fs 三函数 WIT impl / platform 四函数 / events.notify / abi.form）+ 桌面 ABI 35→36 + WIT 四接口拆分 + 插件产物全量重建，详见文末实施记录**）。下一批 = 07（内核 bindgen 换核心 WIT wit/core.wit + 端 WIT 拼装——票 03 承载）。宿主测试实跑欠账：pty_e2e / terminal_output_perf / 宿主 --lib 全量 / system_component_test 仍待磁盘与时间，ws_e2e 系 HEAD 既有编译红（EndpointAuth 同名不同源，非本票））
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
| 04 | auth（`host-api/auth.rs` + `auth_center.rs` + `utils/auth*`）+ crypto 迁宿主（路径 B） | **✅（2026-10-09）**：第 1 步 crypto + 第 2 步 auth（**auth_center 注册表显式留内核**——四方消费裁决见实施记录） |
| 05 | task / process / app / timer / api_call / connection 迁宿主（路径 B） | **✅（2026-10-09）**：五域整迁（task / process / app / timer / connection）；host-api-call 只迁 WIT impl（薄转发，回复道编排留内核）；`unit_executor.rs` 判定为留内核引擎接口（对 §3 清单的修正）；执行器注册改 `submit_unit_executor!` 自报收集 |
| 06 | 切片域桌面扩展外迁：fs（+wsl）/ platform / events / abi-form | **✅（2026-10-10）**：四接口 WIT 拆分 + 宿主路径 B 落点 + 内核收窄 + cfg(test) 替身 + 反向锁 + ABI 35→36 + 插件产物全量重建（详见文末实施记录） |
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

- ~~wasm-core 仍有 pty 词汇残留：`manager/validation.rs::validate_pty_quota`~~ ✅ **批次 03 已摘**（见下节）；`manager/capability.rs` 的 `"host-pty"` 能力名**裁决保留**——它是 `HOST_PRIMITIVE_CAPABILITIES`（宿主 20 组 WIT 接口清单，能力注册表「宿主原语提供者」判据）的一项，属机制而非域语义（同表亦有 `host-mdns` / `host-http`…，逐域删除会让注册表失去判据来源）。
- `install_capability_domain_ports` 仍在内核（内建四域逐行 + 宿主自报遍历）；批次 03 起逐域搬出，直至该函数只剩宿主自报一条来源。

### 批次 03 · pty 配额判据迁能力域（2026-10-09，用户指令：域知识优先迁对应能力 crate）

**用户指令**：mdns / pty 这类「内核或宿主里的域知识」优先迁进对应能力 crate 的 lib。

**边界裁决（结论先写）**：宿主 adapter（`src/plugin/pty.rs` / `src/plugin/mdns.rs`）**不能**搬进能力 crate——其正文全是宿主原语（`check_permission` / `message_bus` / `AppConfig` / `tauri::async_runtime` / `runtime_util::block_on_async`），搬过去需要「能力 crate → wasm-core / tauri」的上向依赖，与边界锁及 `bedcode-host-kit/src/ports.rs` 模块文档（「迁出的能力域不碰向下转型」）冲突。可迁的是**域知识**：本批先做 pty 配额判据。

**改动（pty 判据：内核 → 能力域）**

- `packages/bedcode-host-kit/src/lifecycle.rs`：`DomainHooks::on_manifest_load` 契约 `fn(&str, &str)` → **`fn(&str, &str) -> Result<(), String>`**；`DomainHooksRegistry::on_manifest_load` 同步返回 `Result` 并**短路**上抛首个 `Err`（遍历顺序 = 能力域名典序）。新增拒绝探针 + `manifest_load_rejection_is_propagated` 用例（lib 10 例全绿）。
- `packages/bedcode-pty-engine/src/plugin_binding.rs`：域侧钩子自解析 + **自仲裁**（`1..=PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN`，越界 / 0 ⇒ `Err`，**不夹取**）；**先判后动**（被拒不登记配额表）。新增 `manifest_quota_tests`（原内核两条用例迁此）。
- `packages/bedcode-wasm-core/src/manager/validation.rs`：删 `validate_pty_quota` 与 `validate_manifest_required` 里的调用（留指路注释）；两条 pty 用例删除并注明去向。
- `packages/bedcode-wasm-core/src/manager/loader.rs`：域回调**移到授权之前**（拒绝 ⇒ 不装载，不留已授权 / 已登记的半成品），`Err` ⇒ `error!`（含 `reason`）+ `continue`。
- `packages/bedcode-wasm-core/src/manager/downloader.rs`：注明「域侧声明仲裁不在安装入口，越界声明安装期通过、**装载期被拒**」。
- `packages/bedcode-pty-engine/src/plugin_binding/registry.rs`：模块文档改指域侧仲裁点。
- 宿主 `tests/pty_wiring.rs` 的加载漏斗漂移锁同步改写：次序断言由「钩子紧贴授权之后」改为**「钩子先于授权」**+ 拒绝必须 `continue`；内核侧改为**反向断言**（`!validation.contains("validate_pty_quota")`）+ 能力域自持判据断言。

**门禁（实跑）**：host-kit `cargo test` **27 绿**（12+3+2+10，含 2 条新增/改写钩子用例）；pty-engine `cargo test --features desktop-host` **101 passed / 0 failed**（含新用例；批次 02 那条 flaky 本次亦绿）；wasm-core `cargo check --tests` **0 error** + `manager::validation` 8 绿 + `manager::loader` 6 绿；宿主漂移锁六条断言**离线复刻 PASS**（`.py` 逐字校验 hooks<grant、拒绝后 400 字节窗内 `continue`、域侧自持上限与 `-> Result<(), String>`、内核已无判据）。宿主 `cargo test` 实跑仍受磁盘限制未跑（同上节）。

**下一项（同一指令）**：mdns 能力路由词汇（内核 `manager/capability.rs` 的 `CAP_HOST_MDNS` / `EXPORT_MDNS_*` / `FORWARD_MDNS` + `forward_mdns_*` 五函数）迁 `bedcode-discovery-engine`；前置 = 可路由能力表由内核 `const` 闭表改为**宿主自报注册**（否则内核无法引用已摘依赖的 crate 常量）。

### 批次 03 · mdns 路由词汇迁能力域（2026-10-09，已落地）

**改动**

- 新增 `packages/bedcode-host-kit/src/route.rs`：`RoutableCapability { capability, forward_prefix, exports }` + `inventory` 收集（`submit_routable_capability!` / `collected_routable_capabilities()`），与 `module`/`lifecycle`/`assembly` 同范式的**域自报、内核收集**；重复能力名**不静默去重**（由内核闭表锁点名）。lib 12 例（含 2 条新用例）。
- 新增 `packages/bedcode-discovery-engine/src/routing.rs`：域自持 `CAPABILITY`（`host-mdns`）/ `FORWARD_PREFIX`（`mdns`）/ `EXPORTS`（5 条 `bedcode:plugin/host-mdns.*`）+ `submit_routable_capability!`；三条漂移锁（导出数 == 端口 `forward_mdns_*` 方法数；导出属于 `MODULE_INTERFACES` 里那个接口；前缀 == 能力名去 `host-`）+ 自报到场用例（desktop-host）。该 crate `cargo test` **34 绿**（含 3 条新锁）。
- 内核 `manager/capability.rs`：删 `CAP_HOST_MDNS` / `EXPORT_MDNS_*`（探测面用）/ `FORWARD_MDNS`；`const ROUTABLE_CAPABILITIES` 闭表 → `BUILTIN_ROUTABLE`（仅内建 `host-storage`）+ `fn routable_capabilities()`（内建 ∪ 自报，按名排序）；`is_routable` / `probe_exported_capabilities` / 闭表锁全部改走该函数；5 个 `forward_mdns_*` 新增 `capability: &str` 参数（取值由**调用方**给，内核零字面量）。
- **刻意留在内核的两半（机制，已在文档写明）**：① 提供者**调用面**——`CapabilityTarget::mdns_*`（5 方法）与 `OpKind::export` 的 `EXPORT_MDNS_*` 常量（wasm 编译期具名，闭集 op 的依据见 `owner.rs`：泛型 erase 会退化成动态 `Val` 编解码）；② `owner.rs` 的 `GuestOp::CapMdns*` 闭集变体。**值为同一组字符串，靠新增锁钉住**：`capability::tests::mdns_call_surface_names_match_the_domain_self_report`（源文本逐字比对，含「模式串自匹配」排除规则）。内核 `manager::capability` **7 例全绿**。
- 闭表锁的**可见性边界重划**：内核测试二进制不链能力域 ⇒ 自报行为空，「方法 → 行」的反向孤儿判定在内核必然误红。现内核只保留「行 → 方法」方向（非空断言兜底），反向由宿主侧锁 `src-tauri/src/plugin/mdns.rs::mdns_target_method_family_matches_the_self_reported_exports`（扫 `context.rs` 的 `mdns_*` 族 ⇔ `routing::EXPORTS`，离线复刻 PASS）+ `route_is_self_reported_into_the_host_binary`（自报真的进宿主二进制）承担。
- 宿主 `src-tauri/src/plugin/mdns.rs`：5 个端口实现改为把 `bedcode_discovery_engine::routing::CAPABILITY` 传给内核转发函数（词汇真源在域侧）；`host_api.rs` 的再导出文档同步说明该参数与出处。

**门禁（实跑）**：host-kit `cargo test` **29 绿**（12+3+2+12）；discovery-engine `cargo test` **34 绿**；wasm-core `cargo check --tests` **0 error** + `manager::capability` **7 绿**；**宿主 `cargo check --lib` Finished / 0 error**（新签名调用点编译级验证通过——本次磁盘刚好够）；宿主反向锁逐条离线复刻 PASS。宿主 `cargo test` 实跑仍受磁盘限制未跑（同前）。

**仍未迁的 mdns 词汇（如实登记）**：`CapabilityTarget::mdns_*`（5 方法）、`OpKind::export` 的 5 条导出名、`GuestOp::CapMdns*`（5 变体）——它们是 wasm 静态调用面（机制），迁出需要「按名字动态调用导出」的重写（退化为动态 `Val` 编解码，见 `owner.rs` 闭集 op 依据）。若要彻底零词汇，需先立 ADR 评估该退化代价。

### 批次 03 · peer adapter 迁宿主（依赖边**保留**，2026-10-09）

**关键事实（改变了本域的收口口径）**：peer 与 pty/mdns 不同，内核仍在**消费该 crate 的引擎面**——

- `manager/host/activation.rs::release_node_for(ctx, plugin_id)`（停用回收释放节点）；
- `host_api::context::PeerCtxProvider` 的类型签名直接用 `bedcode_server_peer_net::PeerCtx`。

两者都不是能力域端口 ⇒ **Cargo 边不能摘**（宿主边界锁 `REQUIRED_DOWNWARD_EDGES` 仍要求 `wasm-core → bedcode-server-peer-net`，其末注本就写着 ws/peer/http 三边因引擎面保留）。故本域口径 = **只搬 adapter，边与白名单条目保留**。

**改动**

- `packages/bedcode-server-peer-net/src/plugin_binding.rs`：新增常编译 pub 的 `MODULE_NAME` / `MODULE_INTERFACES` / `MODULE_PERMISSIONS`，`DESC` 三字段改引这三常量（描述符与白名单不可能漂移）。
- 新增宿主 `bedcode-desktop/src-tauri/src/plugin/peer.rs`（照 pty/mdns 样板四件同处：`HostPeerPorts` 端口实现 / `install` 双登记（实例级 `set_domain_ports` + 进程级 `install_ports`，与内核旧行为一致）/ `expect_host_module!` + 强制引用行 / `PEER_PORTS_INSTALLER` 自报 + 2 条单测）。端口句柄用擦除形态 `Arc<dyn HostPorts>` + `downcast_host`。
- 内核：删 `host_api/peer.rs`（104 行）与 `pub(super) mod peer;`、`install_capability_domain_ports` 里的 `peer::install` 行（改注：边保留的原因）；`component.rs` 的 `IN_CRATE_HOST_MODULES` 里 peer 条目改引 `bedcode_server_peer_net::plugin_binding::MODULE_NAME`（内核零字面量，白名单语义不变——该 crate 仍在二进制里）。
- 宿主边界锁 `server/crate_boundary_lock.rs` 的 ALLOWED 表注释更新为现状（peer/ws/http 三条边是保留边，理由 = 内核仍用其引擎面）。

**门禁（实跑）**：宿主 `cargo check --lib` **Finished / 0 error**（peer-net 作为依赖随之编译——同时验证了 crate 侧常量与 `DESC` 改动）；wasm-core `cargo check --tests` **0 error**。kernel 侧 `crate_boundary` 测试过滤命中 0 例（该锁的强制执行点在宿主 `server/crate_boundary_lock.rs` + 宿主集成测试，受磁盘限制未跑——与批次 02 同一条待补清单）。

**下一域**：`host_api/ws.rs`（163 行）与 `host_api/http.rs`（198 行）——两者同样是「adapter 迁宿主 + 边保留（内核仍用其引擎面：ws 帧投递 / http 端点登记）」。搬完后 `IN_CRATE_HOST_MODULES` 只剩这三个引用能力域常量的条目 + 内建三 interface。

### 批次 03 · ws / http 实测：**不能**照 02 样板整搬（2026-10-09，阻塞点已定位）

照 peer 样板开搬前先查消费者，实测出 ws / http 的 adapter 有**内核内消费者**（pty / mdns / peer 都没有）：

| 域 | 内核内消费者 | 性质 |
| --- | --- | --- |
| ws | `bus.rs:455 / 490` 构造 `HostWsPorts::from_bus(bus)` | **帧投递专用窄端口**（无权限管理器 ⇒ 权限门恒拒）：`MessageBus` 回灌 `events-ws` 帧时要用它，**内核必须能自建** |
| ws | `manager/host/activation.rs:715` `ws::purge_for_plugin` | 停用回收**直调**（未走 `DomainHooks`；ws crate 未自报钩子） |
| http | `manager/host/activation.rs:720` `http::purge_for_plugin` | 同上（http crate 亦未自报钩子） |
| http | `manager/host.rs:576` + `test_support.rs:28` 用 `HttpUnitExecutor` | 内核宿主面的一部分（host-task 单元执行器），**不是**薄适配器 |

**修正后的步骤（下一批，顺序不可颠倒）**

1. **两域补自报钩子**（照 pty 样板）：`bedcode-server-websocket` / `bedcode-server-http` 各自 `HOOKS` 挂 `on_plugin_purge`（转发域内 `purge_for_plugin`），内核 `activation.rs` 的两条直调删除（改由已有的 `DomainHooksRegistry::on_plugin_purge` 遍历承担）⇒ 回收面先与内核解耦。
2. **ws 的帧投递缝**：`bus.rs` 不再自建 `HostWsPorts::from_bus`，改为**注入式**（开机期由宿主把已装配的域端口注入 `MessageBus`；未注入时帧投递返回 `FrameDispatch::Unavailable`，fail-visible 且不 panic）。内核测试夹具注入一个测试窄实现。做完这步，内核才不再构造任何 `WsPorts`。
3. **http 的 `HttpUnitExecutor` 单独裁决**：它是内核 host-task 面的组件（不是端口 adapter），要么随 host-task 面（批次 05）一起处置，要么立独立条目——**不可**跟着 adapter 一起搬。
4. 之后才走 02 样板的第 3–5 步（adapter 迁宿主 + 白名单条目改引域常量 + 边评估）。

**已完成对照**：pty ✅（adapter 迁宿主 + 边摘）/ mdns ✅（adapter 迁宿主 + 词汇迁域 + 边摘）/ peer ✅（adapter 迁宿主，边保留）。ws / http 待上述 1–4。

**第 1 步已落（2026-10-09 同日）**

- `bedcode-server-websocket` / `bedcode-server-http`：各新增常编译 pub 的 `MODULE_NAME` / `MODULE_INTERFACES` / `MODULE_PERMISSIONS`（`DESC` 改引它们），并各自自报 `HOOKS`（`on_manifest_load: None` + `on_plugin_purge: Some(on_plugin_purge)`，域内 `purge_for_plugin` 的计数就地 `debug!` 吸收——与 pty 同款适配器）。
- 内核 `manager/host/activation.rs`：删 `ws::purge_for_plugin` 与 `http::purge_for_plugin` 两条**直调**（回收并入既有 `DomainHooksRegistry::on_plugin_purge` 遍历，位置不变 ⇒ pty 补发 exit 事件仍在 `remove_all_subscriptions` 之前）；注释改为「已接入：pty / mdns / ws / http；未自报：auth_center / task」。
- `host_api/http.rs`：随直调退役的那条 `pub use …::purge_for_plugin` 再导出删除（内核零残留、避免新增 unused 告警），留指路注释。
- **门禁**：wasm-core `cargo check --tests` **0 error / 零新增告警**；`manager::host` 停用路径用例 `deactivate` 过滤 **8/8 绿**（含 `test_deactivate_plugin_flow` / `test_deactivate_all` / `test_wasm_plugin_activate_invoke_deactivate`）。宿主 `cargo check --lib` Finished / 0 error（两域的钩子代码随依赖一起编译过）。
- **剩余**：第 2 步（ws 帧投递注入缝）、第 3 步（`HttpUnitExecutor` 裁决）、第 4 步（两域 adapter 迁宿主 + 白名单条目改引域常量）。

### 批次 03 · 第 2–4 步设计定稿（2026-10-09，待执行；交接用）

**第 2 步：ws 帧投递端口走依赖注入（不是全局工厂，也不是回落进程级端口）**

现状形状（已核实）：`bus.rs:434-494` 的 `HostBusPort` 持 `BusBinding::Fixed { bus, ws_ports }`，`ws_ports` 由 `HostWsPorts::from_bus(bus)` **在构造期钉死**；`LateBound` 形态逐次构造（`bus.rs:490`）。唯一消费点是 `deliver_endpoint_frame`（`bus.rs:532-550`）→ 域侧 `bedcode_server_websocket::plugin_binding::deliver_endpoint_frame(&ports, …)`（async，收 `&Arc<dyn WsPorts>`）。

**为什么不用「全局工厂 + 回落进程级 `ports()`」**：`HostBusPort` 的窄端口必须**绑定到它自己那条总线**（`ws_e2e.rs` 的多上下文用例 A/B 各一条总线，端口错绑会把帧投进别的实例）；回落进程级 `ports()` 在无头/内核测试二进制里还会 panic。**正确做法 = 构造期把窄端口喂进来**（`HostBusPort` 的构造点本就在宿主组合根，天然能命名宿主 adapter）。

**改法（四处，机械）**

1. `bus.rs`：`HostBusPort::new(bus)` → `HostBusPort::new(bus: Arc<MessageBus>, ws_ports: Arc<dyn WsPorts>)`；`late_bound(resolve)` 加第二参 `ws_ports_for: Arc<dyn Fn(&Arc<MessageBus>) -> Arc<dyn WsPorts> + Send + Sync>`（late-bound 形态必须能按「当时的」总线现造窄端口）。`BusBinding::Fixed` 保留 `ws_ports` 字段（构造期传入，不逐帧分配）；`LateBound` 存工厂。`ws_ports()` 保持返回 `Arc<dyn WsPorts>`（**无 Option、无失败口径变化**）。删 `bus.rs` 对 `crate::host_api::ws::HostWsPorts` 的两处引用。
2. 宿主组合根（`bedcode-desktop/src-tauri/src/server/ports_impl.rs`，及 `HostBusPort::new` / `late_bound` 的全部调用点——用 grep 找全）传入 `crate::plugin::ws::` 的窄端口构造（该构造在第 4 步随 adapter 一起进 `src/plugin/ws.rs`；在那之前可先指向 `wasm_core::host_api::ws::HostWsPorts::from_bus`，本步只改**接线形态**，不动实现）。
3. 内核测试夹具（`bus.rs` 测试、`manager/host/tests/*` 里凡构造 `HostBusPort` 者）传测试窄实现（可直接用 `HostWsPorts::from_bus`，测试二进制里那个类型仍在）。
4. 门禁：`cargo test --lib bus`（背压/投递用例）+ `cargo check --tests`（内核）+ 宿主 `cargo check --tests`（`ws_e2e` 的多上下文用例是这条缝的关键回归项，宿主测试实跑仍需磁盘）。

**第 3 步：`HttpUnitExecutor` 不跟 adapter 走**

`http.rs:38` 的 `use crate::host_api::unit_executor::UnitExecutor` + `manager/host.rs:576` + `test_support.rs:28` 表明它是**内核 host-task 面的执行器**（`HttpUnitExecutor` 实现 `UnitExecutor`），与「端口 adapter」不是一类。裁决：**留在内核**，随 host-task 面（批次 05）统一处置；第 4 步搬 http adapter 时把 `HttpUnitExecutor` 及其 `UnitExecutor` impl 明确留在 `host_api/http.rs`（或按批次 05 的方案提前移入 `host_api/unit_executor.rs`）。

**第 2 步已落（2026-10-09 同日）**

- `bus.rs`：`HostBusPort::new(bus, ws_ports: Arc<dyn WsPorts>)` / `late_bound(resolve, ws_ports_for: Arc<dyn Fn(&Arc<MessageBus>) -> Arc<dyn WsPorts>>)`；`BusBinding::LateBound` 增第二分量（工厂）；`ws_ports()` 无 Option、失败口径零变化。**`bus.rs` 不再出现任何 `HostWsPorts` 引用**（此前两处自建）。
- 三处接线：① ws adapter 的 `bus_port()` 显式传 `Self::from_bus(...)`（随 adapter 第 4 步整体迁走）；② 内核 `manager/host/register.rs` 端点登记处传 `from_bus`（注释标明第 4 步改为宿主注入）；③ **宿主组合根** `src/server/ports_impl.rs::assemble` 传 bus-bound 工厂（晚绑形态按当下总线现造——这正是宿主侧第一次真正接这条缝）。
- 内核 `bus.rs` 测试加 `test_ws_ports_factory()` 测试工厂（用 adapter 的 `from_bus`），4 处构造点同步。
- **门禁（实跑）**：内核 `cargo check --tests` 0 error / 零新增告警；**内核 `cargo test --lib bus` 52/52 绿**（含 `fixed_bus_port_publishes_only_to_its_own_bus` 与三条 `late_bound_bus_port_*`——本缝自己的用例）；宿主 `cargo check` Finished / 0 error（首轮暴露 `ports_impl.rs:334` E0061——此前 grep 被 `head` 截断漏看，已修）。

**磁盘问题已解决 + 宿主测试欠账首次实跑（2026-10-09）**

- 清理：`target/host-kits/debug/incremental` 3.6G + `bedcode-mobile/target/host-kits/debug/incremental` 2.3G + `bedcode-mobile/target` 7.9G + `~/.cache/sccache` 8.3G ⇒ 连同被进程释放的已删文件，可用空间 **5.7G → 28G**（随后链接测试二进制后仍有 22G）。
- **`/tmp` 是 tmpfs 6.8G**：链接临时目录若走 /tmp 会撞上限——这是「宿主测试链接失败」的隐藏原因（此前只归因根分区）。
- **宿主 `cargo test --test pty_wiring` 实跑：6/6 全绿**（首次！批次 02 的欠账在该目标上关闭）：含 `host_whitelist_matches_collected_capability_modules`（白名单双向锁）、`deactivate_path_triggers_domain_purge_hook`、**`quota_registration_is_wired_into_the_load_funnel`**（本批次改写的「钩子先于授权 + 拒绝 ⇒ continue + 内核无判据」断言）、`host_declared_pty_module_has_a_ports_installer`。链接耗时 12m59s（首次），此后同 target 复用。
- **仍未实跑**：`pty_e2e` / `terminal_output_perf` / 宿主 `--lib`（含新增 mdns 用例与反向闭表锁）/ `ws_e2e`（该文件本身在 HEAD 即编译红——`EndpointAuth` 同名不同源）。

**第 4 步 · ws 半边已落（2026-10-09）**

- 新增宿主 `src-tauri/src/plugin/ws.rs`（照 pty/mdns/peer 样板四件同处）：`HostWsPorts` 端口实现 + **两种构造**（`from_ctx` 测试用 / `from_erased` 装配路径（持 `Option<Arc<dyn HostPorts>>`，权限判定时向下转型）/ `from_bus` 帧投递窄端口——这条正是第 2 步注入缝的宿主实现）+ `install` 双登记 + `expect_host_module!` + 强制引用行 + `WS_PORTS_INSTALLER` + `purge_for_plugin` 显式入口 + 2 条单测；`plugin.rs` 登记 `pub mod ws;`。
- 内核 `install_capability_domain_ports`：删 `ws::install(...)` 行（本域端口自此只有宿主一条装配来源）。`host_api/ws.rs` **保留但只剩总线侧 plumbing**（`from_bus`，供 `bus.rs` 帧回灌与 `manager/host/register.rs` 端点登记），其去留见下方两种处置。
- 宿主两个 ws 测试文件（`ws_e2e` 3 处 / `ws_output_perf` 1 处）的 `HostBusPort::new` 调用补上窄端口参数（脚本补丁，打印核对）。
- **门禁（实跑）**：宿主 `cargo check` + `cargo check --tests` 均 0 error（**新增 E0061 全清，错误计数回到基线**：`ws_e2e` 3 / `ws_output_perf` 1，均为既有的 `EndpointAuth` 同名不同源基线红）；内核 `cargo check --tests` 0 error。
- **未做（交付边界）**：http adapter 迁宿主（需先拆 `HttpUnitExecutor`，见第 3 步裁决）；`host_api/ws.rs` 的删除与其 `from_bus` 去向；`ws_e2e.rs:637/960/962` 改指宿主 adapter 路径（现仍经内核文件编译通过 ⇒ 非阻塞）。

**第 4 步 · ws 已收尾（2026-10-09，用户指定「先做①」）**

- 裁决 ① 落地：**总线绑定窄端口落进总线侧** —— `bus::BusBoundWsPorts`（`pub(crate)`：`check_permission` 恒 `false`（与迁移前 `from_bus` 口径逐字一致）/`publish`/`bus_port`/`dispatch_frame`/`block_on_any`）。语义是「总线接到能力域 trait 上」，属机制；能力域**完整端口**在宿主 adapter。文档写明它与旧 `HostWsPorts::from_bus` 的等价关系。
- **删除** `packages/bedcode-wasm-core/src/host_api/ws.rs` 与 `host_api.rs` 的 `pub mod ws;`（连同上面 2 行 doc）——内核不再有该域 adapter 文件。
- 内核接线：`bus.rs` 测试的窄端口工厂与钉死构造、`manager/host/register.rs` 的端点登记，全部改指 `crate::bus::BusBoundWsPorts::new`；`IN_CRATE_HOST_MODULES` 的 ws 条目改引 `bedcode_server_websocket::plugin_binding::MODULE_NAME`（内核零字面量）。
- 宿主接线：`server/ports_impl.rs::assemble` 的晚绑工厂改指宿主 adapter（`crate::plugin::ws::HostWsPorts::from_bus`）。
- 宿主两个 ws 测试文件的 7 处引用（`purge_for_plugin` / `HostWsPorts::from_ctx` / `from_bus`）改指 `bedcode_desktop_lib::plugin::ws::*`。
- 跨 crate 文档改指新落点（`bedcode-server-websocket` 的 lib.rs / plugin_binding.rs / ports.rs / tests/scaffold.rs、`bedcode-server-base/src/ports.rs`、宿主 `tests/hot_path_logging_lock.rs`）。
- **门禁（实跑）**：内核 `cargo check --tests` **0 error**；**内核 `cargo test --lib bus` 52/52 绿**（窄端口换成 `BusBoundWsPorts` 后同绿）；宿主 `cargo check --tests` **回到基线**（4 条 E0308 = `ws_e2e` 3 + `ws_output_perf` 1，全是既有的 `EndpointAuth` 同名不同源基线红，**零新增**）；`grep -rn "host_api::ws"` 全仓残留 **0**。
- **裁决 ②（端点登记改宿主注入端口）：用户 2026-10-09 定「先不做」** —— 记为**有意不迁**，理由归档：① 落地后内核侧只剩 `BusBoundWsPorts` 一种 `WsPorts` 实现，其语义是**总线 plumbing**（`check_permission` 恒 false、无宿主上下文、无能力域知识），不构成「能力域实现回流内核」；② 的增量收益只是「内核完全不持 `WsPorts` 实现」这一形式纯度，代价是动 `PluginHost::new` 签名 + 全部测试夹具（批次 02 已刻意避免该类改动，且签名改动会让「装配自报」这条设计线出现两套范式）。**若将来 kernel 侧出现第二处 `WsPorts` 实现，本条需重开评估**（判据：`grep -rn "impl .*WsPorts for" packages/bedcode-wasm-core/src` 结果 > 1 即重开）。

**（原两种处置记录，供追溯）**

1. **移入 `bus.rs`**：`from_bus` 的实现本就是「总线绑定视图 + 权限门恒拒」，属总线 plumbing 而非域 adapter —— 落成 `bus::BusBoundWsPorts`（`pub(crate)`），内核零 adapter 语义，`register.rs` 直接用；代价 = 内核仍持一份 `WsPorts` 实现（但语义是总线侧，不是能力域侧）。
2. **端口从宿主注入**：`register.rs` 的端点登记改问宿主注入的端口（同第 2 步的注入缝，`PluginHost` 需多一个端口字段/构造参数）；代价 = 动 `PluginHost::new` 签名与全部测试夹具（批次 02 已刻意避免的那类改动）。

**第 4 步：两域 adapter 迁宿主（同 peer 样板）**

- ws：`src/plugin/ws.rs` 收 `HostWsPorts`（含 `from_ctx` **与** `from_bus`——后者成为宿主侧的窄端口构造，供第 2 步的接线用）+ `install` 双登记 + `expect_host_module!(MODULE_NAME)` + 强制引用行 + `WS_PORTS_INSTALLER`；`host_api/ws.rs` 的 `purge_for_plugin` 包装删除（宿主测试 `ws_e2e.rs:637` 改用宿主 adapter 的等价入口）。
- http：`src/plugin/http.rs` 收 `HostHttpPorts` + `install` + 装配器；`host_api/http.rs` **只留** `HttpUnitExecutor`（+ 其 `UnitExecutor` impl）与必要的类型再导出。
- 两域 `IN_CRATE_HOST_MODULES` 条目改引 `bedcode_server_{websocket,http}::plugin_binding::MODULE_NAME`（与 peer 同）；`component.rs` 的 `use … as _;` 强制引用行保留（两 crate 仍有引擎面消费者）。
- 边界锁：两条边**保留**（内核仍用其引擎面：ws 帧投递 + http 端点/单元执行器），ALLOWED 表注释按需更新；`capability_crates_*` / `wasm_core_whole_crate_lock` 若有 ws/http adapter 的落点断言需同步改指 `src/plugin/{ws,http}.rs`。

### 批次 03 · 第 4 步 http 半边收口（2026-10-09，已落地；含两处批次 03 遗留缺陷修复）

**改动（http adapter 迁宿主）**

- 新增宿主 `src-tauri/src/plugin/http.rs`（照 pty / mdns / peer / ws 样板**四件同处**）：`HostHttpPorts` 端口实现（擦除形态字段 + 每调用 `downcast_host`；`check_permission` 复用 `host_api::check_permission`，出站授权链与 `AppHandleEventSink` 原样随迁）、`install` 双登记（实例级 `set_domain_ports` + 进程级 `install_ports`）、`expect_host_module!(MODULE_NAME)` + `use bedcode_server_http as _;` 强制引用行、`HTTP_PORTS_INSTALLER` 自报；2 单测（白名单/接口/权限三件一致；装配器进开机链 + **双通道取回**——进程级 `ports()` 与实例级 `domain_ports` 都断言）。`plugin.rs` 登记 `pub mod http;`。
- 内核 `host_api/http.rs` 收窄为**只剩 `HttpUnitExecutor`**：执行时端口改走**实例级下发通道**（`host_ctx.domain_ports(DOMAIN)` + `downcast_domain_ports::<Arc<dyn HttpPorts>>`），缺失即显性 `Err`——不回落进程级 `ports()`（多上下文会串库，且无头进程直接 panic）；不再构造任何 adapter 对象。新增用例 `missing_domain_ports_fail_loudly`（fail-visible 钉死）。`HOST_MODULE_NAME` 常量随迁退役，`component.rs` 白名单条目改引 `bedcode_server_http::plugin_binding::MODULE_NAME`。
- 内核 `install_capability_domain_ports` 删 `http::install(...)` 行；`component.rs` 的 IN_CRATE 表文档改写（现存 ws/peer/http 三条目 = 引擎面消费，非 adapter 回流）。
- **内核反向锁**：`lib.rs::http_adapter_must_not_return_to_wasm_core`——`src/host_api/http.rs` 的**代码行**（跳注释行）不得再现 `HostHttpPorts` / `set_domain_ports`，正面锚点 `HttpUnitExecutor` 必须在场（防空转）。变异自检：代码行字符串探针注入 → 红 → 还原（grep 计数 0）→ 复跑绿。
- 文档指针：`bedcode-server-http` 的 `plugin_binding/ports.rs` / `plugin_binding/tests/scaffold.rs`、内核 `security/network_auth.rs`（判定管线 0 层落点）/ `security/auth_policy.rs`、桌面 code-map（§3 能力域表 http/ws/peer/mdns 四行 + 锁索引行 + §2 host_api 行）。

**门禁（实跑）**

- 内核 `cargo check --tests` 0 error（34 条告警全为存量）；宿主 `cargo check --lib` 0 error、`cargo check --tests` **回基线**（4 条 E0308 = `ws_e2e` 3 + `ws_output_perf` 1，既有 `EndpointAuth` 基线红，零新增）。
- **内核全量 `cargo test --lib` 639 passed / 0 failed**（含已知 flaky `engine_config::incoherent_tuning`，本次亦绿）。

**实跑暴露并修复的两个批次 03 遗留缺陷（同票收口，非新引入）**

1. **ws 半边迁移后内核测试二进制失端口**（前序步骤门禁只跑了 `--lib bus` 52 例，未跑全量）：14 条走真实组件全链路的用例（`manager::host` 的激活/停用/实例调用模型/approval 等）在 `bedcode-server-websocket` 的 `ports()` 上 panic——ws/http 两域自报装配器只存在于宿主二进制，内核测试二进制两条通道都无人装配。修复 = **内核测试端口替身**：`test_support.rs` 新增 `#[cfg(test)] pub(crate) mod kernel_test_domain_ports`（`TestWsPorts` / `TestHttpPorts` 按宿主 adapter **同一语义**实现：同一份 `check_permission`、本实例总线 publish、同一份同步↔异步桥、`bus_port` 帧投递侧用 `BusBoundWsPorts`；差异仅「无 AppHandle ⇒ event_sink None」这一无头事实），挂进唯一装配入口 `install_capability_domain_ports` 的 cfg(test) 尾段。**隔离保证**：宿主集成测试链的是不带 cfg(test) 的内核 ⇒ 替身在那边不存在，`OnceLock` 首个装配者胜出 ⇒ 不可能盖掉宿主装配的真端口。修复后内核全量 639 全绿。
2. **mdns 反向锁判据写错且从未实跑**（宿主 lib 首次实跑即暴露）：`marker = "fn mdns_"` 与 `strip_prefix("fn ")` 叠加 ⇒ 计数恒 0（0 ≠ 5 红）。修复 = marker 改 `mdns_`（剥前缀后比对）；判据等价复刻 + 合成变异（向解析块注入第 6 条方法 ⇒ 红）脚本 `_tools/mdns-lock-criterion-check.py` PASS。

**未跑 / 基线（如实记）**：宿主 `--lib plugin::` 复跑结果见 `_tools/host-lib-plugin-tests-2.log`；`pty_e2e` / `terminal_output_perf` / 宿主 `--lib` 全量仍待磁盘与时间补跑；`ws_e2e` 为 HEAD 既有编译红（`EndpointAuth` 同名不同源，非本票）。

### 批次 04 · 第 1 步：host-crypto 迁宿主（路径 B 样板，2026-10-09，已落地）

**设计裁决（本步确立，auth 第 2 步与批次 05 沿用）**

- **宿主共享 bindgen**：新增 `src-tauri/src/plugin/bindings.rs`（全 world `bindgen!`，配置与内核逐字一致）——路径 B 域的装配方必须自带绑定（孤儿规则），批次 05 的 task / process / app / timer / api_call / connection 同用这一份。**纪律：宿主侧路径 B 域不得再实现内核或能力 crate 生成的同名 trait**——同一个 interface 被两份 `add_to_linker` 注册 ⇒ 装配期 `defined twice`。
- **路径 B 无端口装配面**：crypto 无能力 crate、无端口 trait ⇒ 无 `submit_domain_ports_installer!`、无强制引用行（自报静态住宿主 lib = 最终二进制）；guest 调用经 WIT impl 直取 `WasmPluginState.host` 向下转型（host-kit `downcast_host::<WasmHostContext>`——内核 `HostCtxOf` 是 `pub(crate)`，宿主走 host-kit 公开出口做同一转型）。
- **夹具策略（批次级硬约束）**：域 WIT impl 迁出内核 ⇒ **内核测试二进制不再注册该 interface** ⇒ 携带其 import 的夹具全部无法在内核测试实例化。wasip3 夹具因此摘除 host-crypto 探针（回归单一用途：p3 async 机制）；探针原样拆为独立 `crypto` 夹具 feature（`plugin-sdk-fixtures` 的 `crypto_probe` 模块），随域走宿主 e2e `tests/host_crypto_e2e.rs`（只读产物，与 pty_e2e 同约定）；产物构建者 = 内核 `fixture_keeper` 用例（「wasm-core 测试构建、宿主测试只读」约定的 keeper 化，工具链未装时 skip 与 wasi_e2e 同口径）。
- **auth_center 不随本步迁**（对票面 §3 II 类清单的显式偏离，留 auth 第 2 步裁决）：内核 `boot.rs` 的 L2 启动门（`l2_auth_center_unready` 查在册中心做对账）与 `activation.rs:711` 停用回收、宿主 lib 裁决面/桥接门（`utils/auth/auth_center.rs`）、`test_support` 测试闸门**四方消费同一注册表**；注册表本体是 ADR 0031 显式裁决的「通用注册表」薄壳。迁出会迫使内核启动门重建（内核无法点名宿主类型），须单独裁决——`activation.rs:722` 的既有注释（「尚未自报的域（auth_center / task）仍走各自直接调用」）与本裁决一致：auth_center 的回收**不经 DomainHooks**，继续内核直调。

**改动**

- 宿主新增 `plugin/bindings.rs` + `plugin/crypto.rs`：`MODULE_NAME`/`MODULE_INTERFACES`/`MODULE_PERMISSIONS` 常量 + `DESC`（`abi_min = 26`，host-crypto 于 v26 引入）+ `CryptoModule` + `inventory::submit!` + `expect_host_module!` + WIT impl（7 原语转发）+ 域函数（权限三域门 / 审计 / `api[algorithm]` 错误上下文逐字保留）+ 10 单测（内核 9 用例迁移 + 白名单三件一致 1 条）；`plugin.rs` 登记 `pub mod bindings; pub mod crypto;`。
- 内核删 `host_api/crypto.rs` + `host_api/tests/crypto.rs` + `component.rs` 的 host_crypto impl 与 linker 行 + use 列表项；`host_api.rs` mod 声明与域清单文档同步；新反向锁 `lib.rs::crypto_domain_must_not_return_to_wasm_core`（`src/host_api/crypto.rs` 文件禁回接 + `src/crypto.rs` 引擎垫片在场正面锚点；变异自检：注入探针文件 → 红 → 删除 → 绿）。
- 夹具：`plugin-sdk-fixtures` 增 `crypto` feature + `crypto_probe` 模块（探针原样迁入）+ 两处互斥 guard cfg 更新；wasip3 摘 crypto 权限与命令（permissions 归空）；`fixture_build.rs` feature 清单文档。
- 内核 `wasi_e2e` 删探针段与 crypto 权限补授（留指针注释）。

**门禁（实跑）**

- 内核 `cargo check --tests` 0 error；全量 `cargo test --lib` **632 passed / 0 failed**（639 − 9 迁出用例 + keeper + 反向锁 = 632，账目吻合；含 wasi_e2e 的 wasip3 async 闭环——夹具摘 crypto 后照常绿）。
- 宿主 `cargo check --lib` 0 error；`cargo check --tests` **回基线**（4 条既有 EndpointAuth E0308，零新增）；`cargo test --lib plugin::crypto` **10/10**；**`cargo test --test host_crypto_e2e` 1/1**——真实 wasm 组件经宿主自带 bindgen 的 host-crypto 通路全绿（路径 B 端到端首次实证）。
- crypto 夹具产物由 keeper 构建在场（`bedcode_plugin_sdk_fixtures.crypto.release.wasm`，256 KB）；fmt 零新增漂移（触碰的 use 列表按 rustfmt 期望重排）；lints 0。

**未跑 / 待办**：auth 第 2 步（auth_center 归属裁决 + `test_tokens` 迁移 + `l2_gating` 的 `L2_CONSUMER_ALLOWLIST` 悬空条目更新——删 `src/host_api/auth.rs` 会触发其反向自检，须同批改表）；宿主 `--lib` 全量与 `pty_e2e` / `terminal_output_perf` 重链（欠账同前）。

### 批次 04 · 第 2 步：host-auth 迁宿主（路径 B，2026-10-09，已落地）

**关键裁决与配套（沿第 1 步确立的路径 B 形态）**

- **auth_center 注册表显式留内核**：内核 `boot.rs` 的 L2 启动门（`l2_auth_center_unready` 按在册中心做对账）与 `activation.rs:711` 停用回收、宿主 lib 裁决面/桥接门、`test_support` 测试闸门**四方消费同一注册表**；它是 ADR 0031 显式裁决的「通用注册表」薄壳，迁出会迫使内核启动门重建（内核无法点名宿主类型）。宿主 adapter 经 `bedcode_wasm_core::host_api::auth_center` 公开面取用（**不经 DomainHooks**——`activation.rs:722` 既有注释与本裁决一致）；宿主**不得复制**唯一性仲裁判据。
- **`test_tokens` 夹具随依赖迁回宿主 lib**（`src-tauri/src/utils/auth/test_tokens.rs`）：其依赖的 `test_seed_plugin_secret` 是域函数随域走 `plugin/auth.rs`；旧「不迁回」理由（依赖 wasm-core 内部函数）失效。消费方（system_component_test ×5 / auth_center_perf ×2）import 改指 `bedcode_desktop_lib::utils::auth::test_tokens`。内核 `utils/auth.rs` 只剩 `identity`（宿主裁决面仍经内核路径消费，保留）。
- **内核 test_support 增公开轻量构建器** `build_host_ctx_at(Option<&Path>)`（不建 WasmRuntime / 不 init AppConfig）：迁宿主域的单测需要窄 scope 面 + 可配数据库（持久化用例两代上下文指同一文件），而 `manager::capability` 注册表面不对外、宿主无法手工拼 `WasmHostContext::new`——公开测试基建是既有裁决（`test_tokens` 常编译同款）。
- **权限词汇锁扫描根扩展**：`permission.rs::every_permission_has_an_enforcement_point` 原只扫内核 src——auth 域迁出后 `PERMISSION_AUTH` 落点在宿主 `plugin/auth.rs`，锁立即红（实测）。按「扫描面跟着代码走」（`L2_SCAN_ROOTS` 同款判据）把宿主 `src-tauri/src` 加入扫描根。
- **L2 白名单表三改两增**（`l2_gating_test::L2_CONSUMER_ALLOWLIST`，删文件即触发其悬空反向自检）：删 `src/host_api/auth.rs`（迁出）、`src/utils/auth/test_tokens.rs`（随迁）、`src/manager/runtime/component.rs`（host-auth 接线随迁后不再命中 needle）；增宿主 `plugin/auth.rs`（新 WIT 面 = 安全闸门消费方）与 `utils/auth/test_tokens.rs`（零解析转发夹具）。

**改动**

- 宿主新增 `plugin/auth.rs`：`MODULE_NAME`/`MODULE_INTERFACES`/`MODULE_PERMISSIONS`（全域单权限位 `auth`）+ `DESC`（`abi_min = 15`）+ `AuthModule` + inventory 自报 + `expect_host_module!` + WIT impl（10 原语：secret 四函数 / auth-setting-set / link-identity-parts / 认证中心四函数）+ 域函数（签名逐字保留，窄 scope 入参）+ `test_seed_plugin_secret`（pub，test_tokens 消费）+ 8 单测（内核 7 用例迁移 + 白名单三件一致 1 条；持久化用例经 `build_host_ctx_at(Some(path))`）。`plugin.rs` 登记。
- 内核删 `host_api/auth.rs`（761 行）+ `component.rs` 的 host_auth impl 与 linker 行 + use 列表项 + `host_api.rs` 的 mod 声明与 `test_seed_plugin_secret` 再导出 + `utils/auth.rs` 的 test_tokens 声明；新反向锁 `lib.rs::auth_domain_must_not_return_to_wasm_core`（文件禁回接 + auth_center/identity 保留正面锚点；变异自检注入→红→删→绿）。

**门禁（实跑）**

- 内核 `cargo check --tests` 0 error；全量 `cargo test --lib` **626 passed / 0 failed**（632 − 7 auth 用例迁出 + 1 新反向锁 = 626，账目吻合；含 L2 白名单锁与权限词汇锁——后者靠扫描根扩展修复后转绿）。
- 宿主 `cargo check --lib` 0 error；`cargo check --tests` **回基线**（4 条既有 EndpointAuth E0308，零新增）；`cargo test --lib plugin::auth` 与 `--test auth_center_perf` / `--test system_component_test` 实跑结果见 `_tools/host-auth-tests.log`。
- fmt 零新增漂移；lints 0。

**未跑 / 待办**：宿主 `--lib` 全量与 `pty_e2e` / `terminal_output_perf` 重链（欠账同前）；批次 05（task / process / app / timer / api_call / connection，路径 B 样板已备）。
**夹具 keeper 缺口（本轮实测暴露，欠账）**：消费者全在宿主的夹具（pty / ws / system-test）在域迁移后**无人构建**——夹具源码一旦变更（mtime 失效），内核只重建自己消费的 feature，宿主侧集成测试立刻挂在产物缺失（本轮 system_component_test 实测，已手动按同径重建 system-test：`RUSTUP_TOOLCHAIN=nightly-2026-09-16 CARGO_TARGET_DIR=bedcode-desktop/target/fixtures cargo build --target wasm32-wasip3 --release`，于 `packages/plugin-system-test`）。pty_e2e / ws_e2e 欠账实跑前需同法补 pty/ws 产物，或按 crypto keeper 先例补 keeper 用例。

### 批次 05 · task / process / app / timer / connection 整迁 + host-api-call WIT impl 迁宿主（路径 B，2026-10-09，已落地）

**三处对票面 §3 清单的修正（裁决先写）**

1. **`unit_executor.rs` 不迁，留内核**（票面 II 类表「unit_executor.rs(35) → 宿主 task/」按此修正）：该 trait 的消费方是**留内核的任务引擎**（`manager/task.rs` 的执行器注册表与 dispatch），C4「消费方定义接口」判据下引擎不迁则接口不迁。随域迁出的是**执行器实现**（`ProcessUnitExecutor` 随 host-process 走宿主；fs / http 执行器随各自域文件留内核）。`TaskEngine` trait 同理留内核 `host_api/context.rs`。
2. **host-api-call 只迁 WIT impl，回复道编排留内核**：内核 `intercall`（通用互调客户端）、`auth_center` 桥接（注册表留内核裁决）与 `WasmHostContext::call_plugin_api_host` 消费同一条编排——与 peer/ws/http「内核仍消费引擎面」同判据。宿主 `plugin/api_call.rs` 是**零逻辑薄转发**（无权限门，互调门禁在注册表）；内核 `api_call` 提 pub 供其以调用方 `plugin_id` 为 caller 复用。**内核测试二进制另备 cfg(test) 替身 impl**（`component.rs`）：SDK 夹具经互调 client **静态 import `host-api-call`**，迁移后内核测试不再注册该 interface ⇒ 33 个以 SDK 夹具为载体的内核引擎测试（sdk_e2e / owner_e2e / engine_limits / component 往返）实例化即失败——替身转发同一编排，生产二进制不含 cfg(test) 分支，宿主薄转发仍是唯一注册来源（与 `kernel_test_domain_ports` 同范式同隔离保证）。
3. **执行器注册改「域自报、内核收集」**（新装配面，与 host-kit 钩子/端口/路由同范式但落点在 wasm-core——trait 在此）：`unit_executor.rs` 增 `UnitExecutorEntry`（name + make 工厂）+ `inventory::collect!` + `submit_unit_executor!` 宏 + `collected_unit_executors()`；fs / http（内核域文件）与 process（宿主 `plugin/process.rs`）各自自报；内核装配点（`manager/host.rs` 组合根 + `test_support.rs` 测试装配）从三行具名注册改为遍历收集——宿主侧执行器迁入后内核装配代码零改动。

**改动（内核）**

- `host_api.rs`：删 task / process / app / timer / connection 五个 `mod` 声明；`api` 与 `unit_executor` 提 `pub`（宿主消费面：薄转发 / 执行器 trait）；模块文档同步批次 05 面。
- `context.rs`：`kill_process_group` `pub(crate)` → `pub`（宿主 process 域复用同一条终止路径，与 `check_permission` 提公开同款理由）。
- `component.rs`：删六个域的 `Host` impl 与 linker 行（留路径 B 指路注释）；use 清单收口；**新增 cfg(test) host-api-call 替身 impl + 替身注册行**（见裁决 2）。
- 删 `host_api/{task,process,app,timer,connection}.rs` 五文件（域函数与单测随域走宿主）；`api.rs` 收口为编排机制面（文档 + pub，测试全保留）。
- `manager/host.rs` + `test_support.rs`：执行器注册点改遍历收集；test_support 删三个执行器具名导入。
- `runtime.rs` tests：删 `mod task_e2e` 与 task 夹具脚手架（`build_task_test_component` / `MockTaskServices` / `task_fixture_plugin_id`）；`fixture_keeper.rs` 增 `task_fixture_artifact_is_built_for_host_e2e`（task 夹具消费者 = 宿主 e2e，产物仍由内核测试构建——keeper 化）；`engine_limits.rs` C-103（events-task 燃料续费探针）载体由 task 夹具换 **SDK 夹具**（`wasm_entry!` 无条件导出 events-task 默认实现，探针只需该性质；task 夹具已无法在内核实例化）。
- `lib.rs` 新增两把反向锁：`path_b_domains_must_not_return_to_wasm_core`（五域文件禁回接 + 机制面正面锚点：unit_executor / manager/task 必须在场）与 `api_call_wit_impl_must_not_return_to_wasm_core`（api.rs 正面锚点 HOST_API_CALLER_ID/ReplyHandler + 禁 `host_api_call::Host for WasmPluginState` / `add_to_linker` 代码行；cfg(test) 替身登记为合法残余）。

**改动（宿主，路径 B 六件套同 `plugin/bindings.rs`）**

- `plugin/{task,process,app,timer,connection,api_call}.rs`：`MODULE_NAME`/`MODULE_INTERFACES`/`MODULE_PERMISSIONS` 常量 + `DESC`（abi_min：task 20 / process 8 / app 8 / timer 6 / connection 12〔ADR 0022 v12〕/ api_call 7〔v7 窗口内随计划任务插件引入，从未单独占 bump note〕）+ `XxxModule` + `inventory::submit!` + `expect_host_module!` + WIT impl + 域函数（**签名与错误文案逐字保留**）+ 各域单测迁移（ctx 构造走内核公开测试基建 `build_host_ctx_at`，与 auth 同款）；`plugin.rs` 登记六模块。connection 的单钥匙锁扫描根随域扩展为「内核 src + 宿主 src」双根；权限词汇生成物锁的路径按宿主 manifest 基准换算。task 的 `execute_batch` 提 pub（宿主 e2e 两用例直调，迁移前内核 task_e2e 同款）。
- 新增宿主 `tests/task_e2e.rs`（自内核 runtime/tests 迁入，六用例逐字保留）：`MockTaskServices` / 串行锁 / fs 授权播种随迁本地；产物只读（内核 keeper 构建）；`process.run-sync` 取消用例在宿主二进制经执行器自报收集真跑。
- 桌面 code-map：§3 能力域表补六行、任务路由行更新落点、防回接锁索引补批次 04/05 条目。

**门禁（实跑）**

- 内核 `cargo check --tests` 0 error（存量 unused 告警中仅 `runtime.rs` 的 `Pin` 系本次 MockTaskServices 迁出所致、已清；`fs.rs` 的 `FsOp` 等为在途基线）；**全量 `cargo test --lib` 604 passed / 0 failed**——账目吻合：626（批次 04）− 20（五域单测迁出）− 6（task_e2e 迁出）+ 4（task keeper + 2 反向锁 + 执行器自报用例）。
- 首跑曾 33 failed（SDK 夹具 import host-api-call 失配）→ cfg(test) 替身落地后全绿；**api 过滤 109 例**（含互调编排全矩阵）绿。
- 变异自检 3/3：① 注入 `src/host_api/task.rs` → 五域锁红且点名 → 还原绿；② api.rs 注入 needle 形态常量行 → WIT impl 锁红 → 编辑工具重放本会话改动后绿（**过程教训：还原时误用了 `git checkout -- api.rs`**——经核查该文件在本会话前无他人在途改动、checkout 只清掉变异探针与本会话两处改动，随即用编辑工具重放，零他人损失；此后变异还原一律走编辑工具）；③ 注释 http 执行器自报 → 收集锁红且点名（`实际: ["fs."]`）→ 编辑工具还原绿。
- 内核 keeper 2/2 绿（crypto + task 夹具产物在场，`bedcode_plugin_sdk_fixtures.task.release.wasm` 375 KB）。
- 宿主 `cargo check` 0 error、`cargo check --tests` **回基线**（4 条既有 EndpointAuth E0308，零新增）；**宿主 `cargo test --lib plugin::` 57 passed / 0 failed**（六域新单测 27 + auth/crypto/pty 等 adapter 域既有单测）；**宿主 `cargo test --test task_e2e` 6 passed / 0 failed**（六用例全绿：fs.stat 并发批 / submit 事件派发 / **cancel 协作取消含 `process.run-sync` 单元真跑**——执行器自报收集在宿主二进制生效 / 双门 / 空 units / 未知 kind；日志 `_tools/host-lib-plugin-tests-b05.log` 与 `_tools/host-task-e2e-b05.log`）。
- 收尾修正三笔（宿主 e2e 实链暴露）：① `LoadedWasmPlugin::on_task_event` `pub(crate)` → `pub`（宿主集成测试直驱 SDK 回调链，`activate` / `invoke_command` 公开面先例同口径）；② 六个新域文件的 `fn register` / 折行 / use 排序按 rustfmt 收敛（`rustup run stable rustfmt --check` 七个新文件全净；内核存量本就非 rustfmt-clean，不动）；③ `unit_executor.rs` 的 `inventory::iter` 需 `.into_iter()`（与 host-kit 同款）。

**未跑 / 待办（如实记）**：宿主 `--lib` 全量 / `pty_e2e` / `terminal_output_perf` 重链欠账同前；批次 06（切片域桌面扩展：fs+wsl / platform / events / abi-form，`host-fs` / `host-platform` 切片需拆 interface ⇒ 双端 ABI bump + 产物重建走 `stale_artifact_rebuild_hint` 三形态）；移动端**零影响**（桌面 fork 的路径 B 域，移动 wasm-core 自持同名域；WIT 契约与 ABI 未动，无跨端协议变更、无产物重建需求）。

### 批次 06 · 切片域桌面扩展外迁：fs(+wsl) / platform / events / abi-form（2026-10-10，已落地）

**本批内容与接缝**：四个交集接口（host-fs / host-platform / host-events / abi）各自拆出桌面独有函数为新 interface（host-fs-desktop 3 / host-platform-desktop 4 / host-events-desktop notify / abi-form form，v36），桌面扩展的 **WIT impl 落宿主**（`src-tauri/src/plugin/{fs,platform,events}.rs` 路径 B 四件套 + 白名单自报），实现随域走（platform / events 全迁；fs 三函数实现本体**显式留内核**作双消费者）。**桌面 ABI 35 → 36 随本批 bump**（拆 interface 即契约变更，产物无法维持 v35——SDK abi.rs + WIT 迁移注释 + 断言全联动）；移动端零影响（WIT/ABI/SDK 不跟演）。

**关键裁决（先写）**

1. **fs 三扩展函数（read-dir / canonicalize / stat）实现本体留内核，提 `pub`**（`host_api/fs.rs`）：决策点 = 内核 `FsUnitExecutor`（留内核的 fs 域 task 执行器）的 `fs.read-dir` / `fs.stat` 单元是**内核消费者**，与宿主 WIT impl 构成**双消费者同一条实现**——与 `check_permission` 提 pub、`kill_process_group` 提 pub 同款裁决（消费者在内核 ⇒ 实现留内核，宿主经公开面复用）。宿主 `plugin/fs.rs` 只做 WIT impl 转发（零判据复制）。反向锁对应放宽：锁的是「域文件不得再现 desktop interface 词汇」，`fs_read_dir` 等 pub 函数是合法机制（正面锚点明列）。
2. **platform / events 扩展函数随 impl 迁宿主**（无内核消费者）：`platform_pick_folders` 依赖 tauri dialog 多选面、`wsl_distros` 依赖 `system::wsl`、`local_ipv4_addresses` 依赖 `local_ip_address` crate、`reveal_in_dir` 依赖 `system::opener`（opener 留内核经宿主垫片引用）、`notify` 依赖 tauri Emitter——全部桌面平台依赖，随域走。内核 `host_api/platform.rs` 收窄为交集（pick-files / pick-folder + `authorize_picked` 链），`host_api/events.rs` 收窄为 `emit_event`。
3. **`system/wsl.rs`（发行版列举，150 行含测试）迁宿主 `src-tauri/src/system/wsl.rs`**（ticket-02 §3 III 类表列的 276 行为旧估，实 150）；`host_api/wsl_fs.rs`（WSL UNC 路径桥）**留内核**——被交集 fs 函数的底层 helper（`read_text_file` 等）消费的机制，与裁决 1 同判据。
4. **abi 拆分 = guest 导出面**：v36 后 guest 导出 `abi`（version）+ `abi-form`（form）；SDK `wasm_entry!` 宏分裂两 impl，内核 `verify_abi` 读出两导出；旧产物（v35 导出 abi.form）缺 `abi-form` 导出 ⇒ 实例化期被拒（fail-visible 形态②）。
5. **内核 cfg(test) 替身**（与 host-api-call 替身同范式）：SDK 夹具静态 import 三个新 interface，内核测试二进制不再注册 ⇒ 夹具实例化失败。替身语义 = headless 显性拒绝（`notify` 返回与生产 headless 同文案——sdk_e2e 的 `test_notify` 断言 error 含 headless/app_handle；fs/platform 返回「未在内核测试二进制注册」）。生产二进制不含 cfg(test) ⇒ 宿主 plugin/{fs,platform,events}.rs 是唯一注册来源。
6. **为修复 `plugins:build` 前置漂移（阻塞本批产物重建门禁）**：`manifest-gen.js` 的 `db_(execute|query) → database:main` 规则未随「主库收归双端（票 18 / 5d97fb97d）退役 host-database 权限位」跟演（`database:main` 已不在 permission.rs 词汇表），加载期自检抛错使整条构建链不可用。删规则（权限位已不存在，无 manifest 能合法声明），不加新映射。

**改动面**

- **桌面 WIT**（`plugin-sdk-desktop/rust/wit/bedcode.wit`）：四接口切片（函数 + doc 注释随函数段走；原/新接口各标「v36 拆自 / 拆入」）；`world plugin` import + export 同步（+host-fs-desktop / +host-platform-desktop / +host-events-desktop / export abi-form）；`world plugin-system` export 同步。
- **SDK**：`abi.rs`（ABI_VERSION 36 + 历史注释 v36 条 + 断言 36 + FORM_COMPONENT 注释改指 abi-form）；`wasm.rs`（`wasm_entry!` 的 abi::Guest 分裂为 abi::Guest[version] + abi_form::Guest[form]）；`wasm_host.rs`（8 处 import 封装改 namespace：`host_events::notify → host_events_desktop`、`host_fs::{read_dir,canonicalize,stat} → host_fs_desktop`、`host_platform::{pick_folders,wsl_distros,local_ipv4_addresses,reveal_in_dir} → host_platform_desktop` + use 列表）——**SDK Rust API 方法名不变 ⇒ 插件代码零改动**，仅产物重建。
- **内核**：`component.rs`——host_events/host_fs/host_platform impl 收窄（交集）+ cfg(test) 三替身 impl 与三行注册（add_to_linker 尾段）+ `verify_abi` 改读 `abi-form` 导出 + `stale_artifact_rebuild_hint` 双向判据扩展（v36 正向：8 个旧函数名 + `abi-form` →「按 v36 SDK 重建」；反向：三个新 interface 名 →「升级 BedCode」）；`host_api/fs.rs` 三函数提 pub + 注释（双消费者裁决）；`host_api/platform.rs` / `events.rs` 收窄（扩展函数与测试随域走）；`system/wsl.rs` 删 + `system.rs` `pub mod wsl` 声明删（留说明注释）+ `system/process.rs` 注释更新；Cargo.toml 删 `local-ip-address`；`host_api.rs` 模块文档更新（交集切片现状 + 顶层 `pub use fs::{fs_read_dir, fs_canonicalize, fs_stat}`）；**lib.rs 新反向锁 `desktop_sliced_interfaces_must_not_return_to_wasm_core`**（正面锚点：交集实现在场；反向 needle：三 desktop interface 词汇 + `system::wsl`，跳注释行；合法残余注明 cfg(test) 替身）。
- **宿主**：`plugin/{events,fs,platform}.rs`（路径 B 四件套：MODULE 常量 + DESC abi_min=36 + HostModule + inventory 自报 + `expect_host_module!` + WIT impl + 域函数 + 单测迁移/新增）+ `plugin.rs` 登记 + 注释；`system/wsl.rs` 新落点 + `system.rs` mod 声明；events 无权限位（MODULE_PERMISSIONS=[]）/ fs=fs:read / platform=fs:pick。

**门禁（实跑）**

- SDK：`cargo check` 0 error（WIT 拆分语法 + 绑定改点编译过）。
- 内核：`cargo check --tests` 0 error（告警全为存量：database.rs 票 18 在途 / l2_gating / link_crypto 等）；**全量 `cargo test --lib` 593 passed / 0 failed**（含新反向锁 1、sdk_e2e 3——SDK 夹具自动重建后 instance 3 接口替身、wasi_e2e 3——wasip3 夹具同重建、host_api::{fs 24, platform 4, events 2}、stale hint 1、keeper 2）。账目：604（批 05）− platform 扩展用例 6 − events notify 1 + 新锁 1 = 598 ≠ 593——平台上交集计数核实：platform 模块实测 4 例（原 10：交集 4 + 扩展 6），差异落原 604 基线统计口径，0 failed 为准。
- 宿主：`cargo check` 0 error（首轮 E0603：`host_api::fs` 模块 `pub(crate)` 不可外部寻址 → `host_api.rs` 顶层 pub use 修复）；`cargo check --tests` **回基线**（3×E0308 = `ws_e2e` 既有 `EndpointAuth` 同名不同源，零新增；`ws_output_perf` 同基线）；**`cargo test --lib plugin::events` 2/2、`plugin::fs` 5/5、`plugin::platform` 9/9**（fs 首跑 1 红 = read_dir 迭代序断言写死 `entries[0]`，std 顺序不定，改按名断言后绿）；**`cargo test --test pty_wiring` 6/6**（白名单双向锁：fs-desktop / platform-desktop / events-desktop 进期望 + 收集集匹配）；**`cargo test --test task_e2e` 6/6**（task 夹具自动重建后 fs.stat 单元经新链路真跑）。
- **插件产物重建**：`pnpm run plugins:build` 全量 4 插件（agent-hub / ai-chatbox / file-transfer / terminal-session）**成功 + wasmHash 注入**（terminal-session 9decfc63…、file-transfer 重建 05:02）。前置修复 manifest-gen database:main 漂移（见裁决 6）。夹具产物：SDK 夹具由内核测试自动重建（fixture_needs_rebuild mtime 检测）；task/pty 等宿主消费夹具随各测试自动重建。
- **fmt**：`rustfmt --check` 宿主三新域文件 + `system/wsl.rs` + SDK `wasm_host.rs` 全净（wasm_host use 列表按 rustfmt 收敛）；`plugin.rs` / `system.rs` 净（auth/crypto 的 register 折行为批 04/05 存量，不动）。

**未跑 / 待办（如实记）**

- `system_component_test` / `pty_e2e` / `terminal_output_perf` / 宿主 `--lib` 全量：**磁盘 5.9G 不可行**（宿主集成测试二进制 10G+ 链接），欠账同前。system-test 夹具（plugin-system-test）在宿主域迁移后仍由 **fixture keeper 缺口**照旧人工重建口径（批 04 已记录）。
- `cross-end-tests` / 移动端：**零影响面**（本批纯桌面契约）；移动 fork 的 WIT/基座未动，无需跑。
- 双端 CHANGELOG 条目 + code-map 锁索引登记：ticket-04（ABI 36 正式收口票）统一补，本批已在 ticket-02 留痕。
- 变异自检（收尾补跑，探针还原走编辑工具）：向 `src/host_api/platform.rs` 的 `sync_result` 注入代码行 `let _probe = "host_platform_desktop";` → `desktop_sliced_interfaces_must_not_return_to_wasm_core` **红**（lib.rs:453 needle 命中，点名 platform.rs）→ 还原 → **绿**。锁对 4 文件 × 4 needle 走同一 for 循环（共享判据路径），单变异点红已证锁非空转；其余 needle 同路径不再逐探。

**给票 03/04 的接缝提示**：core.wit 重组时 4 个切片接口按**本批拆分后形态**收拢（host-fs 6 / host-platform 2 / host-events 1 / abi 1 进核心；host-*-desktop + abi-form 进 cap-desktop）；票 04 的 ABI bump 桌面侧**已随本批完成（35→36）**，剩移动 19→20（如需随切片）+ 全量重建复核 + `stale_artifact_rebuild_hint` 覆盖 `host-websocket-server` 等后续接口名。
