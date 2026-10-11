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

1. **同 crate 两次 `bindgen!` 必冲突**（生成模块名 = WIT package 名 `bedcode`）⇒ `core_binding.rs` 落地与 `common/component.rs` 桌面 bindgen 的删除**必须原子**，不存在「先建新模块后清理」的中间态。
2. 桌面侧桌面专属面（WASI 预打开装配 `build_wasi_ctx` / `resolve_preopen_dirs`、`abi-form` 探测、SDK 夹具 e2e 测试）依赖**全 world 绑定**（cap 导出不在 core world）⇒ 只能落桌面宿主 `src/plugin/`（已有 `bindings.rs` 全 world bindgen 先例）。**故 T-03 与 T-04 桌面半在实现上耦合，须同批落地**；移动半（T-05）同理。
3. wasm-core 自身 `cargo test` 的夹具 e2e（`common/tests/*` 15 文件）加载 SDK 夹具，夹具静态 import **全 world** ⇒ core-only 绑定后 wasm-core 测试二进制**无法实例化夹具**。按 §5 归属规则，这些装配面测试随宿主迁（桌面 `src-tauri/tests/`），wasm-core 只留机制单测（`engine.rs` 等形态无关者）。
4. 移动 app 产物须 `--rust-only` 重构建才有效（spec 已计划，§7 风险项）。