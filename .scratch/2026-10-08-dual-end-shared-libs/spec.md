# 双端共享 lib 对齐：对等网络（peer-net）+ mDNS（discovery-engine）

> Date: 2026-10-08
> Status: **spec（未实施）**——用户指令「先写 spec 先不用实现」。
> 用户方向指令①：「对等网络应该双端使用同一个 lib，只有前端和文件选择、文件读写不一致，其他全部一致；mdns 也一样应该使用同一个 lib；如果当前的 lib 不满足双端同时引用应该修改至满足。」
> **用户方向指令②（2026-10-08 拍板）**：「对等网络（包括能力域等）应该使用 packages 下的；mDNS 应该解绑桌面端。」——① 对等网络及其能力域统一落根 `packages/`（现状已满足，本节做核验固化）；② mDNS 引擎从桌面 WIT 解绑（§3.1 D1 方向定案）。
> 全部事实取自 2026-10-08 工作区实测（行号 / 依赖 / 消费者可复现），不凭记忆。
> 前置文档：ADR 0018（移动契约独立）/ 0019（双端锁版）/ 0022（宿主插件边界）/ 0035（能力域 crate 化）/ 0037（wasm-core 整核抽出）；`.scratch/2026-10-07-mobile-wasm-core-refactor/`（移动端无业务化重构，票 01–16 已实施，票 13/15 在途）。

---

## 0. 一句话目标

- **对等网络**：核验并固化「双端同引根 `packages/peer-net` 单一 lib」的现状；确认差异只落在前端 UI / 文件选择（SAF picker）/ 文件读写（`SharedSafAccess` 抽象注入）三面，其余（identity / cert / transport / trust_store / discovery / transfer 线协议）逐字一致。
- **mDNS**：把 `packages/bedcode-discovery-engine` 的引擎机制**从桌面 WIT 解绑**，改造为**零 WIT 依赖的双端共享 lib**（移动端当前是 `mdns/engine.rs` + `host_impl/mdns.rs` 的同构复刻——消灭双份，改成同引同一 lib），并顺带统一两端 host-mdns 原语的**事件 topic 格式**差异。

---

## 1. 现状盘点（2026-10-08 工作区实测）

### 1.1 对等网络：已共享，差异面符合用户授权

- **双端同引**：桌面 `bedcode-desktop/src-tauri/Cargo.toml:112` 与移动端 `bedcode-mobile/src-tauri/Cargo.toml:107` 均引
  `bedcode-peer-net = { path = "../../packages/peer-net" }` ——**同一个 crate，零分叉**。
- **lib 内部结构**（`packages/peer-net/src/`）：cert / discovery / error / frame / identity / node / shared /
  transfer（含 batch::DirEntry）/ transport / trust_store，全部平台无关。
- **文件读写抽象（允许不一致面）**：`shared.rs::SharedSafAccess` trait（`list_dir` / `open_stream` 等）——
  - 移动端注入 `SafSharedAccess`（`bedcode-mobile/src-tauri/src/peer_net.rs:921,1285`，适配既有 `SafIo`）+ 接收落点 `MediaLanding`（:932，实现 `FileLanding`）；
  - 桌面**不注入**（`bedcode-server-peer-net/src/lib.rs:913` 构造 `PeerNetNodeConfig` 无 saf 字段 → 默认路径直读）。
- **文件选择（允许不一致面）**：不在 peer-net——移动端宿主 `plugin/saf_io.rs` + `saf_path.rs` + Kotlin `SafTransferPlugin`；桌面走平台对话框。
- **前端（允许不一致面）**：各 wasm-app / 插件前端（桌面 `wasm-apps/file-transfer/src/`、移动端 `plugins/file-transfer/src/`）。
- **发现守护归属**：peer-net **不自建守护**——浏览侧收敛到宿主注入（`MdnsPort` 端口层，`discovery.rs:8-10`）；**例外**：`spawn_peer_mdns_advertiser`（仅广告、无浏览）仍自建 `ServiceDaemon` 且**零生产调用方**（`discovery.rs:15-16` 自述）——死代码，违反「单守护」红线，列入清理。

### 1.1b 对等网络能力域根化确认（用户指令②核验）

根 `packages/` 已含对等网络及其能力域全族（2026-10-07 能力域迁根，ADR 0035），双端引用矩阵实测：

