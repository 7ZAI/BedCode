# 票 02 — 发现引擎类型中枢：type_hub.rs + 注入缝 + engine browse 切换

Status: resolved
Type: task
Blocked by: 01

## 目标

新模块 `packages/bedcode-discovery-engine/src/type_hub.rs`（spec §2.1-§2.3 / §6 D1-D4）：

- **订阅表 fan-out**：`HashMap<service_type, Hub>`；订阅者 `Vec<(sub_id, flume::Sender<ServiceEvent>)>`；通道容量 64；`try_send`——`Full` → 丢弃事件 + 限频 debug；`Disconnected` → 惰性剔除
- **唯一 browse**：每活跃类型全进程恰好一条 `daemon.browse()`，只由中枢调用；新增订阅者不得再触发 browse
- **requery**：`request_type_requery(ty)` → 100ms 合并窗 → 重发 browse + receiver 换绑 + 缓存重放 → 广播全部订阅者；前缘吞并（窗内重复请求丢弃）；新订阅者加入 = 后缘补齐（最后一次加入后 ~100ms 执行一次 browse，保证每个新订阅者都拿到缓存重放——spec §2.1-⑤）
- **生命周期**：首个订阅建档（唯一 browse）；引用计数归零 → 立即 `stop_browse` + 移除（无 linger）；重建幂等
- **守卫退订**：订阅句柄 Drop → 移除发送端（通道关闭）；guard 丢失路径由 fan-out 的 `Disconnected` 惰性剔除兜底
- **注入缝**：daemon 交互抽微型 trait/闭包（生产 = `ServiceDaemon` 包装，测试 = fake），单测不触网（spec §2.3）
- **任务与容错**：中枢泵任务经 `DiscoveryPorts::spawn`；错误边界同 `spawn_with_error_boundary` 族；控制通道已关（任务退出）时下次 subscribe / requery 检测 → 以 drained 同路径重建（spec R7）
- **生命周期日志**（spec T5-③，提前落在此处，票 05 只核验）：`created` / `reused` / `join-rebrowse` / `requery` / `drained-stop`；结构化字段按 AGENTS §8
- **接口**（spec §2.3 逐字）：

  ```rust
  pub fn subscribe_type(service_type: &str) -> Result<MdnsBrowseSubscription, String>;
  pub fn request_type_requery(service_type: &str);
  ```

  `MdnsBrowseSubscription` 形态与票 01 的 `MdnsTypeSubscription` 对齐（私有字段 + `new` / `into_parts()`），供票 04 宿主 glue 转换；转换不得使 receiver / guard 生命周期脱钩

`packages/bedcode-discovery-engine/src/engine.rs` 切换（spec §2.3 / §3）：

- `browse()`（`:147` 一带）：删 `daemon().browse`，改 `subscribe_type`；`BrowserEntry` 增持订阅守卫
- `stop_browser_inner`（`:271`）：删 `daemon.stop_browse`，条目移除即守卫 Drop → 通道关闭 → 浏览任务 recv 断开退出；`mdns browse loop exited` 日志保留（退出形态与今日 stop_browse 断联一致）
- `publish_dir_event` / 属主仲裁 / `is_self_broadcast` / 句柄表 / 权限门全部不动
- 模块头注释 + `shared_daemon` 注释（「切勿在别处另建守护」段）补 listener 级不变量与中枢位置（spec §8 注释同步）
- `packages/bedcode-discovery-engine/Cargo.toml`：`+ flume = "0.12"`

## 单测（spec §7 矩阵 1-4、7，注入缝上做，零触网）

1. 同类型两次 subscribe：只发生一次 `daemon.browse`；两订阅者收到同一事件 —— 变异自检：退回「后到覆盖」实现必红
2. requery 换绑：旧 receiver 失效后，两订阅者仍收到换绑后事件与缓存重放
3. 合并窗：100ms 内两次 requery 只产生一次 browse；窗内加入 + 加入后缘补齐各 1 次
4. 守卫退订：drop 订阅句柄 → 通道关闭、订阅者移除；归零 → `stop_browse` 恰一次 + 中枢移除；重建幂等（重新 subscribe 恢复）
5. 惰性兜底：不经守卫直接 drop receiver → 下次 fan-out 剔除，不影响其他订阅者
7. 中枢重建：控制通道关闭（模拟任务退出）后再次 subscribe 能重建并 browse

## 门禁

- `cd packages/bedcode-discovery-engine && cargo test` 全绿（既有用例零回归）
- 变异自检（第 1 条 + 至少再一条）结果记入票面 Comments

## Comments

### 交付
- 新模块 `packages/bedcode-discovery-engine/src/type_hub.rs`（机制 + 单测同文件）：
  `TypeHubSet`（`HashMap<service_type, Arc<Hub>>` 注册表）+ 泵任务 `run_hub`（控制命令 /
  listener 事件 / 后缘补齐到期三路 `tokio::select!` 收口，单 await 点）+ 惰性 fan-out +
  100ms 合并窗（前缘吞并 / 后缘补齐）+ 守卫同步退订 + R7 僵尸重建。
