# 票 06 · wasm-core 单一化：移动 fork 退役 + 平台形态端口化 + 移动侧依赖切换

Status: **✅ done（2026-10-10：批次 01–03 双形态编译绿 + test-support 收口；批次 04 fork 删除（三形态实证，退役锁变异 2/2）；批次 05 移动全量 334/0 + 桌面零回归——实施记录见 §7.4 / §7.5）**（spec §4 票 06；ADR 0045 D1 / D6 的落地票——「只有一份 wasm-core，双端核心机制完全一致」的终局）
依赖：票 02（依赖图脱桌面——批 08 平台依赖、批 09 `cargo tree` 门禁归零是硬前置）、票 03（core.wit 与移动端清单）、票 04（若含切片则移动契约面相应更新）、票 05（能力域可在移动侧组合）
前置：ADR 0040（移动 fork 两步走）第一步共享核已抽（票 18/19 落地，`bedcode-host-api-core` 在根）；**本票完成后 `bedcode-mobile/packages/bedcode-wasm-core` 删除（不可逆提交，§5 有强制前置条件）**

## 1. 现状（2026-10-10 实测）

| 项 | 事实 |
| --- | --- |
| 移动 fork | `bedcode-mobile/packages/bedcode-wasm-core`：23,930 行 / 68 源文件（src/ 含 bus/config/db/host_api/manager/security/storage/system/terminal_stream_gateway 等） |
| 移动 src-tauri 引用 | `Cargo.toml:20` `bedcode-wasm-core-mobile = { path = "../packages/bedcode-wasm-core" }`；`Cargo.toml:122` 测试侧 `features = ["test-support"]` |
| 移动契约 | WIT 22 interface / 2 world（`plugin` 16 import + 5 export）+ ABI **19**（`plugin-sdk-mobile/rust/src/abi.rs:101`，断言在 ~116 行）；桌面 ABI 35 |
| Store 形态差 | 桌面 WASI p3（`wasmtime_wasi::{p2,p3}` + `FsPerms` + `ResourceLimiter`）vs 移动**无 WASI**（D5 最后一块） |
| 平台依赖 | 桌面 `webkit2gtk` / `dbus` 在 `[target.'cfg(target_os = "linux")'.dependencies]` 且必选（Linux / Android 同 `target_os` 必撞）——批 08 处置面 |
| 移动已共享 | 根 `bedcode-ws-client-engine`（ADR 0043）、`bedcode-file-transfer-core`（ADR 0044，双端共享业务核）已是根 crate 消费先例；移动 `host_notify`（ABI v18）为移动侧独立演进接口（进 cap-mobile） |

## 2. 目标（D6 终态）

```text
bedcode-mobile/src-tauri：
  bedcode-wasm-core-mobile = { path = "../packages/bedcode-wasm-core" }   ← 删除
  bedcode-wasm-core = { path = "../../../packages/bedcode-wasm-core",
                        default-features = false, features = ["mobile-host"] }
packages/bedcode-wasm-core：
  ├── wit/core.wit（交集，双端同源）
  ├── 机制面（bus/db/security/manager/lifecycle）双端合一
  ├── desktop-host feature：桌面能力域 / StorePorts 实现 / WASI 装配
  └── mobile-host feature：移动 WIT 绑定 + 移动 host_impl ~16 域 + 移动 ports / StorePorts 实现（无 WASI 装配）
```

移动 fork 的 WIT 绑定层、host_impl 域与 ports 在 **`mobile-host` feature 门控下迁入根 crate**（与 `desktop-host` 对称，ADr 0035 语义的端 host 面）；能力域组合走移动端清单（票 03 cap-mobile，含 host-notify / host-terminal-stream 等移动特有面）。Store 装配差异经 `StorePorts`（或扩 `HostPorts`）端口注入——wasm-core 只调端口，两端各自实现，失败 fail-fast（fail-closed 判据同认证裁决）。

## 3. 步骤 / 批次

