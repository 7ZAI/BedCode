# wasm-core 单形态 + 绑定层归位（契约组合住宿主/SDK）

> **用户裁决（2026-10-11，两次 + 2026-10-12 补充一次）**：
> 1. 「从功能 / 机制的角度出发，wasm-core 就应该是统一的，不应该有差异代码；差异的部分全部迁移或者移除。」
> 2. 「WIT 通过在宿主或者 SDK 中组合来实现整体对移动端或者桌面端，而不是在 wasm-core 中实现差异统一。」
> 3. 「不；双端都可以开启，默认开启；wasip2 p3 就放在核心——为了以后 worker wasm 使用。或者通过 config 让外部去配置开启或者关闭，默认全部开启。」
>    ⇒ **WASI 面不迁宿主、留核心**（`manager/runtime/wasi.rs`），双端形态都编译、都**默认注册**；开 / 关走外部配置 `CoreConfig::wasi`（`enabled` / `p2` / `p3`，默认全真）。§2 的「WASI 有 / 无」差异行据此作废（形态差异轴消失，见 §裁决 3 落点）。
>
> **撤销原 spec 的 union world 方向**：不在 wasm-core 内做「契约差异统一」（不把 cap-desktop + cap-mobile 并进同一 package、不合并 ABI）。契约端差异 = 各端 SDK `compose.json` 各自组合 cap 分片 + 宿主侧各自注册实现；wasm-core 只保留**双端全等的交集核心**（含机制 + core.wit 交集绑定层）。
>
> 本文件是本次合并的计划真源。原方向（2026-10-11 凌晨版本：union world / 单一 SDK / ADR 0018 撤销）**作废**，本版取代之。

## 1. 目标终态（裁定后）

```text
宿主层（桌面 / 移动 src-tauri/src/plugin/*）：
  bindgen 各端完整 world + 各端 Host impl + linker 装配 + 导出调用面
        ↓ 消费 wasm-core 的纯机制（engine/manager/security/bus/config/db/storage/...）
wasm-core（packages/bedcode-wasm-core，一份形态、零 cfg(common-host/bare-host) 分叉）：
  · 机制核心 + core.wit 交集绑定层（17 个双端全等 interface）
  · 不得持有任何 cap 分片差异 / 端名 / 双 SDK 类型分叉（WASI 面是**共享机制**
    而非形态差异：同一份 `manager/runtime/wasi.rs` 双端编译，见裁决 3）
SDK 层（plugin-sdk-desktop / plugin-sdk-mobile）：
  compose.json 各自组合 core + 各自 cap 分片（desktop: pty/http/ws/desktop；mobile: mobile）
  ABI 各端独立演进（现阶段 37 vs 20，维持 ADR 0018——本合并不合并契约）
```

**裁定派生结论**：
- **不做 union world**：cap-desktop / cap-mobile 永不同时进 wasm-core 的绑定层；同名 interface（`host-auth` / `host-connection` / `events` 等）已按角色改名的现状**保留**（不冲突即不改）。
- **绑定层归位**：wasm-core 内现存的桌面绑定（`common/component.rs`）与移动绑定（`bare/component.rs` + `bare/host_impl/`）全部迁出——分别落桌面宿主 / 移动宿主；wasm-core 只留 core.wit 交集应有的绑定（若双端可共用一份交集绑定则留 core，否则交集绑定也随调用面上移，见 T-03 推敲）。
- ~~**WASI 归桌面宿主**：`wasm32-wasip3` 装配（preopen / fs_auth 过滤）是桌面契约事实，随桌面绑定层移出；wasm-core 零 WASI 词汇。~~
  **（2026-10-12 裁决 3 作废）** WASI 装配面**住核心**：`manager/runtime/wasi.rs`（p2/p3 linker
  注册 + preopen/fs_auth 过滤链，双端同源一份），`wasmtime-wasi` 无条件依赖；
  host-kit 的 `wasi-store` 形态 feature 退役（ctx/table/`WasiView` 常编译）。
  开 / 关是**运行期配置**（`CoreConfig::wasi`，默认全开；配置文件 `wasm-core.json` /
  `set_config` 可改）——理由：将来 worker 类别插件双端都要跑 wasm（ADR 0034 L3.b），
  接线必须已经双端同源，不能再挂在桌面形态上。
- **合一份宿主上下文**：`context::WasmHostContext` 与 `bare_context::WasmHostContext` 合成一个中立类型（或统一 trait 接口），形态差异消除。
- **feature / 依赖面收敛**：`common-host` / `bare-host` 特征退役；`bedcode-plugin-api` vs `bedcode-plugin-api-mobile` 的双 SDK 类型分叉消除（统一到源自交集契约的一份类型词汇）。

