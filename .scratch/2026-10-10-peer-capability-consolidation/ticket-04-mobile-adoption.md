# 票 04 · 移动端接入共享 peer 能力域 crate（执行计划）

> Status: **全部完成**（批次 1–5 落地 2026-10-10 → 2026-10-11；实施记录见文末 §4，批次 5 收口见 §4 补记）
> Date: 2026-10-10（实施记录见文末 §4）
> 前置：票 02/03（`spec.md` §6、`ticket-03-crate-decoupling-and-reconciliation.md`）；R1–R4 已按用户「全部按照建议」拍板（见 §0）。
> 事实基线：移动基线 `cargo check` 绿（29.63s，缓存热）；桌面 `cargo check` 绿（27.39s）；crate `cargo test` **51/0**。

---

## 0. 决策落档（用户拍板：全部按建议）

| # | 决策 | 落点 |
| --- | --- | --- |
| R1 | 接收设置**不落盘**（删 `transfer_settings.json`）：业务真源归插件，宿主副本只服务引擎闸门 | 批次 3 删移动 `persist_settings`/`read_settings_file`/`write_settings_file` + 其调用点；`PeerTransferSettings` 用 crate 版（无 `encryption_enabled`/`concurrency`） |
| R2 | `pull-started` 载荷统一为 `files[]+totalSize`（建行锚点同构，= 共享核契约） | ✅ crate 已改（`lib.rs::engine_pull_started_payload` + 新用例；`peer_engine_remote.rs` 头注同步） |
| R3 | 拉取并发/上限**对齐 crate**：引擎逐文件立即发起，并发节流归调用方（插件）分批自控——crate 头注既定口径（`peer_engine_remote.rs:1-13`）；移动的 `PULL_FILES_CAP` 与并发信号量不迁 | 批次 3 删移动 `PULL_FILES_CAP`/`run_pull_queue` 的 Signal/JoinSet 编排（改为逐条委托 crate `pull_peer_files`） |
| R4 | `node-stopped` 事件**加进 crate**（引擎事实；共享核已能归约） | ✅ crate 已改（`peer_engine_receive.rs::drive_receive_events` 通道关闭路径 + 新用例） |

**crate 侧新增能力（批次 1，已完成）**：
- `engine_ports::PeerPlatformSeam`（平台缝：`shared_saf_access` / `file_landing` / `builtin_shared_dirs`）+ `NoPlatformSeam` 占位；`PeerEnginePorts.platform: Option<...>`。
- `decorate_shared_store`（装配期内置条目注入）；`start_locked` 已接 SAF / landing / 内置条目三处。
- 桌面 `peer_net_cmds.rs` 传 `platform: None`（行为逐字不变，桌面检查绿）。

---

## 1. 移动接线设计（批次 2/3 的落点与签名策略）

**策略：保留四个模块文件与全部既有公开签名，函数体改为「装配 ctx → 委托 crate」**——`lib.rs` / `plugin/manager.rs` / `plugin/host_ports.rs` 的调用点零改动，迁移风险集中在适配层。

```text
bedcode-mobile/src-tauri/src/
├── peer_net.rs        ← 重写为适配层：PeerCtx 装配 + 5 端口实现 + 平台缝实现 + 生命周期/连接/信任/共享根/发现刷新（公开签名逐字保留）
├── peer_transfer.rs   ← 重写：发送/暂停/恢复/取消/active rows/collect_outgoing → 委托 crate；`peer_pick_files`+relay 原样保留（平台原语）
├── peer_receive.rs    ← 重写：接收面 → 委托 crate（设置持久化删除，R1）
├── peer_remote.rs     ← 重写：远端面 → 委托 crate（并发编排删除，R3）
├── peer_events.rs     ← 删除（载荷真源 = crate；topic/节流常量 = crate）
└── peer_migration.rs  ← 保留（携带插件 id 与插件存储键，D5 留宿主）
```

