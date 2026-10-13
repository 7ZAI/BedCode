# wasm-core 运行时单形态（common/bare 壳统一）

- 状态：进行中
- 日期：2026-10-13
- 用户裁决（原文摘录）：
  - 「ABI 版本 为什么和wasm core 有关 wasm-core 提供统一的基础ABI 宿主组装多个sdk 的 wit 后才是真正的ABI」
  - 「装配Linker 应该统一 wasm-core 不一致的有宿主组装实现差异 而不是中wasm core 中」
  - 「其他剩下的一样 统一按照桌面端的来 移动端进行改造」
- 关联：ADR 0049 遗留项 #1（形态 feature 与双 SDK 词汇收敛）、ADR 0045/0047/0018（契约端差异不变）

## 裁决落地方式

| 裁决 | 落地 |
| --- | --- |
| wasm-core 零端 ABI 知识 | `verify_abi` 的版本/形态上限检查参数化：`AbiPolicy { version, form: Option<u32> }` 由宿主装配时注入（桌面 `bedcode_plugin_api` 37 + form=1；移动 `bedcode_plugin_api_mobile` 20 + form=None）。`bedcode-plugin-api` / `bedcode-plugin-api-mobile` 从 wasm-core 依赖摘除 |
| Linker 装配统一 | 只留一份（桌面形态 = `core_binding` 8 域 + 宿主自报 `HostModule` + WASI），无条件编译。端差异 = 各端宿主自报模块集（移动 host_domains 12 域已住宿主） |
| 其余按桌面形态、移动改造 | `WasmRuntime` / `LoadedWasmPlugin` / 实例化 / 导出调用面全部用桌面版；移动宿主改调用（for_plugin 派生 context 传权限、`on_bus_message`→统一 `on_message`、`call_lifecycle_event` 无条件保留） |

## 文件级改动清单

### wasm-core（packages/bedcode-wasm-core）
1. `Cargo.toml`：删 `common-host` / `bare-host` feature；摘除 `bedcode-plugin-api` / `bedcode-plugin-api-mobile` / `bedcode-discovery-engine` / `bedcode-ws-client-engine`；`bedcode-server-websocket` / `bedcode-server-http` 无条件；桌面专属装配面（PluginHost/intercall/auth_center/密码学/server-core/crypto-engine）收进 `desktop-assembly` feature（默认开）
2. `src/lib.rs`：删 `compile_error!` 互斥守卫；feature 门控清理（`intercall` / `host_harness` / `test_support` / facade 再导出）；SHELL_FILES 锁与 `binding_surface_must_not_return_to_wasm_core` 更新
3. `src/manager/runtime.rs`：删 bare 分支；common 壳提升为唯一形态（无条件）
4. `runtime/common.rs` → 并入 `runtime.rs`（WasmRuntime / EngineSetup / AbiPolicy / panic_payload_to_string + tests mod 声明）
5. `runtime/common/component.rs` → `runtime/component.rs`（唯一组件壳；verify_abi 参数化；`call_lifecycle_event` 无条件保留）
6. `runtime/common/tests/` → `runtime/tests/`；删 `bare.rs` + `bare/` + `common.rs` + `common/` 目录
7. `src/manager/host.rs`：`PluginHost::new` 增加 `abi: AbiPolicy` 参数（桌面宿主注入；wasm-core 内部测试用 unconstrained）
8. `src/test_support`：**实施口径（2026-10-14 修正）**——common/bare **不合并**：两子模块 = 两个宿主消费面（桌面夹具 / 移动夹具），合并会造出一个谁都不用的超集；共享符号（`TEST_PLUGIN_ID` / `plugin_raw_store`）单点定义在父模块 `test_support.rs`，`setup_headless_runtime(_with_abi)` 改走统一壳 + `AbiPolicy` 注入（见下方实施记录）
9. 各处 `#[cfg(feature = "common-host")]` 门控清理

### 桌面宿主（bedcode-desktop）
- `src-tauri/src/lib.rs` 与 `tests/support/mod.rs`：`PluginHost::new` 传桌面 AbiPolicy（`bedcode_plugin_api::abi`）
- 其余零改动（统一形态即桌面形态）

### 移动宿主（bedcode-mobile）——改造适配
- `src-tauri/src/plugin/manager.rs` / `loader.rs`：统一 `WasmRuntime` 构造（storage/app_handle/first_party_dirs + AbiPolicy 20/None）+ `instantiate_component(&component, pid, ctx.for_plugin(perms), &[], None)` + `LoadedComponentPlugin` → `LoadedWasmPlugin`
- `manager.rs`：`on_bus_message` 调用点改统一 `on_message`；`call_lifecycle_event` 沿用（统一壳保留）
- `host_domains/**`：白名单声明（`expect_host_module!`）核对；`tests/` 构造点改
- 锁：`mobile_wasm_core_purity_lock` 七判据改单形态；`retired_mobile_wasm_core_fork_lock` 核对
- `Cargo.toml`：依赖面调整（SDK 仍宿主自持）

### 文档
- AGENTS.md §5.4、双端 code-map、ADR 0049 修订、CHANGELOG 双语