| 根 crate | 桌面引用 | 移动端引用 | 说明 |
| --- | --- | --- | --- |
| `peer-net`（线协议引擎）| ✓（Cargo.toml:112）| ✓（Cargo.toml:107）| **双端同引同一 lib**，零分叉 |
| `link-crypto`（链路加密）| ✓（:114）| ✓（:109）| 双端同引 |
| `bedcode-server-peer-net`（桌面对等网络引擎域 + host-peer WIT 绑定层）| ✓（:148）| ✗ | 桌面专用：host-peer 能力域绑定层带桌面 WIT，移动端契约独立（ADR 0018）不跟演；移动端 host-peer 原语自持 `host_impl/peer.rs`（482 行）| 
| `bedcode-server-base/core/http/websocket`（桌面服务端域）| ✓（:117,142,144,146）| ✗ | 桌面服务端（HTTP/WS 服务端、认证中心），移动端是消费端无服务端，ADR 0018 独立 |
| `bedcode-crypto-engine` | ✓（:140）| ✗ | 桌面认证中心密码学（ADR 0033），移动端不入场 |

**结论**：位置裁决已满足——对等网络（含能力域）统一在根 `packages/`；移动端不引 `server-*` 是契约独立设计而非位置问题。**开放问题 D6**：移动端 host-peer 引擎接入层（`peer_net.rs` 1862 行 + `host_impl/peer.rs` 482 行）与桌面 `bedcode-server-peer-net` 的引擎域（lib.rs + peer_engine_*）同构双份——是否也按 mDNS 同法「解绑 WIT、抽根共享」，待用户裁决（见 §7）。

### 1.2 mDNS：双端两套同构复刻（本次改造对象）

| 面 | 桌面 | 移动端 | 差异 |
| --- | --- | --- | --- |
| 引擎 lib | `packages/bedcode-discovery-engine`（engine 机制 + `bindgen!` + HostModule + inventory + Host trait 一体）| **无共享 lib**：`mdns/engine.rs`（42 行守护复刻）+ `host_impl/mdns.rs`（783 行原语复刻）| 双份持有 |
| 共享守护 | `engine.rs::DAEMON`（OnceLock）+ `disable_virtual_interfaces`（peer-net 提供）| `mdns/engine.rs::DAEMON`（OnceLock）+ **Android 多播锁** | 平台钩子不同 |
| 5 原语实现 | `engine.rs::browse/stop_browse/advertise/stop_advertise/is_advertising`（经 `DiscoveryPorts` 端口）| `host_impl/mdns.rs::mdns_browse/...`（直接读 `WasmPluginState`）| 同构复刻 |
| 双句柄表 + 属主仲裁 + purge + host service | engine.rs（BROWSERS/ADVERTISERS）| host_impl/mdns.rs（BROWSERS/ADVERTISERS）| 同构复刻 |
| **事件 topic 格式** | `<owner>::mdns:found` / `<owner>::mdns:lost`（`owned_topic(owner, MDNS_FOUND)`，`engine.rs:563`）| `mdns:found.<owner>` / `mdns:lost.<owner>`（`host_impl/mdns.rs:441-444` 直拼字符串）| **wire 形状不同** |
| 事件循环 | `ports.spawn` + `recv_async()`（宿主运行时）| `std::thread::spawn` + 阻塞 `recv()`（`host_impl/mdns.rs:101`）| 运行时范式不同 |
| 权限门 | `DiscoveryPorts::check_permission(plugin_id, api)` | 直接读 `WasmPluginState`（`check_permission(state)`，host_impl/mdns.rs:64）| 端口化差异 |
| 能力路由 forward | 有（`forward_mdns_*`，票 09 扩表，系统组件提供时转发）| **无** | 移动端无该机制 |
| 自播过滤 | `DiscoveryPorts::local_node_id()` | `crate::peer_net::current_node_id(app)` | 端口化差异 |
| 事件发布 | `DiscoveryPorts::publish(topic, payload)` | `state::try_get_plugin_manager().message_bus().publish(topic, "host", payload)` | 端口化差异 |
| 依赖 | mdns-sd 0.20 / serde / uuid / tokio / **bedcode_plugin_api（桌面 WIT 常量）** / wasmtime / wit-bindgen / host-kit / inventory / peer-net | mdns-sd 0.20（`bedcode-mobile/src-tauri/Cargo.toml:66`）| 移动端缺：**不能拉桌面 WIT（ADR 0037 C2）** |

