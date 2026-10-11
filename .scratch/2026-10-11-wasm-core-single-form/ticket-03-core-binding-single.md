# 票 03 · core 交集绑定层定稿（单份绑定 + 导出调用抽象）

> 输入：`spec.md` §5 T-03 + §7 风险项。前置：T-02（机制零分叉）。
> **依赖**：T-02 → **本票** → {T-04 ∥ T-05}（本票交付物——交集绑定单份 + 导出调用 trait——是双端绑定归位的共同底座，必须先定稿）。
> **测试节奏**：针对性自验（单形态 `cargo check` + 机制单测 + 锁自检）；本票的形态无关测试先行，全量测试留 T-06。
> 目的：wasm-core 只剩**一份** WIT 绑定（core.wit 交集 17 interface），cap 差异零出现；`LoadedWasmPlugin` / `LoadedComponentPlugin` 导出调用抽象合一（泛型薄转发）。

## 1. 现状（已核实）

| 面 | common（桌面） | bare（移动） |
| --- | --- | --- |
| bindgen | `path: ../../bedcode-desktop/packages/plugin-sdk-desktop/rust/wit`，`world: "plugin"`，`exports: { default: async }` | `path: ../../bedcode-mobile/packages/plugin-sdk-mobile/rust/wit`，`world: "plugin"`（同步导出） |
| Host impl | 12 个：storage / plugin_database / log / config / events / fs / bus / platform / api_call / events-desktop / fs-desktop / platform-desktop | 12 个：log / storage / plugin_database / events / http / fs / config / bus / peer / mdns / platform / websocket |
| 导出调用面 | `LoadedWasmPlugin`（invoke_command / activate / deactivate / on_message / on_message_binary / on_ws_frame / on_task_event / on_process_done / get_manifest / on_startup / on_shutdown + raw_store + instance_handle） | `LoadedComponentPlugin`（invoke_command / activate / deactivate / on_bus_message / on_message_binary / call_lifecycle_event / get_manifest + raw_store） |

**互可行性关键事实**（spec §2）：linker 多挂无害、少挂报错；`HostModule` + `inventory` + `verify_whitelist` 已按「能力模块白名单」工作。

## 2. 设计裁决（三选一，实现前定稿并补记 spec）

- **Option A · core-only world 独立绑定**：wasm-core 内新建/改绑 **core.wit 单 world**（只含 17 个交集 interface），bindgen 生成一份 `bedcode` 模块；cap 全部由各端宿主自绑。
  - 优点：wasm-core 无 cap 痕迹（最贴用户裁定）。
  - 代价：core.wit 需有**独立可 bindgen 的 world 声明**（现 core.wit 是 interface 集合 + 部分 world，需确认其 world 形态）；双端生成物路径也不再被 wasm-core 引用（移除 bindgen path 依赖）。
- **Option B · 交集绑定保留在 wasm-core 但只单份**：把 common 12 域中属于 core 的 9 个交集接口（storage/plugin_database/log/config/events/fs/bus/platform/api_call 之交集子集）绑定留在 wasm-core，cap 专属（desktop 3 + mobile 5 + pty/http/ws/peer/mdns 等）注册由宿主。
  - 优点：交集 impl 天然一份，不用搬。
  - 代价：wasm-core 仍持 bindgen（是否接受取决于「交集是否算差异」的语义边界——spec §7 已标注待裁决）。
- **Option C · 交集绑定全上移**：12+12 域 Host impl 全部迁宿主/能力域 crate，wasm-core 零 bindgen；导出调用面经 trait（`PluginExports`）注入宿主。
  - 优点：最彻底（wasm-core 无任何 WIT 词汇）。
  - 代价：`engine.rs` 的 `Store<WasmPluginState>` / 导出调用判定目前绑 `WasmPluginState`（host-kit），导出调用面需要 trait 化或泛型化——改动面最大；`bindings.rs`（宿主侧既有绑定层先例）可承接。

> **默认倾向 Option C**（与「依托宿主/SDK 组合」「wasm-core 零差异」裁定最一致），但须先评估 `engine.rs` 723 行的耦合面（它已被 codebuddy 解耦到只依赖 wasmtime 原语 + host-kit `WasmPluginState`，导出调用结果判定 `trapped/settle*` 是中性函数——泛型化可行）。实现时若 Option C 代价超出预计（engine.rs 大改），**降级 Option B 并向用户说明**。

## 3. 交集 12 域内容核对（迁移时逐域确认）

