# host-pty 能力域整面迁出 wasm-core（pty-engine 承接 WIT 绑定）

> 指令（用户，2026-10-06 13:40）：「不需要绑定，全部迁移所有非 wasm-core 机制的代码；
> 如果 WIT 接口也迁移，通过声明 trait + 静态扫描连接 host api」
> 形态 = 第 5 个照抄 http / ws / peer-net / mdns 四域的能力域（ADR 0035 + ADR 0036 先例）。

## 0. 现状事实（2026-10-06 13:44 实测，非记忆）

| 事实 | 落点 |
| --- | --- |
| `packages/bedcode-host-kit/` 在**仓库根**（双端共享锚点），不在 `bedcode-desktop/packages/` | `packages/bedcode-host-kit/src/{module,ports,state}.rs` |
| 静态自报机制：`HostModule` / `HostModuleDesc` / `ModuleEntry` / `submit_module!` + `ModuleRegistry::collected()` + 白名单双向校验 | `module.rs` + `registry.rs`；宿主收集点 `manager/runtime/component.rs:584-622` |
| 端口下发通道：`HostPorts::domain_ports()` + `downcast_domain_ports` / `downcast_host`（进程级 `OnceLock` 作兜底） | `ports.rs` |
| wasm-core 侧 `host_api/pty.rs` 611 行 + `pty_output.rs` 337 行（948 行待迁），`src/pty.rs` 23 行纯垫片（**删**），`enums/pty_status.rs` 垫片（**删**） | |
| 域函数已大半与宿主解耦：形参就是 `&dyn BusScope` / `&dyn PermissionScope`（宿主侧 trait） | `host_api/pty.rs:187-188` 等 |
| PTY 常量真源在 `bedcode-server-base/src/constants.rs`（`PLUGIN_PTY_*`，pty-engine **已依赖**） | |
| `spawn_with_error_boundary_on` 真源在 `bedcode-server-base/src/error_boundary.rs`（pty-engine 已依赖）；ambient handle 在 wasm-core `runtime_util` | |

## 1. 决策

- **D1 引擎 crate 升格为能力域 crate**：`bedcode-pty-engine` 承接 host-pty 的 **WIT 绑定 + 权限门位置 + 域机制**，自带 `bindgen!` provider 侧生成、`HostModule` 自报。⇒ 推翻 ADR 0038 E1 给 pty-engine 定的「零 wasm 依赖」（引擎本体的**零业务**属性不变，零 wasm 属性作废）。
- **D2 边界用窄端口 trait**：`PtyPorts`（消费方声明、宿主实现，照 `HttpPorts` 先例）。wasm-core 只剩 adapter（`HostPtyPorts` + `install` + 停用回收转发），**零域逻辑**。
- **D3 三条垫片全删**：`src/pty.rs`、`src/enums/pty_status.rs`、宿主 `lib.rs` 的 `pub use bedcode_wasm_core::{db, enums, pty}` 中的 `pty`。调用点一律改成显式路径（不留转发别名）——「不需要绑定」。
- **D4 WSL 留在 wasm-core**：`system/wsl.rs` 属 `host-platform` 平台事实，与 PTY 正交（ADR 0038 E3 判 ①）；`host_api/platform.rs` 的 4 处引用改指 `crate::system::wsl` 即可。WSL 是否单独 crate 化另立票。
- **D5 测试按「谁的真源」重新归属**：域纯逻辑（限频真值表、`running_verdict` 真值表、配额/环容量仲裁）→ pty-engine 单测；**真总线投递语义留在宿主侧**（`MessageBus` 是宿主类型，pty-engine 不可依赖）⇒ `pty_e2e.rs` 留 wasm-core 单测（经 adapter 装真端口），域单测改测「域对 `publish` 的调用契约」。跨 crate 集成测试归 `src-tauri/tests/`（`capability_crates_unit_tests_only` 锁：能力 crate 不得有 crate 根 `tests/`）。
- **D6 反双份锁极性翻转**：`pty_shim_file_contains_no_definitions`（钉「垫片只允许 re-export」）随垫片一起退役，换成**反向**锁：wasm-core 不得再有 `pty` 模块 / `pty.rs` 文件（回接即红）。

## 2. `PtyPorts` 端口形状（域真正消费的 5 件事）

| 方法 | 为什么留在宿主（AGENTS §5.1.3） |
| --- | --- |
| `check_permission(plugin_id, permission, api) -> bool` | 权限门 = 安全闸门，闸门不应可插拔（照 `HttpPorts` 同款） |
| `publish(topic, sender, payload)` | 消息总线投递面在宿主（域只见 topic 字符串，`owned_topic` 拼装在域侧，命名空间门禁仍是宿主 MessageBus 裁决） |
| `config() -> PtyHostConfig` | 宿主配置真源（`AppConfig.terminal.default_cols/rows/read_buffer_size` + `channels.lifecycle_capacity`）；域只拿快照，不认识 AppConfig |
| `block_on_any(fut)` | 同步↔异步桥的唯一实现留宿主（ambient runtime / actix 自锁规避是实测产物，域不得复制第二份——照 `HttpPorts::block_on_any`） |
| `spawn_task(name, fut)` | 退出监听必须派生到 ambient runtime（`spawn_with_error_boundary_on`）；runtime 句柄在宿主 `runtime_util` |

双通道：进程级 `OnceLock`（`install_ports` / `ports()`，宿主生命周期动作：停用回收 / 关停全量回收）+ 实例级 `domain_ports("pty")`（guest 调用面：权限判定落**本实例**的 PermissionManager）。