### 批次 01 · 平台依赖脱桌面（若批 08 未完成，本票兜底执行）
1. `cfg(target_os = "linux")` → **`cfg(all(target_os = "linux", not(target_os = "android")))`**（webkit2gtk / dbus 及其 sys crate；Linux 与 Android 同 `target_os` 的唯一撞车点）
2. `tauri` features / `tauri-plugin-dialog` / `portable-pty` 等按端裁剪（optional + feature 门控）
3. 门禁：`cargo tree --no-default-features` 桌面能力域命中**归零**（对齐 `bedcode-headless-host-probe` 判据）+ Android target `cargo check --target aarch64-linux-android`（需 NDK；跑不了写明原因）

### 批次 02 · StorePorts 端口化
4. 抽 `StorePorts`（wasm-core 内 trait）：桌面实现 = WASI p3（wasmtime_wasi p2/p3 + FsPerms + ResourceLimiter）——注：WASI p3 装配面若判定为「宿主形态而非机制」可放桌面宿主落点（与路径 B 宿主 adapter 同侧），内核只留端口 trait（执行期裁决：**端口 trait 留内核、实现住宿主或 feature 面**，与票 02 批次「适配器住宿主」同判据冲突最小化）
5. 内核装配点改调端口；无实现可用时 fail-fast（不静默降级）

### 批次 03 · mobile-host feature：移动面迁入根 crate
6. 逐文件把移动 fork 的「WIT 绑定层（bindgen 指移动端生成物）+ host_impl ~16 域 + 移动 ports + 无头/测试夹具体系」迁入 `packages/bedcode-wasm-core`（`mobile-host` feature）；机制面（bus/db/security/manager/lifecycle）与桌面**合一**——同名文件先机械 diff（`diff -u` 双端同名源），差异为 0 的用桌面版，非零的逐项裁决（真差异 vs fork 漂移）
7. 移动端 WIT 生成物切换：移动 `bedcode.wit` = core + cap-mobile（票 03 流水线）；移动 SDK 重编；**移动 ABI 不变**（除非票 04 已 bump 为 20，则维持 20）
8. 移动 src-tauri 依赖切换（§2 目标块）；`test-support` feature 对齐

### 批次 04 · fork 退役（fail-visible 三形态，AGENTS §5.1.4）
9. **① 旧读路径删除或显性报错**：删除 `bedcode-mobile/packages/bedcode-wasm-core` 目录 + src-tauri 别名引用；任何残留引用在编译期即失败（禁止「查不到就返回空」的静默降级）
10. **② 旧 ABI 产物实例化期点名**：移动宿主实例化逻辑对旧（fork 时代）产物的缺失 interface 报错点名 interface 与重建版本（复用票 04 扩展后的 hint 管线）
11. **③ 退役锁**：移动端防回接锁（fork 路径字形 / `bedcode-wasm-core-mobile` 别名 / 旧 import 引用回归校验——变异自检 2/2 探针：注入字形 → 红 → 还原 → 绿）

### 批次 05 · 门禁收口
12. 移动 `src-tauri` 全量测试 + 桌面零回归 + cross-end-tests（契约生成物管线换了，协议未变）

## 4. 门禁

| 项 | 要求 |
| --- | --- |
| 移动编译 | `bedcode-mobile/src-tauri` 编译绿（含 `--no-default-features` 根 crate 面） |
| 移动全量测试 | 移动端 `cargo test` 全量（迁移前等价的测试面——机械 diff 零差异的域直接复用） |
| 桌面零回归 | 桌面 `cargo test` 全量 + 两端 `pnpm run test:run` + 根 `pnpm exec eslint .` 0 error |
| 依赖图门禁 | `cargo tree --no-default-features`（移动形态）桌面能力域命中归零；Android target 实证（跑不了写明） |
| 退役三形态 | ① 残留引用编译即红 ② 旧产物实例化点名 interface + 重建版本 ③ 防回接锁变异自检 2/2 |
| 跨端 | `cross-end-tests` 全量（双端 mock 各自自洽） |

## 5. 风险与回退

