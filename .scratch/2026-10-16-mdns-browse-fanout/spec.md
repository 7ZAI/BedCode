# mDNS 类型级浏览中枢：单 listener 不变量 + 进程内 fan-out（方案 1）

> 用户裁决（2026-10-16）：「按方案 1 写文档」→ 同日「确定决策 完善文档」——§6 的
> 推荐项全部采纳为**最终裁决**，各票按此实施，实施期不再重复征询。
>
> 方案 1 = **共享守护上每个服务类型全进程只允许一次 `daemon.browse()`**；所有消费者
> （peer-net 发现循环、各插件的 `host-mdns` 自建 browse、移动端旧链路浏览）在进程内
> 经同一「类型中枢」fan-out 拿事件；重查（requery）也收敛为中枢统一执行。
>
> 触发：双端 file-transfer 互相发现失败（纯排查结论见 §0/§1）。根因是 peer-net 的
> 周期/即时重查反复 `daemon.browse("_bedcode-peer._tcp.local.")`，按 mdns-sd 的
> 「每类型单 listener、后到覆盖先到」语义，把插件 browse 的 listener 覆盖成死信道。
>
> 前序：ADR 0042（双端 mDNS 引擎共享，已实施 2026-10-09）完成了**守护级**共享
> （一个进程一个 `ServiceDaemon`，创建点唯一于 `engine.rs`），但**listener 级**
> 不变量缺位——本规格补齐这一层。

## 0. 事实基线（实测，2026-10-16，桌面 Ubuntu + 移动 Android 真机）

### 0.1 症状与日志时间线

双端现象：宿主层 peer-net 互相发现正常（每 30s `peer discovered`），但 file-transfer
设备列表永远没有对方；无任何报错。

| 时刻（本地） | 端 | 事件 | 日志出处 |
| --- | --- | --- | --- |
| 09:10:56.275111 | 桌面 | file-transfer 完成总线订阅（含 `…::mdns:found`） | `~/.local/share/com.bedcode.app/logs/runtime.2026-10-16.log:292` |
| 09:10:56.275300 | 桌面 | 插件自建 browse 建立 `mdnsbr-2be2450e`（引擎 `daemon.browse`） | 同上 `:301` |
| 09:10:56.277453 | 桌面 | peer-net 发现守护启动（其**初始 browse** 在启动日志前一刻） | 同上 `:306` |
| 09:10:56.277478 | 桌面 | peer-net 节点广播登记（owner=host） | 同上 `:307` |
| 09:11:05 / :26 / :56 | 桌面 | requery：`periodic requery issued` + **`browse receiver swapped`** + `search started` 三连（覆盖机制的现场证据） | 同上 `:478-:481`、`:494-:496`、`:503-:505` |
| 09:13:27.726 | 移动 | 共享守护初始化（与桌面同源引擎） | `bedcode-mobile/.dev-logs/android-dev.2026-10-16.log:188` |
| 09:13:27.727 | 移动 | 插件自建 browse 建立 `mdnsbr-ce993b6c` | 同上 `:189` |
| 09:13:27.728 | 移动 | file-transfer 完成总线订阅（`mdns:found/lost` 等 6 topic） | 同上 `:190-:195` |
| 09:13:27.742 | 移动 | peer-net 发现守护启动（同一守护，初始 browse 即覆盖插件 listener） | 同上 `:200` |
| 09:13:27.811 起 | 移动 | requery 三连（`requery issued` + `receiver swapped` + `search started`），每 30s 复现；09:15:00 起 refresh 即时 requery 4 连发 | 同上 `:213-:215`、`:263-:290` |
| 全天 | 双端 | **引擎 browse 循环零后续日志**：无 `mdns browse self-broadcast filtered`、无 `mdns browse loop exited` | 引擎 debug 在 dev 构建 file filter 可见（`bedcode-desktop/src-tauri/src/system/logging.rs:467`：dev → `debug`；peer-net debug 正常落盘可证） |
| 全天 | 双端 | peer-net `peer discovered` 正常（其 requery 每次换绑最新 receiver） | 桌面 `runtime.2026-10-16.log:535`；移动 `android-dev...:208-209` 等 |

