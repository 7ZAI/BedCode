# 票 03 · 共享 crate 松耦合 + 两端引擎差异对账

> Status: **完成**——已做：端口面收窄（票 02）、落点/设备名归属移出能力域、R2/R4 载荷与事件统一、平台缝端口（`PeerPlatformSeam`：SAF / landing / 内置条目）。R1/R3 由票 04 随移动接线执行（决策已落档 `ticket-04-mobile-adoption.md` §0）；R5/R6 为移动适配层组合，不属 crate 改动。
> Date: 2026-10-10（R2/R4/平台缝当日落地）
> 输入：code-explorer 逐函数对照（本节 §3 即其结论的裁决表）；事实均带 `文件:行`。

---

## 1. 结论：crate 的桌面耦合面很小

`packages/bedcode-server-peer-net` **已是脱绑形态**（`default = []` 纯引擎，`desktop-host` 才装 WIT 绑定，Cargo.toml 注释明写「任何宿主可直接引用：移动端 / 无头 / 第三方」）。桌面耦合只剩：

| # | 耦合 | 处置 |
| --- | --- | --- |
| C1 | `PeerCtx.ports: Arc<ServerPorts>`（12 端口桌面聚合） | ✅ 票 02：收窄为 `PeerEnginePorts`（5 端口，词汇复用 server-base） |
| C2 | `resolve_download_dir` 硬编码 `.join("BedCode")` | ✅ 本票：子目录归各端端口实现（桌面 impl 现拼 `BedCode`，语义零变化） |
| C3 | `resolve_device_name` 硬编码兜底名 `"BedCode Desktop"` | ✅ 本票：改由 `SystemInfoPort::device_name_fallback()`（新 defaulted 方法，桌面覆写保持逐字一致） |
| C4 | `plugin_binding`（WIT 绑定层） | 不动：已 `desktop-host` 门控；移动绑定层留 wasm-core `mobile-host`（D6，避免 Cargo 环路） |

## 2. 本票已改（两编译门已过）

- `packages/bedcode-server-peer-net/src/engine_ports.rs`（新）：`PeerEnginePorts { bus, event_sink, paths, system_info, mdns }`。
- `packages/bedcode-server-peer-net/src/lib.rs`：`PeerCtx.ports` 类型切换；`resolve_download_dir` / `resolve_device_name` 去平台硬编码。
- `packages/bedcode-server-base/src/ports.rs`：`PathsPort::download_dir` 语义澄清（各端自定落点根目录）+ `SystemInfoPort::device_name_fallback()`（defaulted，不破坏既有实现）。
- `bedcode-desktop/src-tauri/src/server/peer_net_cmds.rs`：从 `ServerPorts` 取 5 端口子集装配 `PeerEnginePorts`。
- `bedcode-desktop/src-tauri/src/server/ports_impl.rs`：落点拼 `BedCode`（行为不变）、设备名兜底名覆写（行为不变）。

**门禁（实跑）**：
- `packages/bedcode-server-peer-net`：`cargo check` 绿（1m23s，3 存量 warning）；`cargo test` **49 passed / 0 failed**。
- `bedcode-desktop/src-tauri`：`cargo check` 绿（31.85s，5 存量 warning）。

## 3. 两端引擎差异对账表 + 处置裁决

### 3.1 可在**移动宿主适配层组合**解决（crate 语义不动）——不需要改 crate

| 差异 | 移动现状（B=`bedcode-mobile/src-tauri/src`） | crate 现状（A=`packages/bedcode-server-peer-net/src`） | 适配层解法 |
| --- | --- | --- | --- |
| 发送暂停/恢复双向兜底 | `pause_peer_transfer` 无命中回落 `pause_peer_receiving`（B/peer_transfer.rs:263,286）；`resume` 同（:297,319） | 无兜底（A/peer_engine_transfer.rs:211,241） | 移动 `HostEnginePorts` 实现里组合：crate `pause_transfer` → false 时 crate `pause_peer_receiving` |
| 加密默认值 | `encrypt=None` 读引擎设置 `encryption_enabled`（B/peer_transfer.rs:411-418，写入侧已随票 10 退役 ⇒ 实际恒 false） | `unwrap_or(false)`（A/peer_engine_transfer.rs:363） | 语义等价，直接用 crate |
| 接收 pending 表形状 | 仅 `oneshot` 通道（B/peer_receive.rs:129）⇒ `active-transfers` 的 totalBytes 恒 0 | 存 `PendingOffer{node_id,total_size,ts_ms,reply}`（A/peer_engine_receive.rs:84）⇒ totalBytes 有值 | 用 crate（投影变准，属改善） |
| 设备名解析策略 | 轮询等 SystemInfo 15s + 兜底 `"BedCode Mobile"`（B/peer_net.rs:1101） | 同步读端口（A/lib.rs:1573） | 移动适配层在调 crate 启节点**前**先预热/等待 SystemInfo（保留轮询语义） |
| `*_for_plugin` 包装层 | 无（host_ports 直调内层函数） | 有 13 个 wrapper（A/lib.rs:614-688） | 移动不需要，不调用即可 |
| `publish_bus_only` 独立快照通道 | 无（`emit_json` 内联 topic 判断） | 有（A/lib.rs:1250） | 用 crate（等价） |

