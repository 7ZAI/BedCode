# ADR 0048：对等网络能力域集中化——移动端接入共享 peer 引擎 + mDNS 复用

- 状态：**accepted**（2026-10-10 拍板，票 01–05 实施完成；实施记录见文末）
- 日期：2026-10-10
- spec：`.scratch/2026-10-10-peer-capability-consolidation/spec.md`（票序与门禁真源；票 03 记录 `ticket-03-crate-decoupling-and-reconciliation.md`；票 04 执行计划与实施记录 `ticket-04-mobile-adoption.md`）
- 相关：ADR 0022（宿主/插件边界裁决）、ADR 0035（能力域 crate 化 + 端口抽象）、ADR 0042（双端 mDNS 引擎共享）、ADR 0045（单一 crate 双形态）、ADR 0047（形态命名中立化，common/bare 收口于 2026-10-10）、ADR 0018（移动契约独立）、ADR 0019（双端锁版）
- 用户指令（原文摘录）：「按建议执行 按rust层的代码 架构 将基础服务能力化集中在一起 宿主通过装配这些能力 而wasm app通过调用编排使用这些能力完成业务 基于这一准则来进行 而目前已经封装有mdns能力面了 希望能够尽量复用」

## 背景

1. **两端同构双份引擎**（实测 2026-10-10）：桌面 peer 引擎控制面已能力域 crate 化（`packages/bedcode-server-peer-net` ≈4k 行，端口化、零 `AppHandle`），移动宿主仍自持 ≈4.4k 行（`peer_net` 1862 / `peer_transfer` 1043 / `peer_receive` 600 / `peer_remote` 486 / `peer_events` 277 / `peer_migration` 146），直接持 `AppHandle` + Tauri managed state——同一能力的第二份真源，历史漂移风险（桌面 19 原语绑定 vs 移动 13）。
2. **能力域 crate 移动可引形态本来就位**：`bedcode-server-peer-net` 默认形态 = 纯引擎（`default = []`，零 WIT 依赖，仅保留 base 契约的 BusMessage 类型），`desktop-host`（ADR 0047 后 = `common-host`）才装 WIT 绑定层；引擎实际消费的宿主端口只有 5 个（`ServerPorts` 12 端口里的子集）。
3. **mDNS 已有共享引擎**（ADR 0042）：`bedcode-discovery-engine` 双端同引，广播经 `MdnsAdvertiserPort`；移动宿主另有 `mdns/advertiser.rs` 自建广播实例 + `mdns_start/stop_advertise` 命令——**零活消费者**（App.vue 全注释），第二份广播实现与死面并存。

## 决策

### D1 · 复用 `packages/bedcode-server-peer-net` 为双端唯一 peer 能力域 crate

不新建 crate（再造双份引擎与双份锁）；crate 默认形态即纯引擎（脱绑 P4 产物），移动端直接引用。

### D2 · 引擎端口收窄为 `PeerEnginePorts`（5 端口）

新增 `engine_ports::PeerEnginePorts { bus, event_sink, paths, system_info, mdns }`，trait 词汇全部复用 `bedcode-server-base::ports`（不新造第二套）；类型上可验 peer 引擎引用不到无关端口。桌面适配层从 `ServerPorts` 取 5 端口子集装配（字段名逐字保留，引擎内部调用点零改动）。

### D3 · mDNS 一律复用 `bedcode-discovery-engine`

crate 经 `MdnsPort` 接宿主共享守护（依赖方向锁保持）；移动 `MdnsPort` 实现即 discovery-engine 引擎三函数（`shared_daemon` / `register_host_service` / `stop_host_service`）——零新增 mDNS 代码。移动 `mdns/advertiser.rs` 死面 + 广播命令 + 前端死 composable **退役**（browse 薄壳保留：活消费者 = terminal-session 宿主页）。

### D4 · 平台缝按端注入（crate 只收参数）

`PeerPlatformSeam { shared_saf_access, file_landing, builtin_shared_dirs }` + `PathsPort` 下载目录语义按端实现：SAF 共享访问（移动 `android_saf_bridge::SafSharedAccess`）、落点提升（移动 `MediaLanding`）、内置共享目录（移动 app 私有下载目录 `"下载"`）、下载目录（桌面 = 系统 Downloads/BedCode 子目录；移动 = app 私有下载 + MediaStore 尽力提升）。桌面传 `platform: None`（行为逐字不变）。

### D5 · 移动专属宿主编排留宿主适配层

插件 id（`FILE_TRANSFER_PLUGIN_ID`）、生命周期归属（`sync_node_with_plugin_state` / `plugin_transfer_activated`）、发现刷新订阅（`peer:discovery-refresh`）、peer 迁移（旧 JSON 数据迁插件存储键）——携带产品语义（B1/B6），不生进能力域。