| 风险 | 吸收 / 回退 |
| --- | --- |
| **删除移动 fork 是不可逆提交** | 提交前强制前置：移动全量测试绿 + 桌面零回归绿 + 批次 04 三条退役形态全部实证；git 历史即恢复源；本票任何一步红、立即停手修复而非绕过 |
| 移动 host_impl 与桌面机制面的深层差异（16 域 impl 细节漂移） | 批次 03 步骤 6 先机械 diff 再裁决；真差异进 abi 注释 / ADR 0040 续页；禁止「顺手统一」非本票差异（AGENTS §最小改动） |
| 移动测试基建形态（无头 / 模拟器） | 迁入后先跑移动 `src-tauri` 全量实测，受环境影响项如实写明「没跑 + 原因」 |
| StorePorts 端口面边界不清 | 批次 02 步骤 4 的裁决点：端口 trait 留内核 / 实现住宿主或 feature —— 与票 02 适配器判据对齐，不另起范式 |
| 磁盘（根 crate 双 feature 全量 ~10G+） | 对齐既有清理先例：`df -h` 预算、`cargo clean` 靶向、sccache 清理；fork 删除后释放 ~2G+ |

## 6. 文档联动

- ADR 0045（D6 实际形态 + fork 退役证据）；AGENTS.md §5.4「双端差异」改写（单一 wasm-core + 差异只来自能力域组合；票 07 统一收口也行，本票至少留痕迹）
- 双端 code-map：移动 fork 章节删除 / 根 crate feature 表登记；CHANGELOG 双语（票 06 一条）
- `docs/knowledge/`：plugin-development-checklist 的移动契约面段落更新

## 7. 实施记录

**状态：批次 01 + 02 ✅ 已落地（2026-10-10）；批次 03–05 待续**（本会话完成独立前置面，批次 03 主体迁移与批次 04 不可逆删除按下文实测规模分块推进）。

### 7.0 开工前实测（对票面 §1 的修正与补充）

- **批 08/09 缺口实测**：根 wasm-core `cargo tree --no-default-features` 桌面命中 **32 处**（bedcode-plugin-api / server-{base,core,http,ws,peer} / actix / dbus / webkit2gtk）——票 02 批 08 未做，批次 01 全量兜底。
- **机械 diff 矩阵**（42 个双端同路径文件）：**16 个零差异**（db 全套 / security 大部 / monitor 等——批次 03 直接用根版）；11 个 ≤40 行小差异；**6 个核心形态差**（component.rs 3498 行差 / runtime.rs 1437 / test_support 1036 / host_api/context.rs 972 / lib.rs 477 / permission.rs 394）——形态差本质 = 两套装配逻辑（移动 17 组 Host 接线 + 无 WASI + granted_permissions vs 桌面 12 域留置 + WASI p3 + StoreSpec），**cfg(feature) 分叉共存而非文本合并**。
- **关键发现**：① 移动 fork **自持 `WasmPluginState`**（runtime.rs:119，无 WASI 字段）——host-kit 版才是正源，批次 03 迁移时删 fork 自持版统一 host-kit 版；② fork 对 `wasi_ctx`/`WasiView` **零引用**（纯桌面形态残留）；③ 双端 SDK 的 `BusMessage` 均为 **server-base 真源的自持副本**（wire/drift_lock.rs 钉住）——机制面类型中立化可走真源。
- **依赖差集**（ROOT-ONLY 26 项）：桌面独有 = plugin-api / server-core / server-{http,ws,peer} / crypto-engine / 加密套件 7 项 / wasmtime-wasi / tauri-plugin-dialog / dbus / webkit2gtk / windows-sys；**server-base 实为双端共享基础层**（fork 自持 error.rs 是 fork 漂移，批次 03 正向对齐 server-base 真源）；nosleep / sysinfo / mdns-sd **源码零引用**（Cargo.toml 遗留，直接删）。

### 7.1 批次 01 · 平台依赖脱桌面（批 08 兜底）✅