### 0.2 mdns-sd 0.20.1 的关键行为（`~/.cargo/registry/.../mdns-sd-0.20.1/src/service_daemon.rs`）

1. **每服务类型单 listener**：`service_queriers: HashMap<String, Sender<ServiceEvent>>`
   仅一条（`:1042`）。
2. **重复 browse 覆盖**：`exec_command_browse` 对非重传命令执行
   `self.service_queriers.insert(ty.clone(), listener.clone())`（`:3570-3574`），库注释
   原文：\"If there is already a `listener`, it will be updated, i.e. **overwritten**\"。
3. **事件只投当前 listener**：`call_service_listener(&self.service_queriers, ty_domain, event)`
   （`:3118`）——被顶掉的旧 receiver 从此收不到任何事件。
4. **旧 listener 被重传链永久保活**：每次 browse 都会
   `add_retransmission(next_time, Command::Browse(ty, delay, cache_only, listener))`
   （`:3592-3595`），delay 指数翻倍、**封顶 3600s 且无终止条件**——旧 sender 的克隆
   一直挂在重传队列里，旧 receiver 的通道**永不断开**：既不收事件、也不报错、
   `browse loop exited` 永不触发（这就是 §0.1「引擎零日志」的成因）。
5. **缓存重放只给新 listener**：`query_cache_for_service(&ty, &listener, now)`
   （`:3577`）——peer-net 每 30s 的 `instance found` 即来自此；也是中枢 requery 后
   「所有订阅者一起复活/续期」的机制来源。

### 0.3 现状调用点清单（全仓 `ServiceDaemon` 浏览面，grep 全集）

| 调用点 | 文件:行 | 现状 |
| --- | --- | --- |
| 守护唯一创建点 | `packages/bedcode-discovery-engine/src/engine.rs:52` | ✅ 唯一，不动 |
| peer-net 初始 browse | `packages/peer-net/src/discovery.rs:569` | ❌ 直调（覆盖插件 listener 的元凶） |
| peer-net 重查再 browse | `packages/peer-net/src/discovery.rs:753` | ❌ 直调（每 30s + 即时复现） |
| peer-net stop 退订 | `packages/peer-net/src/discovery.rs:419` | ❌ 直调（`stop_browse` 摘整类型） |
| 插件 browse | `packages/bedcode-discovery-engine/src/engine.rs:147` | ⚠️ 引擎内直调（每个插件各建一条，多插件同类型互相覆盖） |
| 插件 stop | `packages/bedcode-discovery-engine/src/engine.rs:271` | ⚠️ 同上 |
| 移动旧链路浏览 `_bedcode._tcp.local.` | `bedcode-mobile/src-tauri/src/mdns/discovery.rs:57,196` | ⚠️ 宿主直调（今天与 peer 类型不重叠，但破坏「单浏览漏斗」结构锁） |

### 0.4 依赖方向约束（决定接口落点）

- `bedcode-discovery-engine → bedcode-peer-net` 已存在（`engine.rs:56` 调
  `disable_virtual_interfaces`，定义于 `packages/peer-net/src/discovery.rs:507`）⇒
  **peer-net 不得反向依赖引擎**（会成环）。
- `bedcode-server-peer-net` 依赖方向锁
  （`packages/bedcode-server-peer-net/src/dependency_direction_lock.rs`）要求其「只向下
  依赖 server-base / peer-net，不引 discovery-engine」（`engine_ports.rs:28` 同述）
  ⇒ 中枢句柄需以**纯值/基础类型**注入 peer-net。
- 结论：中枢订阅出口 = `flume::Receiver<mdns_sd::ServiceEvent>`（mdns-sd 的 `Receiver`
  无法在库外构造；flume 0.12 已是 peer-net / mdns-sd 既有依赖，版本同源）+ 退订守卫
  + 重查闭包。`bedcode-server-base::ports::MdnsPort` 扩两个方法承载。

### 0.5 为什么不是「双端单 wasm」问题（排除记录）

