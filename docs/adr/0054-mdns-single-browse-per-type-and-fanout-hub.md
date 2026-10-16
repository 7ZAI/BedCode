# ADR 0054：mDNS 每服务类型单一 browse + 进程内 fan-out 类型中枢

- 状态：**已实施（2026-10-16，票 01–05 已合入）**；全量回归验证见票 07
- 日期：2026-10-16
- spec：`.scratch/2026-10-16-mdns-browse-fanout/spec.md`（根因证据、接口定稿、票据拆分；逐票记录同目录 `issues/`）
- 相关：ADR 0042（双端 mDNS 引擎共享守护，本 ADR 补完其 listener 级缺位）、
  ADR 0022（宿主 / 插件边界——中枢是引擎机制，不是产品语义）、
  ADR 0035 / 0037（能力域 crate 化与 wasm_core 整核抽出，中枢落点即共享能力域 crate）、
  ADR 0018 / 0019（移动契约独立 / 双端锁版；本次零 WIT / 零 ABI 变更）

## 背景

### 1. 症状

双端 file-transfer 互相发现失败：宿主层 peer-net 每 30s 正常打印 `peer discovered`，
但 file-transfer 的设备列表永远没有对方，且**全程无任何报错**。

### 2. 根因：mdns-sd 的单 listener 槽语义 × 共享守护

mdns-sd（0.20.1）对**每个服务类型只保留一个 listener 槽**：
`service_queriers: HashMap<String, Sender<ServiceEvent>>`；重复 `daemon.browse(ty)`
对非重传命令直接 `insert`（库注释原文「If there is already a `listener`, it will be
updated, i.e. **overwritten**」）。被顶掉的旧 receiver 从此**既不收事件、也不报错**：
旧 sender 被指数重传链永久保活（delay 翻倍、封顶 3600s、无终止条件），通道永不断开，
`browse loop exited` 永不触发——这就是「引擎 browse 循环零日志」的成因。

ADR 0042 完成了**守护级**共享（一个进程一个 `ServiceDaemon`，创建点唯一于
`engine.rs`），但**listener 级**不变量缺位。于是 peer-net 的「每 30s 重查再
`daemon.browse`」与各插件的 browse 落到了同一个守护的同一个槽上：peer-net 每次重查
都静默杀死插件 browse 的事件流。真机现场：插件 browse 建立后 **2ms（桌面）/ 15ms
（移动）**即被 peer-net 的初始 browse 覆盖，此后每次周期 / 即时重查只继续覆盖
（日志 `browse receiver swapped` 三连为现场证据）。`com.bedcode.file-transfer::mdns:found`
从未上总线 ⇒ guest / 前端零事件 ⇒ 双端互不可见。

断点在**宿主进程内、总线 publish 之前**，根本到不了 wasm——因此这不是「双端是否
同一份 wasm」的问题（双端 bus 订阅与 guest 收消息均已实证可用）。

### 3. 依赖方向约束（决定接口落点）

- `bedcode-discovery-engine → bedcode-peer-net` 已存在（`engine.rs::init_daemon` 调
  `disable_virtual_interfaces`）⇒ **peer-net 不得反向依赖引擎**（会成环）。
- `bedcode-server-peer-net` 依赖方向锁要求其只向下依赖 `bedcode-server-base` /
  `bedcode-peer-net`，**不得引 `bedcode-discovery-engine`** ⇒ 中枢句柄必须以
  **纯值 / 基础类型**注入 peer-net。

## 决策

### D1 · 全进程每服务类型唯一 `daemon.browse`，只由类型中枢代持

新增模块 `packages/bedcode-discovery-engine/src/type_hub.rs`：进程级注册表
`HashMap<service_type, Arc<Hub>>`，每个活跃服务类型持有一条 daemon 浏览（唯一 listener
槽位）。首个订阅者建档（唯一一次 `daemon.browse`），其余订阅者只挂进程内 fan-out 订阅。

**为什么必须持注册表锁调用 `daemon.browse`**：mdns-sd 的单 listener 槽是「后到覆盖」
语义，同一类型的两次 browse 一旦交错，后一次就把前一次的 listener 顶成死信道——持锁
让「建档 browse」与「泵侧重发 browse」严格串行（锁序纪律：注册表 → 状态，永不反向）。