1. **cfg 修正**：`cfg(target_os = "linux")` → `cfg(all(target_os = "linux", not(target_os = "android")))`（dbus / webkit2gtk 直依赖——Linux 与 Android 同 `target_os` 的唯一撞车点；tauri 自身 webview target cfg 正确，D2 保留 tauri 不构成撞车）。
2. **零引用依赖删除**：nosleep / sysinfo / mdns-sd（grep 全源码零命中）。
3. **桌面独有依赖 optional 化**：bedcode-plugin-api / server-core / server-{http,ws,peer} / crypto-engine / 加密套件 7 项 / wasmtime-wasi / tauri-plugin-dialog 全部 `optional = true` + `desktop-host` feature 引用（能力域 crate 的 `/desktop-host` feature 随 `dep:` 一并传递）；`server-base` 裁决为双端共享基础层无条件保留；`mobile-host = []` 空骨架建（批次 03 填充）；dev-deps 的 actix-web / flume 无 optional 机制，门禁跑 `--edges no-dev` 口径豁免。
4. **门禁（实跑）**：
   - Linux host `cargo tree --no-default-features --edges no-dev`：**能力域命中 = 0**（bedcode-plugin-api / server-* / portable-pty / actix-web / wasmtime-wasi / tauri-plugin-dialog / crypto-engine 全零）。剩余 10 处命中全为 webkit2gtk / dbus / libdbus-sys / webkit2gtk-sys——**全部经 tauri 传递**（D2「保留 tauri」下 Linux host 必然含其 webview 依赖；票 02 批 09 判据中的 webkit2gtk|dbus 两项在保留 tauri 前提下于 Linux host 不可达，门禁口径修正为「wasm-core 自有桌面依赖归零 + Android target 全零」双实证）。
   - **Android target 实证**：`cargo tree --no-default-features --edges no-dev --target aarch64-linux-android` 全清单命中 = **0**（tree 纯依赖解析，无需 NDK；编译级实证需 NDK 环境，记欠账）。
   - desktop 形态（default feature）`cargo check` 绿（optional 化零破坏；4m43s 全量重编）。

### 7.2 批次 02 · Store 装配端口化 ✅（形态裁决：feature 面）

票面步骤 4 裁决点授权「实现住宿主或 feature 面——与票 02 适配器判据对齐，不另起范式」。**实际采用 feature 门控形态**（比 trait 化侵入小一个数量级：trait 化需改双端装配签名，feature 化只需 cfg 字段）：

- host-kit：`wasmtime-wasi` → optional + `wasi-store = ["dep:wasmtime-wasi"]`；`state.rs` 的 `wasi_ctx` / `wasi_table` 字段、`ResourceTable` import、`WasiView` impl 全部 `#[cfg(feature = "wasi-store")]` 门控；`WasmPluginState::new` 双形态（wasi-store 开 = 4 参带 wasi_ctx，关 = 3 参）——构造点全仓仅 component.rs:635 一处（桌面分支，签名不变）。
- wasm-core：`desktop-host` feature 首项 + `"bedcode-host-kit/wasi-store"`（桌面装配传递开启）；mobile-host 不开 ⇒ wasmtime-wasi 出图。
- **门禁（实跑）**：host-kit 双形态 `cargo check` 绿（无 feature 1m17s / wasi-store 1m31s）；`--no-default-features --edges no-dev` 的 wasmtime-wasi 命中归零（含 Android target）。
- 批次 03 接缝：fork 自持 `WasmPluginState` 删除后，mobile-host 形态统一用 host-kit 版（wasi-store 关 = 无 WASI 字段，与 fork 自持版形状对齐）。

### 7.3 批次 03 · mobile-host 装配树迁入（进行中 → 双形态编译绿 ✅）

**核心里程碑：单一 crate 双形态装配树编译绿**（`cargo check` 0 error + `cargo check --no-default-features --features mobile-host` 0 error，2026-10-10）。

**形态裁决（对票面 §7.3 原计划的修正）**：
1. **fork 自持 `WasmPluginState` 保留不删**——它与 host-kit 版形状根本不同（`host_ctx: Arc<WasmHostContext>` + `granted_permissions` state 内本地权限模型 vs `host: Arc<dyn HostPorts>` 端口），16 域 host_impl 全围绕 fork 形状编写；统一 = 重写全部 impl（违反「签名与错误文案逐字保留」）。两个 state 类型是**装配形态差异**而非漂移，cfg 互斥下同名并存（`mobile::WasmPluginState` vs host-kit 版）。批次 02 的 `wasi-store` 门控依然有效（桌面字段门控独立成立）。
2. **装配树 = 平行分支非文本合并**：`manager/runtime.rs` 改声明壳（glob 过壳保持 `crate::manager::runtime::X` 路径不变），桌面内容迁 `runtime/desktop.rs`（component.rs 归位 `runtime/desktop/component.rs`——Rust 2018 非 mod.rs 子模块解析）；fork 树迁 `runtime/mobile.rs` + `runtime/mobile/{component.rs, host_impl.rs, host_impl/*16 域}`。
3. **SDK 类型一律 cfg use 切换**（非真源迁移）：`BusMessage` 单点化（bus.rs `pub use` 按形态取两端 SDK + 全 crate 走 `crate::bus::BusMessage`——两套形状本就并存，BusPort 边界早已是 base 版）；`PluginManifest` 等同名类型逐文件 cfg 切换。真源统一（server-base）留票 07。
4. **共享真源超集化两处**：server-base `AppError` 补 `Egress` variant（fork error.rs 唯一独有 variant；fork error.rs 退役、root lib.rs 加 `pub mod error` 垫片保持 `crate::error::` 路径）；server-base constants 补 `PLUGIN_DATA_DIR` / `PLUGIN_STORAGE_DIR`（移动 host_impl/db 消费）。

