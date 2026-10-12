# ADR 0047：wasm-core 形态命名中立化（common / bare）+ 移动独有域落移动宿主

- 状态：**accepted**（2026-10-10 实施：票 02–05 代码落地；票 06 收口进行中——实施记录已回填至本文件末，验证结果见 `.scratch/2026-10-10-wasm-core-mobile-purity/ticket-06-closure.md`）
- 日期：2026-10-10
- spec：`.scratch/2026-10-10-wasm-core-mobile-purity/spec.md`（票序与门禁真源；票 01 核验产出 `.scratch/2026-10-10-wasm-core-mobile-purity/ticket-01-domains.md`；票 02 映射表 `ticket-02-mapping.md`）
- 相关：ADR 0045（单一 crate 双形态——本 ADR 修订其 D6 命名面）、ADR 0035（能力域 crate 化 + 自动装配）、ADR 0022（宿主/插件边界裁决）、ADR 0018（移动契约独立）、ADR 0043（ws-client-engine）、ADR 0042（mDNS 引擎共享）
- 用户指令（原文摘录）：「wasm-core 中不再有任何 mobile 或者 desktop 的语义 现在存在的 desktop 请改名 避免引起误解」/「使用 common 好了 不要使用 wasi」/「移动端特殊的能力 留在移动端宿主侧 不要放入 wasm core 中」/「在完成所有 ticket 的代码修改 后再统一进行 编译验证 单元测试等」

## 背景

1. **结构不对称**（实测）：ADR 0045 D6 迁 fork 时按当时桌面形态把移动装配树整棵搬入 `packages/bedcode-wasm-core`（`manager/runtime/mobile/host_impl/*` 15 域 ≈3.3k 行逐域 WIT 绑定逻辑）；桌面随后走「纯净性收口票 02 批次 03–06」把逐域 WIT impl 迁到宿主 adapter（`src-tauri/src/plugin/*.rs` 自带 bindgen + `HostModule` 自报），移动侧无对应改造 ⇒ 同一 crate 内两个形态面载荷不均。
2. **命名带端名 / 机制名**：形态 feature `desktop-host` / `mobile-host`、模块 `runtime/{desktop,mobile}`、类型 `DesktopPluginInfo` / `MobileWsClientPorts` 等 170+ 标识符命中；用户明确要求 wasm-core 内零端名、且形态名不用 `wasi`（机制名进标识符会误导）。
3. **移动独有 interface 落在 wasm-core**：`host-notify` / `host-terminal-stream` / `host-fs-mobile` / `host-connection.primary-target` / `host-auth` / 移动 events 的 WIT 壳与逻辑在 crate 内（桌面 crate 无对应物）——用户明确这些属端事实，应留移动宿主。
4. **装配链无扩展点**：移动 `build_component_linker` 是硬编码 `add_to_linker` 清单（17 interface），无 host-kit `ModuleRegistry` 注册面 ⇒ 宿主要自行注入 `impl Host` 不可能（孤儿规则 E0117 + state 私有）。

## 决策

### D1 · 形态命名：`common` / `bare`（零端名、零机制名）

> **⚠ 2026-10-13 修订（ADR 0049）**：D1 的**命名成果保留**（`common` / `bare` 中立词、零端名），但**「形态」的含义已变**——票 04/05 把双端 WIT 绑定与端事实全部迁出后，两个形态壳只剩机制接线，**机制零分叉**；WASI 不再是形态差（住内核 + 配置驱动，见 0049 D4）。终态目标是**形态 feature 整体退役**（单形态），届时本 ADR 的 D1 与 `lib.rs` 的 `compile_error!` 守卫一并删除——当前 feature 面收敛未做，属 ADR 0049 已声明的遗留项。

| 载荷 | 新名 | 旧名 |
| --- | --- | --- |
| cargo feature（默认） | `common-host` | `desktop-host` |
| cargo feature（非默认） | `bare-host` | `mobile-host` |
| 模块 / 目录 | `runtime/{common.rs, common/}` | `runtime/{desktop.rs, desktop/}` |
| 模块 / 目录 | `runtime/{bare.rs, bare/}` | `runtime/{mobile.rs, mobile/}` |

**形态机制差只写注释与文档，不进标识符**：`common` 形态装配 WASI p3 Store（插件目标 `wasm32-wasip3`）；`bare` 形态不装配（插件目标 `wasm32-unknown-unknown`）。

### D2 · 命名纪律（判据）

wasm-core 内**自持标识符**零端名（desktop / mobile）、零机制名（`wasi` / `wasip3` / `wasm32`）：形态模块名、形态 feature 名、自持类型名（`DesktopPluginInfo` → `HostPluginInfo` 等）、锁名、包描述。