## 行为变更（标注）
- 移动 `on_message_binary` 探测从平名 `#` 改 ItemName 点号（ADR 0049 遗留项 3 自动修复：二进制消息开始真实投递）
- 移动实例化从 Sync 改 Async（同步组件在 async 路径等价，引擎已有实证）
- 移动实例获得桌面增强面：monitor 指标、可选导出探测（events-ws/task 探测 None 无害）、stale_artifact_rebuild_hint

## 验证
- wasm-core `cargo test --lib`（桌面形态）
- 桌面 `cargo test`
- 移动 `cargo check` + `cargo test`
- 锁组全绿

## 实施记录（2026-10-14 补录）

### 端事实收口（内核零端分支）

第 3 节「其余按桌面形态、移动改造」的余量：`host-platform.platform` 与
`host-config.network.port` 两项端事实仍在内核按 `desktop-assembly` 编译期二选一
（`"desktop"`/`"mobile"` 字面量 + `ServerSupervisor` 直读），违反 ADR 0049 D1 的
「桌面这样写、移动那样写就不是机制」。本批收口为**端口注入**：

- wasm-core：新增 `host_api::ports::HostFactsPort`（`platform_id` /
  `inbound_server_port`，缺省 `UnimplementedHostFacts` fail-visible）+ 上下文
  `set_host_facts` 注入槽；`platform.rs` / `config.rs` 删全部端 `#[cfg]` 分支。
- 桌面宿主：`bedcode-desktop/src-tauri/src/plugin/host_facts.rs`（平台 = 桌面；
  `network.port` 读 `ServerSupervisor` 运行态），作为 `PluginHost::new` 末参注入
  （任何 `activate()` 之前——file-transfer 激活期探测端值）。
- 移动宿主：`plugin/host_ports.rs` 的 `HostPorts` 实现 `HostFactsPort`
  （平台 = 移动；无入站服务器 → Err），在上下文装配后立即注入。
- 随之：wasm-core 生产段不再依赖 `bedcode-server-core`（降为 dev-dependency，
  `host_harness` 测试起服用），桌面 `crate_boundary_lock` 的 wasm-core →
  server-core 生产必填边摘除（ALLOWED 保留 dev 边）。

**行为等价**：两端取值 / 报错文案与收口前逐字相同（wasm-core `cargo test --lib`
与双端锁组验证）。

### 测试面口径修正

文件级清单第 8 条原计划「common/bare 合并」按实修正：两子模块是**宿主消费面**
分叉（桌面夹具 vs 移动夹具，两侧消费物不同），运行时壳与 core 绑定早已统一；
共享符号上提父模块单点定义，`bare` 为 fork 期历史名（模块头注明）。

### 平台选择器端差异修复（2026-10-13 Android 构建实拍）

单形态合并 common/bare 时漏掉了选择器的端差异：`host-api/platform.rs` 只留了
桌面 `tauri-plugin-dialog` 实现，而 `FileDialogBuilder::pick_folders` 是
`#[cfg(desktop)]` 专属 API——`bedcode-mobile` 的 `aarch64-linux-android` 构建
在 wasm-core 上直接报 `no method named pick_folders`（桌面 x86_64 测试全绿，
所以此前门禁未暴露）。按 ADR 0052 D1 改为**宿主装配端口**（不是内核 `#[cfg]`
分支）：

- wasm-core：新增 `host_api::ports::PlatformPickerPort`（`pick_files` /
  `pick_folders`，无 AppHandle 参数——句柄装配期捕获，替身可脱离 tao 事件循环
  驱动全链）+ `UnimplementedPlatformPicker`（fail-visible 缺省）+
  `EnginePortsPlatformPicker`（移动适配器：`HostEnginePorts::platform_pick_files`
  / `platform_pick_shared_directory` → 0/1 树 URI）；`WasmHostContext` 增加
  `platform_picker` 槽（`set_platform_picker` 两阶段注入，`new_with_ports` 自动
  派生移动适配器）；`platform.rs` 只留权限门 + `authorize_picked` + JSON。
- 桌面宿主：`plugin/platform_picker.rs`（`DesktopPlatformPicker`，对话框实现逐字
  自内核搬出），`PluginHost::new` 新增 `platform_picker` 参数在实例化 / 激活前
  写入；无头 / 测试传 `None`。
- 行为：桌面路径与错误文案逐字不变；移动 `pick-files` 恢复 SAF 选择器、
  `pick-folders` 恢复 SAF 树 URI（0/1 项），`fs:pick` 门与结果授权校验双端同源。
- 验证：wasm-core `cargo test --lib` 579/579（platform 7/7，新增端口委派 +
  授权门正反例）；`cargo check -p bedcode-mobile --target aarch64-linux-android
  --lib` 绿；桌面 `cargo check --tests` 绿；移动 `cargo test --lib` 226/226 +
  纯净 / 组合 / fork 锁 16/16。

**遗留**：wasm-core `Cargo.toml` 的 `tauri-plugin-dialog` 依赖现已无源码消费方
（可随下次依赖治理摘除；本次不碰并行线的 Cargo.toml 在途改动）。