- 断点在宿主进程内、总线 `publish` 之前，**根本到不了 wasm**；
- 反证：双端 bus 订阅成功（桌面 `:292` 起 file-transfer 9 topic / 移动 `:190-:195`
  6 topic，均含 `mdns:found/lost`）、双端 guest 都能收总线消息（桌面 terminal-session
  收 `ws:client-connect`；移动 terminal-session 收 `ws:message` 起 live 终端），单
  wasm 的事件出站并集（`on-message`/`on-bus-message`）在两端都被证明可用；
- 症状双端一致是因为**同一套宿主共享守护接线**双端同源，与 wasm 是否同一份无关。

## 1. 根因（一句话）

**mdns-sd 每服务类型只有一个 listener 槽位，而 peer-net 的重查机制建立在「重复
browse 覆盖旧 listener」这个语义上；共享守护迁移（ADR 0042）把 peer-net 与插件
browse 放到了同一个守护，于是 peer-net 每次重查都会静默杀死插件 browse 的事件流。**

插件激活流程顺序（双端同构，见 §0.1 时间线）：插件 `ensure_mdns_browse` → 插件请求
peer-net 节点启动 → peer-net 初始 browse 覆盖插件 listener ⇒ **激活后 2ms（桌面
`301→306`）/ 15ms（移动 `189→200`）插件 browse 即死亡**；此后每次 30s 周期 requery /
refresh 即时 requery 只继续覆盖（`receiver swapped` 三连为现场证据）。
`com.bedcode.file-transfer::mdns:found` 从未上总线 ⇒ guest/前端零事件 ⇒ 双端互不可见。

## 2. 目标不变量与目标架构

### 2.1 不变量（实施后由源码锁钉死）

1. **全进程对同一服务类型，`daemon.browse()` / `daemon.stop_browse()` 只允许由发现
   引擎的类型中枢调用**；`ServiceDaemon::new` 维持唯一创建点（`engine.rs`）。三者合
   一把源码锁覆盖。
2. **类型中枢对每个活跃服务类型只持有一条 daemon 浏览**；任意数量订阅者进程内
   fan-out，新增订阅者不得触发再次 `daemon.browse`。
3. **重查语义唯一**：周期/即时重查都请求中枢统一执行（重发 browse + receiver 换绑 +
   缓存重放 fan-out）；任何消费者不得自行重查。周期节奏属消费方（peer-net 30s 不动）。
4. 引用计数归零 → 中枢立即 `stop_browse` 并移除；再有订阅者时重建（幂等）。
5. **新订阅者享有「立即缓存重放」等价语义**（原始 per-plugin browse 的行为不缩水）：
   由「加入触发的一次合并重查」保证（规则见 §2.3-D3）。

### 2.2 架构对比

```text
现状（bug）：
  ServiceDaemon（共享）
  ├─ service_queriers["_bedcode-peer._tcp.local."] ← 只有 1 个槽，后到覆盖
  ├── peer-net 发现循环 ── daemon.browse ×(初始 + 每30s + 即时) ── 每次都覆盖
  └── 插件 browse（引擎） ── daemon.browse ×1（激活时） ── 2ms 后被杀 → 死信道

目标（方案 1）：
  ServiceDaemon（共享，创建点唯一：engine.rs）
  └─ service_queriers["<type>"] ← 唯一 browse，只由中枢代持
        │
        ▼
  类型中枢（discovery-engine::type_hub，进程级 HashMap<service_type, Hub>）
  ├─ 订阅者表：Vec<(sub_id, flume::Sender<ServiceEvent>)>（fan-out，容量 64，
  │            Full→丢弃+限频 debug，Disconnected→惰性剔除）
  ├─ 退订：订阅守卫 Drop → Unsubscribe(sub_id) → 移除发送端（通道关闭）；
  │        guard 丢失时由 fan-out 的 Disconnected 惰性兜底
  ├─ 重查控制：request_requery →（100ms 合并窗）→ daemon.browse(type) →
  │           receiver 换绑 + 缓存重放 → 广播给全部订阅者
  └─ 生命周期：首个订阅建档（唯一 browse）；归零 → stop_browse + 移除
        │
        ├── 订阅①：peer-net 发现循环（原 browse/换绑/requery 机器全部删除）
        └── 订阅②…N：插件 host-mdns 自建 browse（引擎 browse 改为挂中枢）
             ＋ 移动旧链路 `_bedcode._tcp` 浏览（同迁中枢）
```