**登记保留类**（外部事实，无法亦不应改名）：
- 外部 crate / 类型名：`wasmtime-wasi` / `wasmtime_wasi` / `WasiCtx` / `WasiPreopenDir`（SDK 类型）/ `bedcode_plugin_api_mobile`（移动 SDK crate 名）/ `bedcode-host-kit` 的 `wasi-store` feature 名；
- 端事实路径：`bedcode-{desktop,mobile}/**`、`plugin-sdk-{desktop,mobile}`、`resources/plugins/desktop/`、目标三元组字符串 `wasm32-wasip3` / `wasm32-wasip2`；
- 端 WIT interface 名（wire 契约）：`host-{fs,platform,events}-desktop` / `host-fs-mobile` / `host-websocket-server` / `host-http-endpoint`（含 bindgen 生成的 `host_fs_desktop` 等模块名与锁 needle）；
- 机制描述性字符串与函数 / 变量名：校验文案（wasip3 基线）、`build_wasi_ctx` / `wasi_ctx`（直接操作 `wasmtime_wasi::WasiCtx`，改名反失真）；
- 中文注释中的「桌面 / 移动」历史说明（迁入来源、形态差异理由）。

### D3 · 能力域 crate 装配 feature 同步改名

五个能力域 crate（`pty-engine` / `server-http` / `server-websocket` / `server-peer-net` / `discovery-engine`）与 `headless-host-probe` 的 `desktop-host` feature（= WIT 绑定层装配开关，装配语义载体）同步改名 `common-host`；wasm-core `Cargo.toml` 的 feature 引用行与桌面宿主引用（`bedcode-desktop/src-tauri/Cargo.toml`）同批同步。

### D4 · 域逻辑进共享 crate，各端只留壳

移动形态 15 域按桌面先例收口：域逻辑进共享 crate（WIT-无关函数 + 端口 trait），形态壳收窄为纯转发；**peer 域**（510 行含句柄表 / 自动重拨 / 载荷解析）复用 `bedcode-server-peer-net::plugin_binding` 现成 19 函数（票 03）。

### D5 · 移动独有域整体落移动宿主（路径 X）

`host-notify` / `host-terminal-stream` / `host-fs-mobile` / `host-connection.primary-target` / `host-auth` / 移动 events 的 WIT 壳 + 逻辑 + 端口一律迁 `bedcode-mobile/src-tauri/src/plugin/*`；移动装配链改接 host-kit `WasmPluginState` + `HostPorts` + `ModuleRegistry`（对齐桌面装配面）；宿主自带 bindgen（解孤儿规则 E0117）。

### D6 · 零 ABI / WIT / SDK / wire 变更

纯 Rust 侧落点搬迁与改名 ⇒ 插件产物无需重建（不触发 ABI bump 流程）。

### D7 · 验证时机后移（用户指令）

票 01–05 票内只做代码改动 + 静态自检（grep 复核 / 映射表对表 / 接口清单对表）；编译验证、单元测试、集成测试**统一在票 06 一次性执行**（避免逐票长编译与磁盘反复占满；风险与排障手段见 spec §5）。

## 代价与风险

| 项 | 说明 | 处置 |
| --- | --- | --- |
| 改名面广 | 68 文件 / 170+ 标识符 | 映射表先落（ticket-02-mapping.md）+ 全仓 grep 复核 + 逐类替换；漏改由票 06 统一验证兜底 |
| 锁路径同步 | 移动 4 锁钉 wasm-core 真源路径（`manager/runtime/mobile/host_impl/*` → `bare/`） | 同批同步（本 ADR 实施记录含清单）；票 06 变异自检 |
| 验证后移累积错误 | 票 02 改名 + 票 04 装配面改造的错误最后一次性暴露 | 票内静态自检做足；爆错时按票 `cargo check -p` 定位（排障非门禁） |
| 装配面改造（路径 X） | 移动各域状态访问面重写（`state.host_ctx` → `downcast_host`） | 票 04 逐域展开；host-kit `state.rs` 已预留无 WASI 形态 |

## 验证门禁（票 06 统一执行）

1. 内核双形态 `cargo check`（`common-host` / `bare-host`）+ `cargo test --lib`；
2. `bedcode-mobile/src-tauri` `cargo test` 全量 + `bedcode-desktop/src-tauri` `cargo check` 零回归；
3. 受影响 `packages/bedcode-*` crate `cargo test`；`cross-end-tests` 全量；
4. 两端 `pnpm run test:run` + 根 `pnpm exec eslint .` 0 error；
5. 新增 / 改动锁变异自检（注入 → 红 → 还原核 sha256）+ 双端 code-map 锁索引登记；
6. 移动独有域真机冒烟（通知 / 终端流 / SAF 落盘 / 配对）逐项写「跑了 / 没跑 + 原因」；
7. **⚠ 2026-10-13 修订（ADR 0049）**：本项原为「wasm-core 内自持标识符零端名、零 `wasi` / `wasip3` / `wasm32` 字样」的一次性 grep 复核。两处修订：① **「零 `wasi` 字样」作废**——用户裁决 2026-10-12 定 WASI 装配面**住内核**（`manager/runtime/wasi.rs`，双端同源 + `CoreConfig::wasi` 配置驱动），机制名进标识符不再误导（它不再是「某一形态才有」的东西）；② 「零端名」从一次性 grep 升级为**常驻锁**——`bedcode-mobile/src-tauri/tests/mobile_wasm_core_purity_lock.rs` 判据 7b（声明位零端名 + `DECLARED_NAME_EXEMPTIONS` 豁免登记表，每条须写理由）+ 7a（代码行零 cap 分片 world 名）+ 7c（端 SDK 词汇封顶）。

