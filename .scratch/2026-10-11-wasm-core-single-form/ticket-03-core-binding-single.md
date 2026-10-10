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