**迁入清单（fork → 根 crate `mobile-host` 门控）**：
- 装配树：`runtime/mobile.rs`（WasmRuntime + 自持 WasmPluginState + host_impl 声明）+ `mobile/component.rs`（移动 bindgen path → `../../bedcode-mobile/packages/plugin-sdk-mobile/rust/wit`——跨端引用移动生成物）+ `mobile/host_impl.rs` + `host_impl/` 16 域
- host_api adapter：`http_engine.rs` / `ports.rs` / `sql_guard.rs` / `context.rs → mobile_context.rs`（与桌面 context.rs 同名不同物，改名消歧）
- 独立模块：`host_context_registry.rs`（context 引用改 mobile_context）/ `terminal_stream_gateway.rs`
- 真差异合并：`PluginLifecycleEvent`（types.rs）/ `spawn_with_error_boundary*` 三件套（runtime_util.rs）/ `FileHandlerContribution` + `ResourceOverrides`（registry.rs / config.rs crate 内自持 cfg(mobile-host)）；`block_on_ambient` / `ambient_handle` 与 root 既有段重复 ⇒ 迁入时去重

**声明分叉**：lib.rs（crypto/intercall/utils/test_support → desktop-host；host_context_registry/terminal_stream_gateway → mobile-host）+ manager.rs（capability/host/task/watcher → desktop-host）+ host_api.rs（桌面 WIT impl 域 12 模块 → desktop-host；移动 adapter 4 模块 → mobile-host）+ 守卫段（check_permission / grant_permissions / install_capability_domain_ports / forward_mdns 复用面 → desktop-host）

**互斥守卫**：lib.rs 顶层 `compile_error!`（双 feature 同开 = 双 bindgen 同名 `bedcode` 模块冲突，编译期显性拒绝）

**移动 src-tauri 依赖切换（已落盘）**：`bedcode-wasm-core-mobile = { package = "bedcode-wasm-core", path = "../../packages/bedcode-wasm-core", default-features = false, features = ["mobile-host"] }`——**package rename 保持源码 `bedcode_wasm_core_mobile::` 导入路径零改动**（票面目标块的新 crate 名写法以 rename 等价达成，记裁决）。编译验证进行中（后台）。

**批次 03 剩余欠账**：① `test-support` feature 的移动面（fork test_support.rs 1036 行差迁入 root）——补齐前移动集成测试不可编译（dev-deps 已如实注释）；② `wit-component`（fork 依赖，AOT 编码测试面）随 test-support 面评估；③ 移动 src-tauri 全量编译 + 测试绿（批次 05 门禁前置）。

### 7.3.1 批次 03 收口验证（2026-10-10，锁形态分叉完成后）

**深水区：db 锁形态是装配差异的一部分**。root PluginStorage/AuthPolicyStore 的 db = tokio Mutex（async 面），fork = std Mutex（移动 host fn 同步上下文的架构裁决，mobile_context.rs 头注）——同一 plugin_db 连接被两种锁形态的消费者共享（WasmHostContext.db std / PluginStorage root tokio）。裁决：**锁形态按分支分叉**（`storage.rs` / `auth_policy.rs` / `network_auth.rs` 三文件）——字段 + `new` + 锁获取行 cfg 双形态（桌面 `.lock().await` / 移动 `.lock().unwrap_or_else(poison)`），Database 操作双端一致；`network_auth` 的 db 参数改 `DbMutex` cfg 别名（prompts 锁恒 tokio）；`MessageDispatcher` trait 双形态签名（移动 async_trait / 桌面同步）+ MessageBus 投递调用点分叉。