迁移或归一前列出每个 interface 的函数集（WIT 真源 core.wit / cap-*.wit），确认：
1. 哪些函数归「机制核」直供（如 host-log 调 host-api-core::log、host-storage 调 sqlite_ports——薄转发）；
2. 哪些带桌面扩展（host-fs-desktop 3 函数 / host-platform-desktop 4 / host-events-desktop notify）——这些已确认**迁宿主**（lib.rs 防回接锁注释，T-04）；
3. 哪些是移动独有 interface（host-fs-mobile / host-notify / host-terminal-stream / host-connection-target / host-auth-client）——已确认**迁移动宿主**（T-05，spec §2 现状，且 0047 已删 4 域文件）。

## 4. 导出调用抽象（B2）

- 用户已裁决：「导出调用薄转发可以用泛型」。设计：wasm-core 定义 `trait PluginExportCaller`（或 `plugin_exports` 泛型函数）承载 `invoke_command` / `activate` / `deactivate` / `on_message*` / `get_manifest` 的调用骨架（燃料注入 + trap 判定 + 结果归一），具体绑定类型（sync/async TypedFunc）由宿主实现注入。
- `engine.rs` 的 `trapped` / `settle*` 已中性（T-01 已并入），可直接作为骨架底座。

## 5. 测试面

