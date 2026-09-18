# 插件-宿主接口边界：host 能力只暴露引擎原语，业务编排归插件

## 背景

WIT 契约中各宿主能力接口的函数数对比悬殊：`host-storage` 3 个、`host-events` 3 个、`host-fs` 6 个，而 `host-peer` 有 **27 个**——几乎是对等网络四个模块（peer_net / peer_transfer / peer_receive / peer_remote）Tauri 命令面的 1:1 全量投影（issue 12 切换时的务实选择：与 Tauri 命令同一真源，切换最快）。

后果随迭代持续放大：

1. **每次新增业务功能要同步五处**：WIT → 双端 SDK 绑定 → 双端 host_impl → 插件翻译层 → devMock/测试 fixtures；
2. **DTO 翻译做两遍**：插件翻译层把宿主 DTO 再映射成前端 wire 形状，纯搬运；
3. **第二个消费者无法进入**：对等网络定位为宿主核心服务（spec 决策 11，未来投送/同步等能力挂载其上），但接口形状全是 file-transfer 的任务/历史/策略概念，其他插件被迫接受无关业务形状；
4. **ABI 面不稳定**：业务迭代频繁触碰契约，版本协商与双端 WIT 副本同步压力持续累积。

## 决定

**宿主 import 接口只承载「离开宿主就无法实现」的基础能力，且能力本身不得携带任何业务语义**——mdns、文件增删改查、HTTP、IO 这类中性原语；产品概念（设备列表、共享目录注册表、任务队列、历史、接收策略、设置项、重试编排）一律留在插件层，插件用既有基础接口（host-storage / host-plugin-database / host-bus / host-events）自建。

以 `host-peer` 为首个适用对象。初版裁决保留约 15 个函数；同日复审发现其中仍有状态 CRUD、领域错放与业务投影残留，按同一把尺子二次收紧为最终形态（host-peer 收缩至 11 个 + 新增 host-mdns / host-platform）。

### host-peer 最终原语面（11 个）

| 类别 | 函数 | 留在宿主的理由 |
|------|------|----------------|
| 连接 | `dial-peer(endpoint)` | TLS 引擎句柄持有方；endpoint（地址/端口/期望节点身份）由插件从发现事件解析后显式传入，宿主不再内藏 node-id → 地址解析表 |
| 连接 | `close(handle)` | 统一资源关闭：dial 返回会话句柄、send/pull 返回传输句柄，一律经 close 关闭（合并原 disconnect-peer 与 cancel）；「pending 接收批被关闭即拒绝」是闸门 fail-safe 默认的自然结果，不是独立业务函数 |
| 安全闸门 | `respond-consent` / `respond-transfer` | 两道闸门的应答通道：首连确认（ADR 0028）与接收批放行；fail-safe 默认（无应答/超时即拒）必须在宿主侧——应答原语是闸门的输入口而非业务流，没有它 ask 模式无法放行任何单个接收批（v1 曾误判 respond-transfer 可下沉，此处纠正） |
| 信任存储 | `list-trusted` / `revoke-trusted` | 信任存储是安全边界（ADR 0028） |
| 暴露 | `set-shared-roots(dirs)` | 引擎广播源的全量幂等同步；注册表 CRUD 真源移至插件侧 host-plugin-database（合并原 list/add/remove-shared-directories 三函数） |
| 读对端 | `list-shared-roots(session)` / `browse-directory(session, …)` | 单连接单请求线协议会话的动词镜像，按会话句柄寻址 |
| 写对端 | `send-files(session, …)` / `pull-files(session, …)`（增补续传偏移查询语义） | 数据面引擎（分块/校验/断点/加密）+ 断点真源在接收端落盘侧 |

所有对端寻址统一为**显式句柄**。读对端/写对端的形状确实带「文件」语义，但它们是线协议动词的镜像——协议本身就是文件浏览/传输协议；ABI 跟随稳定的协议动词而非易变的 UI，正是最不容易破 ABI 的切法。

### 新增 host-mdns（browse-only 纯能力）

```wit
interface host-mdns {
    /// 浏览某服务类型，返回 browser 句柄；发现/离开经消息总线推送
    browse: func(service-type: string) -> result<string, string>;
    stop-browse: func(browser-id: string) -> result<_, string>;
}
```

- 发现/离开经 bus topic `mdns:found` / `mdns:lost` 推送，payload 原样透传（instance-name / addresses / port / txt-records），宿主不做任何加工；
- **自我广播（register/advertise）不进插件 ABI**：「本机是谁」与宿主 TLS 监听器同生共死，是宿主身份而非任何插件的产品概念——留在 peer-net 启动流程自动完成，无插件激活时主机照样可被发现；
- **平台脏活留在引擎内部**：Android MulticastLock 按 browser 句柄引用计数、网卡变化重绑定、插件停用时句柄自动回收——引擎健壮性跟随 browse 生命周期，不下沉；
- 设备列表（去重/TTL 过期/展示名/能力位解读）= 发现事件的派生视图，由消费插件自建缓存——派生视图就是插件的活。

