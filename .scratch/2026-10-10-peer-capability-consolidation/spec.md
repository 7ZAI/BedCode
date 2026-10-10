# 对等网络能力域集中化：移动端接入共享 peer 引擎 + mDNS 复用

> Status: **全部完成**（票 01–04 批次 1–4 落地于 2026-10-10；票 04 批次 5 / 票 05 / 票 06 收口于 2026-10-11）
> Date: 2026-10-10
> 实施记录：票 02 spec §6；票 03 `ticket-03-crate-decoupling-and-reconciliation.md`；票 04 `ticket-04-mobile-adoption.md`（批次 5 见该文件 §4 更新）；票 05/06 见 ADR 0048 实施记录。提交序列：`213702188` → `7d559c255` → `1dbd17686` → `9b0e2353a` → `d4e001860` → `fad5eb968` → `0067894a8` → `975651bb2` → `9a65a4c7c` → `340fa8797`
> 前置：`.scratch/2026-10-08-dual-end-shared-libs/spec.md` §7 **D6（本 spec 为其拍板执行：选 a）**；ADR 0022（宿主/插件边界）/ 0035（能力域 crate 化 + 端口抽象）/ 0042（双端 mDNS 引擎共享）/ 0045（单一 crate 双形态）/ 0018（移动契约独立）/ 0019（双端锁版）
> 用户指令（原文）：「按建议执行 按rust层的代码 架构 将基础服务能力化集中在一起 宿主通过装配这些能力 而wasm app通过调用编排使用这些能力完成业务 基于这一准则来进行 而目前已经封装有mdns能力面了 希望能够尽量复用」

---

## 0. 一句话目标

把移动端宿主持有的对等网络**引擎控制面**（`peer_net/peer_transfer/peer_receive/peer_remote/peer_events` ≈ 4.4k 行）集中进**双端唯一 peer 能力域 crate `packages/bedcode-server-peer-net`**（引擎形态已双端可引：`default = []` 纯引擎，`desktop-host` 才装桌面 WIT 绑定）；两端各留绑定/接入层，宿主只做**装配**（端口实现 + 注入 + 生命周期钩子），wasm-app 编排面零变化（host-peer 13 原语 + file-transfer 插件）。mDNS 一律复用既有能力面（`bedcode-discovery-engine` 共享守护 + `MdnsPort` 端口），不新造第二份。

## 1. 架构准则（用户指令的可执行口径）

```text
packages/bedcode-*（能力域 crate：纯引擎 + 端口抽象，零 WIT / 零 tauri 绑定）
        ↑ 只暴露引擎原语
各端宿主（装配：实现端口 + install/注入 + 生命周期钩子；零业务语义）
        ↑ host-* 原语（WIT 绑定层，各端各留：桌面在 crate desktop-host feature / 移动在 wasm-core mobile-host）
wasm app（调用编排：业务真源 / 状态机 / UI）
```

- 新增/迁移判断：**「离宿主无法实现」的引擎与安全闸门进能力域 crate；平台缝（SAF / 落点提升 / 设备名 / 目录）经端口注入；产品语义（插件 id / 生命周期归属 / 业务状态）一律留插件或宿主薄壳。**
- 契约面零变化：host-peer 原语集、权限位 `peer`、wire 形状、双端 ABI（desktop 37 / mobile 20）**均不动**（纯机制搬迁）。

## 2. 现状实测（2026-10-10，本会话核验）

### 2.1 两端对照

| 面 | 桌面 | 移动 |
| --- | --- | --- |
| peer 引擎控制面 | `packages/bedcode-server-peer-net`：lib.rs 1837 + `peer_engine_{transfer,receive,remote}` 1773 + `source_collect` 227，**端口化、零 `AppHandle`** | `bedcode-mobile/src-tauri/src/peer_*.rs` ≈ 4.4k 行（net 1862 / transfer 1043 / receive 600 / remote 486 / events 277 / migration 146），直接持 `AppHandle` + Tauri managed state |
| WIT 绑定/接入 | `plugin_binding`（crate 内，`desktop-host` feature）+ 宿主 `plugin/peer.rs` 174 行端口实现 | wasm-core `mobile-host` 的 `manager/runtime/mobile/host_impl/peer.rs`（绑定层已在共享 crate）+ 宿主 `plugin/host_ports.rs`（`HostEnginePorts` → `crate::peer_*`） |
| 宿主壳 | `server/peer_net_cmds.rs` 71 行（PeerCtx 装配 + 5 命令）；wasm-core activation `release_node_for` 消费引擎面 | lib.rs 装配 + `plugin/manager.rs` 生命周期钩子（ensure/stop）+ `plugin/host_ports.rs` |
| mDNS | 引擎 `bedcode-discovery-engine`（双端同引，M1–M5 已完成）；广播经 `MdnsAdvertiserPort`；无宿主 browse | 同引擎（`mdns/engine.rs` 已删、host_impl 薄转发、topic 统一）；宿主另有 `mdns/discovery.rs` browse（活消费者=terminal-session 宿主页经 `mobileApi`）+ advertise（**零活消费者**，App.vue 全注释） |