### 2.3 接口形态与订阅语义（裁决见 §6，签名落点以此为准）

`packages/bedcode-discovery-engine/src/`（中枢实现 + 引擎 browse 切换）：

```rust
/// 订阅某服务类型的中枢 fan-out。首个订阅者触发唯一 daemon.browse；其余只挂表。
/// 返回值 = 订阅句柄（receiver + 退订守卫，包装形态实施时可调，语义不可变）：
///   - receiver 移交给事件循环；
///   - 守卫 Drop ⇒ 中枢退订、该订阅通道关闭；守卫随持有者生命周期释放。
pub fn subscribe_type(service_type: &str) -> Result<MdnsBrowseSubscription, String>;
/// 请求该类型立即重查（合并窗内吞并/后缘补齐规则见 D3；广播给全部订阅者）。
pub fn request_type_requery(service_type: &str);
```

- `engine::browse()`（`:147` 一带）：删除 `daemon().browse`，改 `subscribe_type`；
  `BrowserEntry` 增持订阅守卫。`stop_browser_inner`（`:271`）：删除
  `daemon.stop_browse`，条目移除即守卫 Drop → 通道关闭 → 浏览任务 `recv` 断开退出
  （`mdns browse loop exited` 日志保留，退出形态与今日 stop_browse 断联一致）。
- `publish_dir_event` / 属主仲裁 / `is_self_broadcast` / 句柄表 / 权限门全部不动。
- 中枢任务走 `DiscoveryPorts::spawn`（与现有浏览事件循环同源）；daemon 交互抽一个
  小注入缝（微型 trait/闭包，生产 = `ServiceDaemon` 包装，测试 = fake），
  供单测在不触网前提下验证 fan-out / 换绑 / 引用计数（§7）。

`packages/peer-net/src/discovery.rs`（peer-net 保持零 server-base/engine 依赖，收纯值）：

```rust
/// 宿主注入的浏览订阅（值形态）。peer-net 不解释 guard——仅随 DiscoveryDaemon
/// 生命周期持有并释放（释放 = 退订）。
pub struct PeerMdnsBrowse {
    pub receiver: flume::Receiver<ServiceEvent>,
    pub guard: Box<dyn Any + Send + Sync>,
    pub request_requery: Arc<dyn Fn() + Send + Sync>, // 绑到中枢 request_type_requery(SERVICE_TYPE)
}

pub fn spawn_peer_mdns_daemon(
    node: &PeerNetNode,
    running: &RunningNode,
    config: DiscoveryConfig,
    daemon: ServiceDaemon,        // 仍用于 register/unregister（广播面，不动）
    browse: PeerMdnsBrowse,       // 中枢订阅（宿主壳铸造）
) -> Result<DiscoveryDaemon>;
```

- 删除：初始 `daemon.browse`（`:568-570`）、`swap_tx/swap_rx` 换绑机器（`:683-687`）、
  `run_requery_loop` 的 browse 分支（`:753` 改为调用 `request_requery` 闭包）、stop 路径
  的 `daemon.stop_browse`（`:419`）。周期节奏（REQUERY_INTERVAL=30s）**留在 peer-net**
  （业务节奏属消费方）；机制（重发 browse / 换绑 / 重放）归中枢。
- `DiscoveryDaemon::request_requery()`（`:478`）改为转调闭包，对外语义不变。

`packages/bedcode-server-base/src/ports.rs`（端口扩展，双端宿主实现）：

```rust
pub trait MdnsPort: Send + Sync {
    // 既有三方法（shared_daemon / register_host_service / stop_host_service）不变
    /// 打开/复用类型中枢订阅（实现 = bedcode_discovery_engine::subscribe_type 的转换）
    fn subscribe_type(&self, service_type: &str)
        -> std::result::Result<MdnsTypeSubscription, String>;
    /// 请求类型级即时重查（实现 = request_type_requery）
    fn request_type_requery(&self, service_type: &str);
}
```