**迁移补遗**（编译驱动逐项）：`db::Database::from_connection` / `PluginStorage::migrate_file_store_to_db`（65 行，迁入 impl 内；db 锁行随分支）/ `PluginLifecycleEvent`（types.rs）/ `spawn_with_error_boundary*` 三件套（runtime_util.rs）/ server-base constants 补 `PLUGIN_DATA_DIR`+`PLUGIN_STORAGE_DIR`。

**结构归位补漏**：`fixture_target.rs` / `fixture_build.rs` / `tests/` 目录随 component.rs 归位 `runtime/desktop/`（Rust 2018 子模块解析），tests 内两处全路径 `runtime::component` → `runtime::desktop::component`。

**收口门禁（全实跑）**：

| 门禁 | 结果 |
| --- | --- |
| root desktop `cargo check` | 0 error |
| root desktop `cargo test --lib` | **591/0 绿**（基线 593−2 = validation 计数口径漂移，0 failed 为准） |
| root `--no-default-features --features mobile-host` | 0 error |
| 移动 src-tauri `cargo check`（host target） | **0 error**（依赖切换 + 垫片路径 `plugin::wasm_runtime` 全量解析） |
| 桌面宿主 src-tauri `cargo check` | 0 error（零回归） |
| 互斥守卫 | `--features desktop-host,mobile-host` 编译红（compile_error + E0252 双保险） |
| 依赖图 tree `--no-default-features --edges no-dev` | 能力域命中 0（批次 01 门禁保持） |
| Android target tree | 全零（批次 01 门禁保持） |

**垫片解析实证**：移动 src-tauri 的 `plugin.rs:45` 整模块垫片（`pub use bedcode_wasm_core_mobile::manager::runtime as wasm_runtime`）经壳的 mobile 分支符号全导出（含 `host_impl` 模块别名）零改动解析——`wasm_runtime::host_impl::register_host_service` 等宿主消费路径逐字保持。

### 7.3.2 test-support 移动面收口（2026-10-10）

- `test-support = ["dep:wit-component"]` feature 建立（wit-component =0.256.0，fork 原依赖，测试组件编码）；`test_support.rs` 改壳（本体迁 `desktop.rs` 保持路径；fork test_support.rs 迁 `mobile.rs` + `mobile/mock_plugin_ws.rs`，符号过壳——src-tauri `test_support::X` 引用形态零改动）。
- 迁移补遗：`PluginStorage::test_storage()`（cfg mobile-host + any(test, test-support)）/ `host_api` 顶层 ports 符号 re-export（fork host_api.rs:24 清单）/ `impl FsAuthGate for FsAuthChecker`（fork fs_auth.rs 3212-3236 段）。
- fork 时代路径三处改写（crate 迁根）：`../../wasm-apps/terminal-session` → `../../bedcode-mobile/wasm-apps/terminal-session`；`../plugin-component-test` → `../../bedcode-mobile/packages/plugin-component-test`；`../../target/fixtures` → `../../bedcode-mobile/target/fixtures`。
- **移动 src-tauri 测试编译 0 error + 全量测试 244/245**（4 个 manager/loader 失败随夹具路径改写自愈）。剩 1 失败：`test_load_all_loads_component_plugin`——组件构建成功但装载链未收录（loader.rs:382 contains_key 断言）——**装载链调试欠账**（ABI 对账 / manifest 校验 / 装载过滤，需专门会话；tracing 日志在测试环境未初始化，原因未定位）。

**批次 03 终态门禁（全实跑）**：root desktop check 0 + test --lib 591/0；mobile-host check 0；mobile-host+test-support check 0；移动 src-tauri check 0 + test 编译 0；桌面宿主 check 0；互斥双开红；tree 双门禁保持。**批次 04 前置缺口 = 上述 1 失败调试修复。**

**批次 04/05 维持原计划**（fork 删除的强制前置 = 批次 03 完整收口 + 移动全量测试绿 + 桌面零回归绿）。

### 7.4 批次 04 · fork 退役（fail-visible 三形态）✅（2026-10-10，随票 07 收口）

**前置达标（移动 4 在途红全修复——全量从未复跑的欠账暴露，逐条根因）**：