**适配层内容清单（peer_net.rs 新职责）**：
1. `pub use bedcode_server_peer_net::{PeerNetState, PeerTransferState, PeerReceiveState, PeerRemoteState, PeerCtx, ...}`（保住 `crate::peer_net::PeerNetState` 等既有路径）。
2. `peer_ctx(app) -> crate::Result<Arc<PeerCtx>>`：托管 state 取 4 句柄 + 装配 `PeerEnginePorts`（5 端口 + `platform`）。
3. 端口实现（零大小结构）：
   - `BusPort`：`pm.message_bus().publish(...)` + `publish_binary` + `subscribe_static`（转 `BusMessageHandler`）+ `deliver_endpoint_frame`（不涉及 → 显式 `unimplemented!`? 不行——用 no-op + 注释：peer 引擎不消费该面）。
   - `EventSink`：`app.emit`（失败仅日志）。
   - `PathsPort`：`app_data_dir` + 落点 = **app 私有下载目录**（`resolve_app_downloads_dir` 结果缓存进 `OnceLock`，端口方法同步读；未就绪返回 Err → 引擎回退行为与迁移前一致）。
   - `SystemInfoPort`：`try_get_system_info()` 读设备名 + `device_name_fallback() = "BedCode Mobile"`；`local_ipv4_addresses`/`app_version` 同源实现。
   - `MdnsPort`：`bedcode_discovery_engine::engine::{shared_daemon, register_host_service, stop_host_service}`（**复用，零新 mDNS 代码**）。
4. `PeerPlatformSeam` 实现：SAF（迁 `android_saf_bridge`，`#[cfg(target_os="android")]`；非 Android 返回 None）+ `MediaLanding`（迁 `MediaLanding` + `mime_type_for`）+ `builtin_shared_dirs() = [("下载", 私有下载目录)]`。
5. 生命周期/设备名：`ensure_node_started`（先 **预热设备名**——保留轮询 15s 语义，再委托 crate `start_node_owned`... 属主由 crate 记账）、`stop_node_for_plugin`、`sync_node_with_plugin_state`、`FILE_TRANSFER_PLUGIN_ID. plugin_transfer_activated`。
6. 发现刷新订阅（`peer:discovery-refresh`）：**留适配层**（crate 版本的就绪判据是 `server_base::ports::get()`，移动未装该全局 → 移动用自己的 `try_get_plugin_manager` 轮询 + `BusPort::subscribe_static`）。

**组合语义（留移动 host_ports.rs，crate 不动）**：
- `peer_pause_transfer` → crate `pause_transfer`，未命中回落 crate `pause_peer_receiving`（移动单原语双向语义）；`resume` 同。
- `peer_send_files_with_policy`：`encrypt=None` → crate 的 `unwrap_or(false)`（移动旧读值写入侧已退役，等价）。
- `active_transfers` 投影：crate `active_transfers_for_plugin`（totalBytes 变准 = 改善）。

---

## 2. 批次划分（每批结束必须编译/测试绿）

| 批次 | 内容 | 门禁 |
| --- | --- | --- |
| 1 ✅ | crate 侧前置（票 02/03 + R2/R4 + 平台缝） | crate `test` 51/0；桌面 check 绿 |
| 2 | 移动 `Cargo.toml` 加 `bedcode-server-peer-net`（默认 features）+ `bedcode-server-base`；`lib.rs` 装配改指向 crate 状态类型；`peer_net.rs` 重写为适配层（端口/平台缝/装配/生命周期）；`peer_events.rs` 删除 | `bedcode-mobile/src-tauri` `cargo check` 绿 |
| 3 | `peer_transfer.rs` / `peer_receive.rs` / `peer_remote.rs` 重写为委托层（R1 删落盘、R3 删并发编排、pick/relay 保留）；`plugin/host_ports.rs` 组合语义（pause/resume 回落）；`plugin/manager.rs` / `lib.rs` 调用点核对 | `cargo check` 绿 + 移动 `cargo test` 全量 |
| 4 | 防回接锁：移动宿主不得再持有引擎状态机/载荷构造/设置落盘（旧读路径已删 + 锁扫描）；更新既有 4 把 peer 相关锁（`retired_mobile_send_orchestration_lock` / `receive` / `peer_transfer_command_face` / `peer_discovery_projection`）的符号基准 | 锁变异自检（注入→红→还原核 sha256） |
| 5 | 文档/CHANGELOG：移动 code-map（peer 段改写）、双端锁索引、CHANGELOG 双语 | 全仓引用核对 + eslint 0 error |

## 3. 已知坑（批次 2/3 前置，动手前逐条核）

1. **`resolve_app_downloads_dir` 是 async**，端口方法是同步 ⇒ 结果必须缓存在适配层（`OnceLock`/managed state），首次未就绪返回 Err（引擎按既有错误语义处置）。
2. **设备名预热**：crate 同步读端口；移动必须在调 crate 启节点前 await 就绪（保留 `SYSTEM_INFO_WAIT_ATTEMPTS` 轮询 + 兜底名）。
3. **就绪判据不同源**：crate 的发现刷新订阅等 `server_base::ports::get()`（桌面装；移动不装）⇒ 移动侧自持该订阅（§1.6）。
4. **`deliver_endpoint_frame`**：peer 引擎不消费，端口实现给 no-op + 注释（不得 panic——订阅期可能被其他面调用）。
5. **既有锁的扫描面**：4 把 peer 锁按符号/字面量扫描，模块重写会改变命中 ⇒ 批次 4 同批更新，先跑锁再改。
6. **行尾与锁路径基准**（memory §5.8/§5.13）：移动插件 crate 禁整 crate `cargo fmt`；锁路径基准 `mobile_root()` 实为 `src-tauri`。