### host-mdns v2（mDNS 基础能力服务，2026-09-15 修订）

v2 将 host-mdns 从 browse-only 升级为**内核内建基础能力服务 `MdnsService`**（spec：`.scratch/2026-09-10-mdns-service-plugin/spec-basic-capability-service.md`，与 host-http / host-database 同构），契约扩展如下：

```wit
interface host-mdns {
    browse: func(service-type: string) -> result<string, string>;          // 不变
    stop-browse: func(browser-id: string) -> result<bool, string>;         // 不变
    advertise: func(config-json: string) -> result<string, string>;        // 新增
    stop-advertise: func(advertise-id: string) -> result<bool, string>;    // 新增
    is-advertising: func(advertise-id: string) -> result<bool, string>;    // 新增
}
```

- **单守护收敛**：全局唯一 `ServiceDaemon`（LazyLock 单例，init 一次性 `disable_virtual_interfaces`），peer-net 引擎与全部插件浏览/广播共享同一实例——消灭「双 daemon 同绑 5353 互抢多播包」的历史结构性病灶（真机实证：只发现自己、发现不了对端）；
- **事件定向投递**：browse 事件按属主发布 `mdns:found.<owner>` / `mdns:lost.<owner>`（owner = 发起 browse 的插件 id），payload 增量追加 `serviceType` / `browserId`（既有 4 字段不变）；消息总线按精确 topic 分发（bus.rs），非属主插件物理上订阅不到——业务隔离（需求①）；
- **广播原语（需求②扩展性核心）**：`advertise(config-json)` 的 serviceType / instanceName / port / txtRecords 全部由调用方构造传入，宿主只校验 serviceType 非空、零业务拼装；实例名缺省时宿主按 `{plugin}-{短指纹}` 默认（D4）；句柄带周期 re-announce 续期；**属主仲裁**——stop-browse / stop-advertise / is-advertising 先权限门（PERMISSION_MDNS）再属主校验，跨插件操作一律拒绝；
- **双表回收**：`purge_for_plugin` 回收某插件全部浏览 + 广播句柄，只碰本人，宿主（owner=host）与它插件登记不受影响；
- **零业务代码红线（D3）**：节点身份广播的 TXT / ServiceInfo 由 peer-net 引擎构造（节点身份/证书/能力位是引擎语义），MdnsService 只做注册 + 句柄登记（owner=host）——「自我广播不进插件 ABI」结论不变，仅登记路径收敛到基础服务，作为「host 与插件 advertise 共存于单守护、互不注销对方」的可验证凭据；
- **平台脏活落地修订**：Android MulticastLock 随单守护首次使用获取、常驻持有（幂等，不再随 browse 句柄增删）；全局发现桥接（`mdns:found`/`mdns:lost` 全局 topic）与缓存重发通道整体退役（D1）——file-transfer 为唯一消费方且一期同迁（D2），无迁移垫片；
- **消费插件迁移**：file-transfer 双端一期同步迁移为 activate 自建 browse + 订阅定向 topic、deactivate stop-browse（宿主 purge 兜底）；设备列表派生视图仍留前端缓存（wire 形状不变）。

### 新增 host-platform（通用平台能力域）

`pick-files` / `pick-folder` 是系统对话框能力，与 peer 领域无关，属错放——移入新的 `host-platform` 接口（与 `host-fs.request-auth` 的授权/平台域同级），供所有插件复用，不再绑定单一插件的流程。

事件总线 topic 调整：`peer:connection/consent/transfer/receive` 保持不变；`peer:devices` 随 `list-devices` 一并退役，职责由 `mdns:found`/`mdns:lost` 接管——事件推送本就是基础能力形态。

### 下沉插件（10 个）

| 泄漏项 | 去向 |
|--------|------|
| `list-devices` | 设备列表 = mDNS 发现事件的派生视图 → 插件订阅 `mdns:found`/`mdns:lost` 自建设备缓存（去重/TTL/展示名）；宿主侧 DiscoveryCache 守护与全量快照比对链路随之退役 |
| `list-transfers` / `clear-transfer-history` / `clear-receiving-history` / `list-receiving` | 任务队列与历史 = UI 编排概念 → host-plugin-database 自管；状态由 bus 事件驱动维护 |
| `retry-transfer` | UX 动作 → 插件记录批元数据后重调 send/pull 原语（带续传偏移） |
| `get-receive-settings` / `set-receive-policy` / `set-download-dir` / `set-transfer-encryption` | 产品设置项 → host-storage 持久化；涉及落盘路径与加密的部分经原语参数传入 |