进程级出口（`packages/bedcode-discovery-engine/src/lib.rs` re-export）：

```rust
pub fn subscribe_type(service_type: &str) -> Result<MdnsBrowseSubscription, String>;
pub fn request_type_requery(service_type: &str);
```

`MdnsBrowseSubscription` 是**生命周期不可脱钩**的值形态：私有字段 `receiver` +
`guard`，只提供 `new` / `into_parts()`（双方同时移交），不实现 `Clone`，也不提供
「只取一方」的访问器——只留 receiver 而丢弃守卫会造成退订泄漏（订阅永留中枢表、类型
browse 永不归零）。

### D2 · 订阅者进程内 fan-out（容量 64 / 惰性剔除）

中枢泵任务 `run_hub` 是该类型唯一浏览事件的 fan-out 点：三路等待（控制命令 / listener
事件 / 后缘补齐到期）在一个 `tokio::select!` 收口。fan-out 用非阻塞 `try_send`：

- `Full` ⇒ 丢弃该订阅者的这一条 + 限频 debug（`DROP_LOG_INTERVAL` = 1s）；mDNS 事件
  可重放（重查 / 缓存重放兜底），丢弃只延迟可见性，不是数据丢失。**慢消费者由此不再
  阻塞 mdns-sd 发送线程**（相对旧实现是改善）。
- `Disconnected` ⇒ **惰性剔除**：守卫丢失路径的兜底；剔除后订阅归零则按 drained 同
  路径拆除中枢。

订阅通道容量 64（`SUBSCRIBER_CHANNEL_CAPACITY`，不小于 mdns-sd 自身 listener 通道
`bounded(10)`）。

### D3 · 重查收敛中枢：100ms 合并窗（前缘吞并 + 后缘补齐）

重发 browse / receiver 换绑 / 缓存重放全部下沉中枢，消费方只保留**节奏**（peer-net 的
30s 周期不动，属业务节奏）。

- **前缘吞并**：距该类型上次 browse < 合并窗（`MERGE_WINDOW` = 100ms）的重查请求直接
  丢弃——同窗口内请求共享同一次缓存重放。
- **后缘补齐**：新订阅者加入触发一次「加入时刻 + 合并窗」的补齐 browse（窗口内多次
  加入反复推后、合并为一次），确保每个新订阅者都拿到自己的缓存重放——原 per-plugin
  browse 的「立即缓存重放」语义不缩水。

### D4 · 守卫退订 + 归零立即 stop（无 linger）

退订由订阅守卫 `SubscriptionGuard` 的 `Drop` **同步**完成（不给泵任务投递退订消息）：
既满足「归零**立即** `stop_browse`」不变量，也避免「泵任务已死 → 退订永不生效」的依赖
倒挂。守卫只持注册表 + 服务类型 + 订阅号，**不持** `Arc<Hub>`（否则守卫会把中枢控制端
一并保活，泵任务永不退出）。

归零 ⇒ `daemon.stop_browse` + 注册表行移除；再次订阅即重建（幂等）。R7 恢复路径：
泵任务 panic / 被取消后，下次 subscribe / requery 检测控制通道已关，按 drained 同路径
拆旧代并重建。

### D5 · 引擎浏览原语切换：只挂订阅，不再自己 browse

`engine::browse()` 删 `daemon.browse`，改 `type_hub::subscribe_type`，退订守卫托管进
`BrowserEntry._subscription`（条目移除即守卫 Drop = 退订）。`stop_browser_inner` 删
`daemon.stop_browse`——旧写法会摘掉**整类型** listener、连别的插件 / peer-net 的浏览
一起杀；现在仍有其他订阅者时 listener 不动，归零才由中枢自行 `stop_browse`。退出形态
与今日一致（浏览事件循环 `recv` 断开 ⇒ `mdns browse loop exited`）。
`publish_dir_event` / 属主仲裁 / `is_self_broadcast` / 双句柄表 / 权限门全部不动。

### D6 · peer-net 收纯值注入，零 daemon 交互