### 3.2 需**用户拍板**的行为差异（改了会动移动端可见行为 / 桌面端插件载荷）

| # | 差异 | 两端现状 | 建议 | 影响面 |
| --- | --- | --- | --- | --- |
| **R1** | 接收设置落盘 | 移动把策略/落点写 `transfer_settings.json`（B/peer_receive.rs:175,193,211）；crate 无落盘（纯内存，真源=插件经原语推送） | **对齐桌面**：删宿主落盘（业务真源在插件，宿主副本只服务引擎闸门）；代价=插件激活前策略为默认 `ask` | 移动行为变更（需真机复验）；删一个宿主侧第二真源 |
| **R2** | `pull-started` 载荷形状 | 桌面发 `{relPath,size}`（A/lib.rs:1368）；移动发 `{files:[{path,size}],totalSize}`（B/peer_events.rs:130）；**共享核 `bedcode-file-transfer-core` 读的是 `files[]+totalSize`（core/transfer.rs:369-400，tests :674）** | **统一为 `files[]+totalSize`**（即桌面侧对齐共享核；现状桌面可能已在途不一致） | 桌面插件可见载荷变更（跨端契约属共享核）；须与 file-transfer-shared-core 在途改动协调 |
| **R3** | 拉取编排护栏 | 移动 `PULL_FILES_CAP=512` + 并发信号量（读无人写的 `concurrency`，B/peer_remote.rs:29,373-414）；crate 串行无上限（A/peer_engine_remote.rs:280,353） | 二选一：① 对齐 crate（丢护栏，与桌面一致）；② 参数化（crate 收 cap/并发参数，两端各自传） | ①=行为变更（无上限）；②=能力域多一组编排参数 |
| **R4** | 节点下线事件 | 移动在接收事件通道关闭时多发 `node-stopped`（B/peer_receive.rs:355）；crate 不发（A/peer_engine_receive.rs:234 只清 pending）；**共享核明确处理该 kind（core/transfer.rs:573「收到 node-stopped ⇒ 在飞批全部中止」）** | **加进 crate**（引擎事实：通道关闭=节点下线）；对桌面=补齐共享核契约已有的行为 | 桌面多一条总线事件（共享核已能归约）；移动行为不变 |

## 4. 票 04（移动端接线）落点预览（供对齐确认）

- 新增移动适配层：`PeerEnginePorts` 实现（bus/event_sink/paths/system_info/mdns）+ `MdnsPort` 实现（复用 `bedcode-discovery-engine::engine` 三函数，**零新 mDNS 代码**）+ `PeerCtx` 装配（对应桌面 `peer_net_cmds::peer_ctx`）。
- 保留在移动宿主：SAF 桥（`SafSharedAccess`）、`MediaLanding`、内置共享目录 `"下载"`、`peer_pick_files`/`relay_copy_upload_source`、`plugin_transfer_activated`/`sync_node_with_plugin_state`/`FILE_TRANSFER_PLUGIN_ID`、`peer_migration`、discovery-refresh 订阅（按 D5 留宿主）。
- 删除：移动 `peer_net.rs` / `peer_transfer.rs` / `peer_receive.rs` / `peer_remote.rs` / `peer_events.rs`（能力域接管）+ 防回接锁。
- `plugin/host_ports.rs` 与 `plugin/manager.rs` 改为调 crate 引擎（保留 R5 组合语义）。

## 5. 待核（进入票 04 前必须闭环）

1. **R2 与 file-transfer-shared-core 在途改动的先后**：桌面插件是否已迁到共享核（memory 记「file-transfer-shared-core T1–T7 已实施未提交」）——若在途，R2 需与该票合并推进，避免互相踩。
2. 移动 `wasm-apps/file-transfer/rust/src/lib.rs:49,51` 的 `mdns:*` 订阅真源形态（M3 已统一 topic，复核一遍）。
3. 移动 `HostEnginePorts`（wasm-core）是否需要新增方法（当前 21 个 peer 方法是否覆盖 crate 引擎入口；`close` 的 session 路由是否在 wasm-core 侧已兜底——探索代理标为「待核」）。