## 2. 现状事实（全部已核对，2026-10-11 04:00）

| 事实 | 桌面（common-host，默认 feature） | 移动（bare-host，default-features=false） |
| --- | --- | --- |
| 形态壳 | `manager/runtime/common.rs` 877→(codebuddy 一批后约 545) + `common/component.rs` 1412 + 测试 + fixture | `manager/runtime/bare.rs` 164 + `bare/component.rs` 1781 + `host_impl/` 12 文件（auth/connection/notify/terminal_stream 已删 = 8 文件在盘） |
| bindgen path | `bedcode-desktop/packages/plugin-sdk-desktop/rust/wit` | `bedcode-mobile/packages/plugin-sdk-mobile/rust/wit` |
| world 组成 | `core` + cap-pty + cap-http + cap-ws + cap-desktop | `core` + cap-mobile |
| import / export | 26 import / 6 export（含 abi-form） | 17 import / 5 export |
| ABI 版本 | 37 | 20 |
| WASI | 有（wasip3 preopen + fs_auth 授权过滤） | 无（wasm32-unknown-unknown）← **2026-10-12 裁决作废此差异轴**：WASI 面住核心、双端默认全开、开关走 `CoreConfig::wasi` |
| 宿主上下文 | `host_api::context::WasmHostContext` | `host_api::bare_context::WasmHostContext`（不同类型） |
| 依赖 | bedcode-plugin-api + wasmtime-wasi + crypto 套件 + server-core | bedcode-plugin-api-mobile + discovery-engine + ws-client-engine |

**core.wit 三处逐字一致**（sha256 同）：真源 `packages/bedcode-wasm-core/wit/core.wit` = 桌面生成物 = 移动生成物。共 17 个双端全等 interface。

**已统一（非本次对象）**：`manager/runtime/engine.rs`（723 行 / 19 个 pub(crate) 函数：AOT 缓存 / Store 装配 / 燃料 / 实例化 / Engine 构建纯函数 / 实例生死观测 / 导出调用结果判定——已与 WIT 解耦，只依赖 wasmtime 原语 + host-kit `WasmPluginState`）；`bedcode-host-kit`（WasmPluginState / StoreSpec / limits）；`bedcode-host-api-core`；`runtime_util::block_on_async`。

**codebuddy 第一批（已完成，工作区未提交）**：engine.rs 统一（trapped/settle/settle_guest/settle_guest_event/start_call_timer 合一份 + build_engine_config 等 7 个纯函数搬入）；`common/component.rs` 测试 `test_engine()` 改走生产构建函数；桌面 12 处 + 移动 8 处 trap 分支改调 `engine::trapped`。门禁：双形态 `cargo check --lib` 0 error（9-10 存量 warning）；core `cargo test --lib` 595/0；锁变异 3/3。**移动端 `cargo test` 未复跑**（磁盘写满中断，须在 T-02 后补）。

**wasm-core 内活跃绑定面（本次迁移对象）**：
- `manager/runtime/common/component.rs`：`bindgen!` 桌面合成包 + 12 个 `bedcode::plugin::*::Host` impl（host_storage/host_plugin_database/host_log/host_config/host_events/host_fs/host_bus/host_platform/host_api_call/host_events_desktop/host_fs_desktop/host_platform_desktop）+ `LoadedWasmPlugin`（invoke_command / activate / deactivate / on_message / on_ws_frame / on_task_event / on_process_done / get_manifest + 导出调用面）
- `manager/runtime/bare/component.rs`：`bindgen!` 移动合成包 + 12 个 Host impl（host_log/host_storage/host_plugin_database/host_events/host_http/host_fs/host_config/host_bus/host_peer/host_mdns/host_platform/host_websocket）+ `LoadedComponentPlugin`（invoke_command / activate / deactivate / on_bus_message / call_lifecycle_event / get_manifest）
- `manager/runtime/bare/host_impl/`：8 域在盘（bus/config/db/event/fs/http/mdns/peer/platform/storage/support/ws 中已删 4 个移域）；另 `host_context_registry.rs` / `terminal_stream_gateway.rs` / `host_api/{ports,sql_guard,bare_context}.rs`
- `lib.rs` 的 `compile_error!` 形态互斥守卫 + `#[cfg(feature = "common-host")]` / `#[cfg(feature = "bare-host")]` 全谱分叉（88 + 56 处）