## 实施记录

- **票 02（2026-10-10 落地）**：feature `desktop-host` / `mobile-host` → `common-host` / `bare-host`（wasm-core 176 处 + 能力域 6 crate 148 处 + 双端宿主引用）；模块 `runtime/{desktop,mobile}*` → `runtime/{common,bare}*`、`test_support/{desktop,mobile}*` → `test_support/{common,bare}*`、`host_api/mobile_context.rs` → `host_api/bare_context.rs`；标识符 `DesktopPluginInfo` → `HostPluginInfo`、`Mobile{Discovery,WsClient}Ports` / `MobileReconnectPolicy` → `State{Discovery,WsClient}Ports` / `StateReconnectPolicy`、测试模块 `wasi_e2e` → `preopen_e2e`；锁 `desktop_sliced_interfaces_must_not_return_to_wasm_core` → `sliced_interfaces_must_not_return_to_wasm_core`；移动 4 锁路径 needle 同步（`manager/runtime/bare/host_impl/*`）；文档（AGENTS §0/§4/§5.4、双端 code-map）同步。映射全表与保留类清单见 `ticket-02-mapping.md`。零 ABI / WIT 变更。历史 ADR（0035 / 0042 / 0045）正文不改写，命名映射以本 ADR 为桥。
- **票 03（peer 同 crate 复用）**：移动宿主新增 `plugin/peer_ports.rs`（`PeerPorts` 端口实现）+ 开机 `install_ports`；wasm-core `runtime/bare/host_impl/peer.rs` 重写为 19 条薄转发（句柄表 / 自动重拨 / 载荷解析下沉 `bedcode-server-peer-net::plugin_binding`）。细节见 `ticket-03-peer-reuse.md`。
- **票 04（移动独有域宿主化，路径 X）**：移动改接 host-kit `WasmPluginState` + `HostPorts` + `ModuleRegistry`（`plugin/bindings.rs` 自带 bindgen 解孤儿规则 + `expect_host_module!` 白名单双向校验）；6 个移动独有域整迁移动宿主 adapter（`plugin/{notify,terminal_stream,fs_mobile,connection,auth}.rs` + 移动 events），wasm-core 删对应 `impl Host` 与生产 `add_to_linker` 项（crate 自身单测保留 `#[cfg(test)]` 替身，`standin_msg` 形态）。**踩坑与机械伤修正**（票 06 首轮 `cargo check` 排障中一次性暴露并修复）：`bedcode_host_kit::limits::defaults::*` 路径、`self.host_ctx(state)` → `host_ctx(self.state)` 的 7 处改写残迹、`block_on_async` 单参形态（旧自持 state 的 `runtime_handle` 已不存在）、`StoreSpec` 需在 `runtime.rs` 的 bare 形态 re-export 列表中出现（`test_support/bare.rs` 消费）。
- **票 05（含逻辑域收窄 / 下沉）**：`http` 下沉能力域 crate（`http_engine.rs` 退役；`HttpPorts::jwt_auth_token` 为默认实现的可选端口，移动覆写 = 宿主全局 token 代注、桌面不覆写 = 零变化）；`fs` 上移共享核 `bedcode-host-api-core::fs`（新模块 + `FsHostPorts`，平台删除差异经端口注入；桌面侧 WSL 桥/任务单元面不在本次范围）；`db` 判定已在目标形态（机制语义全在 `host-api-core::database`，本端只留私有库句柄解析）。移动侧消费方同步：`plugin.rs` 垫片去 http glob、`session_http_flow` 改测试就地端口驱动 crate 真执行器、`retired_mobile_session_control_face_lock` 的 jwtAuth 真源针脚改为 3 条（能力域开关 / 端口契约 / 适配器缺 token 拒绝文案）、`retired_mobile_auth_orchestration_command_face_lock` 的 auth 针脚改钉宿主 adapter（票 04 搬迁后原 `runtime/mobile/host_impl/auth.rs` 路径已失效）。行为对表与覆盖对表见 `ticket-05-domain-sinking.md`。
- **票 06（收口）**：新增纯净性防回接锁 `bedcode-mobile/src-tauri/tests/mobile_wasm_core_purity_lock.rs`（六判据：移动独有域不回流 / 生产装配段无其 `Host` impl / 壳不含域逻辑（http 执行本体 · fs 系统调用 · peer 句柄表与重拨）/ 共享实现核在场 / 壳行数双上限 / 退役引擎不复活；**变异自检 4/4**）；双端 code-map 与锁索引、本 ADR 实施记录、CHANGELOG 双语条目同步；统一验证结果见 `ticket-06-closure.md`。