## 3. 文件地图

### 3.1 pty-engine 新增 / 改

| 文件 | 动作 |
| --- | --- |
| `src/plugin_binding.rs` | 新增：`bindgen!` + `PtyModule`（`HostModule`，`abi_min = 16`）+ `submit_module!` + `impl host_pty::Host for WasmPluginState` + `DOMAIN` + `ports_for` |
| `src/plugin_binding/ports.rs` | 新增：`PtyPorts` / `PtyHostConfig` / `BoxedBlocked` / `BoxedTask` / `block_on` 助手 / `install_ports` / `ports` |
| `src/plugin_binding/registry.rs` | 迁入：`PTYS` 注册表 + `QUOTAS` + `with_entry` / `session_of` / `registered_handles` / `reclaim_handles` + `register_quota` / `purge_for_plugin` / `kill_all_registered` / `live_count` |
| `src/plugin_binding/primitives.rs` | 迁入：6 条原语域函数 + 配额/环容量仲裁 + `running_verdict` + 退出监听（`reap_and_publish`） |
| `src/plugin_binding/output.rs` | 迁入：`OutputNotifySink` + `allow_notify`（`pty_output.rs`） |
| `src/pty.rs` | 删（垫片）；引擎模块改为 crate 内直引用 |
| `Cargo.toml` | 加 `wasmtime`(component-model) / `wit-bindgen =0.60` / `inventory` / `bedcode-host-kit` / `serde_json` / `uuid`；删死依赖 `encoding_rs` |

### 3.2 wasm-core 删 / 改

| 文件 | 动作 |
| --- | --- |
| `src/host_api/pty.rs` | 948 → 薄 adapter：`HOST_MODULE_NAME` / `HostPtyPorts`（`PtyPorts` 5 方法实现）/ `install`（双通道登记）/ 停用回收转发 |
| `src/host_api/pty_output.rs` | 删（逻辑已迁） |
| `src/host_api/tests/pty.rs` | 域逻辑断言迁 pty-engine；**源码文本锁**（loader/lifecycle 调用点）留在本文件并改指新路径 |
| `src/pty.rs`、`src/enums/pty_status.rs` | 删；`lib.rs` / `enums.rs` 去引用（含 `let _: fn(crate::pty::PtySession)` 断言与垫片锁换反向锁） |
| `src/manager/runtime/component.rs` | 删 `impl host_pty::Host`（199-238）与 `host_pty::add_to_linker` 行（646）；`HOST_MODULES` 加 `pty`；加 `use bedcode_pty_engine as _;` 强制引用行 |
| `src/host_api.rs` | `install_capability_domain_ports` 增 `pty::install(host_ctx.clone())` |
| `src/host_api/platform.rs` | 4 处 `crate::pty::` → `crate::system::wsl::` |
| `src/manager/loader.rs` / `manager/host/activation.rs` | `register_quota` / `purge_for_plugin` 改指 `bedcode_pty_engine::plugin_binding::` |
| `src/manager/runtime/tests/{pty_e2e,terminal_output_perf}.rs` | 改指（经 adapter 装真端口的 e2e 保留） |
| `src/crate_boundary_lock.rs` | `bedcode-pty-engine` 注释改写（wasm-core 保留 adapter 面）；边方向不变 |

### 3.3 宿主 src-tauri

| 文件 | 动作 |
| --- | --- |
| `src/lib.rs:26` | 去 `pty`：`pub use bedcode_wasm_core::{db, enums};`（改结构锁 lock 2 钉死字符串） |
| `src/system/lifecycle.rs:265,331` | `kill_all_registered()` / `live_count()` 改指 `bedcode_pty_engine::plugin_binding::` |
| `src-tauri/Cargo.toml` | 已声明 `bedcode-pty-engine`（无需改） |

### 3.4 锁与文档

| 文件 | 动作 |
| --- | --- |
| `src-tauri/tests/wasm_core_whole_crate_lock.rs` | lock 2 的钉死字符串随 `lib.rs` 改；锁 1 禁词表加 `pty` 反向条目 |
| `src-tauri/tests/capability_crates_no_product_ids.rs` | pty-engine 已在扫描面 → 迁移后新增代码须过产品名词扫描 |
| `docs/adr/0039-*.md`（新） | 记录 D1 推翻 ADR 0038 E1 的理由与影响面 |
| `bedcode-desktop/docs/code-map.md` / `CHANGELOG*.md` / `AGENTS.md`（§5/§13 如有落点） | 同步 |

## 4. 风险与 fail-visible

- **R1 同名不同类型的 `Host` trait**（两侧各自 `bindgen!`）：宿主必须**同时**删掉自己的 `impl host_pty::Host` 与 `add_to_linker` 行，否则装配期 `defined twice`——已列入 3.2 同一批改动。
- **R2 白名单漏一行** ⇒ 自报模块未注册即 guest 实例化期报缺 import；`verify_whitelist` 双向校验（missing/extra）会红。
- **R3 并行会话在途改动**：`wasm-core/{lib,enums,crate_boundary_lock}.rs`、`src-tauri/Cargo.toml` 均有他人未提交改动（12:27-12:41）⇒ 一律 `edit` 精确改，不整文件回滚、不 `git checkout`。
- **R4 顺序依赖假绿**：pty 域单测若依赖宿主装端口，单跑会崩；域单测必须自带替身端口（照 mdns 域先例）。
- **R5 磁盘**：上一轮 `src-tauri/target` 已 20G+；跑全量前先看可用空间。