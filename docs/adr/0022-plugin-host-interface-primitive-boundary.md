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

### 新增 host-pty（插件私有伪终端基础能力服务，2026-09-19）

伪终端是「离开宿主就物理上无法实现」的能力：WASI 0.2 无 PTY 接口，wasmtime 默认 deny 设备访问，`portable-pty` 是宿主独占依赖——插件（WASM 沙箱）无论怎么编排都造不出一个 tty。而**跑什么、怎么交互、算不算一会话**都不是引擎语义。据此新增 `host-pty`（6 函数：`spawn` / `write` / `resize` / `kill` / `ring-fetch` / `is-running`），ABI desktop 15 → **16**（v15 归认证中心线的 `host-auth`；mobile 不跟演，见「双端偏离」）。spec：`.scratch/2026-09-19-pty-base-service/spec.md`。

裁决要点：

1. **裁剪线判定（D1）**：`spawn` 只收裸引擎参数 `{command, args?, env?, workingDir?, cols?, rows?, ringBytes?}`，参数数组 exec 天然免注入。**明确不做**：`bash -lic` / PowerShell `-Command` / CMD `/K` 包装、WSL 路径转换、危险字符校验、默认 shell 探测、`name` 标识、特殊键/组合键 API——全部是宿主业务会话线或插件产品的语义（插件要 shell 包装，自己把 `sh -c` 放进 `args`）。
2. **与三条既有边界的划界**：`host-process`（非交互一次性 run/kill，无 TTY 行为）是它的补集；`host-terminal` + `terminal-hooks` 与 `host-session` 服务**宿主业务会话线**（会话配置、SessionManager 生命周期、前端 UI）。三者与本接口互不转发。插件 PTY 不进 `SessionComponents`、不注册 `GlobalOutputManager`、不参与业务会话事件链——共享同一 PTY 引擎（`PtySession`），但两张注册表、两套生命周期。
3. **输出面是纯拉取，不做 push 回调（D3）**：每句柄一条有界环 `PtyRing`，读线程单生产者写入、插件按自己的游标 `ring-fetch`。**否决 push（events-pty 可选导出）两条理由**：① 2026-09-17 `pty-pull-subscribers` 的教训——推送会把背压踢回生产端，慢消费者只能损失自己；② wasmtime Store 不可重入，宿主无法异步唤醒插件，push 在语义上等于「多一层回调的轮询」。故 `truncated + next-offset` 的 resync 语义即契约本体，缺口如实上报、不静默补洞。
4. **属主隔离 + 停用回收（D2）**：全部函数先查属主（`not owner of pty handle`，同 mdns / ws 先例）；插件 deactivate 时宿主 `purge_for_plugin` kill 并摘除其全部 PTY、逐条补发 `pty:exit.<owner>`（reason=killed），只碰本人。
5. **终止与摘除的单一发布者不变量（D4）**：`kill()` 只发起终止；句柄摘除与事件发布统一由 spawn 时起动的退出监听在「读线程 EOF + 子进程回收」齐备（`PtyTerminationGate`）时完成，且只有从注册表 `remove` 成功的一方发布 → 自然退出 / 主动 kill / 停用回收三条路径交汇时每条 PTY 恰好一条 `pty:exit`。事件面只有这一条（spawn 成败在返回值、错误直接上抛）。
6. **权限两域（D8）**：`pty:spawn`（创建/终止，任意命令执行的高风险面）与 `pty:io`（数据面）独立授予与审计——合并成一个 `pty` 会迫使「只想观测的插件」获得在宿主机执行任意命令的能力。五同步点（SDK 常量与 API 映射 / 打包 CLI / 前端合法集合 / 宿主能力清单 / host_impl 权限门）由漂移锁 `permission_sync_points_all_know_pty_domains` 钉住。
7. **限额分级与声明式环容量（D9）**：创建类失败一律 `Err`（每插件在册条数超上限、`ringBytes` 为 0 或超宿主上限），不排队、不静默夹取、不淘汰插件自己已有的句柄；数据面只有**读侧截断**（单次 `ring-fetch` 截到 `PLUGIN_PTY_RING_FETCH_MAX_BYTES`，余下续拉），写侧是拒绝（超 `PLUGIN_PTY_MAX_WRITE_BYTES` 一个字节都不写入——半条命令喂进交互进程比失败更糟）。环容量不做全局档位之争：它是 spawn 的**插件声明参数**（宿主默认 256 KiB，上限 4 MiB），因为业务侧 `channels.global_queue_max_bytes` 的 50 MB 是「每条会话队列」的量级，插件环随句柄存活、每插件可到 8 条，字面对齐即单插件最坏 400 MB 常驻；常驻上界改由「条数 × 容量上限」表达。

## 双端偏离（host-websocket / host-pty 等桌面独有接口）

- 移动端是**远程终端控制端**，不承载 PTY / mDNS 广播 / WS 服务端等主机侧引擎，故 `host-websocket`（desktop v14）、`host-auth`（v15）、`host-pty`（v16）均为**桌面独有接口**：mobile 的 WIT / ABI / SDK 不跟演（ADR 0018 双端各自演进的文档化偏离，同 wasmtime 桌面 48 / 移动 47 分叉先例）。
- **恢复条件**：当移动端需要同类能力（例如本地跑交互进程）时，再在该端 WIT 增补对应 interface 并对齐 ABI 计数；在此之前「改 WIT 必须双端同步」这一硬约束的适用范围限于**双端共有的接口**（host-peer / host-fs / host-http 等）。
- SDK 双端独立包（`plugin-sdk-desktop` / `plugin-sdk-mobile`），互不影响；宿主侧 `version > 当前 → 拒绝` 的兼容语义保证旧插件（≤v15）零迁移仍可加载。