- server-base 新增 `flume = "0.12"` 依赖（与 peer-net / mdns-sd 传递依赖同版）。
- 订阅句柄在两个 crate 各有一个值形态（engine 内部型 / server-base 端口型），转换在
  宿主 glue；**硬性要求：不得提供 receiver 与守卫生命周期脱钩的构造**（否则退订泄漏，
  由 §7-单测 4 钉）。

**D3 重查合并规则（定稿）**：

- 周期/即时重查（peer 30s、refresh 即时）：**前缘吞并**——距该类型上次 browse
  < 100ms 的请求直接丢弃（同窗口内请求共享同一次缓存重放）。
- 新订阅者加入：**后缘补齐**——窗内多次加入合并为一次，在最后一次加入后 ~100ms 执行
  一次 browse，确保每个新订阅者都拿到自己的缓存重放（不缩水的 2.1-⑤）。

### 2.4 装配时序（修复后）

```text
插件激活                         宿主壳（server-peer-net / 移动 peer 装配）
  │ ensure_mdns_browse
  │  └─ engine::subscribe_type("_bedcode-peer._tcp.local.")
  │       └─ 中枢建档：daemon.browse ×1 ───────────┐
  │       └─ 守卫随插件 browse 条目托管            │ 此刻起该类型 browse 唯一
  │ 插件请求 peer-net 节点启动                      │
  │  └─ spawn_peer_mdns_daemon(..., browse=PeerMdnsBrowse)
  │       ├─ 仅挂表（复用同一中枢，不再 browse）
  │       ├─ 加入触发后缘重查 → 缓存重放同时到达两订阅者
  │       └─ requery 闭包 → 中枢（30s 节奏照旧）
  └─ 事件 fan-out：同一事件分别进 peer 缓存 / 插件定向 topic（publish_dir_event）
```

激活后任何时刻的 requery（peer 30s / refresh 即时）→ 中枢重发 browse → 缓存重放
→ **peer 与插件同时收到重放**（插件设备续期/首屏因此天然可用）。

## 3. 涉及面清单

| 变更面 | 文件 |
| --- | --- |
| 中枢实现（新模块） | `packages/bedcode-discovery-engine/src/type_hub.rs`（订阅表 / 唯一 browse / requery 合并 / 守卫退订 / 惰性剔除 / 注入缝） |
| 引擎 browse 切换 | `packages/bedcode-discovery-engine/src/engine.rs`（browse / stop_browser_inner / BrowserEntry 持守卫 / 模块头注释补 listener 级不变量） |
| peer-net 订阅化 | `packages/peer-net/src/discovery.rs`（`PeerMdnsBrowse` / spawn 签名 / 事件循环 / requery / stop；`node.rs` 文档引用同步） |
| 端口扩展 | `packages/bedcode-server-base/src/ports.rs` + `Cargo.toml`（flume） |
| 宿主接线（桌面） | `packages/bedcode-server-peer-net/src/lib.rs:1011-1033`（spawn 处铸造订阅）、`:1459`（requery 转发）；`bedcode-desktop/src-tauri/src/server/ports_impl.rs:318`（MdnsPort 扩实现） |
| 宿主接线（移动） | `bedcode-mobile/src-tauri/src/peer/ports.rs:226`、`peer/net.rs`（注释同步）；`bedcode-mobile/src-tauri/src/mdns/discovery.rs:54-58,196`（旧链路迁中枢：`subscribe_type` 持有句柄，stop 改 drop；`recv_timeout` 同签名适配，`RECV_TIMEOUT_SECS=1` 常量不变——`system/constants/mdns.rs:6`） |
| 锁（新） | `packages/bedcode-discovery-engine/src/mdns_browse_singleton_lock.rs`（§4-T5；**实施修订 2026-10-16**：由 `tests/` 改 `src/` 内模块——治理锁 `capability_crates_unit_tests_only` 禁止 `packages/bedcode-*` crate 根 `tests/`，先例 `server-peer-net/src/dependency_direction_lock.rs`） |
| 锁（扩展） | `packages/bedcode-server-peer-net/src/dependency_direction_lock.rs`（不引 discovery-engine；engine→peer-net 白名单 `disable_virtual_interfaces`） |
| 文档 | `docs/adr/0054-mdns-single-browse-per-type-and-fanout-hub.md`（新开）、ADR 0042 增补执行记录、两端 `docs/code-map.md` 防回接锁索引 |