**宿主已迁绑定先例（路径 B，桌面）**：`src-tauri/src/plugin/` 已有 17 域文件（api_call / app / auth / bindings / connection / crypto / events / fs / http / mdns / peer / platform / process / pty / task / timer / ws）——桌面侧绑定外迁是**既成惯例**，本次只是把 wasm-core 内最后一批（交集 12 域 + runtime 壳）也归位。移动侧 `src-tauri/src/plugin/` 已有 auth / bindings / connection / fs_mobile / host_ports / manager / notify / peer_ports / terminal_stream / saf_io 等。

**宿主消费面**：桌面宿主通过 `use bedcode_wasm_core::{host_api::*, permission::*, runtime_util, bus, test_support}` 引用机制层（不直接 new runtime 壳，runtime 壳在 wasm-core 内部自组装）；移动宿主经 package rename `bedcode-wasm-core-mobile`（`default-features = false, features = ["bare-host"]`）消费，路径 `plugin::wasm_runtime::*` 垫片。

## 3. 移动 wasm-apps 现状（SDK 消费面）

3 个 app（ai-chatbox / file-transfer / terminal-session）依赖 `@binblink/bedcode-plugin-sdk-mobile`，实际使用 core 9 域 + `HostAuth` + `HostTerminalStream`；`notify` / `fs-mobile` 由 SDK 自身 `WasmHost` 实现（非 app 直接用）。**本合并不改 app 的 SDK 引用**（保持各自端 SDK，不单 SDK 化）——移动 app 契约面零变化，只需在移动宿主绑定层归位后**重构建 app 产物**（`node scripts/build.js --rust-only`）。

## 4. 阻塞项（按新方向重排）

- **B1 · bindgen 产物不可跨端复用**：`bindgen!` 对合成包生成 `bedcode` 模块；双端 world 不同 ⇒ 产物天然两份。核心问题：交集 12 域在 common 与 bare 里各实现一份，函数体多为薄转发（调 host_api / host-api-core）⇒ **沉淀为共享中性绑定层**（core-only world 或交集 world 的独立 bindgen，放 wasm-core 或独立 crate），双端宿主只挂各自 cap 的 HostModule。已有机制支撑：`HostModule` + `inventory` + `verify_whitelist` + linker 多挂无害。
- **B2 · 导出调用面类型不同**：common `LoadedWasmPlugin`（async exports）vs bare `LoadedComponentPlugin`（sync exports）；T-03 需裁决「交集导出调用抽象」——泛型薄转发（用户已认可「导出调用薄转发可以用泛型」）。
- **B3 · 双份宿主上下文**：`context` / `bare_context` 合成一份中立项。
- **B4 · WASI 差异**：wasip3 预开 + fs_auth 过滤是桌面事实，随桌面绑定层移出；wasm-core 零 WASI。
- **B5 · feature / 依赖面**：`compile_error!` 互斥守卫删除、两特征表与 optional 依赖重组、双 SDK 类型词汇合一份。
- **B6 · 移动宿主收口**：`bedcode-wasm-core-mobile` rename 别名删除 → 直连单形态；`plugin::wasm_runtime` 垫片改写。
- **B7 · 测试面**：common / bare 各有形态专属测试（15 + n 文件）与 fixture 构建；归位后测试归属宿主侧，核心交集测试留 wasm-core。
- **B8 · 锁与文档**：`retired_mobile_wasm_core_fork_lock.rs`、`mobile_wasm_core_purity_lock.rs`（钉「wasm-core 形态壳纯净性」）需按新终态重写；code-map 两端的锁索引同步。

## 5. 落地批次（ticket）+ 依赖图

```text
T-01 基线收口（唯一前置，工作区归零）
   └─→ T-02 机制侧去分叉（只动 wasm-core 机制面，不碰绑定层）
          └─→ T-03 交集绑定层定稿（依赖 T-02 的交集绑定底座：双 SDK 类型剥离 / 上下文合一 / feature 收敛）
                 ├─→ T-04 桌面绑定归位（依赖 T-03 定稿；只动桌面宿主 + wasm-core 桌面临界）
                 └─→ T-05 移动绑定归位（依赖 T-03 定稿；只动移动宿主 + wasm-core 移动临界）
                        [T-04 ∥ T-05 可并行——各端宿主/依赖互不重叠]
                           └─→ T-06 契约/文档/锁/ADR 收口（依赖 T-04 + T-05）
```