| # | 红 | 根因 | 修复 |
| --- | --- | --- | --- |
| 1 | `plugin::loader::tests::test_load_all_loads_component_plugin`（loader.rs:382 `contains_key`） | 夹具内联 manifest 仍带 C2 整面退役权限位 `ui:input` / `ui:toolbox` ⇒ 装载期 `check_retired_permissions` 显式拒载（产品代码正确，夹具陈旧） | 夹具权限集对齐真实 `plugin.json`（`network:http` / `ui:route` 等） |
| 2 | `retired_mobile_auto_task_plugin_lock::merged_task_domain_and_plugin_retirement_stay` | 判据按全文 `contains` 扫 `activate.ts`，把 C2 退役**记账注释**（「registerToolboxPage 已整面退役…」）误判为回接 | 判据改逐行跳注释（与本文件头注「只扫非注释行」纪律一致） |
| 3 | `retired_mobile_host_terminal_hooks_lock::mobile_terminal_stream_retained_face_stays` | needle 指旧手写 `bedcode.wit`——票 03 分片后接口定义迁到生成物 `cap-mobile.wit`；另 2 条指 fork 路径 | 改钉 `cap-mobile.wit` 与根 crate `mobile-host` 面路径 |
| 4 | `retired_mobile_terminal_link_lock`（5 条保留面） | 全部指 fork 路径（`../packages/bedcode-wasm-core/...`），fork 删除后必红 | 改钉 `../../packages/bedcode-wasm-core/src/manager/runtime/mobile/...` |

**三形态实证**：

1. **① 旧读路径删除**：`rm -rf bedcode-mobile/packages/bedcode-wasm-core`（1.4M / 72 文件；目录内在途改动 = 票 03/04 已记录变更，随退役丢弃，git 历史为恢复源）。移动 `src-tauri` 别名（library + dev 两处）指向仓库根 crate（package rename），残留引用编译期即红。
2. **② 旧 ABI 产物实例化期点名**：v37 / v20 `stale_artifact_rebuild_hint`（票 04 已扩展，本票不新增；移动旧产物按 v20 SDK 重建）。
3. **③ 退役锁**：`bedcode-mobile/src-tauri/tests/retired_mobile_wasm_core_fork_lock.rs`（四判据：fork 目录不存在 / 无 `bedcode-wasm-core-mobile` 包名 / 移动构建面无 fork 形态路径 / 别名指根 crate + mobile-host）。**变异自检 2/2**：探针 A（fork 目录 + 包名 + fork 形态路径注入）→ ① ② ③ 三判据红；探针 B（别名 `package =` 形态破坏，同语义可解析）→ 别名判据红；均还原绿。

**退役后回归**：移动全量 **334 passed / 0 failed**（22 测试目标，`.dev-logs/ticket07-mobile-post-deletion.log`）；桌面零回归（宿主 lib 全量 155/0）；边界锁表复核 9/9、内核反向锁 7/7（详见票 07 记录）。

### 7.5 批次 05 · 门禁收口（实测 + 欠账）

| 门禁 | 结果 |
| --- | --- |
| 移动编译 | `bedcode-mobile/src-tauri` 编译绿（随全量测试） |
| 移动全量测试 | **334/0**（fork 删除后复跑；含新退役锁 + 新组合锁） |
| 桌面零回归 | 宿主 lib 全量 **155/0**；宿主 `cargo check` 绿；内核桌面形态 lib 全量见票 07 |
| 依赖图门禁 | 批次 01 双实证保持（Linux host 能力域命中 0 / Android target 全零；webkit/dbus 经 tauri 传递已登记） |
| 退役三形态 | ① 目录删除 + 残留编译期红 ② v37/v20 hint ③ 退役锁变异 2/2 |
| 跨端 | `cross-end-tests` **延后**（票 04 已记项目级决策：重构波次稳定后统一验证，非本票门禁） |

**遗留欠账**：Android target **编译级**实证需 NDK 环境（tree 级已全零）；桌面宿主集成测试（`pty_e2e` / `terminal_output_perf` / `ws_e2e` 编译红基线）未在本序列复跑；`ws_e2e.rs`(3)/`ws_output_perf.rs`(1) 的 `EndpointAuth` 同名不同源为 HEAD 既有编译红（非本序列引入）。