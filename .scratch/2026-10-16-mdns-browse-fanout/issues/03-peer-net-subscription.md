# 票 03 — peer-net 订阅化：PeerMdnsBrowse + spawn 签名 + 删直调

Status: resolved
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

## Comments

### 改动摘要

`packages/peer-net/src/discovery.rs`（浏览面全量改为「消费宿主注入的中枢订阅」）：

- 新增值形态 `PeerMdnsBrowse { receiver: flume::Receiver<ServiceEvent>, guard: Box<dyn Any + Send + Sync>,
  request_requery: Arc<dyn Fn() + Send + Sync> }`；类型文档写明三字段同源同生命周期、本模块不解释 guard
  内容（只随 `DiscoveryDaemon` 持有 / 释放，释放即退订）。
- `spawn_peer_mdns_daemon(node, running, config, daemon, browse)`：新增第 5 参 `browse`；函数内解构
  `PeerMdnsBrowse`——`receiver` 移交事件循环，`guard` + `request_requery` 留句柄；`daemon` 仍只用于
  register / unregister / re-announce（广播面零改动）。
- 删除：初始 `daemon.browse`（原 `:568-570`）、`swap_tx/swap_rx` 换绑机器（原 `:683-687`）与事件循环
  swap 分支、`run_requery_loop` 的 `daemon.browse` 分支（原 `:753`，改为转调 `request_requery` 闭包）、
  stop 路径的 `daemon.stop_browse`（原 `:419`，改为释放订阅守卫；停机标志保留为兜底）。
- 事件循环 receiver 类型换 `flume::Receiver<ServiceEvent>`（`recv_async` + 现有 tokio `timeout(SWEEP_INTERVAL)`
  包裹，语义等价；退出形态与旧 `stop_browse` 断联一致：`Ok(Err(_))` 断连即 break）。
- `DiscoveryDaemon::request_requery()`（原 `:478`）改为转调闭包；`REQUERY_INTERVAL=30s` 周期节奏留在 peer-net
  （周期任务与即时路径共用同一闭包）。`run_requery_loop` 首 tick 仍跳过（订阅建立时中枢已 browse + 缓存重放）。
- 注释同步：模块头「浏览侧已收敛到全局共享守护」段改写为「浏览面走类型中枢订阅（ADR 0054）」段（含
  single-listener 不变量成因、peer-net 不再 browse/stop_browse、重查收敛口径）；`DiscoveryDaemon` /
  `stop` / `request_requery` / 事件循环 / 重查循环文档与逐条行内注释按新机制改写；`node.rs`
  `with_discovery` 文档同步（事件来源改为「中枢浏览订阅」）；`lib.rs` crate 文档补浏览面一句与
  `PeerMdnsBrowse` 出口。
- `src/error.rs`：删除 `PeerNetError::MdnsBrowse` / `MdnsStopBrowse`（唯一构造点即上述被删直调，留下即死 API）。
- `Cargo.toml`：flume 注释同步（不再是「仅引用 `RecvTimeoutError`」，而是承载订阅 receiver，见偏差 2）。

### 测试（新增 4 例 / 全在本文件既有 `#[cfg(test)] mod tests`，注入式、零真实守护零网络）

| 契约 | 用例 | 断言要点 |
| --- | --- | --- |
| 票面 `request_requery` 闭包转调生效 | `request_requery_forwards_every_call_to_hub_closure` | 每次调用恰转调闭包一次（连续两次 = 2，不自行吞并）；夹具句柄不经真实守护 |
| 事件循环消费中枢 fan-out + 退出形态 | `event_loop_consumes_hub_fanin_events_and_exits_on_disconnect` | 注入 flume 通道喂合成 `ServiceResolved`：对端入缓存（node_id/addr/device_name 三项断言）、**自播回显不入缓存**（同 FIFO 通道 ⇒ 无需 sleep 即证已过滤）、goodbye 即时移除；发送端断开 ⇒ 循环退出不依赖停机标志 |
| 停机标志兜底 | `event_loop_exits_on_shutdown_flag` | 置位后立即退出（断言超时 4s < `SWEEP_INTERVAL` 5s，见「变异自检」） |
| 守卫随句柄生命周期（stop 退订） | `stop_releases_subscription_guard_once_and_joins_event_loop` | 存活期守卫不释放（计数 0）→ `stop()` 释放恰一次（计数 1）→ 返回时事件循环已 join（探针发送端 `is_disconnected`） |

既有注入式用例全部零改动通过（签名变化不影响它们——`tests/*.rs` 不经 daemon、不直调 `spawn_peer_mdns_daemon`，
已 grep 实证），故此票不需改测试文件。

### 门禁命令与结果