### 2.2 crate 侧就绪度（实测）

- `packages/bedcode-server-peer-net/Cargo.toml`：`default = []`（纯引擎，**任何宿主可直接引用：移动端 / 无头 / 第三方**）；`desktop-host` 门控 WIT 绑定层——移动可引形态**本来就位**（脱绑 P4 的产物）。
- 引擎实际消费的宿主端口只有 5 个（`lib.rs` 全量 grep）：`bus`（publish）/`event_sink`（emit）/`paths`（app_data_dir + download_dir）/`system_info`（device_name）/`mdns`（shared_daemon + register/stop_host_service）——即 `ServerPorts` 12 个端口里的 5 个子集。
- 移动 `AppError` = `bedcode_wasm_core_mobile::error::AppError` = server-base 共享错误类型（批次 03 超集化）；与 crate 的 `bedcode_server_base::error::Result` **同源**，错误互操作零成本。
- mDNS 复用路径已存在：crate 经 `MdnsPort` 接宿主共享守护，不引 discovery-engine（依赖方向锁保持）；移动端口实现即 discovery-engine 三函数，**零新增 mDNS 代码**。

### 2.3 移动侧未对齐面（票 03 清单）

- **端口装配**：移动只有 5 端口素材，无法（也不应）装配 `ServerPorts` 12 端口全量 → 收窄为 `PeerEnginePorts`（票 02）。
- **平台缝注入**：SAF 共享访问（`SharedSafAccess`，移动 `android_saf_bridge::SafSharedAccess`）、落点提升（`FileLanding`，移动 `MediaLanding`）、内置共享目录（移动 app 私有下载目录条目 `"下载"`）、下载目录语义（桌面 = `系统 Downloads/BedCode`；移动 = app 私有下载 + MediaStore 尽力提升）——按端注入，不写进 crate。
- **移动专属宿主编排（留宿主）**：`sync_node_with_plugin_state` / `FILE_TRANSFER_PLUGIN_ID`（产品 id）/ `plugin_transfer_activated` / discovery-refresh 静态订阅（`peer:discovery-refresh`）/ `peer_migration`（旧数据迁插件存储键）。
- **能力面差异核对**：19（桌面）vs 13（移动）原语绑定分居两端（不动）；引擎函数逐一对表（票 03 §）。

## 3. 决策

| # | 决策 | 依据 |
| --- | --- | --- |
| D1 | **复用 `packages/bedcode-server-peer-net` 作为双端唯一 peer 能力域 crate**，不新建 crate | 「集中在 packages/」；crate 默认形态即纯引擎（脱绑 P4）；新建 crate 会再造双份引擎与双份锁 |
| D2 | 引擎端口**收窄为 `PeerEnginePorts`（5 端口）**，词汇复用 `bedcode-server-base::ports` 的既有 trait（不新造第二套） | 能力只吃自己的端口（对齐 discovery-engine `DiscoveryPorts` 先例）；移动无需实现 12 端口全量；类型上可验 peer 引用不到无关端口 |
| D3 | mDNS **一律复用** `bedcode-discovery-engine`：crate 经 `MdnsPort`（现状），移动端口实现即引擎三函数；移动 `mdns/` 宿主壳只保留 browse（含前端投影），**advertise 死面退役** | 用户指令「mdns 能力面尽量复用」；零活消费者的死面不留 |
| D4 | 平台缝（SAF / FileLanding / 内置共享目录 / 下载目录）**按端注入**，crate 只收参数 | 差异面唯一落点 = 各端宿主适配层（不做配置开关分叉） |
| D5 | 移动专属编排（插件 id / 生命周期归属 / 发现刷新 / 迁移）**留宿主适配层** | 携带产品语义（B1/B6），不生进能力域 |
| D6 | 移动 WIT 绑定层**留 wasm-core `mobile-host`**（不搬进 crate） | 绑定层用 `WasmPluginState`，搬进 crate 会与 wasm-core 形成 Cargo 环路（desktop-host 已反向依赖） |

## 4. 票划分