### 1.3 「不满足双端同时引用」的卡点（discovery-engine 现状）

`packages/bedcode-discovery-engine/src/` 分层本身已为双端复用铺路（`ports.rs` 头注：**「本 crate 不依赖 tauri、不依赖宿主 bin crate，因此移动端将来要复用时只需换一个端口实现」**），但：

1. **`lib.rs` 绑定层**：`bindgen!`（桌面 WIT path）+ `HostModule`/`inventory::submit!`（host-kit 类型）+ `impl Host for WasmPluginState`（wasmtime 类型）——桌面 WIT 硬依赖，移动端整 crate 拉不动；
2. **`engine.rs` 两处 WIT 常量**：`bedcode_plugin_api::host::bus::owned_topic` + `host::mdns::{MDNS_FOUND, MDNS_LOST}`（定义在 `bedcode-desktop/packages/plugin-sdk-desktop/rust/src/host/{bus,mdns}.rs`）——纯字符串逻辑，却挂在桌面 SDK 上；
3. **topic 格式分叉**：移动端 SDK/宿主自持 `mdns:found.<owner>` 旧格式（`packages/plugin-sdk-mobile/rust/src/host/mdns.rs:3,11` + `abi.rs:31`），桌面已用 `<owner>::` 属主命名空间（wasm-core `host_api/bus.rs:60` 明确「legacy directed-topic form retired」）——**同一原语、两种 wire**。

---

## 2. 目标架构

```text
双端共享引擎层（零 WIT 依赖）
├── peer-net            —— 已共享（根 packages/peer-net，双端同引，零改动面）
│    差异仅：前端 UI（各端 wasm-app）/ 文件选择（移动端 SAF picker）/ 文件读写（SharedSafAccess 注入）
└── bedcode-mdns-engine —— 新建共享（或改造 discovery-engine，§3.1 D1）
      engine.rs：共享守护 + 双句柄表 + 属主仲裁 + 事件定向 + re-announce + purge + host service
      ports.rs：DiscoveryPorts 端口 trait（纯 Rust，无 wasmtime）
      types.rs：SERVICE_TYPE / topic 常量（自持，不再依赖桌面 SDK）
      ├── 桌面绑定壳（WIT 专属，桌面独有）：bindgen! + HostModule + inventory + Host impl → 转发共享引擎
      └── 移动端接线（移动独有）：mdns/engine.rs 删除；host_impl/mdns.rs 收窄为薄转发 + 移动端端口适配
            （Android 多播锁钩子 / 移动 bus / peer-net 节点 ID）
```

判据：宿主只留引擎机制与安全闸门（ADR 0022 四类薄壳①②）；host-mdns 原语语义、权限位 `network:mdns`、
订阅方隔离全部不变；**仅 wire 形状（topic 格式）按 D2 统一**。

---

## 3. 方案设计

### 3.1 共享 crate 形态（**用户指令②已定案：解绑桌面端；具体形态 D1**）

**方向已定**：`bedcode-discovery-engine` 必须**从桌面 WIT 解绑**——引擎机制零 WIT 依赖、双端同引；桌面 WIT 绑定层（bindgen! / HostModule / inventory / Host impl）留在桌面侧专属面。形态二选一：

| 选项 | 做法 | 优 | 劣 |
| --- | --- | --- | --- |
| **A（推荐）改造 discovery-engine + feature gate** | `bedcode-discovery-engine` 默认无 WIT：`engine.rs` 的 `owned_topic`/`MDNS_FOUND`/`MDNS_LOST` 自持；`lib.rs` 绑定层整体包 `#[cfg(feature = "desktop-host")]`（wasmtime/wit-bindgen/host-kit/inventory/bedcode-plugin-api 全部 optional）；桌面 Cargo.toml 加 `features = ["desktop-host"]`，移动端默认引 | 保持「同一个 lib」字面语义（一个 crate 双端同引）；不动治理锁（SPLIT_CRATES / dependency_direction_lock）；桌面零结构变化；「解绑」落在 feature 边界上，肉眼可验（移动端视角依赖面零 WIT）| optional 依赖 + cfg 包裹增加桌面绑定层维护复杂度；crate 描述/名称仍叫 discovery-engine |
| B 新建共享 crate | 抽 `packages/bedcode-mdns-engine`（零 WIT）；`discovery-engine` 收窄为桌面绑定壳（引共享 crate）| 依赖面物理上最干净（移动端引到的 crate 无 WIT 代码）| 新增 crate：`packages/.cargo/config.toml` SPLIT_CRATES、双端 Cargo.lock、check-target-size、依赖方向锁全动 |
| ~~C 绑定层移回 wasm-core~~ | 已否：破坏 ADR 0035 能力域 crate 化形态（host-mdns 能力域回内核）| — | — |