### D6 · 移动 WIT 绑定层留 wasm-core `bare-host`（不搬进 crate）

绑定层消费 `WasmPluginState`，搬进 crate 会与 wasm-core 形成 Cargo 环路（`common-host` 已反向依赖）。契约面零变化：host-peer 原语集、权限位 `peer`、wire 形状、双端 ABI 均不动（纯机制搬迁）。

## 代价与风险

| 项 | 说明 | 处置 |
| --- | --- | --- |
| 两端引擎「同构双份」历史漂移（19 vs 13 原语、SAF/landing/内置目录差异） | 票 03 逐函数对表；行为差异按 D4 注入端口/参数，不改 crate 语义 | 对表记录 `ticket-03` |
| 迁移期双份共存 | 票 04 同票删除 + 防回接锁，不留过渡态入库 | 锁 `peer_engine_consolidation_lock.rs` |
| 移动适配层就绪判据不同源（crate 等 `server_base::ports::get()`，移动未装） | `BusPort::is_ready` 默认 `true`，各端自报就绪；移动发现刷新订阅自持 | `bedcode-server-base::ports::BusPort` 增量默认方法 |
| 下载目录异步解析 vs 端口方法同步 | 适配层 `OnceLock` 缓存 + 首次未就绪返回 Err（引擎按既有错误语义处置） | ticket-04 已知坑 1 |
| 广播死面误删 browse | browse 薄壳有活消费者，锁正反两面判据钉住 | 锁 `retired_mobile_mdns_advertise_lock.rs` |

## 验证门禁（2026-10-10 实跑）

1. crate `packages/bedcode-server-peer-net` `cargo test` **56/0**（票 03 51/0 → 票 04 54/0 → 本收口 56/0）；
2. 移动 `bedcode-mobile/src-tauri` `cargo test` 全量 **0 failed**（27 测试二进制；票 04 记录 314 passed，本收口含新锁 2 例）；桌面 `src-tauri` `cargo check` 绿（5 warning 存量基线）；
3. 新锁变异自检：`peer_engine_consolidation_lock` 1/1、`retired_mobile_mdns_advertise_lock` 1/1（注入 → 红 → 还原核 sha256）；
4. 跨端协议面零变化（ABI / WIT / wire 不动）——`cross-end-tests` 与双端前端测试按 AGENTS §10 在任务收尾统一回归；
5. 手工项：真机 SAF / MediaStore / 多播场景未跑（原因：无真机环境，行为差异经端口注入无新增平台代码）。

## 实施记录

- **票 01（2026-10-10）**：现状实测 + 两端对照 + 逐函数对表（spec §2、ticket-03）。
- **票 02**：`engine_ports.rs`（`PeerEnginePorts` + 桌面装配改）——crate check 绿、行为零变化（spec §6）。
- **票 03**：crate 移动化接缝——`PeerPlatformSeam` + `start_locked` 接 SAF/landing/内置条目 + `decorate_shared_store` + R2（`pull-started` 载荷统一 `files[]+totalSize`）+ R4（`node-stopped` 事件进 crate）+ 落点/设备名归属端侧（桌面 `ports_impl.rs`：`device_name_fallback` + 下载目录 `BedCode/` 子目录）。
- **票 04**：移动接线——`Cargo.toml` 引能力域（默认纯引擎）+ `peer_net.rs` 重写为适配层（5 端口 + 平台缝 + `peer_ctx` 装配 + 生命周期薄壳）+ `peer_transfer/receive/remote.rs` 委托层（R1 删设置落盘、R3 删并发编排）+ 删 `peer_events.rs` + `host_ports.rs` 聚合投影改调能力域 + 新锁 `peer_engine_consolidation_lock.rs`（变异自检 1/1）+ 3 处陈旧路径锁修复。
- **票 05（本次收口补做）**：mDNS 广播死面退役——删 `mdns/advertiser.rs` / `AdvertiseConfig` / 广播命令 / `lib.rs` 装配与命令注册 / 前端死 composable `useMdnsAdvertiser.ts`；新锁 `retired_mobile_mdns_advertise_lock.rs`（退役面缺席 + browse 薄壳在场 + 共享守护复用锚点，变异自检 1/1）。
- **提交（dev，2026-10-11）**：`213702188`（crate 引擎收窄 + R2/R4 载荷）→ `7d559c255`（桌面窄端口装配）→ `1dbd17686`（移动适配层委托 + 防回接锁）→ `9b0e2353a`（移动 Cargo.toml 能力域依赖）→ `d4e001860`（host_ports 投影改调能力域）→ `fad5eb968`（移动 code-map 对等网络节改写）→ `0067894a8`（mDNS 广播死面退役 + 归零锁）。
- **CHANGELOG 双语 / 双端 code-map 锁索引**：本 ADR 落档同批收口。
