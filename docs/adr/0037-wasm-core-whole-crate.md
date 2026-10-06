# 桌面端 wasm_core 整核抽出为可复用 crate（bedcode-wasm-core）

## 状态

**已实施**（2026-10-06，桌面端；**ABI / WIT / 协议零变动**：`world plugin` 的 22 个
import 一个未动，`ABI_VERSION` 不变，已装 `.wasm` 无需重建）。移动端零改动
（ADR 0018 契约独立，不跟演桌面部分破坏性变更）。
spec：`.scratch/2026-10-06-wasm-core-whole-crate/spec.md`（票 01-06；01-04 已实施，
05/06 为契约收口与独立验收）。前作：`.scratch/2026-10-04-wasm-core-lib-split/`
（能力实现出内核，ADR 0035）+ `.scratch/2026-09-24-wasm-core-decouple/`（内部依赖
单向化）。边界单一事实源仍是 **ADR 0022**；本 ADR 只定「机制整核的物理位置」。

## 背景

ADR 0035 把**能力实现**（mdns / websocket / peer / http 四域，55 条原语）搬进了
`packages/` 的能力 crate，并建出机制内核 `bedcode-host-kit`（1,110 行）。但**机制本体
没有动**：`bedcode-desktop/src-tauri/src/wasm_core/` 仍是 54,394 行 / 119 个 rs
文件（票 01 复核实测），直接编译在 bin crate `bedcode-desktop` 内部，不是库。

三个病灶：

1. **不是库，无法被复用**。不能独立编译、独立跑测试、独立发布；任何 Tauri 宿主想引入
   插件机制，只能把 54,394 行搬进自己的 bin crate。
2. **边界从未被画过**。ADR 0035 只画了「能力实现 vs 机制」，机制与「bin 宿主」的边界
   没有人定义过：`db`（真源）在 lib、机制在 bin 内的 wasm_core、引擎（pty / opener /
   auth_center / session_gateway）散在 lib 各处。
3. **迁移成本被高估**（实测反向）：出边里 5 类已经是 packages/ 的 crate 或 shim
   （AppError / constants / error_boundary / identity / crypto），真正要随迁或转端口的
   lib 模块只有约 4,500 行。

## 决策

### D1 · 整核 crate 命名 `bedcode-wasm-core`，落 `bedcode-desktop/packages/`

不放仓库根 `packages/`：本 crate 必然依赖桌面基础层（`bedcode-server-base` 的错误/常量、
`bedcode-plugin-api` 桌面 WIT、四个桌面能力 crate）——放根 `packages/` 会让移动端误以为
可直接复用（ADR 0018 契约独立，移动端永远拉不动）。**双端共享锚点仍是
`bedcode-host-kit`**（仓库根，双端共享机制）。命名对齐 `bedcode-host-kit` /
`bedcode-server-*`。

### D2 · crate 保留 tauri 依赖，不做零-tauri 反转

119 处引用含 `#[tauri::command]`（api_bridge 命令桥架构）、`Emitter`、`Manager`；
反转 = 独立立项。**「复用」= 任何 Tauri 宿主可直接 path 依赖**；移动端复用是后续阶段
（ADR 0018）。

### D3 · lib 垫片 `pub use`，既有引用零改动

`lib.rs`：`pub use bedcode_wasm_core as wasm_core;` + `pub use bedcode_wasm_core::{
db, enums, pty};`；`system.rs` / `utils.rs` / `utils/auth.rs` / `server/ports_impl.rs`
改 `pub use` 垫片。全部既有 `crate::wasm_core::*` / `crate::db::*` 引用（lib 9 文件 +
5 集成测试 + cross-end-tests）**零改动**编译通过。垫片文件只允许 `pub use`
（反双份锁：`src-tauri/tests/wasm_core_whole_crate_lock.rs`，票 05 新增）。

### D4 · `db` 随迁（机制与真源同侧，ADR 0036 延续）

`host-api/database.rs` / `storage.rs`（机制面）与 `src/db/`（真源 `plugin_*` 四表 +
`schema.sql` 单一事实源）**必须同 crate**。整核移出后二者都在新 crate，归属只有
一个答案——这是对 ADR 0036 的延续（内核整体迁出），不是推翻。