`packages/peer-net/src/discovery.rs` 新增值形态 `PeerMdnsBrowse`：

```rust
pub struct PeerMdnsBrowse {
    pub receiver: flume::Receiver<ServiceEvent>,      // 中枢 fan-out 接收端 → 事件循环
    pub guard: Box<dyn Any + Send + Sync>,            // 退订守卫：本模块只持不解释，随句柄释放
    pub request_requery: Arc<dyn Fn() + Send + Sync>, // 绑到中枢 request_type_requery(SERVICE_TYPE)
}
pub fn spawn_peer_mdns_daemon(node, running, config, daemon: ServiceDaemon, browse: PeerMdnsBrowse)
    -> Result<DiscoveryDaemon>;
```

peer-net **不认识 mDNS 引擎 crate**（D1 的依赖方向约束）：它不解释 guard 内容，只随
`DiscoveryDaemon` 生命周期持有、`stop` 时释放（释放即退订）。删除：初始 `daemon.browse`、
`swap_tx/swap_rx` 换绑机器、`run_requery_loop` 的 browse 分支、stop 路径的
`daemon.stop_browse`（`PeerNetError::MdnsBrowse` / `MdnsStopBrowse` 一并删除——唯一构造点
即被删直调，留下即死 API）。`DiscoveryDaemon::request_requery()` 改为转调闭包，对外语义
不变；广播面（register / unregister / re-announce）仍用注入的 `daemon`，零改动。

### D7 · 端口扩展 + 宿主接线（双端）

`bedcode-server-base::ports::MdnsPort` 新增两方法承载中枢句柄（既有三方法不变）：

```rust
fn subscribe_type(&self, service_type: &str) -> std::result::Result<MdnsTypeSubscription, String>;
fn request_type_requery(&self, service_type: &str);
```

端口侧 `MdnsTypeSubscription` 与引擎侧 `MdnsBrowseSubscription` 逐字对齐（私有字段 +
`new` / `into_parts`）；两个值形态的转换只在宿主 glue，硬性要求「不得提供 receiver 与
守卫生命周期脱钩的构造」。`bedcode-server-base` 新增 `flume = "0.12"` 依赖（与 peer-net /
mdns-sd 传递依赖同版）。

宿主接线：

- 桌面 `packages/bedcode-server-peer-net/src/lib.rs`（`start_locked`）：`subscribe_type(SERVICE_TYPE)`
  → `into_parts()` → `PeerMdnsBrowse { receiver, guard, request_requery }`，整份移交
  `spawn_peer_mdns_daemon`（5 参）；守卫随 `DiscoveryDaemon` 存续期持有，stop 释放即退订。
- 桌面 `bedcode-desktop/src-tauri/src/server/ports_impl.rs`：`HostMdnsPort` 实现两方法（直转引擎出口）。
- 移动 `bedcode-mobile/src-tauri/src/peer/ports.rs`：`MobileMdnsPort` 同源实现两方法。
- 移动 `bedcode-mobile/src-tauri/src/mdns/discovery.rs`：旧链路 `_bedcode._tcp.local.` 浏览迁中枢
  （`subscribe_type` 持有句柄，`stop` 改释放守卫）；删 `daemon.stop_browse(SERVICE_TYPE)`
  直调——旧写法摘整类型 listener，会连 peer-net / 插件订阅一起杀。`RECV_TIMEOUT_SECS=1`
  与事件循环 `recv_timeout` 语义不变（flume 同签名）。
- **移动装配时序（本票新增的装配事实）**：中枢泵任务经 `DiscoveryPorts::spawn` 派生到宿主
  运行时，而中枢的进程级出口走 `ports::ports()`。移动端**不经**桌面的 host-kit 域装配链
  （移动的 `impl Host for WasmPluginState` 是自持 adapter，端口逐次按插件实例传参，从不装
  全局），故必须由宿主开机期显式装一次：`bedcode-mobile/src-tauri/src/lib.rs` setup 内调用
  `plugin::host_domains::mdns::register_process_ports()`（`install_ports` 首装胜出、幂等），
  且必须**早于任何 `subscribe_type`**（老链路 mdns 命令 / peer-net 节点启动）——否则
  `subscribe_type` 里 fail-visible panic。无头探针（`tests/shared_artifact_probe.rs`）走同一
  生产入口，不自造替身。