- 注入缝：`TypeDaemon`（只 `browse` / `stop_browse` 两条）与 `TaskSpawner`（派生泵任务）；
  生产实现 `SharedDaemonSeam`（惰性取 `engine::shared_daemon`，不订阅不拉起守护）/
  `PortsSpawner`。单测经 `TypeHubSet::with_parts` 建独立实例，进程级静态表不参与。
- 接口按 spec §2.3：`subscribe_type(&str) -> Result<MdnsBrowseSubscription, String>`、
  `request_type_requery(&str)`；`MdnsBrowseSubscription` 与票 01 的 `MdnsTypeSubscription`
  逐字对齐（私有字段 + `new` / `into_parts()`，不实现 `Clone`，receiver 与守卫同生共死）。
- `engine.rs` 切换：`browse()` 改挂中枢订阅并把守卫托管进 `BrowserEntry._subscription`；
  `stop_browser_inner` 删 `daemon.stop_browse`（条目移除即守卫 Drop = 退订），共用
  `mdns browse loop exited` 退出形态；模块头 + `shared_daemon` 注释补 listener 级不变量
  （ADR 0054）。`publish_dir_event` / 属主仲裁 / `is_self_broadcast` / 句柄表 / 权限门未动。
- `lib.rs`：`pub mod type_hub` + 模块图 + 出口 re-export。
- `Cargo.toml`：`+ flume = "0.12"`；tokio features `+ macros`（`select!` 用，先例：peer-net）。

### 命令与结果
- 基线（改动前）：`CARGO_BUILD_JOBS=2 cargo test` → **34 passed / 0 failed**（票面预告 ~31）。
- 改动后：`CARGO_BUILD_JOBS=2 cargo test`（cwd `packages/bedcode-discovery-engine`）→
  **42 passed / 0 failed / 0 ignored**（新增 8 个，全在 `type_hub::tests`）。既有 34 例零回归。
- `type_hub::tests` 连跑 5 轮均 8 passed（纯时序用例无抖动）。
- 本 crate 无 `tests/` 集成测试目录（只有 lib 单测），故无集成面增补。
- `cargo fmt --check`：本任务文件（type_hub / engine / lib）clean；`wire.rs` /
  `wire/drift_lock.rs` 的既有格式差异是 HEAD 基线遗留、与本任务无关，已还原不碰。
- `cargo clippy --all-targets`：4 条 warning 全在 HEAD 既有文件（advertiser.rs:92 /
  engine.rs:82 `map(|d| d.clone())` / wire.rs:36,49），新增代码 0 warning。

### 单测矩阵（spec §7）
覆盖 1、2、3、4、5、7（第 6 条属 peer-net 票 03）：
1. `two_subscribers_share_one_browse_and_both_receive_the_event` — 唯一 browse + 全员同事件。
2. `requery_rebinds_listener_and_replays_to_every_subscriber` — 换绑 + 缓存重放广播全员。
3. `merge_window_absorbs_requeries_and_coalesces_joining_subscribers` — 前缘吞并 + 后缘合并。
4. `dropping_subscriptions_removes_rows_and_drains_hub_exactly_once` — 守卫退订 + 归零摘除 + 重建幂等。
5. `lazy_eviction_drops_dead_subscribers_and_drains_when_last` — 惰性剔除 + 晚到守卫幂等。
7. `subscribe_rebuilds_hub_after_pump_task_exit` — 泵退出后按 drained 同路径重建。
附加（票面未列、契约补强）：`empty_service_type_is_rejected_before_touching_daemon`（触网前拒绝）、
`full_subscriber_channel_drops_events_without_evicting_or_blocking_others`（R1/D2 慢消费者只丢不阻塞不剔除）。

### 变异自检结论（三条，均先红后绿并还原）
1. **退回「每订阅者各自 browse（后到覆盖）」**（在 reuse 分支加一次 `daemon.browse`）：
   矩阵 1 红——`browse_calls` 变两条（`新增订阅者不得再触发 browse` 断言失败）。
2. **关闭惰性剔除**（`Disconnected` 分支改为保留订阅者）：矩阵 5 红——`subscriber_count`
   为 2 而非 1（`死订阅者被惰性剔除` 断言失败）。
3. **关闭后缘补齐**（`arm_join_window` 不再置 `join_deadline`）：矩阵 2、矩阵 3 均红——
   新订阅者拿不到自己的缓存重放（`next_event` 2s 超时）。
变异均已 `git diff` / `grep MUTATION` 确认零残留。

### 偏离与开放项
- 基线用例数 34（票面预告 ~31）——晚于票面又并入的既有用例，非本次新增。
- 合并窗生产值 = `MERGE_WINDOW = 100ms`（spec D3）；单测注入 20ms 短窗，另有用例用 60s
  长窗以隔离「后缘补齐不得干扰计数」。
- `futures-util` 未引入：泵任务单 await 点用 `tokio::select!` + `std::future::pending()` 实现，
  不新增依赖。
- 下游 glue（票 04）经 `subscribe_type(..).into_parts()` 取 `(receiver, guard)` 转
  `bedcode-server-base::ports::MdnsTypeSubscription`；本票未触该端口。
- 锁白名单（`type_hub.rs` 唯一 browse/stop_browse 的结构性封口）与 ADR 0054 / code-map 登记
  属票 05 门禁面，本票只交付机制。