### D5 · 唯一端口 = `PeerCtxProvider`；不建 DbPort / PtyPort

`PluginHost::new` 增第 5 参 `Option<Arc<dyn Fn(&AppHandle) -> Arc<PeerCtx>>>`
（lib.rs 传 `Some(peer_net_cmds::peer_ctx)`，测试/无头传 `None` →
`HEADLESS_UNAVAILABLE` 语义逐字不变）。`Database` / `PtyRing` API 作端口 = 复制整套
接口（ADR 0036 拒绝的形状）；AppHandle 已随 `PluginHost::new` 第四参注入
`WasmHostContext`。

### D6 · `AppConfig`（839 行）随迁

引擎级配置（config.rs 头注释明示「宿主只存引擎级配置」），pty 引擎 + host_config
只有 3 个读点；lib（server / commands）经垫片零改动。

### D7 · `auth_center` / `session_gateway` / `test_tokens` 随迁

内容 90% 是 wasm_core 互调 + 零 lib 依赖；`test_tokens` 供 wasm_core 测试用，留 lib
会造成 crate 测试 dev-dep 环。`test_tokens` / `test_seed_plugin_secret` 在 crate 内
**常编译**（不带 `#[cfg(test)]`）——依赖 crate 的 cfg(test) 项对 lib 集成测试不可见。

### D8 · `HostBusPort` 从 `ports_impl.rs` 抽出迁入 crate

包 `MessageBus`（crate 属物）；lib 的 server 组合根经垫片再导出，`assemble()` 本体
留 lib。

### D9 · `peer_ctx` 本体留 lib，作端口实现注入

它读 tauri managed state + lib `ports_impl::assemble()` 兜底（组合根唯一性，
crate_boundary_lock 断言⑤）；随迁会把它与 `assemble()` 拆散。

### D10 · 宿主上下文注册表（crate 内机制，单入口）

mdns adapter 按调用取 `WasmHostContext` 的零大小类型改走 **crate 内全局注册表**
（`crate::host_context_registry`，`OnceLock<Weak<WasmHostContext>>`）。装配点 =
`install_capability_domain_ports`（**单入口纪律**，2026-10-05 实测教训：装配链曾三份
拷贝导致顺序依赖假绿；本注册表禁止出现第二个装配点）。`None`（未装配 / 弱引用失效）=
无头，与 `AppContext::try_global() → None` 逐字一致（fail-safe 拒绝 / 跳过 /
`HEADLESS_UNAVAILABLE` 文案不变）。

### D11 · 锁与文档随迁移同步落地，不许滞后

`crate_boundary_lock.rs` 的 `SPLIT_CRATES` 登记表**上提 crate**（`bedcode-wasm-core`
自身也登记进表），lib 侧改单向引用（lib → crate）；`hot_path_logging_lock.rs`
`LOCKED_SITES` 路径改指 crate 内文件；新增反双份结构锁（`src-tauri/src/wasm_core/`
无实现文件 + lib.rs 垫片是 `pub use`）。

## 迁移清单（M1-M11，票 02-04 合并执行）

| # | 源（`bedcode-desktop/src-tauri/src/`） | 落点（`bedcode-desktop/packages/bedcode-wasm-core/src/`） | 行数 |
| --- | --- | --- | --- |
| M1 | `wasm_core/`（整目录） | 根（`crate::wasm_core::` → `crate::` 批量改写 546 处） | 54,394 |
| M2 | `db/` | `db/`（schema.sql 真源随迁） | 387 + schema(73) |
| M3 | `pty/` | `pty/` | 2,469 |
| M4 | `enums/` | `enums/` | 33 |
| M5/M6 | `system/{process,opener}.rs` | `system/{process,opener}.rs` | 22 + 378 |
| M7 | `system/config.rs`（AppConfig） | `system/config.rs` | 839 |
| M8 | `utils/auth/auth_center.rs` | `utils/auth/auth_center.rs` | 216 |
| M9 | `utils/session_gateway.rs` | `utils/session_gateway.rs` | 254 |
| M10 | `utils/auth/test_tokens.rs` | `utils/auth/test_tokens.rs` | 130 |
| M11 | `server/ports_impl.rs` 的 HostBusPort | `bus.rs` | ~53 |