> **推荐 A**：改动面最小、保持「同一 lib」字面，解绑落 feature 边界；若实施中发现 cfg 包裹扩散（bindgen! 宏在 cfg 下行为异常等），退 B 不犹豫（票内记录原因）。

### 3.2 topic 格式统一（**开放裁决 D2**）

- 统一为**桌面终态** `<owner>::mdns:found|lost`（`owned_topic` 规则，属主命名空间仲裁所需）。
- 移动端迁移面（同批，缺一不可）：
  1. `packages/plugin-sdk-mobile/rust/src/host/mdns.rs:3,11` 注释 + `abi.rs:31` 注释（topic 契约描述）；
  2. `host_impl/mdns.rs::publish_dir_event` 的 topic 拼法；
  3. 移动端插件订阅字面量（`plugins/file-transfer/src/composables/deviceState.ts` 的订阅源——实施时核对事件订阅写法）；
  4. 移动端 SDK 事件常量若存在（`mdns_event_topic` 等）同步。
- 桌面零改动（已是终态）。属双端共有接口的移动端单向对齐（ADR 0018/0019 记录），无 ABI 变更（host-mdns WIT 函数签名不变，仅事件 topic 字符串）。

### 3.3 移动端接线（实施票 M3 主体）

1. **`mdns/engine.rs` 删除**——共享 lib 接管守护（`mdns/discovery.rs`、`mdns/advertiser.rs`、`peer_net.rs:982`、`host_impl/mdns.rs:21` 改引共享 lib 出口）；防回接锁：移动端不得再 `ServiceDaemon::new()`。
2. **`host_impl/mdns.rs` 收窄为薄转发**：5 原语 + purge 改为调共享引擎域函数（传移动端端口适配），与桌面 wasm-core `host_api/mdns.rs` 形态对齐；句柄表/事件循环/自播过滤逻辑全部进共享引擎（消灭双份）。
3. **移动端端口适配**（实现 `DiscoveryPorts`）：
   - `check_permission`：移动端 `WasmPluginState` 权限判定（语义不变）；
   - `local_node_id`：`crate::peer_net::current_node_id`（host_impl 现有逻辑搬入适配器）；
   - `publish`：`state::try_get_plugin_manager().message_bus().publish`（移动 bus，语义不变）；
   - `spawn`：移动端运行时任务派生（tauri async_runtime，替代 `std::thread::spawn`——顺带消灭线程阻塞范式）；
   - `forward_mdns_*`：移动端无能力路由 → 恒 `None`（契约保留，语义 = 无提供者走引擎，逐字不变）。
4. **平台钩子（D3）**：共享引擎 `init_daemon` 需可插拔平台初始化——选项 a) 端口加 `daemon_init_hook()` 方法（默认空）；b) 进程级 `set_init_hook(|| ...)` 装配（与 `install_ports` 同模式）。推荐 b（守护初始化与端口生命周期解耦；桌面传 `disable_virtual_interfaces`，移动端传 Android 多播锁）。若选 a 则桌面/移动端口各自实现即可。

### 3.4 桌面侧接线（实施票 M2）

- 若 D1=A：`bedcode-desktop/src-tauri/Cargo.toml` 加 `features = ["desktop-host"]`；`bedcode-wasm-core` 的 mdns 装配（`install_ports` 调用点 + `HostDiscoveryPorts` 适配器）不变；`engine.rs` 常量改自持后 wasm-core 侧零改动（原 `owned_topic` 引用面仅本 crate 内 + `bedcode-server-websocket`，后者属 SDK 常量消费方，**不动**——桌面 SDK 保留 `owned_topic`/`MDNS_*` 供 ws 域用，引擎自持副本用结构锁防漂移）。