| 票 | 内容 | 门禁 | 状态 |
| --- | --- | --- | --- |
| **01** | 核验与端口/差异清单（本 spec §2 + 逐函数对表） | 事实可复现（命令/行号入档） | ✅ 本会话 |
| **02** | crate 端口收窄：`PeerEnginePorts`（5 端口）+ 桌面适配层改装配 | crate `cargo check` + 桌面临界面编译；行为零变化 | ✅ 本会话（记录见 §6） |
| **03** | crate 移动化接缝：SAF / FileLanding / 内置共享目录 / 下载目录参数化 + 移动专属能力面核对补齐 | crate 单测全绿 + 桌面零回归 | ✅ 完成（端口收窄 + 落点/设备名归属 + R2/R4 载荷与事件 + 平台缝 `PeerPlatformSeam`；crate 51/0、桌面 check 绿——见 `ticket-03-crate-decoupling-and-reconciliation.md`） |
| **04** | 移动端接线：宿主适配层（PeerEnginePorts 实现 + 平台缝注入 + 生命周期钩子保留）+ `host_ports.rs` 改道 + 删移动 `peer_*` 四模块 + 防回接锁 + 移动全量测试 | 移动 `cargo test` 全量 + 防回接锁变异自检 | ✅ 2026-10-11（批次 1–4 落地 2026-10-10：移动 314/0、crate 54/0、锁变异 1/1；批次 5 收口：移动 code-map 对等网络节改写 `fad5eb968`、锁索引登记 `340fa8797`、CHANGELOG 双语 `975651bb2`/`9a65a4c7c`、ADR 0048） |
| **05** | mDNS 收口：移动 advertise 死面退役 + browse 薄壳复核 + 移动端 `ServiceDaemon::new()`/广播命令归零锁 | 移动全量 + 锁变异 | ✅ 2026-10-11（`0067894a8`：删 advertiser/广播命令/前端死 composable + 新锁变异 1/1；移动全量 27 二进制 0 failed） |
| **06** | 文档/锁/CHANGELOG：双端 code-map、锁索引、ADR（D6 执行记录）、CHANGELOG 双语 | 全仓引用核对 + eslint 0 error | ✅ 2026-10-11（ADR 0048 + CHANGELOG 双语 + 锁索引登记；待续项：收尾全量回归见 spec §5 门禁） |

## 5. 门禁（通用）

1. 每票实际运行并贴结果；Rust 过滤按各自 crate 根（`packages/bedcode-server-peer-net` 自带锁文件；`bedcode-desktop/src-tauri`；`bedcode-mobile/src-tauri`）。
2. 收尾：双端 `cargo test` 全量 + `cross-end-tests`（跨端互连，协议面不动也要跑）+ 两端 `pnpm run test:run` + 根 `pnpm exec eslint .` 0 error。
3. 防回接锁（票 04/05）：旧读路径显性移除 + 变异自检（注入 → 红 → 还原 → 核 sha256）。
4. 未纳入自动化的手工项（真机 SAF/MediaStore/多播场景）逐项写「跑了/没跑 + 原因」。

## 6. 票 02 实施记录（2026-10-10）

- 新增 `packages/bedcode-server-peer-net/src/engine_ports.rs`：`PeerEnginePorts { bus, event_sink, paths, system_info, mdns }`（`Arc<dyn Trait>` 字段；trait 全部复用 `bedcode-server-base::ports`）。
- `PeerCtx.ports` 类型 `Arc<ServerPorts>` → `Arc<PeerEnginePorts>`（字段名逐字保留，引擎内部 `ctx.ports.X` 调用点零改动）。
- 桌面适配层 `server/peer_net_cmds.rs`：从 `ServerPorts` 取 5 个端口子集装配 `PeerEnginePorts`（其余 7 端口不属于 peer 引擎）。
- 行为零变化：端口实现对象同一批（同一 `Arc`）；字段访问路径与语义不变。

## 7. 风险与回退

| 风险 | 吸收 |
| --- | --- |
| 两端引擎「同构双份」的历史漂移（19 vs 13 原语、SAF/landing/内置目录） | 票 03 逐函数对表；行为差异按 D4 注入端口/参数，不改 crate 语义 |
| 迁移期双份共存（移动旧模块未删期间） | 票 04 同票删除 + 防回接锁，不留过渡态入库 |
| 磁盘 / 编译成本（wasmtime 面） | 各票按 crate 粒度检查；大编译前 `df -h`，超限先清理（AGENTS §3） |
| `ServerPorts` 其他消费方误用新端口类型 | 类型系统拦（`Arc<PeerEnginePorts>` 不实现 `ServerPorts`）；桌面适配层单点转换 |