### D8 · 锁白名单（结构性封口）

新增源码锁 `packages/bedcode-discovery-engine/src/mdns_browse_singleton_lock.rs`，把两条
不变量变成可执行判据：

1. `.browse(` / `.stop_browse(` **仅**允许出现在 `packages/bedcode-discovery-engine/src/type_hub.rs`；
2. `ServiceDaemon::new` **仅**允许出现在 `packages/bedcode-discovery-engine/src/engine.rs`。

扫描面 = 仓库根 `packages/**/src` + 双端 `src-tauri/src` 全部 `.rs`（排除 `target/` 与锁
自身）；**跳过纯注释行**（文档里提到 `host-mdns.browse(...)` / `ServiceDaemon::new` 是合法
散文，不是调用点）；哨兵文件 + 文件数下限防「枚举为空 → 断言恒真」。白名单精确到**文件**
（不是目录）：新增调用点必须显式改表并写明理由，改动必然出现在 diff 里。

> 实施修订（2026-10-16）：锁原定落 `tests/`，与治理锁
> `bedcode-desktop/src-tauri/tests/capability_crates_unit_tests_only.rs` 冲突（该锁禁止
> `packages/bedcode-*` crate 出现 crate 根 `tests/`）——改按先例
> `packages/bedcode-server-peer-net/src/dependency_direction_lock.rs` 落 `src/` 内模块，
> `lib.rs` 以 `#[cfg(test)] mod mdns_browse_singleton_lock;` 接线。

`packages/bedcode-server-peer-net/src/dependency_direction_lock.rs` 扩展：`FORBIDDEN_DEPS`
增 `bedcode-discovery-engine`（中枢句柄以纯值经 `MdnsPort` 注入）；新增反向断言——
`bedcode-peer-net` 清单不得引 `bedcode-discovery-engine`，把「引擎 → 共享引擎单向」这条
既有合法边（`disable_virtual_interfaces`）钉死（反向会成环）。

### D9 · 零 WIT / 零 ABI 变更

中枢是**引擎机制**不是产品语义（ADR 0022）：它只回答「机制怎么做」，不认识会话 / 设备 /
文件传输等产品概念，宿主侧零新增业务类型 / 状态 / 存储 / 路由。`host-mdns` WIT 面与
`MdnsPort` 之外的契约零变化 ⇒ **不触发 ABI bump**，双端同步部署（同一份 `packages` 源码）。

## 后果

- **修复**：同类型多方 browse 互相杀死事件流的根因消除；peer-net 与插件 / 移动旧链路
  在同一类型上共存，互不可见症状消失。
- **结构性收益**：慢消费者不再阻塞 mdns-sd 发送线程（`try_send` 非阻塞 + 限频丢弃日志）；
  重查机制唯一化（不再有「换绑 receiver」散落各消费者）；退订与归零拆除语义单一。
- **日志面**：中枢生命周期日志齐备——`mdns type hub created` / `reused`（debug）/
  `rebrowse`（`reason` 字段区分 `requery` / `join-rebrowse`）/ `drained-stop`，均结构化
  字段（AGENTS §8）。
- **不做项**（明确排除）：
  - 不改 `host-mdns` WIT / 不动 ABI；不升级 / 不 monkeypatch mdns-sd（0.20.1 不动，不赌
    上游修复）；
  - 不新建第二个 `ServiceDaemon`（守护唯一创建点红线不可越）；
  - 不把 30s 周期节奏搬进引擎（节奏留消费方，机制归中枢）；
  - 不借机重构 `advertiser` / 广播面 / 移动旧链路其他逻辑（迁移仅做浏览漏斗切换）；
  - peer-net 测试保持注入式（`tests/*.rs` 不经真实守护、不直调 daemon）；未来确需真守护
    用例必须登记锁白名单并写明理由。
- **未来第三处直调 browse** 由 D8 源码锁拦截（白名单精确到文件、条目注释写明理由）。

## 验证

只列**实际跑过**的门禁（逐票记录见 `.scratch/2026-10-16-mdns-browse-fanout/issues/`）：