### 3.5 peer-net 清理项（实施票 M4）

- `packages/peer-net/src/discovery.rs::spawn_peer_mdns_advertiser`：零生产调用方 + 自建守护 → **删除**（或收敛为经 `MdnsPort` 注入，二选一，推荐删除——广告面已有宿主自播/插件 advertise 覆盖）；同步删 `MdnsPort` 里若有的对应端口方法与导出。
- 双端 `MdnsPort` 注入形态核对：桌面 `bedcode-server-peer-net` 与移动端 `peer_net.rs` 的守护注入源在共享化后统一指向**同一个共享引擎**的 `shared_daemon()`（消灭「桌面 discovery-engine vs 移动 mdns::engine」两个注入源）。

---

## 4. 票划分（渐进：每票一件事、可验证、可回退）

> 票号独立于 `2026-10-07-mobile-wasm-core-refactor` 序列（该 spec 票 13/15 在途，避免撞号）；实施时可按需并入或顺延。

| 票 | 内容 | 门禁 |
| --- | --- | --- |
| **M1 · 共享引擎落地（D1 选型后）** | discovery-engine（或新 crate）去 WIT 化：`owned_topic`/`MDNS_*` 自持 + topic 常量 + 结构锁（引擎自持副本 vs 桌面 SDK 常量漂移锁）；绑定层 cfg/拆壳。**前置依赖：本 spec P2（discovery-engine 脱绑）已完成**——M1 直接消费 P2 的脱绑成果，只做 mDNS 专属接线（topic 统一预置 + 移动端引用验证）| 桌面 `cargo test` 全量绿（wasm-core 零回归）；`cargo metadata` 依赖面核对（移动端视角无 wasmtime/wit-bindgen） |
| **M2 · 桌面接线** | Cargo.toml feature；装配点核对；`HostDiscoveryPorts` 适配器零改动回归 | 桌面 mdns 针对性测试（engine 单测 + host-api mdns 测试）全绿 |
| **M3 · 移动端接线** | `mdns/engine.rs` 删；`host_impl/mdns.rs` 收窄薄转发 + 端口适配；`mdns/{discovery,advertiser}.rs` + `peer_net.rs` 改引；Android 多播锁钩子；topic 统一（§3.2 迁移面 4 处）| 移动端 `cargo test --lib mdns` + peer_net 回归；`component.rs` host-mdns 集成测试；防回接锁（移动端零 `ServiceDaemon::new()`）；**注：移动端全量测试被在途票 13/15 阻塞（egress 6 失败基线）——只跑针对性目标** |
| **M4 · peer-net 清理** | `spawn_peer_mdns_advertiser` 删除 + MdnsPort 注入源统一 | peer-net crate 测试 + 双端 peer 集成回归 |
| **M5 · 文档与锁收口** | AGENTS.md 路径基准（`packages/bedcode-mdns-engine` 或 discovery-engine 双端同引表述）、双端 code-map、ADR（修订 0035/0037 或新增「双端 mDNS 引擎共享」ADR，含 topic 统一记录）、CHANGELOG 双语、`.scratch` 本 spec 收口 | 全仓引用核对（rg `mdns:found\.` 归零）；双端文档与事实一致 |

---

## 5. 验证门禁（每票通用）

1. 改动落在宿主/引擎侧时过 §5.1 判据（B1–B6 零命中——本专项全是机制/引擎，无产品名词新增）；
2. Rust：对应 crate 根 `cargo test`（桌面全量、移动端针对性目标——全量被在途阻塞时写明原因）；
3. topic 格式迁移：`rg "mdns:found\.|mdns:lost\."` 移动端全仓归零（注释/字面量/SDK）；
4. 防回接锁：移动端 `ServiceDaemon::new()` 扫描归零；桌面 discovery-engine 的 WIT 依赖面不得回渗共享引擎（结构锁/feature 边界）；
5. 双端事件 wire 等价性：桌面 `owned_topic("plugin-a", MDNS_FOUND) == "plugin-a::mdns:found"` 与移动端统一后相同（单测互证）。

---

## 6. 约束与风险（不可自行放松，冲突按 AGENTS §0 优先级上报）