零改动面（明确写出）：`host-mdns` WIT / ABI（无契约变更、无需 bump）；`advertiser` /
`register_host_service` 广播面；插件 guest 与双端前端（下游链路本次排查证明已就绪）；
mdns-sd 版本（0.20.1 不动，不赌上游修复）；peer-net 既有测试全部为注入式
（`packages/peer-net/tests/{discovery_injection,harness,pause_symmetry,shared_dirs,transfer_session}.rs`
不经真实守护、不直调 daemon——grep 实证），改造后以「注入通道喂合成事件」适配即可。

## 4. 票据拆分

| 票 | 内容 | 门禁 |
| --- | --- | --- |
| T1 | server-base 端口词汇：`MdnsTypeSubscription` + `MdnsPort` 扩 `subscribe_type` / `request_type_requery` + flume 依赖 | server-base `cargo test` 绿 |
| T2 | 引擎中枢：`type_hub.rs` + 注入缝 + `engine::browse`/`stop_browser_inner` 切换 + 合并窗/后缘补齐 + 单测（§7 矩阵 1-4、7） | engine `cargo test` 绿；变异自检（注入「覆盖旧 listener」必红） |
| T3 | peer-net 订阅化：`PeerMdnsBrowse` + spawn 签名 + 删 swap/直调 browse + requery 改投递 + stop 清理；注入式测试适配 | peer-net `cargo test` 绿 |
| T4 | 宿主接线：server-peer-net（`:1011` 铸造 + `:1459` 转发）+ 桌面 ports_impl + 移动 peer/ports.rs + 移动旧链路浏览迁中枢 | server-peer-net / 桌面 / 移动 `cargo test` 绿 |
| T5 | 锁与索引：① 源码锁 `mdns_browse_singleton_lock.rs`——`packages/**/src` + 双端 `src-tauri/src` 中 `ServiceDaemon` 的 `.browse(` / `.stop_browse(` 仅允许 `type_hub.rs`，`ServiceDaemon::new` 仅允许 `engine.rs`（白名单精确到文件 + 注释写明理由）② server-peer-net 依赖方向锁扩展 ③ 中枢生命周期日志（created / reused / join-rebrowse / requery / drained-stop）④ 两端 code-map 防回接锁索引登记 | 锁测试绿 + 变异自检（删白名单条目必红） |
| T6 | 文档：ADR 0054（单 listener 不变量 + 中枢）+ ADR 0042 增补 + code-map；CHANGELOG 按 §2 发布前汇总（本票不写） | 文档核对 |
| T7 | 全量验证：§7 清单逐项「跑了/没跑」 | 见 §7 |

依赖：T1 → T2 / T3 → T4 → T5 → T6 → T7（T2 的注入缝语义先落，T3 与 T2 可并行）。

## 5. 风险与对策