### 新增 host-websocket（WS 基础能力服务，2026-09-18）

WS 传输是「离开宿主就无法实现」的能力（移动端链接、TLS/握手、帧编解码、连接生命周期），但**消息语义不是**：谁跟谁连、消息怎么拼、房间/重连/心跳策略都是插件的活。据此新增 `host-websocket`
（14 函数：客户端域 5 + 服务端域 9）+ 可选导出 `events-ws`，ABI desktop 13 → **14**（mobile 保持 11，ADR 0019 双端各自演进）。

裁决要点（spec：`.scratch/2026-09-18-ws-base-service/spec.md`）：

1. **零业务代码红线（D1）**：宿主只做引擎原语——连接生命周期、帧收发、句柄登记、属主仲裁、按属主回收、事件定向投递。宿主不拼装、不解读任何业务字段（同 host-mdns v2 的尺子）；
2. **属主作用域事件 topic（D3）**：状态事件 topic 内嵌属主——客户端域 `ws:open|error|close.<owner>`、服务端域 `ws:client-connect|client-disconnect.<owner>`，连接/对端标识放 payload。**理由**：消息总线是精确匹配、无重放无缓冲的，若按宿主生成句柄做 topic，插件必须先拿到句柄才能订阅 → 必然丢「连接已建立」事件；属主作用域 topic 让插件在 activate 期即可订阅（丢失时的自愈靠 `is-connected` / `list-clients` / `list-endpoints` 快照查询）；
3. **端点命名空间由宿主注入（D5）**：插件只提供路径后缀，完整挂载路径为 `/ws/plugin/<plugin-id>/<path>`。**理由**：属主段进路径后，插件之间物理上不存在路径抢占，宿主也不需要维护跨插件冲突表；
4. **权限按域拆分（D6）**：`ws:client`（出站，SSRF 暴露面）与 `ws:server`（入站，对外暴露面）互相独立，按最小必要授予。**理由**：两域的风险方向不同，合并成一个 `ws` 权限会使「只想连外部服务的插件」被迫获得「在局域网开端口」的能力；
5. **过滤链参与、链路加密排除（D9）**：插件端点帧走 `TrafficChannel::WsPlugin` 进入 `TrafficFilterChain`（inbound / outbound 均执行），但 `LinkEncryptionFilter::should_process` 对该通道恒 `false`。**理由**：链路加密是移动端配对设备的专用协商协议（双 ECDH + 密钥表按连接标识键控），插件端点的第三方客户端不参与该握手，且过滤链是宿主的统一审计/改写入口，能力不应绕过；
6. **本期仅 `ws://`（D7）**：`wss://` 显式拒绝。**理由**：`tokio-tungstenite` 未启用 TLS feature，接受 `wss://` 会以「握手失败」掩盖真实原因；TLS 客户端支持单独立项；
7. **发送队列满即 `Err`（D10）**：宿主不做无界缓冲与背压等待，fail-visible 优于静默丢弃；踢出/注销/停用/停机的关闭码固定（4004 / 4005 / 1001），`wasClean` 仅在对端主动 Close 且 code ∈ {1000,1001} 时为 true（D11）。

## Considered Options

- **维持全量命令面投影（现状）**：切换成本最低，但五处同步税、双份 DTO 翻译与 ABI 不稳定随每个功能持续付费。
- **通用动态通道**（`invoke(name, args)` 单入口）：函数数不膨胀，但 DTO 本就以 JSON 字符串承载，类型化名存实亡；再加动态分发只会失去 WIT 编译期漂移检测的价值，退化为第二个 `command.invoke`。
- **仅分层拆分 interface**（discovery/connection/trust/data 四个接口）：不减少函数数，但权限按接口声明、意图清晰——采纳为本决策的**前置过渡步骤**，非终点。
- **mDNS 纯化拆分 vs `list-devices` 明示豁免**：豁免（明写「唯一有意保留的业务投影」）保住设备列表常热性与函数总数，但在裁剪线上开了原则性口子；纯化拆分使发现回归中性能力，并顺手解开 `dial-peer` 对宿主内部缓存的耦合。采纳拆分（见 host-mdns 节），代价是设备缓存下沉与首屏常热损失。
- **数据面降为字节流会话管道**：把 send/pull/browse 进一步抽象成裸流是最「纯」的形态，但分块/校验/断点/加密须在每个消费插件的 WASM 里重写一遍，直接违背 spec 决策 11——否决。结论：**线协议动词即原语**，ABI 跟随稳定的协议而非易变的 UI。
- **原语化收缩 + 业务下沉（本决定）**：一次重构换取稳定的 ABI 面；代价是插件侧需要重建任务/历史/设置的自持逻辑（一次性成本，且这些本就是插件的产品职责）。

## Consequences