**依赖规则**：
- **串行链**：T-01 → T-02 → T-03（强依赖，后者需要前者产物）；T-06 是唯一收口。
- **可并行**：T-04 与 T-05（各端宿主互不重叠——T-04 动 `bedcode-desktop/src-tauri` + wasm-core 桌面侧临界，T-05 动 `bedcode-mobile/src-tauri` + wasm-core 移动侧临界；若并行须先确认二者改动的 wasm-core 文件集不相交，必要时由 T-03 交付物把两个临界切开）。
- **测试节奏（用户裁定 2026-10-11）**：每个 ticket 内只做**针对性自验**（`cargo check` + 与之直接相关的单元测试 + 锁自检）；**全量测试统一在完成全部 spec ticket（即 T-06 收口）之后集中跑**——两端 `cargo test`、两端 `pnpm run test:run`、根 `pnpm exec eslint .`、`cross-end-tests`、app 重建，一次收尾（AGENTS.md §10 两段式：开发中针对性自验，集成+全量回归留收尾）。
- **优先写代码**：ticket 顺序实施时，先落代码再补文档注释（注释随代码走）；spec 的修订不在实现中间反复，确需改裁决停下问用户。

- **T-01 基线收口**：认领并在途提交（0047 staged rename `mobile→bare`/`desktop→common` + codebuddy engine.rs 第一批 + docs/adr 4 份删除），工作区归零，双端 `cargo check` 确证基线绿。→ `ticket-01-baseline.md`
- **T-02 机制侧去分叉（wasm-core 内零 cap）**：wasm-core 删除形态互斥守卫与 cap 相关 cfg 分叉的**机制面**部分（M1-M13）；上下文合一。→ `ticket-02-core-mechanism-unify.md`
- **T-03 交集绑定层定稿**：裁决并落地「core 交集绑定」形态（A/B/C 三选一）；导出调用抽象合一（泛型薄转发）；交集 12 域 Host impl 合一份。→ `ticket-03-core-binding-single.md`
- **T-04 桌面绑定归位**：common 壳 + WASI + 桌面扩展 3 域 + abi-form 消费 → 桌面宿主 `plugin/`；桌面 `cargo check` + 针对性测试绿。→ `ticket-04-desktop-binding-host.md`
- **T-05 移动绑定归位**：bare 壳 + host_impl 8 域 + cap-mobile 5 域 → 移动宿主 `plugin/`；rename 别名删除、垫片改写直连；3 app 重构建。→ `ticket-05-mobile-binding-host.md`
- **T-06 契约/文档/锁收口**：compose/ABI 确认（各自不合并）；锁重写；ADR 0018 维持 + 新 ADR；CHANGELOG 双语 + **全量测试收尾**。→ `ticket-06-contract-docs-locks.md`

## 6. 门禁（按测试节奏分层——用户裁定）

**每 ticket 针对性自验（实施中实时跑，红了立即修）**：该 ticket 改动范围相关 crate 的 `cargo check` 0 error + 直接关联单元测试 + 新建/改写锁的变异自检（sha256 字节级还原）。

**全量收尾（完成全部 spec ticket = T-06 后集中跑一次，贴结果）**：wasm-core `cargo test --lib`；桌面 `cargo test`；移动 `cargo test`；两端 `pnpm run test:run`；根 `pnpm exec eslint .`（0 error）；`pnpm run check:wit`；`cross-end-tests`（协议面零变化按用户指示可跳过）；3 个移动 app 重建；`cargo fmt` / `cargo clippy` 自查。

## 7. 风险与未跑

- 磁盘紧张（83%）⇒ 大编译前 `df -h`，必要时 `CARGO_INCREMENTAL=0`、双端 target 错峰。
- 移动 app 产物必须重构建才有效（`--rust-only`）；native 绿 ≠ 可交付。
- T-03 的「交集绑定层」形态是本轮最大设计点：若交集绑定放 wasm-core 会让 wasm-core 仍持一份 bindgen（是否违反「零差异」取决于返回值落在「单份交集 ≠ 双端差异」），实现时须明确语义边界并在 spec 补记。
- 工作区有并行在途（0047 会话 renamed staged + codebuddy untracked），T-01 认领时严禁整文件回滚他人改动（§11 文件回滚规范）。

## 8. 越线条款（须用户确认后动）

- **ADR 0018 维持不撤销**（契约独立演进保留）；本合并只动 wasm-core 实现结构与绑定归位，不合并契约——如实现中必须触碰 cap 契约本身，停下向用户确认。
- **ABI 版本不合并**（37 / 20 各自演进）；如交集绑定对双端共存产生 ABI 影响，须先评估再定。