| # | 风险 | 对策 |
| --- | --- | --- |
| R1 | fan-out 订阅者通道满 → 事件丢弃 | 容量 64（≥ mdns-sd 自身 bounded(10)）；`try_send` 区分 `Full`（丢弃 + 限频 debug）与 `Disconnected`（剔除订阅者）；mdns 事件可重放（30s requery / 缓存重放兜底），丢弃只延迟可见性不是数据丢失 |
| R2 | 中枢任务层级阻塞 → 拖慢守护线程 | 现状慢消费者会阻塞 mdns-sd 发送线程；中枢改泵任务 + 非阻塞 try_send 属**改善**；泵任务按 `spawn_with_error_boundary` 同族包装，panic 不拖垮进程 |
| R3 | 订阅者全部退出后重建中枢，短暂丢事件窗口 | 归零即 `stop_browse`；重建即 `daemon.browse`，缓存重放天然补齐（与现状换绑等价）；重建打 info 日志留痕 |
| R4 | Receiver 类型切换（mdns-sd Receiver → flume Receiver）API 差异 | 事件值同型 `ServiceEvent`；`recv_async` / `recv_timeout` / 通道断开语义 flume 0.12 全有（peer-net 已依赖 flume）；移动旧链路循环自带 1s 超时轮询（`RECV_TIMEOUT_SECS=1`），退出语义兼容 |
| R5 | 双端装配漏改一侧 → 运行期行为分裂 | 无 ABI，编译期即断（签名变更 + 锁）；T4 双端均跑测试；真机清单双端同验 |
| R6 | 未来第三处直调 browse 绕过中枢 | T5 源码锁（白名单精确到文件、条目注释写明理由），条目与 code-map 索引同源维护 |
| R7 | 中枢单点故障（任务 panic / 控制通道死） | 中枢任务包错误边界；下次 subscribe / requery 检测控制通道已关 → 以 `drained` 同路径重建（幂等）；单测 7 覆盖 |

## 6. 已确认裁决（2026-10-16，用户「确定决策」）

- **D1 中枢落点**：新模块 `packages/bedcode-discovery-engine/src/type_hub.rs`；锁白名单
  = `type_hub.rs`（唯一 `browse`/`stop_browse`）+ `engine.rs`（唯一 `ServiceDaemon::new`）。
- **D2 订阅通道**：容量 64；`Full` → 丢弃事件 + 限频 debug；`Disconnected` → 惰性剔除。
- **D3 重查合并窗**：100ms；周期/即时 requery 前缘吞并；新订阅者加入后缘补齐（§2.3）。
- **D4 排空策略**：守卫退订 + 归零**立即** `stop_browse` + 移除中枢（无 linger）；
  重建幂等；guard 丢失路径由惰性剔除兜底。
- **D5 移动旧链路浏览**：`mdns/discovery.rs` **迁移中枢**（不留锁白名单——锁按全集
  做结构性封口）；`stop` 改守卫释放。
- **D6 ADR 形态**：**新开** `docs/adr/0054-mdns-single-browse-per-type-and-fanout-hub.md`
  （跨 crate 机制不变量）；ADR 0042 在「验证/与既有 ADR 的关系」处增补一行执行记录
  指向 0054；两端 code-map 登记新锁。
- **D7 peer-net 接口形态**：**值注入** —— `PeerMdnsBrowse { receiver, guard, request_requery }`
  （§2.3）；不为单一消费者抽端口 trait，未来出现第二消费者再评估。

## 7. 验证清单（Definition of Done）

单测矩阵（T2 注入缝 + 变异自检）：

1. 同类型两次 subscribe：**只**发生一次 `daemon.browse`；两订阅者收到同一事件
   （变异：退回「后到覆盖」实现必红）。
2. requery 换绑：旧 receiver 失效后，两订阅者仍收到换绑后事件与缓存重放。
3. 合并窗：100ms 内两次 requery 请求只产生一次 browse；窗内加入 + 加入后缘补齐各 1 次。
4. 守卫退订：drop 订阅句柄 → 该通道关闭、订阅者移除；归零 → `stop_browse` 恰一次 +
   中枢移除；重建幂等（重新 subscribe 恢复）。
5. 惰性兜底：不经守卫直接 drop receiver → 下一次 fan-out 剔除，不影响其他订阅者。
6. peer-net：签名改造后既有注入式用例全绿；删 swap 机器无残留引用；`request_requery`
   闭包转调生效。
7. 中枢重建：控制通道关闭后（模拟任务退出）再次 subscribe 能重建并 browse。

集成/全量（按 §10 收尾口径）：

- `packages/peer-net`、`packages/bedcode-discovery-engine`、`packages/bedcode-server-peer-net`、
  桌面 `src-tauri`、移动 `src-tauri` 各自 `cargo test` 全量；