- WIT 变更需 bump ABI 版本并同步两份副本（desktop / mobile SDK 各一份），双端同版发布。
- 对端寻址全面句柄化、`dial-peer` 改 endpoint 入参：Tauri 命令面本身不动，host_impl 投影翻译层负责适配差异；双端迁移完成后，宿主侧 `DiscoveryCache` / 全量快照指纹比对链路可退役。
- 设备列表失去「插件未激活也常热」特性（browse 随插件激活才开始）：file-transfer 以 activate 即 browse + 自持久化 last-seen 缓存缓解首屏空窗。
- 断点续传的真源仍在接收端落盘侧：`pull-files` 需增加 resume 语义（或独立的已写偏移查询原语），插件的重试编排依赖它。
- 接收策略的「超时自动拒绝」默认行为保留在宿主闸门侧（fail-safe），插件的策略设置只是预配置该闸门的参数；ask 模式的逐批应答经保留的 `respond-transfer` 进入闸门，弹窗编排在插件——安全语义不下沉。
- file-transfer 插件复杂度上升（自管任务状态与设备缓存），但其前端 wire 形状翻译收敛回一层，总体代码量预期下降。
- 本决策修正的是 issue 12 的**投影粒度**，不推翻 spec 决策 11（peer-net 是宿主核心服务）；后续所有新宿主能力接口（含未来领域）均按「无业务语义的基础能力」这条裁剪线执行。

## 修订记录

- **2026-08-26 v1**：初版裁决——host-peer 27 → 约 15（任务/历史/设置等业务面下沉）。
- **2026-08-26 v2**：同一裁剪线二次收紧——① 共享目录 CRUD 三函数 → `set-shared-roots` 全量推送；② `disconnect-peer` + `cancel` 合并为统一 `close(handle)`，对端寻址全面句柄化；③ `pick-files`/`pick-folder` 移交新 `host-platform`；④ `list-devices` 拆分为 `host-mdns` browse-only 纯能力（自我广播留宿主自动生命周期）；⑤ 纠错：`respond-transfer` 从下沉清单改判安全闸门应答原语（无它则 ask 模式无法放行单个接收批）。最终 host-peer = 11 个函数。本修订仅定契约，代码尚未实施（WIT 现状仍为 27 函数全量投影）。
- **2026-08-26 v3（当前）**：Phase 1–2 已实施（新原语并存、ABI desktop v9 / mobile v7），Phase 3–4 规格落成时发现本 ADR 内部张力：Consequences 段「插件的策略设置只是预配置该闸门的参数」暗示存在配置通道，v2 退役表却将 `set-receive-policy` / `set-download-dir` 列入下沉。经裁决修正：二者是「引擎安全闸门/落盘配置」而非业务编排，符合本文裁剪线，保留为终态原语；`get-receive-settings`（读接口）维持下沉。**host-peer 终态 = 13 个函数**（11 + 二配置原语）；上文「最终 host-peer = 11 个函数」为 v2 时点表述，以本修订为准。实施规划见 `.scratch/peer-network/spec-plugin-self-hosting.md`。
- **2026-09-15 v4**：host-mdns 升级为 mDNS 基础能力服务（见「host-mdns v2」节）：新增 advertise / stop-advertise / is-advertising 三原语（config-json 纯引擎参数）、浏览事件定向投递 `mdns:found.<owner>` / `mdns:lost.<owner>`（payload 增 serviceType/browserId）、单守护收敛（全局唯一 ServiceDaemon，peer-net 与插件共享）、双表属主仲裁与按属主回收、宿主身份广播登记（owner=host，零业务代码红线 D3）、Android 多播锁随单守护常驻获取；全局发现桥接与缓存重发通道退役（D1），file-transfer 双端一期迁移（D2）。ABI desktop 12→13 / mobile 10→11（ADR 0019 双端同版）。实施验收后落 ADR（D5 定案）。
- **2026-09-18 v5（当前）**：新增 host-websocket（见「新增 host-websocket」节）：客户端域（connect / send-text / send-binary / close / is-connected）+ 服务端域（register-endpoint / 收发 / 广播 / 踢出 / 注销 / 清单）共 14 函数 + 可选导出 `events-ws`（宿主动态探测，未导出则帧丢弃 + 首次 warn + 计数）；状态事件改 **owner 作用域 topic**（`ws:<event>.<owner>`，标识在 payload，D3）；插件端点挂载 `/ws/plugin/<plugin-id>/<path>`（命名空间由宿主注入，D5）；权限按域拆 `ws:client` / `ws:server`（D6）；插件端点帧过流量过滤链但不参与链路加密（`TrafficChannel::WsPlugin`，D9）；本期仅 `ws://`（D7）。ABI desktop 13→**14**（mobile 11 不变，ADR 0019 双端各自演进）。