| 票 | crate（各自 crate 根） | 结果 |
| --- | --- | --- |
| 01 | `packages/bedcode-server-base` `cargo test` | 28 passed / 0 failed（基线 25 + 订阅守卫生命周期 3 例） |
| 02 | `packages/bedcode-discovery-engine` `cargo test` | 42 passed / 0 failed（基线 34 + `type_hub::tests` 8 例） |
| 03 | `packages/peer-net` `cargo test` | lib 85 passed（基线 81 + 4）+ `discovery_injection` 1 + `harness` 9 + `pause_symmetry` 4 + `shared_dirs` 6 + `transfer_session` 13，全绿 |
| 04 | `packages/bedcode-server-peer-net` `cargo test` | 56 passed / 0 failed |
| 04 | 移动 `bedcode-mobile/src-tauri` `cargo test --no-fail-fast` | 37 个测试二进制全绿，395 passed / 0 failed |
| 04 | 桌面 `bedcode-desktop/src-tauri` `cargo test --no-fail-fast` | 37 个测试二进制全绿，仅 1 失败 `capability_crates_unit_tests_only`（`bedcode-host-api-core/tests/boundary_lock.rs`，**base 既有、与本改动无关**，主仓同存同败） |
| 04 | `cross-end-tests` `cargo test --test mdns_health_flow` | 1 passed / 0 failed（桌面广播 → 移动旧链路发现 / `/api/health` 探测 / stop 清列表） |
| 05 | `packages/bedcode-discovery-engine` `cargo test` | 45 passed / 0 failed（含新锁 3 例） |
| 05 | `packages/bedcode-server-peer-net` `cargo test` | 56 passed / 0 failed（含扩展后依赖方向锁 3 例） |

单测矩阵（`type_hub::tests`，spec §7-1…7，全部在注入 fake 上零触网）：唯一 browse +
全员同事件 / requery 换绑 + 缓存重放广播全员 / 合并窗前缘吞并 + 后缘合并 / 守卫退订 +
归零摘除 + 重建幂等 / 惰性剔除 + 晚到守卫幂等 / 泵退出后重建（R7）；附加契约：空服务类型
触网前拒绝、慢消费者只丢不阻塞不剔除。

变异自检（均先红后绿并还原，`git diff` 确认零残留）：

- 中枢退回「每订阅者各自 browse（后到覆盖）」⇒ 矩阵 1 红；
- 关闭惰性剔除 / 关闭后缘补齐 ⇒ 矩阵 5 / 矩阵 2、3 红；
- peer-net `request_requery` 空实现 / 事件循环断连不退出 / stop 泄漏守卫 / 去掉自播回显
  过滤 ⇒ 各自用例红；
- 锁：删 `BROWSE_ALLOWLIST` 条目 / 在 `engine.rs` 加一处代码行 `.browse(` ⇒ 浏览锁红；
  把 `SHARED_ENGINE_MANIFEST` 指向真声明 `bedcode-discovery-engine` 的清单 ⇒ 依赖方向锁红。

**全量回归验证（spec §7 集成 / 真机清单）留票 07**，本 ADR 不预写其结果。

## 与既有 ADR 的关系

- **ADR 0042（双端 mDNS 引擎共享守护）**：本 ADR 是其**listener 级补完**。0042 收敛了
  「一个进程一个 `ServiceDaemon`」；本 ADR 收敛「同一服务类型一个 listener 槽的所有者」。
  两条不变量互补，共同构成完整的 mDNS 单例结构（守护唯一创建点 + 类型 browse 唯一出口）。
- **ADR 0022（宿主 / 插件边界）**：中枢是**引擎机制**（回答「机制怎么做」），不携带产品
  语义——B1–B6 零命中，宿主侧未新增业务类型 / 状态 / 存储 / 路由 / 业务默认值。中枢落在
  共享能力域 crate（ADR 0035 / 0037），不是宿主。
- **ADR 0018 / 0019**：双端同引同一份 `packages` 源码，移动契约独立轴不适用；`host-mdns`
  WIT 面零变化 ⇒ **不触发 ABI bump**，无需双端契约同步流程。