**lib 垫片**：`wasm_core.rs` 删除 → `lib.rs` `pub use bedcode_wasm_core as wasm_core;`；
`db.rs` 删除 → `pub use bedcode_wasm_core::db;`；`system/process.rs` / `opener.rs` /
`config.rs`、`utils/auth.rs`、`utils.rs`（session_gateway）、`server/ports_impl.rs`
（HostBusPort）改 `pub use` 垫片。

**附加改造**（spec §2.3 之外实测所需）：`PluginDevWatcher::start` 增 `Weak<PluginHost>`
参（不再经 `AppContext::global()`）；`runtime_util` 及 4 处可见性 `pub(crate)` → `pub`；
`bindgen!` 路径改 `"../plugin-sdk-desktop/rust/wit/bedcode.wit"`（crate 相对）；测试
harness `host_harness::start_http_server`（`#[cfg(test)]`，§4.5——顺带成为第三方宿主
如何装配本 crate 的可执行范例）。

## 验证

- crate 根 `cargo check --lib` 绿；`cargo test --lib` **784 passed / 2 failed**
  （两个失败均既有基线：`test_session_task_domain_closed_loop` 9-30 起红 +
  `perf_p2_guest_ring_fetch_batch_curve` 墙钟阈值 flake）
- src-tauri `cargo check` / `cargo check --tests` 绿（垫片生效）；lib `cargo test --lib`
  76 passed / 0 failed
- cross-end-tests `cargo check --tests` 绿（`PluginHost::new` 第 5 参已同步）
- 无头测试语义逐字不变（`None` → 拒绝/跳过文案不变）
- 票 05/06（本 ADR 落档同日）完成契约收口 + 三全量验收后，以票面记录为准

## 防回接三形态（§5.1.4 平移）

| 形态 | 落点 |
| --- | --- |
| ① 宿主侧回查显性失败 | 垫片是**编译期**证据（引用断 = 编译错）；结构锁断言 `src-tauri/src/wasm_core/` 无实现文件；`PeerCtxProvider` 缺失 = `HEADLESS_UNAVAILABLE`（显性文案，无静默） |
| ② 旧产物实例化期点名 | ABI 未动（WIT 零改动），`stale_artifact_rebuild_hint` 行为不变 |
| ③ 退役词汇加载即抛 | 不涉及词汇表；权限位随迁不变 |

## 既有承诺不回归

- **不推翻 ADR 0022**：四类薄壳判定不变；`session_gateway` 迁入 crate 是**位置**变化，
  角色（零解析窄转发）不变。
- **不推翻 ADR 0035/0036**：机制内核 `bedcode-host-kit`、四个能力 crate、
  `host-database` 三域留内核——本 ADR 把「内核」整体搬出 bin crate，归属唯一性更强。
- **不触发 ABI bump**：`world plugin` 22 个 import 一个不动。

## Out of scope

1. 移动端任何改动（ADR 0018 契约独立；共享对象仍是 `bedcode-host-kit`）。
2. tauri 反转为零依赖（D2；独立立项）。
3. L2（interface 出 core / ABI bump）。
4. `bedcode-wasm-core` 发布为 crates.io 包（仓库内 path 依赖形态；发布是产品决策）。

## Comments

- 2026-10-06：规格成稿 + 实施。全部量测取自工作区实测（`dev` 分支），未凭记忆书写。
- 2026-10-06：初稿曾设想 4+ 个端口（DbPort / PtyPort / AppHandlePort / ConfigPort），
  实测后砍到 **1 个**：`app_handle` 早已注入 `PluginHost::new` 并存进
  `WasmHostContext`，`db` / `pty` 随迁后无需端口，mdns adapter 的全局取用改由 crate 内
  注册表承担。端口数与「复用成本」直接相关，这是本 ADR 最值得审的点。
- 2026-10-06：`system/constants.rs` / `error_boundary.rs` / `crypto.rs` / `identity.rs`
  已是纯 `pub use` 垫片（指向 bedcode-server-base / bedcode-crypto-engine）——垫片方案
  不是发明新形态，是复用既有纪律。
