# 票 04 — 宿主接线：双端铸造订阅 + 移动旧链路迁中枢

Status: ready-for-agent
Type: task
Blocked by: 02, 03

## 目标

- **`packages/bedcode-server-peer-net/src/lib.rs`（spawn 处 `:1011-1033` 一带）**：铸造订阅——
  `ctx.ports.mdns.subscribe_type(SERVICE_TYPE)` → 转 `bedcode_peer_net::PeerMdnsBrowse { receiver, guard, request_requery: 绑 ctx.ports.mdns.request_type_requery(SERVICE_TYPE) }` → 传新 spawn 签名；`request_discovery_refresh`（`:1459`）requery 转发随 `DiscoveryDaemon::request_requery` 语义自动生效（核对注释同步）
- **桌面**：`bedcode-desktop/src-tauri/src/server/ports_impl.rs`（`HostMdnsPort` `:318` 一带）实现两个新端口方法——实现 = `bedcode_discovery_engine::subscribe_type` / `request_type_requery` 的转换（`into_parts` 形态，receiver 与守卫不得脱钩）
- **移动**：`bedcode-mobile/src-tauri/src/peer/ports.rs`（`MobileMdnsPort` `:226` 一带）同扩；`src/peer/net.rs` 注释同步
- **移动旧链路浏览迁中枢**（spec §6 D5）：`bedcode-mobile/src-tauri/src/mdns/discovery.rs:54-58,196`——
  - start 改 `subscribe_type` 并持有订阅句柄（`MdnsDiscovery` 增持守卫字段）
  - stop 改 drop 句柄（删 `daemon.stop_browse`）
  - 事件循环 `recv_timeout` 适配 flume 同签名；`RECV_TIMEOUT_SECS=1`（`system/constants/mdns.rs:6`）不变
- **既有锁保持绿**：`retired_mobile_mdns_advertise_lock`（`MdnsDiscovery` 薄壳与三命令在场）/ `peer_engine_consolidation_lock`（共享守护复用锚点在 `src/peer/ports.rs` 在场）
- 移动 `plugin/host_domains/mdns.rs:16` 红线注释复核（spec §8）

## 门禁

- `packages/bedcode-server-peer-net`、桌面 `src-tauri`、移动 `src-tauri` 各自 `cargo test` 全绿
- `cd cross-end-tests && cargo test --test mdns_health_flow`——覆盖「桌面广播 → 移动旧链路 browse（`_bedcode._tcp.local.`）发现 + stop 清列表」，正是 D5 迁移面的集成回归；两端 wasm 产物已在位（已确认）