- `cross-end-tests`：评估后跑（本改动不触认证/会话/终端面，但 peer 发现属双端数据面，
  可跑则跑）；
- 真机清单（双端实测，逐项记录）：
  a) 双端同时打开文件传输 → 互见（≤30s，且点刷新即时出现）；
  b) 断开一端 → 对端列表 goodbye 消失；重启 → 自动恢复；
  c) 插件停用（purge）→ 订阅剔除 + 插件 browse 句柄回收（`browse loop exited`）日志；
     peer-net 循环不受影响；
  d) 慢消费者（人为压测）不阻塞 peer-net 续期与第二订阅者；
  e) 移动旧链路发现页面（`_bedcode._tcp`）回归无退化（迁移后仍能发现桌面）；
  f) journal/adb 日志核对：`type hub created/reused/join-rebrowse/requery/drained-stop`
     齐备、peer `instance found` 30s 节奏不变、无异常静默。

## 8. 纪律与不做项

- 不改 `host-mdns` WIT / 不动 ABI（无契约变更）；不做 mdns-sd 上游升级或 monkeypatch；
- 不新建第二个 `ServiceDaemon`（守护唯一创建点红线下不可越）；不把 30s 周期节奏搬进
  引擎（节奏留消费方，机制归中枢）；
- 不借机重构 `advertiser` / 广播面 / 移动旧链路其他逻辑（迁移仅做浏览漏斗切换）；
- peer-net 测试保持注入式（实证：`tests/*.rs` 不经守护）；如未来确需真守护用例，
  必须登记锁白名单并写明理由；
- 双端同步部署（同一份 packages 源码），移动不跟演轴不适用；
- 所有改动区注释同步复核（§6 注释规范）：peer-net `discovery.rs` 模块头「浏览侧已
  收敛到全局共享守护」段落、engine.rs `shared_daemon` 注释「切勿在别处另建守护」段
  （补写 listener 级不变量与中枢位置）、移动 `plugin/host_domains/mdns.rs:16` 红线
  注释、ADR 0042 关联段落。

## 附录 A：根因证据摘要（供实施对照，行号均经 2026-10-16 复核）

- 桌面被杀时序：`runtime.2026-10-16.log:301`（插件 browse 01:10:56.275300Z）→ `:306`
  （peer-net 守护 01:10:56.277453Z，初始 browse 在其前）⇒ ~2ms。
- 移动被杀时序：`android-dev.2026-10-16.log:189` → `:200` ⇒ ~15ms；`:213-:215` 起
  requery 三连每 30s 复现；`:263-:290` refresh 即时 requery 4 连发。
- 换绑机制现场证据：桌面 `:479-:481`、移动 `:213-:215`——`peer mDNS periodic requery
  issued` + `peer mDNS browse receiver swapped (periodic requery)` + `peer mDNS search
  started`（实现位置 `packages/peer-net/src/discovery.rs:683-687`）。
- 静默性证明：桌面全日志仅 daemon init（`:226`，01:10:43.733Z）+ 各 info，引擎 browse
  相关 debug **零条**；而 dev file filter 为全局 debug
  （`bedcode-desktop/src-tauri/src/system/logging.rs:467`），peer-net debug 正常落盘
  （`:312` 起）——排除「日志级别遮挡」。
- mdns-sd 源码行号（0.20.1 registry 副本）：`:1042`（单槽）、`:3570-3574`（覆盖语义）、
  `:3118`（只投当前）、`:3592-3595`（重传保活封顶 3600s 无终止）、`:3577`（缓存重放）。
- 下游链路已就绪的旁证（非根因，防误伤）：桌面 `MessageBus: plugin subscribed …
  mdns:found`（`:292`）；移动同项（`:190`）；双端 guest 收总线消息实证
  （terminal-session 两端的 ws 事件处理）。
- 宿主侧 peer 发现正常：桌面 `:535`（01:13:28Z）起、移动 `:208-:209` 起 `peer
  discovered` 持续；两 re-query 驱动力（30s 周期 + refresh 即时）在 §0.1 表内逐条列明。