- 交集绑定测试（如有 `core_modules_are_public` 等）升级为不依赖具体 cap 的形态无关测试；
- 形态专属测试（common/tests/* 15 文件 + bare 侧）明确归属：交集机制留 wasm-core、装配面随宿主（T-04/05）。

## 6. 门禁

- wasm-core（无默认 feature）`cargo check --lib` 0 error；`cargo test --lib` 全绿。
- 桌面 / 移动宿主 `cargo check` 各自 0 error（宿主绑定层尚未动，应为零变化——本票只动 wasm-core 内部与依赖接口）。
- `pnpm run check:wit` 通过（若本票调整 core.wit 的 world 形态）。
- 新增防回接锁或更新既有锁：wasm-core 内禁 cap 词汇（`cap-` interface 名 / 端生成物路径）出现。

## 7. 交付物

- 绑定层单份化裁决记录（A/B/C 取舍 + 理由）补记 spec §7；
- core 交集绑定单份 + 导出调用 trait 化落地；
- 门禁结果附本票。

## 8. 裁决（用户 2026-10-11 定：Option A + 单份 async）

**结论**：wasm-core 内 **Option A**——`bindgen!` 改绑 `wit/core.wit` 的 `world core`（真源直指，**不经端生成物**），core 8 域一份共享；cap 全由各端宿主自有 `bindgen!` + `HostModule` 自报汇入 linker。core 导出调用面**单份 async**（`exports: { default: async }`）。

### 8.1 为什么不是 C / B（裁决依据，非偏好）

- **C 被新事实否决**：core 8 域（storage / log / config / plugin-database / events / fs / bus / platform）在两端**逐字同构**——都是对共享 `host_api::*` 的薄转发（桌面直调 `storage::storage_get(&sqlite::ports_for(..))`；移动经 `bare/host_impl` 转发到同一处机制）。上移宿主 = 在两个宿主各写一份，重新制造差异代码，正撞用户裁决「wasm-core 就应该是统一的，不应该有差异代码」。
- **B 留 cap 痕迹**：仍绑桌面 SDK 生成物（`world plugin` 含 cap-pty / cap-http / cap-ws / cap-desktop），wasm-core 间接持 cap 痕迹，与 §1「零 cap」目标有差距。
- **A 的落地前提已核实成立**：linker 第二段 `host_module_registry()?.install_all(linker)` **已在消费宿主的 inventory 自报**，cap 域汇入通路跑通，宿主加域不必碰 wasm-core；`Linker` 仅在同名 interface 重复注册时报 `defined twice`，core 8 与 cap 域天然不重叠。

### 8.2 `engine.rs` 阻碍不成立（用户指定以 engine.rs 实际代码为准）

票 §2 对 C 的「engine.rs 大改」顾虑**按现状撤销**：全文签名零 WIT 依赖（只用 `Store<WasmPluginState>` / `Linker` / `Component` + 泛型 `T`，`export` 一律 `&str`），导出判定 `settle*` / `trapped` / `start_call_timer` 已是纯泛型。引擎机制**不参与本票迁移**，仍留 wasm-core 单份。

### 8.3 导出面切分（本票实施前新查证，spec §2 表未记）

`world core` 导出 = **command / lifecycle / manifest**（3）。`abi` 与 `events` 由各端 cap 的 world 追加导出（`cap-desktop`: events + abi + abi-form；`cap-mobile`: events + abi）⇒ 合成 world 导出数 6 / 5 与 §2 表吻合。`events-binary` 明确**不进**必选导出（实例化后探测）。

切分：

| 导出 | 归属 | 机制 |
| --- | --- | --- |
| `command.invoke` / `lifecycle.*` / `manifest.get` | **wasm-core**（core 绑定的类型化 Guest） | 业务导出调用面 |
| `abi.version` | **wasm-core** | `abi` 虽是 core interface 但未进 `world core` 导出表 ⇒ 走**原生 `wasmtime::component::TypedFunc` 探测**（`() -> u32`），不需绑定，零 cap 痕迹 |
| `events.*` / `abi-form` | **各端宿主**（自有绑定） | cap 导出，按端形状不同（`events` 双端各自定义，非 core.wit 的 17 interface 之一） |

### 8.4 迁移的连带事实（决定实施顺序，spec 未记）

1. ~~**同 crate 两次 `bindgen!` 必冲突**~~ **已实测证伪（2026-10-11）**：wasmtime 的
   `bindgen!` 在**当前模块作用域**生成 `mod <package 名>`，不同模块内的两次 bindgen
   各生成各的 `bedcode` 模块、互不冲突。落地时 `core_binding.rs`（world core）与
   `common/component.rs`（world plugin）并存编译通过。真正的约束是**同一模块内**
   两次 bindgen 才冲突。此条纠正了 MEMORY §2 的旧记（「勿再提议合并」的理由之一）。
2. 桌面侧桌面专属面（WASI 预打开装配 `build_wasi_ctx` / `resolve_preopen_dirs`、`abi-form` 探测、SDK 夹具 e2e 测试）依赖**全 world 绑定**（cap 导出不在 core world）⇒ 只能落桌面宿主 `src/plugin/`（已有 `bindings.rs` 全 world bindgen 先例）。**故 T-03 与 T-04 桌面半在实现上耦合，须同批落地**；移动半（T-05）同理。
3. wasm-core 自身 `cargo test` 的夹具 e2e（`common/tests/*` 15 文件）加载 SDK 夹具，夹具静态 import **全 world** ⇒ core-only 绑定后 wasm-core 测试二进制**无法实例化夹具**。按 §5 归属规则，这些装配面测试随宿主迁（桌面 `src-tauri/tests/`），wasm-core 只留机制单测（`engine.rs` 等形态无关者）。
4. 移动 app 产物须 `--rust-only` 重构建才有效（spec 已计划，§7 风险项）。

## 9. 状态：进行中（2026-10-11 下午，提交 181105b70）

### 9.1 已完成并提交（181105b70）

- **`core_binding.rs` 落地**：`bindgen!` 绑契约真源 `wit/core.wit` 的 `world core`
  （`exports: { default: async }`，单份 async 裁决），core 8 域 `Host` impl +
  `add_to_linker` 迁入，双端共用一份。
- **桌面壳收缩**：`common/component.rs` 的 core 8 域实现与逐行 linker 列删除，
  改调 `core_binding::add_to_linker`；只留桌面专属面（WASI / 导出调用面 / 桌面 cap）。
- **机制面常编译化**：`host_api` 的 bus / config / database / events / fs / log /
  platform / sqlite / sqlite_ports / status / storage / wsl_fs / unit_executor、
  `manager::capability`、`check_permission` 解除 `common-host` 门控；
  `tauri-plugin-dialog` 转无条件依赖。
- **`host-config` 词汇中性化**：双端 SDK `ConfigKey` 实质不同 ⇒ 内核持并集枚举，
  端专属项（`network.port` / `app.downloads_dir`）经 `HostEnginePorts` 取宿主注入值，
  新增 `WasmHostContext::ports()` 访问器与端口方法（默认 `None` ⇒ fail-visible）。

**门禁实证**：wasm-core 三态 `cargo check --lib` **0 error**（common / 无 feature /
bare-host）；桌面宿主 `cargo check --lib` **0 error**。

### 9.2 阻塞：移动端 fs_auth 自持实现与内核槽类型不符（须用户裁决，勿自行推进）

移动宿主 `src/plugin/manager.rs:257` 向 `WasmHostContext::new_with_ports` 传
`plugin::fs_auth::FsAuthChecker`（移动**自持**实现），与内核槽当前的
`Arc<FsAuthChecker>`（= `security::fs_auth` 的桌面实现）不是同一类型 ⇒
移动宿主 `cargo check` 1 error。

这源于**用户新裁决 2026-10-11「wasm-core 机制全部以桌面端为准收敛一套：fs 授权统一到
桌面判定链，移动自实现已退役」**。该裁决的直接后果就是移动 SAF URI 授权面需要退役
重建，属跨端**安全语义变更**（AGENTS §8 安全红线）+ 触及 `bedcode-mobile/src-tauri/
src/plugin/**` 并行会话在途文件 ⇒ **本票不推进，留用户裁决**。

配套事实（本票已按该裁决把内核侧对齐完毕，工作区可编译）：
- 内核 `FsAuthScope` 增 `fs_auth_checker()`（默认实现克隆 `fs_auth()`），让只拿得到
  `&dyn FsAuthScope` 的窄消费方（`fs.rs` / `platform.rs` / 宿主 `plugin/{fs,platform}.rs`）
  触达机制方法；8 处 `.fs_auth_checker().expect(...)` 收口为直调（恒真 `Option` 会把
  断链藏进 panic）。
- `ports::FsAuthGate` 降为纯判据签名（去掉 `Any` 超 trait）；`bare/host_impl/fs.rs`
  的共享核 `FsOp` → 安全域 `FsOp` / `FsOps` 映射改为显式两函数（此前是窄 trait 版）。

### 9.3 并行会话冲突记录（重要）

`packages/bedcode-wasm-core/src/host_api/context.rs` 在本会话开始前**已被另一条线**
按「fs 授权统一到桌面具体 `Arc<FsAuthChecker>`」改到一半（`new_with_ports` /
`new_headless` / `FsAuthScope` 未跟上 ⇒ HEAD 状态三态皆红）。本票一度按 ticket-02 的
`Arc<dyn FsAuthGate>` 超集方向改，随后**按对方新裁决回退对齐**（保留其字段/构造器/
trait 的具体类型方向 + 补齐其漏改处以恢复可编译）。该文件**连同
`src/db/database.rs` / `src/manager/types.rs` 未纳入本票提交**，留给对方认领。

### 9.4 导出调用面单份化（已完成并提交 af27c1fe8）

- **6 处 core 导出调用改走 core 绑定**：`LoadedWasmPlugin::core_exports()`（command.invoke
  / lifecycle.activate·deactivate·on_startup·on_shutdown / manifest.get）。与既有
  `exports()`（cap 侧）分工明确，两者对**同一 Instance** 建视图，不重复实例化。
- **`abi.version` 改走原生 `TypedFunc` 探测**（`core_binding::abi_version`）：
  `abi` 虽是 core.wit interface 却不在 `world core` 导出表内，绑进内核等于让内核对
  「谁的 abi」表态。**探测必须用 `ItemName` 路径语法 `bedcode:plugin/abi.version`
  （点号）**——组件接口导出是嵌套实例形态，`iface#func` 平名 str 查找恒不命中
  （踩过一次：首版写成 `abi#version`，42 个夹具 e2e 全红）。
- **`network.port` 取数宿主修正**：`HostEnginePorts` 有 46 个必需方法且**桌面从不实现它**
  （桌面生产走 `bedcode_host_kit::ports::HostPorts`）⇒ 我一度加的 `ServerPorts` 适配器
  不可行，已删除。改回 `config_get` 内按 `common-host` 直读 supervisor + 非桌面
  fail-visible 分支；仅移动的 `app.downloads_dir` 保留在 `HostEnginePorts`。
  **「服务器未启动时报错而非回退配置默认值」是刻意选择**——回退会让插件拿到看似合法
  实则连不上的端口，排查成本高于一次显式失败。

**门禁**：wasm-core 三态 `check` 0 error；`cargo test --lib` **597/0**（基线 595 +
2 条端专属键新用例）；桌面宿主 `check` 0 error。

### 9.6 T-04 的硬约束：`events` 导出无法用原生探测表达（已实测否决，勿再试）

**尝试**（本轮做过并回退）：把桌面 `events.on-message` / `on-process-done` 从全 world
绑定改为原生 `TypedFunc` 探测，与 `abi.version` 同款——若成立，wasm-core 生产面就不需要
任何全 world 绑定，`common/component.rs` 的 `bindgen!` 可整体删除。

**失败原因**：`Result<(), String>`（WIT `result<_, string>` 的返回形状）**不实现
`ComponentNamedList`**，不能作 `get_typed_func` 的 `Results` 参数。`result` 返回值是
bindgen 的特例处理，原生 `TypedFunc` 层面无对应表达。既有可选导出探测
（`events-binary` / `events-ws` / `events-task`）返回 `()`（无 `result`）所以能用。

**结论与后续**：桌面 `events` 的两个导出**必须**经绑定访问 ⇒ wasm-core 在 T-04 完成前
**生产面必须保留桌面全 world `bindgen!`**。这是 T-04 的固有工作量，不是绑定层能消除的。
T-04 因此需要处理「桌面 `events` 导出的调用方是 wasm-core 自己的事件循环
`manager/host/owner.rs`」这一反向依赖——迁宿主会倒置架构，可行路径是把 owner 的
guest-event 派发抽成宿主实现的 sink trait（机制留内核、契约面上移），或整体把 owner 的
事件循环按端下沉。**属架构裁决，实施前须停下问用户。**

另记：双端 `events` 词汇**完全不同**——桌面 `on-message` / `on-process-done`，移动是
`on-bus-message` / `on-auth-success` / `on-disconnect` / `on-session-created` /
`on-session-stopped`（已逐字比对），故它进不了 core.wit。

### 9.8 guest 事件派发契约面上移（已完成并提交 f24d0145c）

按用户授权「按建议和架构原则进行」，§9.6 的两条路径取 **①宿主实现 sink trait**
（机制留内核、契约面上移，符合 ADR 0022 §5.1.3；②事件循环整体下沉改动面过大）。

- **中立 trait** `runtime::guest_events::GuestEventDispatcher`（`on_message` /
  `on_process_done`）：机制侧只透传 `(Store, Instance)` 与消息，**不含任何端词汇**。
  `WasmPluginState` 从 host-kit 直取（`runtime::common` 是形态门控模块，两侧都要能用）。
- **`LoadedWasmPlugin` 持派发器快照**（实例化时从 runtime 取），调用计时与燃料续费
  仍留内核 —— 派发契约上移不等于把机制一起搬走。
- **注入面**：`WasmRuntime::set_guest_events(&self, ...)` 走 `RwLock`——宿主只拿到
  `PluginHost::wasm_runtime() -> &Arc<WasmRuntime>`（不可变 Arc），`&mut self` 不可得；
  与 `peer_ctx_provider` / `domain_ports` 同范式。缺省 `UnwiredGuestEvents`
  **派发即 fail-visible**（不静默丢消息）。
- **桌面生产实现** `src-tauri/src/plugin/guest_events.rs`：用桌面自有 `bindings`
  绑定的 `Plugin` 调 `bedcode_plugin_events()`；组合根（`lib.rs`）在
  `PluginHost::new` 之后、任何实例化之前注入。行为口径与内核替身逐字一致
  （guest 自报失败记 warn 后 Ok，trap 上抛）。
- **内核测试替身** `TestGuestEvents`（`component.rs` cfg(test) 区，用本文件已有的
  全 world 绑定），`test_support` 注入——与既有三个替身同一范式、同一隔离保证。
- **`abi-form.form()` 同步改原生探测**（`() -> u32` 无 `result` 可表达），
  `verify_abi` 至此**不再需要全 world 绑定**。

**门禁**：wasm-core 三态 `check` 0 error；`cargo test --lib` **597/0**；桌面宿主
`check` 0 error。

**遗留**：`common/component.rs` 的 `bindgen!` 现仅被 `cfg(test)` 替身（`host-api-call` /
`host-{events,fs,platform}-desktop` 四处）与 `exports()` 使用 ⇒ **生产面已零全 world
绑定**，但代码里 bindgen 仍在。彻底删除需把那 4 个测试替身与 15 个夹具 e2e 测试迁到
桌面宿主 `src-tauri/tests/`（T-04 剩余项）。

### 9.9 后续待办（本票未做）

1. **导出调用面合一**：`LoadedWasmPlugin`（桌面 async）迁到 core 绑定的类型化 Guest；
   `abi.version` 改原生 `TypedFunc` 探测；`events.*` / `abi-form` 交各端宿主。
2. **T-04 桌面半**：WASI 装配 + 夹具 e2e 测试迁桌面宿主 `src/plugin/`。
3. **T-05 移动半**：`bare/` 壳 + `host_impl` cap 4 域（http / peer / mdns / websocket）
   迁移动宿主；`bedcode-wasm-core-mobile` rename 别名删除。
4. **feature 收敛**：`common-host` / `bare-host` 退役 + `compile_error!` 守卫删除 +
   双 SDK 依赖（`bedcode-plugin-api` / `-mobile`）退出 —— 依赖上述三步完成后进行。