| # | 约束/风险 | 影响 |
| --- | --- | --- |
| C1 | 移动端不能拉桌面 WIT（ADR 0018 / 0037 C2）| 共享引擎必须零 WIT 依赖；桌面绑定层留在桌面侧（feature/拆壳）|
| C2 | 双端共有接口改 WIT 需双端同步评估（ADR 0019）；topic 格式是 wire 非 ABI | topic 统一属移动端向桌面终态的单向对齐，记录 ADR + 双端 code-map；**不影响 ABI 版本**（host-mdns 函数签名不变）|
| C3 | **并行会话在途（票 13/15，session 01a1191e 活跃）** | M3 涉及 `bedcode-mobile/src-tauri/`（host_impl/mdns.rs、mdns/*、peer_net.rs **不在**票 13/15 文件清单内，冲突面小）；但移动端全量编译/测试被在途中间态阻塞——M3 只跑针对性目标，实施前 `git status` 认领在途文件，不碰 |
| C4 | 桌面 `owned_topic`/`MDNS_*` 常量另有消费方（`bedcode-server-websocket/channel/plugin.rs:29,222,397`、wasm-core `host_api/bus.rs:4`）| 引擎自持副本 ≠ 删除 SDK 常量；双真源用结构锁防漂移（副本与 SDK 值逐字一致锁）|
| C5 | mdns-sd 0.20 双端同版（桌面 discovery-engine Cargo.toml、移动端 Cargo.toml:66）| 共享后版本约束收敛到共享 crate 一处，双端锁版税降低 |
| C6 | 移动端 `std::thread::spawn` 事件循环改为宿主运行时 spawn | 行为等价（阻塞 recv → recv_async），但线程/任务归属变化；移动端 bus publish 在任务上下文可达性需实测（`try_get_plugin_manager` 全局单例，应无碍）|

---

## 7. 开放裁决点（待用户拍板；每项给出推荐）

- **D1 · mDNS 解绑形态（方向已定：解绑桌面端）**：A 改造 discovery-engine + feature gate（**推荐**，一个 crate 双端同引，解绑落 feature 边界）/ B 新建 bedcode-mdns-engine + 桌面绑定壳。~~C（绑定层移回 wasm-core）已否~~。
- **D2 · topic 格式统一时机**：随 M3 同批（**推荐**，一次 ABI 窗内收口 wire）/ 单独出票后置。
- **D3 · 平台钩子形态**：b 进程级 `set_init_hook`（**推荐**）/ a 端口方法 `daemon_init_hook`。
- **D4 · peer-net `spawn_peer_mdns_advertiser`**：删除（**推荐**，零消费者）/ 收敛为注入。
- **D5 · 实施窗口**：等票 13/15 提交落盘后实施 M1–M5（**推荐**）；或 M1/M2（桌面面，不受在途影响）先行。
- **D6 · 移动端 host-peer 引擎接入层是否也解绑共享（新开放）**：`bedcode-mobile/src-tauri/src/peer_net.rs`（1862 行，节点身份/守护装配/SAF 注入）+ `host_impl/peer.rs`（482 行，移动 host-peer 原语）与桌面 `bedcode-server-peer-net` 引擎域（lib.rs + peer_engine_*，共 4,882 行含 WIT 绑定层）**同构双份**。选项：a) 按 mDNS 同法——引擎部分（无 WIT）抽根共享、双端各留 WIT 绑定/接入（**推荐**，与用户指令①「对等网络应该使用同一个 lib」的完整兑现一致）；b) 维持现状（移动端自持，契约独立 ADR 0018）；c) 仅核验不动。**待用户拍板，不并入本 spec 的 M1–M5 执行面**。

---

## 8. Out of scope

- 对等网络业务编排下沉（票 06–10 已完成面不重做；移动端拉取编排 B2 遗留见 `audit.md` §2.4 票 D，本 spec 不涉及）。
- 桌面 `bedcode-server-websocket` / wasm-core 对 `owned_topic`/`MDNS_*` 的既有消费面（保持引用桌面 SDK 常量，不动）。
- 移动端插件机制复用 wasm-core（`2026-10-07-mobile-wasm-core-refactor` 票 17–19 范畴，本 spec 不涉及）。
- 「插件 → wasm-app」命名对齐（另见 `.scratch/2026-10-08-mobile-wasm-app-rename/audit.md`）。