- `cd packages/peer-net && CARGO_BUILD_JOBS=2 cargo test`
  → lib `85 passed; 0 failed`（基线 81 + 新增 4）+ `discovery_injection 1` + `harness 9` + `pause_symmetry 4`
  + `shared_dirs 6` + `transfer_session 13` + doc-tests `0`，全绿、零失败。
- `cargo test --lib discovery::` 连跑 5 轮：13 passed ×5（新增异步用例无 flake；无 sleep、有界轮询）。
- `grep -rn "swap_tx\|swap_rx" packages/peer-net/src packages/peer-net/tests` → 零命中。
- `grep -rn "\.browse(\|\.stop_browse(" packages/peer-net/src` → 零命中（本 crate 内已无浏览直调；
  注释里出现的「不再 `daemon.browse` / `stop_browse`」等字面属说明文本，非调用点）。
- `cargo clippy --all-targets`：`src/discovery.rs` 零诊断（本票改动面干净）。注：crate 既有 `never_loop`
  deny 报错（`shared.rs:811` / `frame.rs:247` / `transfer/message.rs:364`）与若干 warning 均为存量、非本票引入。
- `cargo fmt --check`：本 crate **无 `rustfmt.toml`**（兄弟 crate 均有）⇒ 默认配置下全 crate 报差异（存量），
  已按文件既有风格（≈120 列）人工对齐新增代码，未做全 crate 重排（最小改动）。

### 变异自检（临时注入后已还原，均实测）

1. `request_requery` 改为空实现 → `request_requery_forwards_every_call_to_hub_closure` 红（计数 0 ≠ 1）。
2. 事件循环对断连不再 break（`Ok(Err(_)) => {}`）→ `event_loop_consumes_...` 红（5s 超时 `Elapsed`）。
   注：该用例声明为 `flavor = "multi_thread"`，正是让这类「循环空转」回归表现为确定的超时失败而非测试悬挂。
3. `stop()` 泄漏守卫（`mem::forget(self.browse_guard.take())`）→ `stop_releases_...` 红（释放计数 0 ≠ 1）。
4. 去掉自播回显过滤 → `event_loop_consumes_...` 红（缓存内出现本机记录，自播不得入缓存断言失败）。

弱项自知：「stop 中提前释放守卫（在 join 之前）vs 函数尾随 self drop 释放」不可从外部观测（stop 已置停机
标志，两条路径都退出），故该用例只钉「随句柄生命周期释放恰一次 + stop 返回时循环已 join」，不钉释放时序。

### 与票面的偏差与理由

1. **删除立即重查通道 `requery_tx/requery_rx`**（票面删除清单未列）。理由：票面要求
   `DiscoveryDaemon::request_requery()`「转调闭包」——保留通道只能做到「投递到循环再由循环转调」，
   与票面字面不符；且旧通道满（8）即静默丢弃刷新请求，直调闭包后突发由中枢 100ms 合并窗吞并，语义更强。
   周期任务改用 `Arc::clone` 的同一闭包，公开签名 `request_requery(&self)` 不变（T4 的 `:1459` 调用点无需改）。
2. **删除 `PeerNetError::MdnsBrowse` / `MdnsStopBrowse`**：两变体唯一构造点即被删的两处直调，留下即无构造点的
   死 API（AGENTS §6 陈旧代码清理）。未顺带删除**同样已无构造点**的 `MdnsDaemon` / `MdnsShutdown`（它们早于
   本票在共享守护票 04 时失去构造点，超出票面范围，见未决问题 1）。
3. **修正既有 doc 错位**：改前 `spawn_peer_mdns_daemon` 的函数文档被紧随其后的 `VIRTUAL_IFACE_PATTERNS`
   常量「吸收」（常量文档与函数文档连成一块，函数本身无 doc）。本次各归其位（常量 + 函数各持自己文档），
   属同一区域注释同步（AGENTS §6）。
4. 行号核对：票面 `:568-570`（初始 browse）、`:683-687`（换绑机器）、`:753`（requery browse 分支）、
   `:419`（stop_browse）、`:478`（request_requery）与改动前实测**逐条一致**，无漂移。
5. 测试落点：追加在既有同文件 `#[cfg(test)] mod tests`（AGENTS §6 形态①，未新建测试文件）。

### 未决问题

1. `packages/bedcode-server-peer-net/src/lib.rs:1012` 的 `spawn_peer_mdns_daemon` 调用点**未改**（票 04 范围）
   ⇒ 该 crate 在票 04 落地前编译失败（缺第 5 参 `browse`）。这是拆分预期，非回归；本票门禁只覆盖
   `packages/peer-net`。
2. 周期（30s）节奏本身无法在单测中廉价验证（未启用 tokio `test-util` 时钟暂停）；节奏由常量 +
   「转调同一闭包」构成，闭包转调已由即时路径用例覆盖，节奏常量未变。
3. 真机 / 全量验证（spec §7 清单 a-f）不属本票（票 07）。
