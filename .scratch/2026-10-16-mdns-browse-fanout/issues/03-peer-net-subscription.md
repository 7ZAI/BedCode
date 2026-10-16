# 票 03 — peer-net 订阅化：PeerMdnsBrowse + spawn 签名 + 删直调

Status: ready-for-agent
Type: task
Blocked by: 01

## 目标

`packages/peer-net/src/discovery.rs`（spec §2.3 / §6 D7；peer-net 保持零 server-base / 零 discovery-engine 依赖，只收纯值）：

- 新增值形态：

  ```rust
  pub struct PeerMdnsBrowse {
      pub receiver: flume::Receiver<ServiceEvent>,
      pub guard: Box<dyn Any + Send + Sync>,
      pub request_requery: Arc<dyn Fn() + Send + Sync>, // 绑到中枢 request_type_requery(SERVICE_TYPE)
  }
  ```

  peer-net 不解释 guard——仅随 `DiscoveryDaemon` 生命周期持有并释放（释放 = 退订）

- `spawn_peer_mdns_daemon(node, running, config, daemon: ServiceDaemon, browse: PeerMdnsBrowse)`——`daemon` 仍用于 register / unregister（广播面不动）
- **删除**：
  - 初始 `daemon.browse`（`:568-570`）
  - `swap_tx / swap_rx` 换绑机器（`:683-687` 一带）与事件循环的 swap 分支
  - `run_requery_loop` 的 browse 分支（`:753`）→ 改为调用 `request_requery` 闭包
  - stop 路径的 `daemon.stop_browse`（`:419`）——浏览退订随订阅句柄释放
- `DiscoveryDaemon::request_requery()`（`:478`）转调闭包，对外语义不变
- `REQUERY_INTERVAL=30s` 周期节奏**留在 peer-net**（业务节奏属消费方）；机制（重发 browse / 换绑 / 重放）归中枢
- 事件循环 receiver 类型改 flume（`recv_async` / 超时包裹语义等价；flume 0.12 无 `recv_async_timeout`，沿用 tokio 超时包裹）；退出形态与今日 `stop_browse` 断联一致
- `node.rs` 文档引用同步；`discovery.rs` 模块头「浏览侧已收敛到全局共享守护」段落按新形态改写（spec §8 注释同步）

## 门禁

- `cd packages/peer-net && cargo test` 全绿——既有测试全部注入式（`tests/{discovery_injection,harness,pause_symmetry,shared_dirs,transfer_session}.rs` 不经真实守护、不直调 daemon）；以「注入通道喂合成事件」适配签名变化
- 删换绑机器后无残留引用（grep `swap_tx` / `swap_rx` 归零）