---

## 4. 实施记录（2026-10-10，批次 2–4）

**批次 2（适配层）**
- `Cargo.toml`：新增 `bedcode-server-peer-net`（默认纯引擎形态）+ `bedcode-server-base`（端口 trait 词汇）。
- `src/peer_net.rs` 重写为适配层：5 端口实现（`MobileBusPort` / `MobileEventSink` / `MobilePathsPort` / `MobileSystemInfoPort` / `MobileMdnsPort`——mDNS 直接接 `bedcode-discovery-engine::engine` 三函数）+ `MobilePlatformSeam`（SAF / `MediaLanding` / 内置 `"下载"` 条目，SAF 桥与 MediaLanding 整体迁入）+ `peer_ctx` 装配 + 平台预热（设备名轮询 15s / 下载目录 `OnceLock` 缓存）+ 生命周期薄壳 + 引擎入口委托；全类型/入口按原路径 `pub use` 再导出（宿主其余模块零改动）。
- `src/lib.rs`：四状态改 `Arc::new(...)` 托管（与桌面同形）；删 `pub mod peer_events;`。

**批次 3（委托层）**
- `peer_transfer.rs` / `peer_receive.rs` / `peer_remote.rs` 重写为委托层（签名逐字保留）；`peer_pick_files` + relay 保留（平台原语）；DTO 由能力域再导出（wire 零变化）。
- R1 落地：删 `transfer_settings.json` 落盘（设置真源归插件）；R3 落地：`PULL_FILES_CAP` 与并发信号量退役（能力域口径：并发节流归调用方）；R4 复用能力域 `node-stopped`。
- `plugin/host_ports.rs`：`peer_active_transfers` 改调能力域聚合投影；`pause/resume` 双向兜底留在适配层组合（crate 语义不动）。
- 删 `src/peer_events.rs`（载荷真源归能力域）。

**批次 4（锁）**
- 新增 `tests/peer_engine_consolidation_lock.rs`（三面判据：退役面缺席 `src/peer_*.rs` 扫非注释行 + 适配面在场 + 共享依赖在场）；**变异自检 1/1**（注入 `fn start_locked(` → 测红；副本字节级还原，sha256 `0daf30a6…` 前后一致）。
- 修 3 处**既有在途陈旧路径锁**（fork 退役/抽根后未同步，属 HEAD 基线红，非本票引入）：`mobile_host_websocket_client_domain_lock` 两路径、`retired_mobile_auth_orchestration_command_face_lock` 两处、`retired_mobile_session_control_face_lock` 两处 → 均改为 `../../packages/bedcode-wasm-core/src/manager/runtime/mobile/...` 与根 `packages/bedcode-ws-client-engine/...`。
- 覆盖对齐：移动旧引擎单测绝大部分在 crate 同源在场；补 3 个进 crate（`parse_node_id` 正反例、`map_peer_net_error` 上下文、句柄表未知批 fail-safe）；`MediaLanding` 三例留在移动适配层（平台缝主场）。

**门禁（全实跑）**：移动 `cargo test` 全量 **314 passed / 0 failed**（24 个测试二进制）；crate `cargo test` **54/0**；桌面 `src-tauri` `cargo check` 绿（30.71s）；新锁变异自检 1/1 + 陈旧路径锁 3 处修复后转绿。

**批次 5 待续**：移动/桌面 code-map 的 peer 段改写、锁索引登记、CHANGELOG 双语、ADR（D6 执行记录）。


---

## 4.1 批次 5 补记（2026-10-11）

- 移动 code-map「对等网络」节改写提交（`fad5eb968`）；锁索引登记两把锁（`peer_engine_consolidation_lock` / `retired_mobile_mdns_advertise_lock`，`340fa8797`）。
- CHANGELOG 双语 peer 条目（`975651bb2` / `9a65a4c7c`）；ADR 0048（D6 执行记录）落档。
- 票 05（mDNS 广播死面退役）与票 06（ADR/CHANGELOG/锁索引）同期收口，见 spec §4 票表与 ADR 0048。