## 抽象提取候选（登记，不在本期实施）

- **`PtyRing` ↔ 业务会话输出环（`session_output.rs::UnifiedOutputQueue`）的代码级合并**：二者形态同源（单生产者 + 全局偏移 + 游标拉取 + 字节/条目双上限），但生命周期不同（业务环随会话、插件环随 pty 句柄），且 2026-09-17 刚重构完的业务链路不背回归风险 → 本期刻意自持实现（约 130 行）。第二处同类形态出现时（或业务环新增维度需要插件侧同步时）再抽取。
- **PTY 引擎与业务会话线的进一步解耦边界**：票 01 已把「输出汇可注入」「终态门与退出码」下沉到 `PtySession`，票 02 追加「命令来源可注入」（`PtyCommandSource::Business | Raw`），`PtySlaveFdPolicy`（业务 `Hold` / 插件 `ReleaseOnSpawn`）仍是会话构造器的分支。若第三条消费线（如 AI 工具执行器）出现，应把「策略三元组（sink / command source / slave policy）+ 尺寸与 env」收敛为一个显式的会话装配参数结构，替代构造器家族。
- **终态可查询面**：`PtyTerminationGate` 已有 `reader_closed()`（信号 ①），缺「终态事件是否已发出」的访问器。补上它可让 host-pty 对「订阅晚于终态」的竞态彻底免疫（当前靠 `spawn` 内「订阅早于 start」的构造顺序防御，该防御无法被测试确定性锁定，见票 04 变异 M2 存活）。
- **`is-running` 判据的归属**：`running && !output_terminated` 是 host-pty 的语义组合（引擎的 `running` 故意不随自然退出翻下，业务线依赖这一点），故该纯判定放在 `host_impl/pty.rs::running_verdict` 而非引擎层——若未来业务线也要「如实的存活」，应新增引擎层访问器而不是反向挪用本判定。

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
- **2026-08-26 v3**：Phase 1–2 已实施（新原语并存、ABI desktop v9 / mobile v7），Phase 3–4 规格落成时发现本 ADR 内部张力：Consequences 段「插件的策略设置只是预配置该闸门的参数」暗示存在配置通道，v2 退役表却将 `set-receive-policy` / `set-download-dir` 列入下沉。经裁决修正：二者是「引擎安全闸门/落盘配置」而非业务编排，符合本文裁剪线，保留为终态原语；`get-receive-settings`（读接口）维持下沉。**host-peer 终态 = 13 个函数**（11 + 二配置原语）；上文「最终 host-peer = 11 个函数」为 v2 时点表述，以本修订为准。实施规划见 `.scratch/peer-network/spec-plugin-self-hosting.md`。
- **2026-09-15 v4**：host-mdns 升级为 mDNS 基础能力服务（见「host-mdns v2」节）：新增 advertise / stop-advertise / is-advertising 三原语（config-json 纯引擎参数）、浏览事件定向投递 `mdns:found.<owner>` / `mdns:lost.<owner>`（payload 增 serviceType/browserId）、单守护收敛（全局唯一 ServiceDaemon，peer-net 与插件共享）、双表属主仲裁与按属主回收、宿主身份广播登记（owner=host，零业务代码红线 D3）、Android 多播锁随单守护常驻获取；全局发现桥接与缓存重发通道退役（D1），file-transfer 双端一期迁移（D2）。ABI desktop 12→13 / mobile 10→11（ADR 0019 双端同版）。实施验收后落 ADR（D5 定案）。
- **2026-09-18 v5**：新增 host-websocket（见「新增 host-websocket」节）：客户端域（connect / send-text / send-binary / close / is-connected）+ 服务端域（register-endpoint / 收发 / 广播 / 踢出 / 注销 / 清单）共 14 函数 + 可选导出 `events-ws`（宿主动态探测，未导出则帧丢弃 + 首次 warn + 计数）；状态事件改 **owner 作用域 topic**（`ws:<event>.<owner>`，标识在 payload，D3）；插件端点挂载 `/ws/plugin/<plugin-id>/<path>`（命名空间由宿主注入，D5）；权限按域拆 `ws:client` / `ws:server`（D6）；插件端点帧过流量过滤链但不参与链路加密（`TrafficChannel::WsPlugin`，D9）；本期仅 `ws://`（D7）。ABI desktop 13→**14**（mobile 11 不变，ADR 0019 双端各自演进）。
- **2026-09-19 v6（当前）**：新增 host-pty（见「新增 host-pty」节）：6 函数（spawn / write / resize / kill / ring-fetch / is-running）+ 唯一生命周期事件 `pty:exit.<owner>`；输出面定为**纯拉取**（否决 push 回调），限额四项按「创建类失败可见 / 数据面读侧截断」分级，环容量改为 spawn 的插件声明参数（宿主上下限仲裁）。同时首次把**桌面独有接口的双端偏离**成文（「双端偏离」节：host-websocket v14 / host-auth v15 / host-pty v16，mobile 不跟演 + 恢复条件），并登记两条抽象提取候选（「抽象提取候选」节）。ABI desktop 15→**16**（v15 由认证中心线 `host-auth` 占用；mobile 不跟演）。实施与验收见 `.scratch/2026-09-19-pty-base-service/`（票 01-07）。
