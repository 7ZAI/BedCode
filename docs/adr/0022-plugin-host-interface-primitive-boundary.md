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

> **阅读约定**：本文按**决策批次**累积，多数小节记录的是当时的中间态。已被后续修订取代的结论
> 均在原地加「现状 / 终态更正」标注，**当前形态以各小节的更正标注 + 末节「修订记录」首条
> （desktop ABI v31 / mobile 14）为准**。文中出现的旧路径 `host_impl/` = 现
> `wasm_core/host_api/`；`SessionComponents` / `GlobalOutputManager` / `src/session/` /
> `protocol/` / `src/events/` / `server/websocket/{message.rs,services/,terminal_ws/,subscription.rs}`
> 均已删除（v27 / v28）。

### host-peer 原语面（v2 裁决 11 个 → **终态 19 个**）

| 类别 | 函数 | 留在宿主的理由 |
| --- | --- | --- |
| 连接 | `dial-peer(endpoint)` | TLS 引擎句柄持有方；endpoint（地址/端口/期望节点身份）由插件从发现事件解析后显式传入，宿主不再内藏 node-id → 地址解析表 |
| 连接 | `close(handle)` | 统一资源关闭：dial 返回会话句柄、send/pull 返回传输句柄，一律经 close 关闭（合并原 disconnect-peer 与 cancel）；「pending 接收批被关闭即拒绝」是闸门 fail-safe 默认的自然结果，不是独立业务函数 |
| 安全闸门 | `respond-consent` / `respond-transfer` | 两道闸门的应答通道：首连确认（ADR 0028）与接收批放行；fail-safe 默认（无应答/超时即拒）必须在宿主侧——应答原语是闸门的输入口而非业务流，没有它 ask 模式无法放行任何单个接收批（v1 曾误判 respond-transfer 可下沉，此处纠正） |
| 信任存储 | `list-trusted` / `revoke-trusted` | 信任存储是安全边界（ADR 0028） |
| 暴露 | `set-shared-roots(dirs)` | 引擎广播源的全量幂等同步；注册表 CRUD 真源移至插件侧 host-plugin-database（合并原 list/add/remove-shared-directories 三函数） |
| 读对端 | `list-shared-roots(session)` / `browse-directory(session, …)` | 单连接单请求线协议会话的动词镜像，按会话句柄寻址 |
| 写对端 | `send-files(session, …)` / `pull-files(session, …)`（增补续传偏移查询语义） | 数据面引擎（分块/校验/断点/加密）+ 断点真源在接收端落盘侧 |

所有对端寻址统一为**显式句柄**。读对端/写对端的形状确实带「文件」语义，但它们是线协议动词的镜像——协议本身就是文件浏览/传输协议；ABI 跟随稳定的协议动词而非易变的 UI，正是最不容易破 ABI 的切法。

**下表是 v2 裁决的 11 个基线，不是现状。** 终态 19 个（WIT 真源：`packages/plugin-sdk-desktop/rust/wit/bedcode.wit` 的 `host-peer`，2026-09-26 核对），后续 8 个追加均属**引擎控制面**而非业务编排，未破裁剪线：

| 追加函数 | 批次 | 判据 |
| --- | --- | --- |
| `set-receive-policy` / `set-download-dir` | v3 纠错 | 引擎安全闸门参数与落盘位置，不是策略「设置项」 |
| `pause-transfer` / `resume-transfer` | 会话语义下沉批次 | 句柄面上的传输控制（redial 续传的断点真源在接收端落盘侧） |
| `start-node` / `stop-node` | v25（审计票 12） | peer-net 节点生命周期，属主原语 |
| `active-transfers` / `collect-outgoing` | v30（纯增量） | 句柄面自愈快照 + 发送源枚举（引擎事实投影） |

`list-devices` / `get-receive-settings` / `retry-transfer` / `clear-*-history` / `list-transfers` / `list-receiving` / `set-transfer-encryption` 等业务面**未回归**（后者已从 WIT 整体消失）。

### 新增 host-mdns（browse-only 纯能力）

```wit
interface host-mdns {
    /// 浏览某服务类型，返回 browser 句柄；发现/离开经消息总线推送
    browse: func(service-type: string) -> result<string, string>;
    stop-browse: func(browser-id: string) -> result<_, string>;  // v2 统一为 result<bool, string>（见下节）
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
- **事件定向投递**：browse 事件按属主发布 `<owner>::mdns:found` / `<owner>::mdns:lost`（owner = 发起 browse 的插件 id，**属主私有 topic**；早期草案写作 `mdns:found.<owner>`，票 05 命名空间门禁后统一为 `<plugin-id>::<name>` 形状，构造与 SDK 共用 `host::bus::owned_topic`），payload 增量追加 `serviceType` / `browserId`（既有 4 字段不变）；消息总线按精确 topic 分发（bus.rs），非属主插件物理上订阅不到——业务隔离（需求①）；
- **广播原语（需求②扩展性核心）**：`advertise(config-json)` 的 serviceType / instanceName / port / txtRecords 全部由调用方构造传入，宿主只校验 serviceType 非空、零业务拼装；实例名缺省时宿主按 `{plugin}-{短指纹}` 默认（D4）；句柄带周期 re-announce 续期；**属主仲裁**——stop-browse / stop-advertise / is-advertising 先权限门（PERMISSION_MDNS）再属主校验，跨插件操作一律拒绝；
- **双表回收**：`purge_for_plugin` 回收某插件全部浏览 + 广播句柄，只碰本人，宿主（owner=host）与它插件登记不受影响；
- **零业务代码红线（D3）**：节点身份广播的 TXT / ServiceInfo 由 peer-net 引擎构造（节点身份/证书/能力位是引擎语义），MdnsService 只做注册 + 句柄登记（owner=host）——「自我广播不进插件 ABI」结论不变，仅登记路径收敛到基础服务，作为「host 与插件 advertise 共存于单守护、互不注销对方」的可验证凭据；
- **平台脏活落地修订**：Android MulticastLock 随单守护首次使用获取、常驻持有（幂等，不再随 browse 句柄增删）；全局发现桥接（`mdns:found`/`mdns:lost` 全局 topic）与缓存重发通道整体退役（D1）——file-transfer 为唯一消费方且一期同迁（D2），无迁移垫片；
- **消费插件迁移**：file-transfer 双端一期同步迁移为 activate 自建 browse + 订阅定向 topic、deactivate stop-browse（宿主 purge 兜底）；设备列表派生视图仍留前端缓存（wire 形状不变）。

### 新增 host-platform（通用平台能力域）

`pick-files` / `pick-folder` 是系统对话框能力，与 peer 领域无关，属错放——移入新的 `host-platform` 接口（与 `host-fs.request-auth` 的授权/平台域同级），供所有插件复用，不再绑定单一插件的流程。

事件总线 topic 调整（本节为 v2 时点记录）：`peer:devices` 随 `list-devices` 一并退役，职责由 `<owner>::mdns:found` / `<owner>::mdns:lost` 接管——事件推送本就是基础能力形态。**当时的 `peer:connection/consent/transfer/receive` 也已全部改形**：首连确认现为 `peer:consent`，传输与接收的编排事件在 v30/v31 后为公开 topic `peer:transfer-event` / `peer:receive-event`（引擎原始事件直推，旧快照 topic `peer:transfer` / `peer:receive` 退役）。

### 下沉插件（10 个）

| 泄漏项 | 去向 |
| --- | --- |
| `list-devices` | 设备列表 = mDNS 发现事件的派生视图 → 插件订阅 `<owner>::mdns:found` / `<owner>::mdns:lost` 自建设备缓存（去重/TTL/展示名）；宿主侧 DiscoveryCache 守护与全量快照比对链路随之退役 |
| `list-transfers` / `clear-transfer-history` / `clear-receiving-history` / `list-receiving` | 任务队列与历史 = UI 编排概念 → host-plugin-database 自管；状态由 bus 事件驱动维护 |
| `retry-transfer` | UX 动作 → 插件记录批元数据后重调 send/pull 原语（带续传偏移） |
| `get-receive-settings` / `set-receive-policy` / `set-download-dir` / `set-transfer-encryption` | 产品设置项 → host-storage 持久化；涉及落盘路径与加密的部分经原语参数传入 |
| → **纠错（v3）**：`set-receive-policy` / `set-download-dir` | 改判为**引擎闸门参数**（fail-safe 闸门 + 落盘位置）而保留为原语，只有读侧 `get-receive-settings` 维持下沉；`set-transfer-encryption` 已从 WIT 整体消失（无回归） |

### 新增 host-websocket（WS 基础能力服务，2026-09-18）

WS 传输是「离开宿主就无法实现」的能力（移动端链接、TLS/握手、帧编解码、连接生命周期），但**消息语义不是**：谁跟谁连、消息怎么拼、房间/重连/心跳策略都是插件的活。据此新增 `host-websocket`
（v14 = 14 函数：客户端域 5 + 服务端域 9；**v28 起 15 函数**——服务端域追加 `connection-context`，即下面「websocket 业务下沉」后的脱敏连接事实原语，见修订记录 v19）+ 可选导出 `events-ws`，ABI desktop 13 → **14**（mobile 保持 11，ADR 0019 双端各自演进）。

裁决要点（spec：`.scratch/2026-09-18-ws-base-service/spec.md`）：

1. **零业务代码红线（D1）**：宿主只做引擎原语——连接生命周期、帧收发、句柄登记、属主仲裁、按属主回收、事件定向投递。宿主不拼装、不解读任何业务字段（同 host-mdns v2 的尺子）；
2. **属主作用域事件 topic（D3）**：状态事件走**属主私有 topic**——客户端域 `<owner>::ws:open|error|close`、服务端域 `<owner>::ws:client-connect|client-disconnect`（早期草案写作 `ws:open|error|close.<owner>`，票 05 命名空间门禁后统一为 `<plugin-id>::<name>` 形状，构造与 SDK 共用 `host::bus::owned_topic`），连接/对端标识放 payload。**理由**：消息总线是精确匹配、无重放无缓冲的，若按宿主生成句柄做 topic，插件必须先拿到句柄才能订阅 → 必然丢「连接已建立」事件；属主作用域 topic 让插件在 activate 期即可订阅（丢失时的自愈靠 `is-connected` / `list-clients` / `list-endpoints` 快照查询）；
3. **端点命名空间由宿主注入（D5）**：插件只提供路径后缀，完整挂载路径为 `/ws/plugin/<plugin-id>/<path>`。**理由**：属主段进路径后，插件之间物理上不存在路径抢占，宿主也不需要维护跨插件冲突表；
4. **权限按域拆分（D6）**：`ws:client`（出站，SSRF 暴露面）与 `ws:server`（入站，对外暴露面）互相独立，按最小必要授予。**理由**：两域的风险方向不同，合并成一个 `ws` 权限会使「只想连外部服务的插件」被迫获得「在局域网开端口」的能力；
5. **过滤链参与、链路加密排除（D9）**：插件端点帧走 `TrafficChannel::WsPlugin` 进入 `TrafficFilterChain`（inbound / outbound 均执行），但 `LinkEncryptionFilter::should_process` 对该通道恒 `false`。**理由**：链路加密是移动端配对设备的专用协商协议（双 ECDH + 密钥表按连接标识键控），插件端点的第三方客户端不参与该握手，且过滤链是宿主的统一审计/改写入口，能力不应绕过；
6. **本期仅 `ws://`（D7）**：`wss://` 显式拒绝。**理由**：`tokio-tungstenite` 未启用 TLS feature，接受 `wss://` 会以「握手失败」掩盖真实原因；TLS 客户端支持单独立项；
7. **发送队列满即 `Err`（D10）**：宿主不做无界缓冲与背压等待，fail-visible 优于静默丢弃；踢出/注销/停用/停机的关闭码固定（4004 / 4005 / 1001），`wasClean` 仅在对端主动 Close 且 code ∈ {1000,1001} 时为 true（D11）。

### 新增 host-pty（插件私有伪终端基础能力服务，2026-09-19）

伪终端是「离开宿主就物理上无法实现」的能力：WASI 0.2 无 PTY 接口，wasmtime 默认 deny 设备访问，`portable-pty` 是宿主独占依赖——插件（WASM 沙箱）无论怎么编排都造不出一个 tty。而**跑什么、怎么交互、算不算一会话**都不是引擎语义。据此新增 `host-pty`（6 函数：`spawn` / `write` / `resize` / `kill` / `ring-fetch` / `is-running`），ABI desktop 15 → **16**（v15 归认证中心线的 `host-auth`；mobile 不跟演，见「双端偏离」）。spec：`.scratch/2026-09-19-pty-base-service/spec.md`。

裁决要点：

1. **裁剪线判定（D1）**：`spawn` 只收裸引擎参数 `{command, args?, env?, workingDir?, cols?, rows?, ringBytes?}`，参数数组 exec 天然免注入。**明确不做**：`bash -lic` / PowerShell `-Command` / CMD `/K` 包装、WSL 路径转换、危险字符校验、默认 shell 探测、`name` 标识、特殊键/组合键 API——全部是宿主业务会话线或插件产品的语义（插件要 shell 包装，自己把 `sh -c` 放进 `args`）。**票 06 已落地**：按键组合 → ANSI/ASCII 转义字节的翻译移至 `com.bedcode.terminal-session` 私有域（`keys.rs`），`session-input` 收 `specialKey` 组合串自译自写、统一直写语义；宿主 `session_gateway::special_key` 只转发组合串、不再 `to_pty_bytes`（同批的 `terminal_service` 宿主 WS 侧转发层已于 v28 随业务面整删，见修订记录 v19）。
2. **与三条既有边界的划界**：`host-process`（非交互一次性 run/kill，无 TTY 行为）是它的补集；`host-terminal` + `terminal-hooks` 与 `host-session` 服务**宿主业务会话线**（会话配置、SessionManager 生命周期、前端 UI）。三者与本接口互不转发。插件 PTY 不进 `SessionComponents`、**不默认注册 `GlobalOutputManager`**（业务会话迁插件后，宿主对插件 PTY 输出的广播按 spawn 的 `hostBroadcastSessionId` 声明 **opt-in 只读订阅**，见修订记录 v15——未声明的句柄任何宿主广播面都读不到，安全边界在缺省侧）、不参与业务会话事件链——共享同一 PTY 引擎（`PtySession`），但两张注册表、两套生命周期。
   **（终态更正，v27 + v28）**：三方划界已全部作废——`host-session` / `host-terminal` / `terminal-hooks` 于 v27 整删（修订记录 v16），内核 `SessionComponents` 与 `GlobalOutputManager` 于票 11 随 `src-tauri/src/session/` 整目录删除，`hostBroadcastSessionId` 声明与 `broadcast_handle_for_session` 于 v28 删除（修订记录 v19）。**现状 = 一张引擎注册表**：业务会话与插件私有 PTY 同为 `host-pty` 句柄，PTY 引擎不再知道 session id，输出只经 `ring-fetch` 拉取（见下方「会话真源下沉」第 2 条）。
3. **输出面是纯拉取，不做 push 回调（D3）**：每句柄一条有界环 `PtyRing`，读线程单生产者写入、插件按自己的游标 `ring-fetch`。**否决 push（events-pty 可选导出）两条理由**：① 2026-09-17 `pty-pull-subscribers` 的教训——推送会把背压踢回生产端，慢消费者只能损失自己；② wasmtime Store 不可重入，宿主无法异步唤醒插件，push 在语义上等于「多一层回调的轮询」。故 `truncated + next-offset` 的 resync 语义即契约本体，缺口如实上报、不静默补洞。
4. **属主隔离 + 停用回收（D2）**：全部函数先查属主（`not owner of pty handle`，同 mdns / ws 先例）；插件 deactivate 时宿主 `purge_for_plugin` kill 并摘除其全部 PTY、逐条补发 `<owner>::pty:exit`（reason=killed），只碰本人。
5. **终止与摘除的单一发布者不变量（D4）**：`kill()` 只发起终止；句柄摘除与事件发布统一由 spawn 时起动的退出监听在「读线程 EOF + 子进程回收」齐备（`PtyTerminationGate`）时完成，且只有从注册表 `remove` 成功的一方发布 → 自然退出 / 主动 kill / 停用回收三条路径交汇时每条 PTY 恰好一条 `pty:exit`。事件面只有这一条（spawn 成败在返回值、错误直接上抛）。
6. **权限两域（D8）**：`pty:spawn`（创建/终止，任意命令执行的高风险面）与 `pty:io`（数据面）独立授予与审计——合并成一个 `pty` 会迫使「只想观测的插件」获得在宿主机执行任意命令的能力。五同步点（SDK 常量与 API 映射 / 打包 CLI / 前端合法集合 / 宿主能力清单 / 宿主权限门 `wasm_core/host_api/pty.rs`，锁在 `host_api/tests/pty.rs::permission_sync_points_all_know_pty_domains`）由该漂移锁钉住。
7. **限额分级与声明式环容量（D9）**：创建类失败一律 `Err`（每插件在册条数超上限、`ringBytes` 为 0 或超宿主上限），不排队、不静默夹取、不淘汰插件自己已有的句柄；数据面只有**读侧截断**（单次 `ring-fetch` 截到 `PLUGIN_PTY_RING_FETCH_MAX_BYTES`，余下续拉），写侧是拒绝（超 `PLUGIN_PTY_MAX_WRITE_BYTES` 一个字节都不写入——半条命令喂进交互进程比失败更糟）。环容量不做全局档位之争：它是 spawn 的**插件声明参数**（宿主默认 256 KiB，上限 4 MiB），因为业务侧 `channels.global_queue_max_bytes` 的 50 MB 是「每条会话队列」的量级，插件环随句柄存活、每插件可到 8 条，字面对齐即单插件最坏 400 MB 常驻；常驻上界改由「条数 × 容量上限」表达。

### 会话语义下沉批次（v18 + v19，2026-09-20 桌面端）

阶段 2 与阶段 3 的一部分在本仓库**首次合并执行**（原计划各自成阶段、各开一次 ABI 窗），
落地为单一内置插件 `com.bedcode.terminal-session`（终端会话中心：设备与配对 + 会话编排 + Agent
任务域；2026-09-22 票 06 起新 id，旧 id `com.bedcode.session` 的 HTTP 前缀与互调 api 名留双投窗口，
见下方 v9 条目），宿主侧只追加**既有 interface 的函数**，未新开任何 interface。
spec：`.scratch/2026-09-19-terminal-session-plugin/spec.md`（D2–D7），实施票 01–18。

| 面 | 追加 | 批次 | 权限位 |
| --- | --- | --- | --- |
| `host-auth` 记录面 | `trusted-devices-list` / `trusted-device-revoke` / `connection-history-list` / `auth-setting-set` | v18 | `auth` |
| `host-session` 配置面 | `config-upsert` / `config-get` / `config-delete` | v19 | `session:config`（新增位） |
| `host-session` 创建与动作面 | `create-with-spec` / `remove` / `rename` / `resize`（原表的 `restart` 已于 v21 退役，见下「v21 收敛退役」） | v19（函数级追加不 bump） | `session:write` |
| ~~`host-session` 事实面~~ | ~~`annotate`（注解槽）~~ / ~~`connections-list`~~ | v19 | ~~`session:write`~~ / `connection:read`（原 `session:read`） |
| `host-connection`（v14 新增 interface） | `connections-list`（宿主 server 在册连接原始记录） | v14（函数级搬迁，不 bump） | `connection:read`（新增位） |
| `host-platform` | `wsl-distros` | v19 | `platform` 现状 |
| `auth-policy` 导出 | `verify-device-token`（宿主中间件验签后取策略） | v17 | 能力导出，非宿主原语 |
| 前端贡献面 | `ui.registerSettingsSection`（设置分组扩展点） | 无 WIT（纯前端） | `ui:settings`（新增位） |

**本表各行的终态（2026-09-26 核对，下沉后无一例外留在宿主）**：

- **`host-auth` 记录面 → v24 退役三函数**：`trusted-devices-list` / `trusted-device-revoke` /
  `connection-history-list` 随主库 `pairings` / `connection_history` 删表退役，真源迁到认证中心私有库
  `auth_records/`（表 `auth_pairings` / `auth_connection_history`）；`auth-setting-set` **保留**（白名单键写
  内核 `settings`）。`host-auth` 终态 = 密钥托管 + 生物凭证 bound/verify/bind + device-token +
  link-identity + setting。
- **`host-session` 配置面 → v23 删写原语、v27 全删**：`config-upsert` / `config-delete` 无调用者即死接口，
  v23 删除；读取面 `config-list` / `config-get` 曾作一次性 legacy 迁移通道（权限收编 `session:read`），
  v24 迁移窗口关闭（主库表删）→ **v27 随整 interface 退役一并消失**。终态无任何会话配置读原语。
- **`host-session` 创建与动作面 / 事实面 → v27 整 interface 退役**（票 10）：`create-with-spec` /
  `remove` / `rename` / `resize` / `annotate` / 输出环 / 列表 / 查询 / 关闭 / 两条观察注册面**全部删除**；
  `connections-list` 早在 v14 迁 `host-connection`，此处旧别名同批删除。会话事实只在
  `com.bedcode.terminal-session` 登记域（防回接锁 `retired_kernel_session_domain_is_not_reintroduced`）。
- **仍在（WIT 与代码核对无变更）**：`host-platform.wsl-distros`（6 函数面其余为 picker/reveal）、
  `auth-policy#verify-device-token` 导出、`host-connection.connections-list`、前端
  `ui.registerSettingsSection` 贡献面。

裁决要点：

1. **「映射决策归插件、执行留内核」的切口是 `create-with-spec`**：插件算好
   `{command, args, cwd, cols, rows, env, name}` 交给宿主，宿主只做 shell 包装 / WSL
   转换 / 尺寸缺省 / ID 预生成。会话配置真源同时从主库表迁入**插件私有库**
   （`host-plugin-database`），主库旧表保留一版只读退役，走一次性幂等迁移。
   **（终态更正，v11 / v27）**：`create-with-spec` 这条「执行端留内核」的切口已被 v11 收口
   （宿主 shell 包装 / WSL 转换 / cwd 兜底 / 会话身份 env 注入全退役，argv 由插件算好经
   `host-pty.spawn` 送入）与 v27 废止（`create-with-spec` 随 `host-session` 整 interface 删除）
   两步作废；`annotate` 注解槽亦随之退役。**终态：会话创建完全在插件侧，宿主只提供 PTY 原语。**
2. **注解槽取代内核任务字段（D5）**：内核只按 `session-id → map<string,string>` 搬运
   与透传，**绝不解释键名**；线协议里 `taskStatus` 等字段形状不变，值由插件经 `annotate`
   写入后由内核透传 → 移动端零改动。这是「内核去业务化」与「线协议不破」的唯一共存形态。
3. **`resize` 裁决分家**：谁是当前渲染端（正统端）的**事实登记**在内核，**裁决规则**
   （谁覆盖谁、何时提示）在插件。与 host-pty 第 2 条的「两张注册表」同一划界思路。
   **（终态更正，v27）**：正端归属这一「事实」本身是产品语义，已随内核会话域删除整体迁到
   `com.bedcode.terminal-session`（插件 `session/resize` 裁决 + 私有登记表），宿主 `resize`
   退化为裸 `winsize` 透传、不承诺同步生效时序。
4. **设置分组扩展点是本批次内核唯一多做的 UI 事**：宿主从「7 个写死分组」改为
   「内置分组 + 注册表分组按 `order` 合并渲染」，共享状态由父级持有下传。它换来
   宿主配对分组整体退役 + 界面归属换人而**像素不变**（D6「界面维持，贡献方换人」）。
5. **权限清单与能力映射一对一对应**：合并插件按实际消费者定 **15 项**（含新增
   `session:config` / `ui:settings` / `ui:input`）；spec D2 表里列的 `terminal:output`
   与 `ui:dialog` 因前后端查无调用点**不预声明**（票 17 复核，见其 §2-③）——
   「多一项就是审计噪音」优先于照抄规格表格。
6. **裁剪线的反向验证**：本批次宿主未新增任何通道（三域全部落在 20 组既有 `host-*`
   原语内），也**没有**新开宿主 Tauri 领域命令；输出订阅/ack 原语（`host-session-output`）
   被显式否决——逐帧输出不进 WASM 是性能红线（同 host-pty 第 3 条）。
   **2026-09-21 修订**：该红线已按性能验证结果放宽，见下节「终端输出消费插件化 ·
   性能红线修订」——保留的是「输出字节禁止经 JSON-RPC 命令通道搬运」（实测 ~75 ms/MB
   不可接受），开放的是「经 WIT 二进制原语（`list<u8>` 直传）可进 WASM」（实测
   ~40 µs/op / ~2.6 ms/MB）。`host-session-output` 若未来开设，契约必须是二进制直传。
7. **故障半径是本批次的代价而非缺陷**：三域同实例后，配对侧 trap 会连带会话与任务 tick。
   补偿四条（认证路径保留宿主降级 + warn、按域 `Result` 边界与分域计数、activate 分段
   落 `Degraded`、UI 贡献面 error 态整组摘除 + 兜底壳）均为验收项，票 18 §2 记行为测试。
   **2026-09-21 修订**：其中「配对 / QR 保留宿主降级轨」一条已**整体退役**——宿主命令面
   注销同批（`.scratch/2026-09-21-host-rust-residue/issues/05`）删除了 `auth_center` 的
   配对 / QR 桥接函数、`PairingService`、`QrTokenManager`、`utils/auth/pairing.rs` 与
   应用上下文装配链：该回退已无任何入口（其唯一入口是宿主 Tauri 命令面），留着即僵尸代码，
   且会造成「宿主也能签发配对码」的错觉。**现认证只剩两条宿主侧面**：`host-auth` 记录面
   （密钥托管 / 设备与历史记录，属主隔离 + 权限门）与 `auth-policy` capability（策略取用，
   传输失败时回退放行以防认证中心故障误杀连接）。插件未激活时配对 / QR 相关前端命令面
   显性报错，不存在宿主代签路径。

### 会话真源下沉（P1-b，2026-09-23/24 桌面端）

会话的**事实面**（登记表、状态机、生命周期分发、注解槽、尺寸归属）从宿主
`session/`（3759 行）迁入 `com.bedcode.terminal-session` 的私有登记域，宿主侧的会话操作
收口为 `utils/session_gateway.rs` 一条**纯互调 api** 通道。spec：
`.scratch/2026-09-23-session-engine-downsink/spec.md`（P1 前置 / P1-a / P1-b）。

1. **裁剪线依据**：「哪条记录算一会话、它处在什么状态、谁能覆盖它的尺寸」全是产品语义；
   宿主留的只有物理上不可下沉的部分（PTY 引擎 + `host-pty` 六原语）。这比 v19 批次的
   「映射决策归插件、执行留内核」更进一格——**执行也归插件**，因为执行所需的原语
   （`host-pty.spawn/write/kill/resize/ring-fetch`）本身已是引擎级。
2. **`host-pty` 第 2 条的划界现状更正（终态，v27 + v28）**：该条原写「两张注册表、两套生命周期」。
   演进两步到位——P1-b 起业务会话本身就是一个 `host-pty` 句柄（**合并为一张**）；**v27（票 11）
   起只剩引擎这一张**：内核会话目录整目录删除，`SessionComponents` 的 PTY 注册表与
   `GlobalOutputManager`（业务输出环）都不复存在，业务会话与插件私有 PTY **同为引擎句柄**。
   **v28 补最后一刀**：v15 引入的 `hostBroadcastSessionId` opt-in 只读订阅声明与
   `broadcast_handle_for_session` 一并删除（PTY 引擎不再知道 session id，P3 形态 B 失去宿主侧
   载体）——因此「业务线与插件线的唯一区别」这一表述**已失效**：两条线现在**完全同形**，
   输出只经插件自己的 `ring-fetch` 游标拉取（宿主 server 不再直读任何 PTY 环）。移动端 M6/M7
   受损的根因即这张表的切换，形态 B 的恢复改由插件自持（历史决策链见修订记录 v15 / v19）。该条里
   「`host-session` 服务宿主业务会话线」的三方划界**同样已终结**：`host-session` 与 `host-terminal`
   两个 interface 均已删除。
3. **配额是自我声明的静态事实**（`ptyQuota`，前置 B）：`spawn` 判据按属主声明值，加载期区间仲裁
   越界即拒 manifest，运行期不夹取。terminal-session 声明 8 = 退役前内核上限，**不借下沉放大**。
4. **输入面不做「宿主绕一圈」**：任务队列下发曾在插件内调 `host.terminal_send` → 宿主查内核属主
   → 回同一插件的会话（真源已移出），恒拒且失败静默。定案：**同实例内的输入直接调自家写入管线**
   （`session::input_via_pty`），跨边界只留给真正的跨插件/跨端消费方（宿主命令面
   `plugin_terminal_send_input` → 窄转发层 → `session-input` 互调 api）。
   判据可迁移性：WASM 实例内自调用没有属主问题（属主就是自己），绕宿主一圈只会把
   「真源换了地方」这件事变成一个静默失败点。
5. **事件载荷必须自足**：宿主不再持有会话事实后，`SessionCreated` 等事件若仍靠处理器回查内核
   取会话名 / 概要，就会得到空。故 SDK 的会话变体把 `session` / `sessionName` 随事件携带，
   宿主只转发；`source_device` 由请求侧透传（广播排除语义）。

## WS 动作词表声明式化（票 09a/09b/09c，2026-09-24 桌面端）

> **现状标注（2026-09-25 v28 硬切后）**：本节描述的是 v17 时期的一次中间态，**其中宿主侧的三个
> 载体已全部删除**——`services/session_control.rs` 转发层、`/ws/event` 路由与
> `Message::SessionControl` 信封（ticket 08 整删，宿主 WS 面收为通用 transport）、
> `/ws/terminal/session/{id}` 数据面与订阅引擎（同批删除）。**仍然成立的两条**：① manifest
> 声明面 `contributes.wsEndpoints` **仍在**（`com.bedcode.terminal-session` 现声明 `session-control`
> 与 `terminal` 两个端点，激活期经 `register_declared_ws_endpoints` 登记，路径约束不变）；②
> 「动作词表的解释权归插件」这一裁决本身反而被 v28 强化——宿主不再有任何业务动作名语义。
> 详见修订记录 v19。

WS 会话/终端控制动作的**词表来源**从「宿主硬编码 switch」改为「插件 manifest 声明 +
插件侧分派」，对齐 `_http_endpoint` 模式。spec：
`.scratch/2026-09-24-host-crypto-business-downsink/issues/09a/09b/09c`。

1. **expand（09a）：声明面 + 激活期登记**。SDK `PluginContributes` 增 `ws_endpoints`
   （两形态同 httpEndpoints），宿主激活成功时经 `register_declared_ws_endpoints` 登记进
   WS 端点表（挂载 `/ws/plugin/<id>/<path>`，端点路径**单段约束**——与 host-websocket
   `register-endpoint` 同口径，路由是两段式 `{plugin_id}/{suffix}`）。登记锚定**激活期**：
   deactivate 会 `purge_for_plugin` 回收 ws 端点，激活期重登记才让 deactivate→activate
   循环不丢。此前的运行时 `ws_register-endpoint` 原语不变（插件仍可自管理端点）。
2. **migrate（09b）：词表解释平移插件**。`com.bedcode.terminal-session` 声明端点
   `session-control`（auth=jwt），新模块 `ws_control` 承接动作分派：list / start / stop /
   remove / resize 五域的**词表解释**（参数校验、编排调用、回包形状）唯一在插件；
   插件实现 `events-ws` 的 `on-client-message` 响应该端点的直连帧，宿主只做认证与转发。
3. **contract（09c）：宿主硬编码 switch 删除**。宿主 `services/session_control.rs` 的
   `match action { ListSessions => … }` 逐臂翻译表删除，`/ws/event` 旧 `Message::SessionControl`
   协议改走**声明式转发**：声明闸门（端点已声明且插件激活，否则显性报错）→ 原始动作 JSON
   转发插件互调 api `session-ws-control`（宿主不解动作名语义）→ 响应动作 JSON 套回
   `Message::SessionControl` 信封（原 `message_id`；信封 `session_id` 取自响应动作的
   `session_id` 字段——start = 新建会话 id，与旧宿主路径逐字一致）。
4. **传输面契约仍在宿主（H2）**：`Message` 枚举与编解码继续宿主持有（移动端线协议需要）；
   迁走的是**词表解释与业务编排**。`SessionControlAction` / `SessionSummary` 等 wire **形状**
   自会话事件下沉专项票 01 起不在宿主定义——收编进 SDK `bedcode-plugin-api::wire`，宿主
   `enums/` 只剩 re-export 垫片（修订口径见下「会话事件面与线协议真源」节 H2′）。
   `SessionControlAction` 请求分派不再在宿主出现——grep 断言宿主 WS 层无业务词表 switch。
5. **数据面不动（H1）**：终端输出订阅 / 输入 / 双速模式（`/ws/terminal/session/{id}` 控制帧）
   是引擎原语（PtyRing + 订阅者执行体），不迁插件；`Message::Terminal` 的 Input/Subscribe/
   Unsubscribe 分支保持宿主侧引擎操作。
6. **移动端零改动**：旧 `/ws/event` 协议 wire（`Message::SessionControl` 请求/响应形状）逐字
   不变（翻天覆地测试：`pty_session_chain` 经转发层全绿）；声明端点是新路由，老客户端不受影响。

## 会话事件面与线协议真源（专项票 01–04，2026-09-24 桌面端，ABI 不变）

> **现状标注（2026-09-25 v28 硬切后）**：本节建立的宿主事件三层转换（`SyncEvent` →
> `DesktopSyncEvent` → `SyncEventHandler`）**已随 websocket 业务下沉整体退役**，不再需要逐项维护：
> `src-tauri/src/events/` 整目录删除（`AppEvent` / `publish` / `EventMatcher` / `HostSyncEvent` /
> `sync_handler`）、`host-events.broadcast-sync` 破坏性退役、SDK `wire::{sync, control}.rs` 子模块
> 删除（`SyncEvent` / `SyncPayload` / `SessionControlAction` / `TerminalAction`）、宿主
> `Message` 业务枚举与 `terminal_ws/` 订阅类型一并消失；**下文提到的两条防回接锁
> （`retired_session_event_mirror_is_not_reintroduced` / `sync_handler_does_not_interpret_session_variants`）
> 随目录删除一并移除**——真源清空后该命名空间本就不存在。**仍然成立的是本节的边界结论**：
> 事件的**形状不算原语**、解释权归 SDK/插件；插件事件面现为 `host-bus.publish` + `host-events.emit`。
> 本节保留为决策历史（它证明了「同一 wire 不在两侧各持一份」这条口径的来源）。

P1-b 之后会话真源已在 `com.bedcode.terminal-session`，但宿主的**事件路径**仍是三段重复转换：
SDK `SyncEvent`（内部标签 PascalCase、字段平铺）→ `DesktopSyncEvent` 穷尽 `From` 镜像 →
`SyncEventHandler` 按 11 个变体业务 match 重建 `SyncPayload`（顺带把线格式改写成
adjacently tagged snake_case、把状态 `format!("{:?}").to_lowercase()`）。宿主因此继续持有一份
**会话业务事件枚举与其解释权**，与本文件的裁剪线冲突。落地为四票 expand–contract，
实施与实测见 `.scratch/2026-09-24-session-events-app-event-poly/`。

1. **线协议真源进 SDK（票 01）**：`SyncPayload` / `SessionSummary` / `SessionControl*` /
   `Terminal*` / `KeyCombo` 五类跨端形状收编 `bedcode-plugin-api::wire`（宿主 `enums/` 对应
   四文件缩为 `pub use` 垫片，导入路径零改动，运行行为零变化）。
   **（v28 终态更正）**：`sync` / `control` 两子模块与对应垫片已随事件面退役删除，SDK `wire`
   只剩 `summary` + `key`；宿主 `enums/` 只保留 `special_key.rs`（`KeyCode` / `KeyCombo` 垫片）
   与 `plugin.rs`（`PluginQuestion` 垫片）两个真源再导出，`pty_status.rs` 是引擎类型。锁分两层：SDK 侧全变体
   wire 形状锁 + 移动端平行副本逐变体对照锁（`mobile_parallel_copy_shape_lock`）；宿主侧
   **类型身份锁**（`crate::enums::*` 与 SDK 路径必须是同一类型，编译期）+ **垫片零定义**
   （源层面扫 `pub enum` / `struct` / `impl`，变异自检对 HEAD 旧内容命中 4/2/14/64 处）。
2. **`AppEvent` 从空 marker 变成发送协议（票 02）**：`source_device()` / `validate()` /
   `to_sync_payload()` 三方法 + 统一入口 `events::publish()`（校验 → 查源 → 投递）。
   `to_sync_payload` **无默认实现**——新增事件类型必须显式回答走不走同步通道，否则「事件发了、
   没人广播、测试全绿」就是默认形态。`publish` 在无事件源时 `Err(NoSource)`：底层
   `EventMatcher::publish` 的「无源即丢弃返回 Ok」原语义保留，显性失败补在统一入口。
3. **两跳合一 wire（票 02，D1）**：`SyncEvent` 改 `tag="type", content="data",
   rename_all="snake_case"`，字段类型与 `SyncPayload` 对齐（`session` 用类型化
   `wire::SessionSummary`，状态用 wire 字符串）。唯一例外：`session_stopped` / `session_removed`
   的 `source_device` 是信封字段，**不出站**（移动端形状逐字节不变）。插件→宿主这一跳的 JSON
   不进 WIT 类型（仍 `event-json: string`），**故不 bump ABI**（`host-events.broadcast-sync`
   签名不变）；换格式后未随包重建的旧产物在**解析期**被点名拒绝，不降级成「无事件」——
   §8「真源换了地方就要 fail-visible」的第 ① 形态。
4. **宿主只剩薄适配 + 瘦处理器（票 03）**：`broadcast_sync` = 解析 → `HostSyncEvent` newtype
   （orphan rule 所需，同时标出「已进入宿主面」这条边界）→ `publish`；处理器只剩折载荷 +
   排除源设备 + `Message::SyncData` 广播，11 个 `handle_*` 与状态 Debug 重格式化删除。
   `to_sync_payload` 是同一 wire 的**机械折算**而非逐变体 match——机械 match 落在宿主就是
   解释权重回宿主的第一块跳板，改由 SDK 侧三条同构锁保证「新增变体漏配即红」。
5. **镜像与遗留面退役（票 04）**：`DesktopSyncEvent` 与其 `From` 整文件删除；宿主本地 wire
   定义无残留；`Message::SessionEvent`（`session_event` 构造器）两端**零生产调用方**
   （历史 `git log -S` 亦无生产发送点）→ 变体与构造器一并退役，会话变更通知的唯一面是
   `SyncPayload::session_created/stopped/removed/status_changed`（由插件发布）。防回接锁：
   `retired_session_event_mirror_is_not_reintroduced`（`src/events/**` + `src/enums/**` 实现段
   不得再现镜像枚举 / `SyncPayload::` 逐变体构造 / `SessionStatus` 解读）+
   `sync_handler_does_not_interpret_session_variants`（处理器实现段零变体分支），
   两条都做了变异自检。
6. **口径边界（H2′，修订上文第 4 点）**：宿主仍**持有** `Message::{SyncData, SessionControl,
   Terminal}` 枚举与编解码（传输面），但**形状定义与解释权在 SDK / 插件**——宿主不解动作语义、
   不按事件变体决定推送内容。裁剪线判据不变：宿主能力只暴露「离宿主无法实现、且无业务语义」
   的原语；**事件的形状不算原语**。
7. **移动端零改动**：移动端 `SyncPayload` 保留平行副本（ADR 0018/0019 双端分叉口径），
   与真源的一致性由 SDK 的逐变体对照锁钉住；出站 JSON 逐字节不变（票 02 双轨对照用例
   在切换前实测：9 条样本新旧路径 `SyncPayload` 序列化结果全等，唯一已知分叉是旧路径
   对 `SessionStatusChanged` 的 Debug 重格式化，而该变体零生产者）。
8. **门禁补口**：形状锁主战场迁进 SDK 后，`test.yml`（只在两端 `src-tauri` 跑 `cargo test`）
   不再执行它们 → 桌面 job 新增 `cargo test --manifest-path
   bedcode-desktop/packages/plugin-sdk-desktop/rust/Cargo.toml`。移动端 SDK 的同一空档
   登记为后续对称项。

## 加密引擎化与 enums 三分类处置（票 01–09c，2026-09-24 桌面端，ABI 25 → 26）

用户 2026-09-24 方向指令第一部分：「WS 层面只留加密抽象层；加密具体实现在宿主侧，通过聚合全局
加密方法大全实现具体加密；插件通过指定加密方法调用宿主加密」。落地为 `crypto/` 引擎模块 + `host-crypto`
原语 + 协商套件参数化，spec：`.scratch/2026-09-24-host-crypto-business-downsink`。

1. **crypto/ 引擎（票 01）**：`src-tauri/src/crypto/` 算法注册表（名称 → 实现 + 白名单 +
abstract trait `AeadProvider` / `KdfProvider` / `KeyAgreementProvider`）。最小子集 aes-256-gcm /
chacha20-poly1305 / hkdf-sha256 / x25519（rsa/hybrid 留待扩展）。裁剪线 = **引擎级**：算法是
应用无关的 POSIX 级能力，宿主按其名持有与调度；`link_crypto` 不再内联任何具体算法调用（票 02，
grep 断言）——WS/HTTP 过滤层只依赖注册表抽象接口。
2. **host-crypto 契约面（票 03/04，ABI 25 → 26，desktop 独有）**：WIT interface `host-crypto`
（aead 加解/密钥/随机数 + key-agreement 生成/共享 + kdf 派生），权限按风险域拆三位
`crypto:aead` / `crypto:asym` / `crypto:kdf`（对齐 ws:client/ws:server 先例），宿主实现带权限门与审计。
**非旁路红线（H3）**：原语只给中性算法、不给编排；算法名白名单（引擎级词汇表）；宿主
密钥（Kd / JWT keystore）**不**经原语暴露，只服务内部 filter 链——认证链路只走既有 auth 模块。
3. **协商套件参数化（票 05）**：WS 链路加密协商（`server/websocket/conn.rs` 的挑战-应答）改按名选套件，
expand–contract 增量演进：suite 可选字段缺省 → 默认套件（x25519 + aes-256-gcm +
hkdf-sha256），未知名套件 fail-visible 拒绝（不静默降级到默认）；移动端旧端零改动（只读 {v,ek}）。
   **（结构更正）**：`CryptoProposal` 结构体已不存在，套件判定现落在
   `server/core/link_crypto.rs::resolve_link_suite`（未知名返回 `Err` → conn.rs close 4003，
   fail-visible 不静默降级），默认套件与算法注册表的一致性由该文件内测试守住。
4. **enums 三分类处置（票 06-09，本专项附录 §4.2 表的落地）**：`special_key` 按键→转义字节翻译
迁插件（票 06，宿主 pty 只收裸字节）；`shell.rs`（ExecutionEnvironment / WindowsShell /
SessionLaunchConfig）整文件删除（票 07，宿主零业务消费）；`SessionStatus` / `SessionType` 收窄为
线协议形状并归位 `protocol/session.rs`（票 08，enums.rs 留兼容 re-export）；动作词表 switch 声明式化
（票 09a/09b/09c，见上节）。`enums/` 终态 = 只剩引擎级类型（pty_status）与传输面契约形状。
5. **双端偏离**：host-crypto 是桌面独有 interface（移动端插件生态薄、加密线协议已有共享 crate），
纳入 ADR 0018 偏离登记（双端偏离节 v26 条目）；协商参数化对移动端 old 端零破坏（增量演进）。

## 终端输出消费插件化 · 性能红线修订（2026-09-21）

roadmap 阶段 3（`.scratch/2026-09-10-plugin-kernel-roadmap/spec.md`）把「终端 UI/渲染
下沉」的前置条件定为「输出分发管道的『内核保有 + 消费插件化』先行验证通过」——该项
长期**未验证**。本修订基于只读探针实测（`.scratch/2026-09-21-terminal-output-consumer-perf/`：
探针 `terminal_output_perf.rs` P1/P2/P2b/P3，release，`--test-threads 1`，1061/0 全绿）
**放宽原性能红线**，并钉死替代禁令。

### 实测证据（release）

| 层级 | 实测 | 解读 |
| --- | --- | --- |
| 纯 Rust 环（无 WASM） | 1.37 µs/op，88 µs/MB | 内核下界 |
| **宿主直调原语**（无 WASM 无 JSON） | **1.3 µs/op，81 µs/MB** | 权限门+锁+环 ≈ 零净开销 |
| guest 原语往返（a03 P5 D 引用） | **38 µs/op** | WASM 边界 + guest 执行固定成本 |
| **真实插件路径**（WIT `list<u8>` 直传） | **≈40 µs/op，~2.6 ms/MB** | 原语往返 + memcpy |
| JSON-RPC 命令通道（fixture 反例） | 84→1178 µs/op，**~75 ms/MB** | 瓶颈是 JSON 数组编解码（~73 µs/KB 线性），非原语 |

钳制事实同时被证实：`PLUGIN_PTY_RING_FETCH_MAX_BYTES`=16 KiB 生效（64K 请求被截断，
1 MiB 拉满恒 64 次调用）。负载折算：1 MB/s 输出风暴下插件消费路径单核占比
**~0.26%**，10 MB/s 极限 ~2.6%；现状 4 ms 合并窗口节奏可直接沿用。

### 裁决

1. **原红线修订为**：「输出高频逐帧分发不进 WASM」→「输出字节**禁止经 JSON-RPC
   命令通道搬运**（~75 ms/MB = 1 MB/s 时 7.5% 单核，随吞吐线性恶化）；**经 WIT 二进制
   原语（`list<u8>` 直传线性内存）消费可进 WASM**（内核保有 ring，插件按游标拉取，
   ~2.6 ms/MB，风暴 <0.3% 单核）」。恐惧来源是 JSON 序列化，不是 WASM 边界本身。
2. **host-pty 第 3 条的形态裁决不动**：仍是「纯拉取 + 游标 + truncated/resync」，
   push 回调维持否决（2026-09-17 背压教训 + wasmtime Store 不可重入，两条理由不因
   本性能数据改变）。本次放宽的是「数据能不能进 WASM」，不是「谁来唤醒消费」。
3. **批量策略保持现状**：16 KiB 钳制足够（1 MiB 64 次调用、风暴 <0.3%），无需为
   终端迁移放宽 `PLUGIN_PTY_RING_FETCH_MAX_BYTES`。
4. **落地形态约束**：若阶段 3 开设输出订阅原语（原否决的 `host-session-output`），
   契约必须与 host-pty 同构——二进制直传 + 游标 + truncated/resync，禁止 `Vec<u8>`
   JSON 数组化；实现可复用 `PtyRing`（不自造平行轮子，取消登记抽象提取候选里的
   ring 抽取项即可）。
5. **移动端不受影响**：输出订阅原语若落地仍为桌面独有（host-pty 先例，双端偏离）。

## 双端偏离（host-websocket / host-pty 等桌面独有接口）

- 移动端是**远程终端控制端**，不承载 PTY / mDNS 广播 / WS 服务端等主机侧引擎，故 `host-websocket`（desktop v14；**2026-10-08 mobile v14 起以客户端 5 函数子集引入**——服务端域 9 函数与 `connection-context` 仍是桌面独有，见 ADR 0041）、`host-auth`（v15 密钥托管；v18 追加认证记录面四函数，其中三函数已于 v24 随删表退役、`auth-setting-set` 保留）、`host-pty`（v16）、`auth-policy` 导出（v17，认证能力——宿主 server 中间件验签后取策略）、**会话语义下沉批次（v18 / v19：`host-session` 配置面 + 创建与动作面 + 注解槽 + 连接清单、`host-platform.wsl-distros`）** 均为**桌面独有接口**：mobile 的 WIT / ABI / SDK 不跟演也不投影（ADR 0018 双端各自演进的文档化偏离，同 wasmtime 桌面先行 48 / 移动暂留 47 的分叉先例——该分叉已于 2026-09-26 关闭，双端均为 48，见 ADR 0019）。当前 **desktop v31 / mobile 11**（v23 = host-session 配置面写原语退役，见 v10 登记；v24 认证记录下沉、v25 host-peer 节点生命周期原语、v26 host-crypto 契约面、**v27 = 会话原语域整 interface 退役**、**v28 = websocket 业务下沉：新增 `host-websocket.connection-context` + 破坏性退役 `host-events.broadcast-sync` / `host-pty.spawn` 的 `hostBroadcastSessionId`**、**v29 = HTTP 路由代码注册下沉：新增 `host-http` 服务端域（register-endpoint / unregister-endpoint，函数级追加，旧产物仍可实例化但不具备注册能力）**、**v30 = 传输编排下沉票 1：host-peer 追加 `active-transfers` / `collect-outgoing`（纯增量）+ 引擎原始事件桥双写**、**v31 = 传输编排下沉票 3：破坏性删除 `resume-all-transfers` + `send-files` 语义收窄「即发即会话」（`concurrency` 载荷字段退役）+ 旧快照 topic 退役**；desktop 独有接口持续演进不要求移动端跟演，但 v27 / v28 / v31 都是**删 import / 删 export / 删函数**的破坏性变更，旧产物在实例化期即失败，须按对应版本 SDK 重建）。**移动端不在 v28/v29 兼容范围**：websocket 业务下沉与 HTTP 路由代码注册两个专项只改桌面端，移动端旧 WS 客户端 / 路由 / wire 形状不动、不承诺兼容（见 `.scratch/2026-09-25-websocket-business-downsink/spec.md` 与 `.scratch/2026-09-25-http-route-registration-downsink/spec.md`）。
- **偏离不止 WIT 面**：本批次同时经用户 2026-09-19 授权**豁免 AGENTS.md §9「协议改动必须两端同步部署」**，豁免范围严格限于该 spec（`.scratch/2026-09-19-terminal-session-plugin/spec.md` D1）。自守边界：线协议**形状**（会话 DTO 字段、同步事件、WS 控制帧、认证握手报文）保持不变——保持它并不需要移动端改一行代码，且它是后置适配专项的成本基线。移动端受损面 M1–M5 已挂进路线图（`.scratch/2026-09-10-plugin-kernel-roadmap/spec.md`），桌面端不为其负责（spec Out of Scope）。
- **恢复条件**：当移动端需要同类能力（例如本地跑交互进程）时，再在该端 WIT 增补对应 interface 并对齐 ABI 计数；在此之前「改 WIT 必须双端同步」这一硬约束的适用范围限于**双端共有的接口**（host-peer / host-fs / host-http 等）。
- SDK 双端独立包（`plugin-sdk-desktop` / `plugin-sdk-mobile`），互不影响；宿主侧 `version > 当前 → 拒绝` 的兼容语义只保证「不高于当前 ABI」的产物不被版本门拦下；**「旧插件（≤v16）零迁移仍可加载」的口径已被 v27 / v28 / v31 三次破坏性变更取代**——这三种产物在**实例化期**即被点名拒绝并要求按对应 SDK 重建（`LoadedWasmPlugin::stale_artifact_rebuild_hint`），见上段与修订记录 v16 / v19 / v20。

### v21 收敛退役：`host-session.create` / `restart` 删除（首个接口函数删除）

host-business-decarriage 收尾批次（`.scratch/2026-09-20-host-business-decarriage/`，
后续批次记录见 `.scratch/2026-09-21-host-rust-residue/`）。裁决要点：

1. **删除而不是保留**：`create(config-id)`（v6 遗留创建）与 `restart(session-id)`
   是「让宿主读主库配置表做映射决策」的最后两处入口。前者在定时任务域改走
   `create-with-spec` 后零消费者；后者随插件把重启改为 `remove` + 同 id
   `create-with-spec` 后零消费者。二者一并删除，宿主侧
   `create_session_with_id` / `create_session_with_source_and_id` / `restart_session`
   / `DefaultNamingService` / `DefaultConfigMapper` / `SessionStorage` 随之退役——
   **内核从此不读会话配置表**。
2. **映射决策的最后一块收口**：重启所需的同 id 重建经 `create-with-spec` 的
   spec 增可选 `sessionId`（JSON 字段，不属接口变化）表达；指定 id 已被在册会话
   占用时宿主显性拒绝，**编排顺序（先 remove）仍是插件的责任**。
3. **投影退役**：主库 `session_configs` 的写面（`session_config_bridge` 投影）
   停写、降级读删除——投影的读者（内核创建/重启）已不存在。表与
   `host-session.config-list|get` 保留为插件的一次性迁移通道（老库 → 插件私有库，
   marker 幂等），其退役需先确认各安装点迁移已跑过。
4. **对外形状不变**：重启仍保持同一 `sessionId`，且前端 `session-restarted`
   事件由插件在 `Created` 生命周期之后经 `host-events.emit` 补发（载荷与退役的
   宿主 `SessionRestartEvent` 同形），桌面端可观察顺序与迁移前一致。
5. **兼容性代价（登记）**：这是首个**删除接口函数**的 ABI 变更——旧产物（≤ v20）
   若仍 import 这两函数会在实例化期被拒，须按 v21 SDK 重建。本仓所有随包产物
   由源码构建，故无外部影响；第三方插件需按此口径升级。

## 传输编排整体下沉（v30 纯增量 + v31 破坏性，2026-09-25 桌面端）

`.scratch/2026-09-25-peer-transfer-orchestration-downsink/`。**裁决要点**：宿主
`peer_engine_*` 所持的传输任务状态机（活跃任务表 / 并发闸门 1..=8 / 历史封顶 200 与 100 /
serve 供流记账 / peer_name 解析 / 取消原因码映射 / pull 任务行预登记与并发信号量）全是
产品语义——与「宿主回查内核拿会话」漂移同型，整体下沉 `file-transfer` 插件（事件归约
状态机，真源在其私有库）：

1. **事件回流 = 方案 A**：引擎 `TransferEvent` 逐条 JSON 化直推 `peer:transfer-event`
   （send 方向）与 `peer:receive-event`（receive 方向），载荷带 `tsMs`（wasm32 无时钟）；
   宿主桥接层只做序列化 + 150ms 进度节流（纯性能）+ OfferPending oneshot 回执登记
   （回执通道不可序列化，必须留宿主）。**旧快照 topic `peer:transfer` / `peer:receive`
   退役**——v30 双写过渡（票 1），v31 删除映射（票 3）。插件以事件归约为唯一任务真源
   （建行/推进/终态/原因码按方向映射/封顶/重试回放单点），快照 merge 在双写期退化为
   校正 + 对账（`reconcile_diff` 偏差 warn 留痕）。
2. **`send-files` 语义收窄（v31 破坏性 ①）**：一次调用 = 一个会话立即发起——宿主并发
   闸门删除，发送方向并发节流由调用方自控（插件侧 `PENDING_SENDS` 队列 + 终态放行）；
   旧载荷 `concurrency` 脉冲字段退役，宿主**运行期显性报错**（fail-visible 双保险之
   行为级）。
3. **`resume-all-transfers` 退役（v31 破坏性 ②）**：「全部恢复」编排归插件遍历自身暂停批
   逐个调 `resume-transfer`；宿主句柄表不再承担批量调度。旧产物实例化期被拒
   （`stale_artifact_rebuild_hint` 点名 v31 重建，fail-visible 三形态②）。
4. **宿主终态 = 句柄表 + 引擎事件桥**：`peer_engine_transfer.rs` 收敛为
   `batch_id → SendSessionHandle`（CancelToken / PauseSlot / epoch / sources——sources 是
   redial 续传必需的引擎事实，spec §4.4）；`peer_engine_receive.rs` 收敛为询问回执表 +
   接收事件桥 + 策略闸门（设置只剩 policy/timeout/download_dir，concurrency 字段删除）；
   `peer_engine_remote.rs` pull 逐文件即发（并发信号量删除）+ `pull-started` 引擎事实事件；
   发送源收集剥离 `source_collect.rs`（纯文件系统事实，send-files 内部收集与
   `collect-outgoing` 原语共用）。`active-transfers` 实现改三处句柄面投影（接口形状不变）。
5. **防回接锁**：`retired_peer_transfer_orchestration_is_not_reintroduced`
   （wasm_flow_test，15 符号源码扫描）+ 变异自检通过；fail-visible 双保险之另一保险 =
   `send-files` 载荷 `concurrency` 字段检测。
6. **双端偏离**：host-peer 是 desktop 独有面，移动端不跟演（SDK 11 不变）；受损清单
   逐条记账于 `mobile-impact.md`——当前无实际跨端受损（wire 数据面协议零改动、两端总线
   物理隔离），唯一可观测差异是桌面 UI 展示名兜底路径变化。

## 授权策略 = 安全闸门，不是业务默认值（2026-09-28 桌面端，ABI 不变）

> **⚠️ 本节「认证策略 capability」部分已被 ADR 0031 修订**（2026-09-29）：`auth-policy`
> 的发现方式从「能力探测 + 排序取首个」改为**显式注册**（认证中心注册表，单中心），
> 且**无中心 / 调用失败一律拒绝**（取代原「传输失败回退放行」的 fail-open 降级）。
> 配套见 `.scratch/2026-09-29-auth-center-registration/`。

`.scratch/2026-09-27-host-authorization-policy/`（spec + 票 01–09）。**裁决要点**：授权策略
（每 app × 每类资源的「总是询问 / 默认 / 始终允许」三档）与其配套的授权记录落在宿主，
但它们**不构成 B5（业务默认值）的越线**——理由与边界如下：

1. **判据：安全闸门归宿主**。策略只回答一件事：**遇到授权记录未覆盖的目标时，要不要问
   用户**。它不决定权限位是否生效（那是 ADR 0020 的批准位与 manifest 声明门），不解释
   产品语义，不持有产品事实。§5.1.3 的「安全闸门」正是这类职责，与「业务默认值」的分界
   在于**它是否替插件决定「业务上该怎样」**。
2. **fail-safe 方向**：默认值 = 默认档（读记录、未命中才问）。任何不认识的档位值一律
   回落默认档（`AuthStrategy::parse`）——绝不回落成更宽松的「始终允许」。读不出档位时按
   「询问」处理，不按放行处理。
3. **manifest 不得声明档位**（加载期显性报错）：策略由**用户**在设置页设，插件在 manifest
   里声明自己「应该被信任到什么程度」是镜像问题——那正是「宿主替插件决定业务策略」的
   变体。这条已落加载期拒绝（`manager::validation`）。
4. **任何档位都放行不了硬闸门**：manifest 声明门、SSRF 与公网→私网重定向拦截、路径
   规范化失败即拒、配额、属主隔离全部在策略层之外或更靠前。**「忽略权限」是错误措辞**
   （spec §4.1 明确否决）：安全界面宁可低估，不可高估。
5. **落账即事实可见**：免询问自动放行（`always_allow`）也必须以
   `source='always_allow'` 留痕，界面标「未经确认」。取消授权 = 删 allow + 落 deny；deny
   可在界面自行移除（两种意图都有出口，spec §8.4）。
6. **生命周期**：停用保留、**卸载清空**（重装即全新授权，与本 ADR 0020 内容哈希钉扎同调）。
   两者语义不同、各有防回接锁（`lifecycle_test::auth_records_survive_deactivate_and_are_purged_on_uninstall`）。
7. **契约影响**：**不 bump ABI**。网络出站询问是 `host-http` 内部新增的失败模式，
   `http_fetch` 的 import 签名与返回类型一字未改；应答通道复用 fs 侧那套宿主面凭证绑定
   机制（新事件 + 新命令，宿主内部）。移动端不跟演（ADR 0018 双端偏离：移动端是自持业务
   App，SAF 选择器本就是按 tree URI 授予）。双端 WIT 副本零变更。
8. **第一方免询问目录 = 固定层，但必须可见**：它优先级高于档位（否则「总是询问」会把一次
   技能同步拆成 N 次点击），因此界面**必须**列出这批条目（spec §7）；撤销靠落 deny 记录
   （deny 优先于第一方层）。不可见的特权就是 AGENTS §8 要防的不可见面。

**防回接**：`security::strategy` 是档位→动作的**唯一**映射点与顺序真源（fs / network
共用）。两处各写一遍必然漂移，而漂移的形态是安全语义级的（“总是询问在文件侧跳过记录、
在网络侧却仍读记录”两边都自洽）。spec §12.2 的五个变异逐条实测杀死 ≥1 项。

## 抽象提取候选（登记，不在本期实施）

- ~~**`PtyRing` ↔ 业务会话输出环（`session_output.rs::UnifiedOutputQueue`）的代码级合并**~~：**已消解（v27）**——业务会话输出环已随内核 `src-tauri/src/session/` 整目录删除，`PtyRing` 成为唯一输出环形态，无需抽取（ADR 0022「会话真源下沉」第 2 条）。原登记理由（形态同源、生命周期不同、暂不自持）随被比较的一侧消失。
- **PTY 引擎与业务会话线的进一步解耦边界**：票 01 已把「输出汇可注入」「终态门与退出码」下沉到 `PtySession`，票 02 追加「命令来源可注入」（`PtyCommandSource::Business | Raw`），`PtySlaveFdPolicy`（业务 `Hold` / 插件 `ReleaseOnSpawn`）仍是会话构造器的分支。若第三条消费线（如 AI 工具执行器）出现，应把「策略三元组（sink / command source / slave policy）+ 尺寸与 env」收敛为一个显式的会话装配参数结构，替代构造器家族。
  **（2026-09-23 后续）**：本候选的前两件已提前落定——`PtyCommandSource` 与 `pty_handler` 已退役、`PtySlaveFdPolicy` 票 3 已统一为 spawn 后释放 slave，`PtySession` 现只收调用方算好的 `CommandBuilder` + sink（见修订记录 v11）；「第三条消费线出现时再收敛策略三元组」的触发条件已不成立。
- **终态可查询面（登记仍有效）**：`PtyTerminationGate`（现居 `src-tauri/src/pty/lifecycle.rs`）已有 `reader_closed()`（信号 ①），缺「终态事件是否已发出」的访问器。补上它可让 host-pty 对「订阅晚于终态」的竞态彻底免疫（当前靠 `spawn` 内「订阅早于 start」的构造顺序防御，该防御无法被测试确定性锁定，见票 04 变异 M2 存活）。
- **`is-running` 判据的归属**：`running && !output_terminated` 是 host-pty 的语义组合（引擎的 `running` 故意不随自然退出翻下，业务线依赖这一点），故该纯判定放在宿主能力实现 `wasm_core/host_api/pty.rs::running_verdict`（旧路径 `host_impl/pty.rs` 已随 wasm_core 重构更名）而非引擎层——若未来业务线也要「如实的存活」，应新增引擎层访问器而不是反向挪用本判定。

## Considered Options

- **维持全量命令面投影（现状）**：切换成本最低，但五处同步税、双份 DTO 翻译与 ABI 不稳定随每个功能持续付费。
- **通用动态通道**（`invoke(name, args)` 单入口）：函数数不膨胀，但 DTO 本就以 JSON 字符串承载，类型化名存实亡；再加动态分发只会失去 WIT 编译期漂移检测的价值，退化为第二个 `command.invoke`。
- **仅分层拆分 interface**（discovery/connection/trust/data 四个接口）：不减少函数数，但权限按接口声明、意图清晰——采纳为本决策的**前置过渡步骤**，非终点。
- **mDNS 纯化拆分 vs `list-devices` 明示豁免**：豁免（明写「唯一有意保留的业务投影」）保住设备列表常热性与函数总数，但在裁剪线上开了原则性口子；纯化拆分使发现回归中性能力，并顺手解开 `dial-peer` 对宿主内部缓存的耦合。采纳拆分（见 host-mdns 节），代价是设备缓存下沉与首屏常热损失。
- **数据面降为字节流会话管道**：把 send/pull/browse 进一步抽象成裸流是最「纯」的形态，但分块/校验/断点/加密须在每个消费插件的 WASM 里重写一遍，直接违背 spec 决策 11——否决。结论：**线协议动词即原语**，ABI 跟随稳定的协议而非易变的 UI。
- **原语化收缩 + 业务下沉（本决定）**：一次重构换取稳定的 ABI 面；代价是插件侧需要重建任务/历史/设置的自持逻辑（一次性成本，且这些本就是插件的产品职责）。

## Consequences

- WIT 变更需 bump ABI 版本并同步两份副本（desktop / mobile SDK 各一份），双端同版发布。**（已被「双端偏离」节修正）**：桌面独有接口不再要求移动端跟演，mobile ABI 停在 11；「双端同版」只适用于双端共有的 interface。
- 对端寻址全面句柄化、`dial-peer` 改 endpoint 入参：Tauri 命令面本身不动，host_impl 投影翻译层负责适配差异；双端迁移完成后，宿主侧 `DiscoveryCache` / 全量快照指纹比对链路可退役。**（已完成）**：`list-devices` 退役后两者均已删除，设备列表由插件订阅 mDNS 事件自建。
- 设备列表失去「插件未激活也常热」特性（browse 随插件激活才开始）：file-transfer 以 activate 即 browse + 自持久化 last-seen 缓存缓解首屏空窗。
- 断点续传的真源仍在接收端落盘侧：`pull-files` 需增加 resume 语义（或独立的已写偏移查询原语），插件的重试编排依赖它。
- 接收策略的「超时自动拒绝」默认行为保留在宿主闸门侧（fail-safe），插件的策略设置只是预配置该闸门的参数；ask 模式的逐批应答经保留的 `respond-transfer` 进入闸门，弹窗编排在插件——安全语义不下沉。
- file-transfer 插件复杂度上升（自管任务状态与设备缓存），但其前端 wire 形状翻译收敛回一层，总体代码量预期下降。
- 本决策修正的是 issue 12 的**投影粒度**，不推翻 spec 决策 11（peer-net 是宿主核心服务）；后续所有新宿主能力接口（含未来领域）均按「无业务语义的基础能力」这条裁剪线执行。

## 修订记录

本节按**写入批次**而非时间序排列；**「当前」= 首条 v20**（对应 desktop ABI v31 / mobile 14）。其余条目标记的「（当前）」是写入当时的时点表述，已按本节实际状态移除——被后续修订取代的结论以本节对应条目为准，
正文中被取代的时点表述均已就地加「现状 / 终态更正」标注。

- **2026-10-08 移动端票 15 阶段 B（host-terminal / terminal-hooks 整面退役 · mobile ABI 16 → 17 · 破坏性）**：
  `.scratch/2026-10-07-mobile-wasm-core-refactor/`（详见 `ticket-15-terminal-ui-downsink.md` §2.9）。
  阶段 A 把终端消费 UI 域迁入插件前端后退役面零消费者成立（宿主 `terminal_output_activity` 链
  无生产构造点、`TerminalAPI.onOutput` 是无发射点的悬挂监听——「文档承诺兑现不了即退役」）。
  ① **WIT 删面**：import `host-terminal`（send）与导出 `terminal-hooks`
  （on-terminal-input / on-terminal-output）整 interface 删除；旧产物在 v17 宿主**实例化期**
  因缺失 import interface 被点名失败（fail-visible ②），内置插件随 APK 同分发无旧产物。
  ② **SDK 删面**：`HostTerminal` trait / `TerminalHandler` trait / `BedcodePlugin.terminal_handlers`
  扩展点 / `LifecycleContribution.onTerminal*` / `TerminalContribution.inputHandlers|outputParsers` /
  TS `TerminalAPI`（sendInput / onOutput）/ `LifecycleAPI.onTerminal*` / `PERMISSION_TERMINAL_INPUT`
  权限位（manifest-gen 的 `.terminal` 宽推导线同批清理——否则插件源码 `.terminal` 子串会把退役
  权限自动加回 manifest）。③ **宿主删面**：`host_impl/terminal.rs`、`component.rs` 的
  `host_terminal` Host impl 与 linker 注册、`PluginLifecycleEvent::TerminalInput/TerminalOutput`
  变体、`router/event.rs` 的 `terminal_output_activity` listener、前端 `LifecycleAPI` terminal 两条。
  ④ **保留面**（防回接锁反向断言）：`host-terminal-stream.forward-output` + `terminal:output`
  权限位 + `terminal_stream_gateway` 窄转发 + `host-connection.primary-target` 原样在场
  （C3 二进制出口与票 13 地基不随回调面退役）。⑤ 新锁
  `retired_mobile_host_terminal_hooks_lock.rs`（4 例：WIT / Rust 接线 / 前端词汇 / 保留面）。

- **2026-10-08 移动端票 16（auto-task 插件并入 terminal-session app · mobile ABI 16 不变 · 零 WIT 变更）**：
  `.scratch/2026-10-07-mobile-wasm-core-refactor/` 阶段 3 收口票（详见 `ticket-16-auto-task-merge.md`），
  spec D6 选项 A：`com.bedcode.auto-task`（TS 面板 + 极简 rust 壳）整体并入 `com.bedcode.terminal-session`
  （移动版 app，与桌面同名不同职责，C8 登记见 ADR 0018）。① **随迁即删**（D6 强制①）：极简 rust 壳
  （`invoke_command` 显式全拒、TS 从未 invoke）与 `contributes.commands` 4 条 `auto-task.*` 命令退役；
  `contributes.lifecycle` 四钩子（未注册 handler、宿主 dispatch 空转）不再声明；任务域前端按域重组进
  `plugins/terminal-session/src/task/`（activate / api / i18n / devMock / components / composables，
  i18n 前缀随插件 id 自动切换）。② **B1–B6 零命中**：合并属插件间整合，宿主零新增能力；本票零 WIT /
  ABI 变更（mobile 停 v16）。③ **HTTP 基址切换**：`/api/plugin/com.bedcode.auto-task` →
  `/api/plugin/com.bedcode.terminal-session`（消解 roadmap M1 受损项），桌面
  `LEGACY_HTTP_PLUGIN_ALIASES` 别名表保留不动（桌面改动 Out of scope，切断列双端同批另立项）。
  ④ **fail-visible 三形态**：插件目录 / 打包资源目录删除（①）、退役 id / 视图 id / 命令 id 全量清退
  （③）+ 防回接锁 `retired_mobile_auto_task_plugin_lock.rs`（4 例 + 变异自检 4/4）；零 ABI 变更故无
  第二形态判据扩展。⑤ 硬引用同批换 id：宿主 fs_auth 白名单 / 根插件清单 / dev 注册表 / dev-shell
  mock 基址 / 开发文档；CI 插件安装列表补 terminal-session。终端订阅 / 认证编排域（票 12 / 14）零改动。

- **2026-10-08 移动端票 11（host-websocket 客户端域 · mobile ABI 13 → 14 · 纯增量）**：
  `.scratch/2026-10-07-mobile-wasm-core-refactor/` 阶段 3 首票（票 12 终端订阅迁插件的铺路票），
  落 ADR 0041：新增 `host-websocket` 客户端 5 函数（connect / send-text / send-binary / close /
  is-connected）——**桌面 15 函数的真子集**，服务端域 9 函数与 `connection-context` 不存在于移动端
  （ADR 0018 不跑 WS 服务器；边界锁 `mobile_host_websocket_client_domain_lock` 锁 WIT 函数名与
  权限词汇，变异自检 2/2）。权限位只加 `ws:client`（SSRF 面 fail-closed，移动端四同步点）；
  投递双通道 = 状态事件 JSON 属主 topic（`<owner>:ws:open|error|close`）+ 下行帧二进制属主 topic
  （`<owner>:ws:message`，帧信封 `kind + handle 长度 + handle + payload`，复用 v9 `events-binary`
  导出——与 spec 的 events-ws 方案偏差，见 ADR 0041 D4）；消费插件须同时持有 `bus` 权限位
  （订阅总线本体要求，集成测试实证）。引擎 = 移动端自建 `host_impl/ws.rs`（桌面
  `bedcode-server-websocket` 客户端段与 actix 服务器栈耦合不可复用，同构但分叉，条件触发转共享
  子 crate）；不做 wss / 重连编排（退避重连 / 心跳 / 订阅协议归插件，票 12）；`connect` 同步阻塞
  握手、timeout 上限 5s（host fn 同步上下文，ADR 0029 不长挂实例）。纯增量：v13 产物照常加载但
  无 ws 能力（单向协商，票 12 须同批处理能力探测）。

- **2026-10-08 移动端票 12（终端订阅协议客户端迁插件 · mobile ABI 14 → 15 · 纯增量 + config 增强）**：
  `.scratch/2026-10-07-mobile-wasm-core-refactor/` 阶段 3 第二票（详见 `ticket-12-terminal-link-downsink.md`）。
  ① **B2/B4 下沉兑现**：宿主 `terminal_link.rs`（1,363 行）与 `enums/special_key.rs` 整体退役，
  终端订阅协议状态机（fresh subscribe 门控 / 本地字节计数 / ack 节流 / ring_resync 重锚 /
  session_missing 三振 / 文本+特殊键共存输入计划）迁入新建内置 wasm app
  `com.bedcode.terminal-session`（id 沿用桌面同名 = spec D6 选项 A——**两端同 id 职责不同**，
  本端是远程终端控制端，ADR 0018 契约独立，C8「同名 ≠ 契约同一」在本条与 ADR 0018 登记）。
  ② **两个新接口**：`host-terminal-stream.forward-output`（插件把输出**裸字节**交宿主转发到
  已登记前端页面 Channel——**C3 二进制出口的最终形态**，宿主零解析按 session-id 寻址，
  四类薄壳④；权限复用既有 `terminal:output` 词汇）与 `host-connection.primary-target`
  （主连接目标引擎事实读取，**无权限门**对齐 host-platform 例外先例；与桌面 `host-connection`
  同名不同形——桌面 15 函数连接上下文域，移动端 1 函数；票 13 会话控制迁插件复用同一地基）。
  ③ **host-websocket config 原地增强**（零 WIT 形状变化）：`jwt-auth`（宿主代发首消息认证帧，
  token 从宿主认证状态取、零过境插件——C4；auth 帧形状是两端宿主传输面契约，与桌面
  `PluginChannel` AuthFrame 对称）、`heartbeat-secs`（连接级心跳 + 3× 静默判死，对齐桌面
  服务器骨架心跳——「心跳归引擎」）、`auto-reconnect`（断线自动重连复用宿主
  `connection::reconnect::ReconnectManager` 全局单一事实源——杜绝第二张退避表；重建连接
  `ws:open` 携带 `reconnectedFrom` 旧句柄供插件把新句柄接回等待中的订阅，每轮退避发布
  `ws:reconnect-scheduled` 事件保留倒计时 UX；取消 = 插件对句柄 close 或停用 purge）。
  ④ **宿主保留面**：`terminal_stream_gateway.rs` 页面 Channel 表 + 窄转发（Channel 是 Tauri
  传输机制插件无法持有，`terminal_page_subscribe/unsubscribe` 命令迁至该层），其余 8 个
  `terminal_*` 命令注销、前端协议面走 `plugin_invoke`。WASM 插件无时钟——terminal_link 的
  三处时间驱动面（重连 / 心跳 / ack 空闲轮询）按 ③ 归位，ack 空闲轮询退役改由前端
  onWriteParsed 持续驱动（偏差记录于票文档 §6.3）。⑤ 纯增量：v14 产物照常加载；票 11 的
  「v13 产物无 ws 能力」暴露面以分析销账（terminal-session 为 v15 首发新 id、与宿主同 APK
  分发、版本错位窗口 = 0；v15 产物在 v14 宿主实例化期点名缺失接口 = fail-visible ②）。
  防回接锁 `retired_mobile_terminal_link_lock.rs`（4 例 + 变异自检 4/4）。

- **2026-10-07 移动端票 06（发送编排下沉 file-transfer 插件 · mobile ABI 11 → 12 · 破坏性）**：
  `.scratch/2026-10-07-mobile-wasm-core-refactor/` 阶段 2 首票，处置同本节 v20–v31 桌面口径
  （移动端自有演进，ADR 0018 契约独立；ADR 0022 判据同源适用）：
  ① host-peer 删 `resume-all-transfers`（批量恢复编排归插件——插件遍历自身 paused 条目逐个调
  `resume-transfer`）；② `send-files` 语义收窄「即发即会话」（一次调用 = 一个会话立即发起，
  返回值即传输句柄），`concurrency` 载荷字段**运行期显性拒绝并点名 ABI v12 重建**（宿主并发闸门
  删除后无第二道拦截）；③ 发送方向回流改走新公开 topic `peer:transfer-event` 引擎原始事件
  （progress 150ms 节流 / terminal / paused / resumed / pull-served），旧快照 topic
  `peer:transfer` 与其前端事件 `peer-transfer-changed` 的总线桥接映射一并退役。
  宿主 `peer_transfer.rs` 收敛为「`SendSessionHandle` 句柄表 + 事件桥」——任务表、发送并发闸门与
  队列泵、终态历史文件（`transfer_history.json` 读写路径删除）、serve 供流记账、原因码映射
  全部下沉插件 `transfer_store::reduce_event`（唯一任务真源）+ 插件侧 `PENDING_SENDS` 并发闸门。
  防回接：`retired_mobile_send_orchestration_is_not_reintroduced`（结构面）+ 载荷字段检测
  （行为面）双保险，均已变异自检。接收方向（`peer_receive.rs` 任务表 + `peer:receive` 快照）
  属同阶段票 07，**本批不动**。

- **2026-09-28（授权策略 = 安全闸门；ABI 不变，desktop 仍 v31 / mobile 11；不占 v 编号）**：见上方
  「授权策略 = 安全闸门」节。三档策略（总是询问 / 默认 / 始终允许）× 授权记录真源落在宿主，
  明确**不构成 B5 业务默认值越线**（只决定「问不问」，不放松任何硬闸门；manifest 不得声明档位）。
  网络侧首次有出站授权记录与询问（`host-http.fetch` 增「权限位 + 记录」双门，**WIT 签名与
  import/return 面零变化**——按 v22 总线 topic / `fs:pick` 先例登记在 WIT「不 bump 版本号的
  语义变更」第三例）；fs 侧记录按**操作集**拆分（旧扁平表退化为只读回退）。生命周期
  **停用保留 / 卸载清空**。移动端零改动（ADR 0018）。防回接：档位 → 动作的映射与判定顺序收在
  `security::strategy` 一处（fs / network 共用），spec §12.2 的五个变异逐条实测杀死 ≥1 项。
  实施与验收见 `.scratch/2026-09-27-host-authorization-policy/`（spec + 票 01–09）。

- **2026-09-25 v20（当前 · desktop v31 / mobile 11）**：传输编排整体下沉（ABI 29 → 30（纯增量：`active-transfers` /
  `collect-outgoing` + 引擎原始事件桥 `peer:transfer-event` / `peer:receive-event` 双写）→
  31（破坏性：`resume-all-transfers` 删除 + `send-files` 语义收窄「即发即会话」+
  `concurrency` 载荷字段运行期拒绝 + 旧快照 topic 退役）；移动端明确不在兼容范围、零改动）。
  宿主 `peer_engine_*` 收敛为「会话句柄表（`SendSessionHandle`）+ 引擎事件桥（150ms 节流）+
  询问回执表 + 策略闸门」——任务状态机 / 并发闸门 / 历史封顶 / serve 供流记账 / peer_name 解析 /
  原因码映射 / pull 任务行预登记全部下沉 `file-transfer` 插件事件归约状态机（真源在插件私有库）。
  防回接锁 `retired_peer_transfer_orchestration_is_not_reintroduced`（15 符号源码扫描）+
  `stale_artifact_rebuild_hint` v31 判据（实例化期点名重建）。详见上方「传输编排整体下沉」节、
  `.scratch/2026-09-25-peer-transfer-orchestration-downsink/`（spec + mobile-impact）与 `CHANGELOG.md`。

- **2026-09-25 v29（HTTP 路由代码注册下沉）**：**宿主 HTTP 传输面的路由登记权整体移交插件**——新增 `host-http` 服务端域（`register-endpoint` / `unregister-endpoint`，ABI desktop v28 → **v29**）：插件在 activate 期代码注册自身路由（内部端点段 + 对外 URL 别名 + 方法 + 认证档位），宿主只保留通用注册表（`server/http/registry`：命名空间 key = owner + host_path + method、对外 URL 空间唯一仲裁、停用回收）/ 通用判定（网关查动态表，含 `{id}` 模板匹配 + 捕获值经 `params` 注入插件，宿主不拿捕获值构造路径）/ 通用转发（复用 `forward_to_plugin`）/ 验签引擎。**用户裁定链**：认证（含 jwt_auth 前缀规则）下沉认证中心、网关不绑业务路由、插件代码运行时注册（不通过 manifest 声明）、插件名 = 路由命名空间 + 激活期同名拒绝、本次含 WS 认证对齐（`verify_endpoint_jwt` 补 `enforce_connection_policy`）、认证中心角色改 capability 发现（扫描导出 `auth-policy` 的激活插件，取代硬编码 `SESSION_PLUGIN_ID`）。**静态声明面退役**：manifest `contributes.httpEndpoints` / `toolProviders`（从未落地消费）不再登记，manager registry 的 http_endpoints 表整体删除。**迁移**：terminal-session 42 条代码注册（业务 10 + 认证 7 + 任务 17 + sessions REST 7 + terminal-bg 1），`/api/sessions*` 七条 REST 与 `/api/auth/*` 下沉插件域（`sessions_http` / `auth_http`），`session_controller.rs` 删除；terminal-bg 因二进制通道缺口保留宿主读文件 + 注册表门控。对外 URL 逐字不变；未激活宿主别名由 200+1007 变为 404（fail-visible）。

- **2026-08-26 v1**：初版裁决——host-peer 27 → 约 15（任务/历史/设置等业务面下沉）。
- **2026-08-26 v2**：同一裁剪线二次收紧——① 共享目录 CRUD 三函数 → `set-shared-roots` 全量推送；② `disconnect-peer` + `cancel` 合并为统一 `close(handle)`，对端寻址全面句柄化；③ `pick-files`/`pick-folder` 移交新 `host-platform`；④ `list-devices` 拆分为 `host-mdns` browse-only 纯能力（自我广播留宿主自动生命周期）；⑤ 纠错：`respond-transfer` 从下沉清单改判安全闸门应答原语（无它则 ask 模式无法放行单个接收批）。最终 host-peer = 11 个函数。本修订仅定契约，代码尚未实施（WIT 现状仍为 27 函数全量投影）。
- **2026-08-26 v3**：Phase 1–2 已实施（新原语并存、ABI desktop v9 / mobile v7），Phase 3–4 规格落成时发现本 ADR 内部张力：Consequences 段「插件的策略设置只是预配置该闸门的参数」暗示存在配置通道，v2 退役表却将 `set-receive-policy` / `set-download-dir` 列入下沉。经裁决修正：二者是「引擎安全闸门/落盘配置」而非业务编排，符合本文裁剪线，保留为终态原语；`get-receive-settings`（读接口）维持下沉。**host-peer 终态 = 13 个函数**（11 + 二配置原语）；上文「最终 host-peer = 11 个函数」为 v2 时点表述，以本修订为准。实施规划见 `.scratch/peer-network/spec-plugin-self-hosting.md`（**该 scratch 目录已不在仓库中**，2026-09-26 核对；本条裁决本身以本节表格与 WIT 为准）。
- **2026-09-21 v21**：**首次删除接口函数**——`host-session.create` / `restart` 退役（ABI desktop v20 → v21），宿主侧创建与重启执行器、命名/配置映射服务、`SessionStorage` 与主库配置投影写一并删除，内核不再读会话配置表。裁剪线依据：创建/重启的**映射决策**（命名唯一化 / config→launch / 何时启动 / 先 remove 再重建）全部是产品语义，宿主只保留 `create-with-spec` 执行端（shell 包装 / 发行版转换 / 尺寸缺省 / id 仲裁）与 `remove` 注册表清理。详见上方「v21 收敛退役」节与 `CHANGELOG.md`。
- **2026-09-21 v23**：**性能红线修订（roadmap 阶段 3 前置验证通过）**——「逐帧输出不进
  WASM」放宽为「输出字节禁止经 JSON 命令通道搬运，经 WIT 二进制原语（`list<u8>` 直传）
  可进 WASM」。依据：`.scratch/2026-09-21-terminal-output-consumer-perf/`（只读探针
  P1/P2/P2b/P3，release：真实插件路径 ≈40 µs/op / ~2.6 ms/MB，1 MB/s 风暴单核 0.26%；
  JSON 反例路径 ~75 ms/MB）。host-pty 纯拉取形态不变；`host-session-output` 若开设必须
  二进制直传。详见「终端输出消费插件化 · 性能红线修订」节。
- **2026-09-21 v22**：`host-platform.reveal-in-dir` 原语化（ABI desktop v21 → v22；desktop 独有，双端偏离同 `host-platform`）：把「在系统文件管理器中定位并选中文件/目录」从「宿主 Tauri 命令 `plugin_reveal_in_dir` + `system:open` 权限 + 前端 `context.system.revealInDir` 桥」改为内核原语。裁剪线依据：定位是**平台交互动作、不读取任何数据**（路径本就由调用方提供），与 `pick-files` / `pick-folder` 同口径——因此**不叠加权限门**（宿主 `host-platform` 域保持「无权限门」的一致性）。`system:open` 权限随宿主命令面与前端插件 API 一并退役，五同步点全落：SDK 常量与 API 映射 / 前端合法集合 / 宿主命令面（`require_system_open` 门） / 唯一消费方 file-transfer 的 manifest 与调用点（改经自身命令 `file-transfer.reveal-in-dir` 走 SDK 原语） / 打包侧校验。实现本体（Windows Shell COM / macOS `open -R` / Linux `xdg-open`）归引擎模块 `system/opener.rs`，宿主 `open_log_dir` 与插件原语共用一份。实施记录见 `.scratch/2026-09-21-host-rust-residue/issues/04`。
- **2026-09-15 v4**：host-mdns 升级为 mDNS 基础能力服务（见「host-mdns v2」节）：新增 advertise / stop-advertise / is-advertising 三原语（config-json 纯引擎参数）、浏览事件定向投递 `<owner>::mdns:found` / `<owner>::mdns:lost`（payload 增 serviceType/browserId）、单守护收敛（全局唯一 ServiceDaemon，peer-net 与插件共享）、双表属主仲裁与按属主回收、宿主身份广播登记（owner=host，零业务代码红线 D3）、Android 多播锁随单守护常驻获取；全局发现桥接与缓存重发通道退役（D1），file-transfer 双端一期迁移（D2）。ABI desktop 12→13 / mobile 10→11（ADR 0019 双端同版）。实施验收后落 ADR（D5 定案）。
- **2026-09-18 v5**：新增 host-websocket（见「新增 host-websocket」节）：客户端域（connect / send-text / send-binary / close / is-connected）+ 服务端域（register-endpoint / 收发 / 广播 / 踢出 / 注销 / 清单）共 14 函数 + 可选导出 `events-ws`（宿主动态探测，未导出则帧丢弃 + 首次 warn + 计数）；状态事件改 **owner 作用域 topic**（``<owner>::ws:<event>`，标识在 payload，D3）；插件端点挂载 `/ws/plugin/<plugin-id>/<path>`（命名空间由宿主注入，D5）；权限按域拆 `ws:client` / `ws:server`（D6）；插件端点帧过流量过滤链但不参与链路加密（`TrafficChannel::WsPlugin`，D9）；本期仅 `ws://`（D7）。ABI desktop 13→**14**（mobile 11 不变，ADR 0019 双端各自演进）。
- **2026-09-19 v6**：新增 host-pty（见「新增 host-pty」节）：6 函数（spawn / write / resize / kill / ring-fetch / is-running）+ 唯一生命周期事件 `<owner>::pty:exit`；输出面定为**纯拉取**（否决 push 回调），限额四项按「创建类失败可见 / 数据面读侧截断」分级，环容量改为 spawn 的插件声明参数（宿主上下限仲裁）。同时首次把**桌面独有接口的双端偏离**成文（「双端偏离」节：host-websocket v14 / host-auth v15 / host-pty v16，mobile 不跟演 + 恢复条件），并登记两条抽象提取候选（「抽象提取候选」节）。ABI desktop 15→**16**（v15 由认证中心线 `host-auth` 占用；mobile 不跟演）。实施与验收见 `.scratch/2026-09-19-pty-base-service/`（票 01-07）。
- **2026-09-19 v7**：`host-auth` 追加**认证记录面**四函数（`trusted-devices-list` / `trusted-device-revoke` / `connection-history-list` / `auth-setting-set`）：读**内核原始记录**（`pairings` 全表含软删行、`connection_history`、`settings` 白名单键），排序 / `is-active` 过滤 / 展示组织与派生视图一律归插件（裁剪线：宿主不解释「什么算已连接设备」）；`pairings` 的凭据列（session token / public key）不出内核；属主说明——记录是宿主全局数据、无句柄表，故无属主段校验，权限门 `auth` 即授权边界。ABI desktop 17→**18**（mobile 不跟演，同「双端偏离」节）。实施与验收见 `.scratch/2026-09-19-terminal-session-plugin/`（票 05）。
- **2026-09-20 v8**：会话语义下沉批次落成（见「会话语义下沉批次」节）：ABI desktop 18→**19**（`host-session` 配置面 + `create-with-spec` + 动作四项 + `annotate` / `connections-list` + `host-platform.wsl-distros`，同批次函数级追加不再 bump），新增两个权限位 `session:config` / `ui:settings`，设置分组扩展点与「内置入口按贡献插件运行态让位」两条内核 UI 改动落地，`com.bedcode.devices` 与 `com.bedcode.auto-task` 两个桌面插件退役并合并进 `com.bedcode.session`（旧 HTTP 前缀由宿主别名表兜底、切断时机并入移动端专项）。移动端零改动，其受损清单与 §9 同步豁免一并记入「双端偏离」节与路线图。实施与验收见 `.scratch/2026-09-19-terminal-session-plugin/`（票 01–18）。
- **2026-09-22 v9**：**插件 id 变更登记 + 终端窗口域下沉收口 + 输出原语落地**。① **id 变更**：
  `com.bedcode.session` → `com.bedcode.terminal-session`（票 06，全链改名；ABI/接口面零变化——plugin id
  不入 WIT/线协议）；旧 HTTP 前缀 `/api/plugin/com.bedcode.session/*` 与旧互调 api 名在**双投窗口**内由
  宿主别名兜底到新插件（HTTP 走 `LEGACY_HTTP_PLUGIN_ALIASES`、互调走 `activation::with_api_aliases`，
  属主仍解析到新插件、回复道 sender 校验口径一致；票 07）。② **私有库路径迁移**（票 07）：插件私有库
  按 id 分文件，改名后既有用户数据（会话配置/任务历史/迁移账本）在新路径缺失——宿主新增
  `plugin/session_db_migration.rs`（沿用 task_data_migration 形状：账本即版本戳 / INSERT OR IGNORE 幂等 /
  best-effort 不阻断启动 / 列名交集拷贝 + task 域改名表字典对齐；目标库缺失走纯文件重命名）。
  ③ **输出原语落地（v23 性能红线的正例）**：`host-session.output-ring-fetch`（票 04）——v23 修订
  「输出字节禁止经 JSON 命令通道搬运，经 WIT 二进制原语（`list<u8>` 直传）可进 WASM」的落地点：
  v22 内**函数级追加不 bump**，插件经 `session.output.pull` 拉取会话 ring（权限 `terminal:output` + 属主
  校验），宿主 Channel 输出传输（`terminal_stream` 命令面）随前端消费方一并摘除。④ **终端窗口域下沉收口**
  （票 05）：宿主 `TerminalPreview` / `composables/terminal` / `utils/terminal` 与 attachSink 契约摘除，
  宿主终端窗口 API 过激活门禁（session 插件停用 → 显性报错，不留降级代办）。移动端零改动，双端偏离表不变。
- **2026-09-22 v10**：**host-session 配置面写原语退役（ABI desktop 22→23）**：
  删 `config-upsert` / `config-delete`（业务配置真源自票 08 起在插件私有库，宿主写原语无调用者——
  死接口删除，行为零变化）；读取面 `config-list` / `config-get` 保留为**一次性 legacy 迁移通道**
  （`terminal-session` 激活时读主库 `session_configs` 迁入私有库，marker 幂等；/api/sessions/start 已
  走插件编排不再读主库——票 09 起的退役绑定条件已满足）；权限位 `session:config` 同步退役
  （config-get 改挂 `session:read`，五同步点全落，`gen:permissions` 重出）；`SessionConfigManager`
  收缩为只读迁移通道（写路径 + Config 事件发布删除，引擎层 SQL 写接口保留为基础服务）；
  主库 `session_configs` 表保留为迁移源，观测信号（启动 `legacy_rows` 计数）归零后作 contract 删除
  （迁移窗口结束再删读面与表）。桌面独有接口，移动端零改动。实施见
  `.scratch/2026-09-22-pty-business-downsink/spec.md`（阶段 1）。
- **2026-09-23 v11**：**PTY 引擎去业务化收口（ABI 不变，desktop 仍 v25；移动端零改动）**。
  ① **宿主 shell 包装退役**：删 `pty/command.rs::build_command`（`bash -lic` / PowerShell `-Command` /
  CMD `/K` 包装、cwd 兜底、WSL 路径转换）与 `pty/wsl.rs::windows_to_wsl_path` / `execute_command`
  （`pty/wsl.rs` 只留发行版列举，即 `host-platform.wsl-distros` 原语的实现）。shell 包装的唯一实现
  是插件 `terminal-session/rust/src/launch.rs::build_argv`（`.scratch/2026-09-22-pty-business-downsink`
  票 1 已先行落地，本批次删除宿主旧路径）。裁剪线依据同「新增 host-pty」第 1 条 D1——包装 / 路径转换 /
  cwd 兜底 / 会话身份 env 注入都是产品决策。
  ② **引擎面收敛为「只收 argv」**：`PtyCommandSource`（`Business` / `Raw` 枚举）与 `pty_handler`
  工厂 trait 退役（trait 无 `dyn` 消费者）；`pty/` 不再 import `SessionLaunchConfig` /
  `ExecutionEnvironment` / `BEDCODE_SESSION_ID`，从类型上不可能再包装 / 转换 / 注入。构造入口
  `PtySession::with_command`（业务线）/ `with_private_command`（host-pty 私有线）。
  ③ **业务实现归位**：业务翻译单点落在 `session/session_manager.rs::launch_command`（argv / cwd 仅
  原生环境显式设置 / env 透传 / `BEDCODE_SESSION_ID` 注入）；业务输出汇 `SessionOutputSink` 自
  `pty/output_sink.rs` 归位 `session/session_output.rs`（引擎只留 `PtyOutputSink` 抽象）。
  ④ **旧产物不静默断流**：`SessionLaunchConfig.command_args` 由 `Option` 改**必需** `Vec<String>`，
  `create-with-spec` 对缺省 / 空 `commandArgs` 与旧 `args` 字段**显性拒绝**（报错点名用新 SDK 重建），
  不保留旧路径回退分支；`command` 字段降级为纯诊断串。接受的能力收窄：CMD 分支不可达（插件
  environment 词表只映射 PowerShell），其危险字符拒绝随宿主实现退役——该收窄需用户复核。
  ⑤ **记入既有发现**（非本批次引入）：`pty_reader` 在读线程**入队**尾帧后即 `mark_reader_closed`，
  `sink.on_bytes` 在独立消费者任务里异步执行 → 终态事件到达 ≠ sink 已收到尾帧；原注释表述相反，已
  按事实更正，消费方（含测试）须有界轮询。残余风险与后续（会话状态机 / 登记 / 生命周期分发下沉、
  输出面形态 B）见 `.scratch/2026-09-23-session-engine-downsink/spec.md`。
- **2026-09-23 v12**：**`host-app.plugin-resource-dir` 原语（ABI 不变，desktop 仍 v25；
  同批次函数级追加不 bump；移动端零改动）**。插件取自身资源目录（安装目录绝对路径，含随包资源
  如 Agent 集成 hook 脚本）的**唯一**原语。裁剪线依据：宿主本来就持有插件加载路径
  （`extension_path`），返回的是**调用方自己的**目录、不含任何跨插件信息、零业务语义——
  因此**不设权限门**（同 `host-platform` 与 v22 `reveal-in-dir` 口径：没有可授予的权力，
  加门只会造出一个恒过的死门）。存在理由（前置性）：会话创建编排整体移交插件后
  （`.scratch/2026-09-23-session-engine-downsink` P1-b）宿主不再产生 `Creating` 生命周期事件，
  而事件 payload 的 `resource_dir` 字段**曾是该目录的唯一来源**——不先立原语，创建路径会被
  自己的下沉卡死（fail-visible 而非 fail-silent：插件取不到即显性告警并跳过集成注入，
  不用空串拼路径去读一个不存在的文件）。宿主实现与生命周期事件 payload 同源
  （`strip_verbatim_prefix(extension_path)`），未知插件显性报错。实施见
  `.scratch/2026-09-23-session-engine-downsink/spec.md`（P1-b 前置 A）。
- **2026-09-24 v13**：**会话真源下沉落地（P1-b，ABI 不变，desktop 仍 v25；移动端零改动，
  受损清单 M6–M9 记入路线图）**。业务会话的登记 / 状态机 / 生命周期分发 / 创建 / 停止 / 输入 /
  尺寸裁决整体归 `com.bedcode.terminal-session` 私有登记域，宿主只剩 PTY 引擎与 `host-pty` 原语面。
  ① **创建走 `host-pty.spawn`**：会话 id 由插件自产、`BEDCODE_SESSION_ID` 由插件注入 spawn `env`
  （v11「宿主原语化 vs 插件持有」开放点的定案），`ptyQuota: 8` 按声明式配额（前置 B 的加载期区间
  仲裁首次有真实声明方）。② **`host-session` 12 原语中 9 条转为零消费者**（`list-sessions` / `get` /
  `create-with-spec` / `close` / `remove` / `rename` / `resize` / `annotate` / `output-ring-fetch`），
  整 interface 退役与 ABI bump 随 P4；仍活的三条：`connections-list`（宿主 server 连接事实，
  按 v12 裁决 5 迁独立原语）、`lifecycle-register` / `input-register`（插件 activate 仍订阅，
  降为兼容面，随 P4 删）。③ **`host-terminal.terminal_send` 转为零生产消费者**——其属主判定与写入
  都查内核 `SessionManager`，真源切换后对真实会话恒 `not owner of session`；队列下发改在插件实例内
  直调自家写入管线（见下「会话真源下沉」节第 4 条），本原语随 P4 一并裁定退役。④ **事件面不 bump**：
  `SyncEvent` 的四个会话变体是 `broadcast_sync` 的 JSON 载荷增量（函数签名零变化、载荷自足），
  与 v22 的 `host-bus` 命名空间同类「不 bump 的行为变更」口径；旧产物不静默断流（宿主侧处理器
  保留「回查内核」兜底分支）。实施与验收见 `.scratch/2026-09-23-session-engine-downsink/spec.md`。

- **2026-09-24 v14**：**`host-connection` 独立原语落成（票 04，ABI 不变；移动端零改动）**。
  v12 裁决 5「连接清单迁独立原语」实施：WIT 新 interface `host-connection` + `world plugin`
  追加 import + 宿主实现落 `wasm_core/host_api/connection.rs`。函数名**沿用
  `connections-list`**（WIT 里 `list` 是关键字，故迁出零改名——插件调用点与派生视图回归用例
  断言一字未动，票 04 验收第 2 条的「改动只能来自改名」因此不触发）。
  ① **权限判据 `session:read` → `connection:read`**（新增位，域名与位名同源）。
  ② `host-session` 上的旧入口改为**同判据的别名转发**，不留第二把钥匙：只授 `session:read`
  的插件走两条路径都拿 `permission denied`（宿主侧有单钥匙锁与字节一致锁两条用例）。
  ③ 属**不 bump 的行为变更**（同 v22 `host-bus` topic 命名空间口径）：未声明新位的旧产物
  fail-visible 拒绝，不静默降级；生产唯一消费方 `com.bedcode.terminal-session` 与本票同批
  重建产物（manifest 声明 + 插件 rust/前端两处权限 pin 同步）。
  ④ 返回字节逐字不变（六字段 camelCase、注册表存储序、连 `session error: …` 错误前缀都保留——
  改文案属线协议变更，另案）。旧别名随 `host-session` interface 退役（票 10）删除。
  实施与验收见 `.scratch/2026-09-23-session-engine-downsink/issues/04-connections-list-own-primitive.md`。

- **2026-09-24 v15**：**`host-pty` 宿主广播声明 `hostBroadcastSessionId`（票 05，
  ABI 不变、字段级追加不 bump；移动端零改动）**。会话语义下沉 P3 形态 B 的引擎侧前置。
  ① **语义 = opt-in 只读订阅**：spawn config-json 可选字段 `hostBroadcastSessionId`（=
  本句柄服务的会话 id），给出即声明「宿主 server 可只读订阅本句柄输出」，宿主据此在
  **既有句柄注册表**（`host_api/pty.rs` 的 `PTYS`）内登记只读的「会话 id → pty 句柄」映射
  （**不新增第二份表**；登记随 spawn、摘除随终态，复用句柄生命周期单点）。未声明的句柄
  **任何宿主广播面都读不到**（反向锁是行为用例，见票 05 验收）。② **同属主「同 id 重建」
  （重启路径）取最新句柄**（登记序 `registered_seq` 最大）；他属主撞 id 在 spawn 显性拒绝
  （失败可见，不静默当作未声明；空串同样拒绝）。③ **多消费者并发拉取语义写进契约为 07
  的输入**：同一句柄的环可被属主插件（`ring-fetch`）与宿主广播面（直读同进程 `PtyRing`）
  同时拉取——游标各调用方自持、`fetch` 是纯读（不消费不推进全局状态）、淘汰由**产出量**
  全局驱动，任一消费者的读取都不释放空间，慢消费者落后即 `truncated`。宿主直读**不受**
  `PLUGIN_PTY_RING_FETCH_MAX_BYTES` 约束（那是 WASM 边界拷贝限额）。④ 消费方
  `com.bedcode.terminal-session` 与本票同批声明（`launch.rs::spawn_session` 每次 spawn
  带 `hostBroadcastSessionId = session_id`）——移动端输出面（票 06）据此直读恢复。
  ⑤ 本票同时完成「第 2 条措辞修订」的预留项（见上「会话真源下沉」节第 2 条）。实施与
  验收见 `.scratch/2026-09-23-session-engine-downsink/issues/05-host-pty-broadcast-declaration.md`。

- **2026-09-24 v16**：**会话原语域退役完成 + 内核会话目录删除（票 10 / 票 11，
  ABI 26 → **27**；移动端零改动）**。本专项（`.scratch/2026-09-23-session-engine-downsink/`）
  的收口裁决，也是本项目迄今**唯一一次破坏性契约变更**：

  ① **删 import 两个 interface**：`host-session`（12 函数全删，含 v14 的
  `connections-list` 旧别名）与 `host-terminal`（`send`——「宿主替插件往交互终端注入按键」的
  最后一处业务面入口，零消费者且实现 100% 依赖会话域）。② **删 export 两个**：
  `terminal-hooks` 整 interface 与 `events` 的 `on-session-lifecycle` / `on-input-submitted`
  （派发源已于票 03 消失，本批收口 WIT 面）。③ **权限位退役两位**：`session:write` /
  `terminal:observe`（词汇 34 → 32）；`session:read` 保留但判据面收缩为宿主终端窗口事实
  （初始网格 / 窗口开关 / 在场查询）。④ **宿主侧零会话对象**：内核会话目录
  `src-tauri/src/session/` 整目录删除（登记 / 状态机 / 属主表 / 注解槽 / 业务输出环 /
  配置管理器），`AppContext` 的会话字段、`PluginHost::new` / `WasmHostContext::new` 的
  会话形参一并删除；关停只走引擎 `kill_all_registered`、关窗守卫只走引擎 `live_count()`。
  ⑤ **共享订阅类型迁出而非删**：`SubscribeResponse` / `SubscriberHandle` / `SubscriberStats`
  迁 `server/websocket/terminal_ws/subscriber.rs`、`MODE_REALTIME` / `MODE_BATCH` 迁同目录
  `forward.rs`——它们的真实归属是订阅者，不是会话层。⑥ **旧产物 fail-visible**：v27 后旧产物
  在**实例化期**即失败，宿主在其报错后追加「缺失 interface 名 + 按当前 SDK 重建」的指引
  （`LoadedWasmPlugin::stale_artifact_rebuild_hint`），不是 trap 也不是静默降级——本条同时是
  §8「真源换了地方就要 fail-visible」判据的第 ② 形态的落地先例。⑦ 防回接：宿主源码扫描锁
  `retired_kernel_session_domain_is_not_reintroduced` 禁止把内核会话对象加回来。
  实施与验收见 `.scratch/2026-09-23-session-engine-downsink/issues/10-host-session-interface-abi-26.md`
  与 `.../11-kernel-session-dir-deletion.md`。

- **2026-09-24 v17**：**WS 动作词表声明式化（票 09a/09b/09c，ABI 不变；
  移动端零改动，wire 逐字不变）**。WS 会话控制动作的词表来源从「宿主硬编码 switch」
  （`services/session_control.rs::handle_control` 的 `match action { ... }` 逐臂调
  `session_gateway`）改为「插件 manifest 声明端点 + 插件侧分派」，见上
  「WS 动作词表声明式化」节。① **expand（09a）**：SDK `contributes.wsEndpoints`
  声明面 + 宿主激活期登记（端点表挂载 `/ws/plugin/<id>/<path>`，单段路径约束，
  与 host-websocket `register-endpoint` 同口径；deactivate 回收、激活期重登记）。
  ② **migrate（09b）**：`com.bedcode.terminal-session` 声明 `session-control` 端点
  （auth=jwt），插件新模块 `ws_control` 承接列表/创建/停止/移除/尺寸五域动作解释
  （互调 api `session-ws-control` + `events-ws.on-client-message` 直连帧协议），
  插件补 `ws:server` 权限位（端点的 events-ws 回包判据位，五同步点只有一个新位
  声明、无新词表条目）。③ **contract（09c）**：宿主 `services/session_control.rs`
  重写为**传输面转发层**——声明闸门（端点已声明且插件激活，否则显性报错）→ 原始
  动作 JSON 转发插件互调 api → 响应动作 JSON 套回 `Message::SessionControl` 信封
  （原 `message_id`；信封 `session_id` 取自响应动作的 `session_id` 字段 = 新建会话 id，
  与旧宿主路径逐字一致）。旧 `handle_control` 业务 switch 删除；`SessionControlAction`
  等 wire 形状类型保留为传输面契约（**宿主 WS 层不再内联任何业务动作名语义**，
  grep 断言）。终端输出订阅/输入等数据面（H1）不迁插件。移动端旧 `/ws/event` 协议
  wire 逐字不变（`pty_session_chain` 集成测试经转发层全绿）。实施与验收见
  `.scratch/2026-09-24-host-crypto-business-downsink/issues/09a/09b/09c`。

- **2026-09-24 v18**：**加密引擎化 + enums 三分类处置收口（票 01-09c，ABI 25 → 26
  只发生在 host-crypto 契约面；移动端零改动）**。见上「加密引擎化」节与「WS 动作词表声明式化」节。
  ① **crypto/ 引擎（票 01/02）**：算法注册表（名称→实现 + 白名单 + abstract trait），`link_crypto`
  不再内联具体算法（WS/HTTP 过滤层只依赖注册表抽象接口）。② **host-crypto 契约面（票 03/04）**：
  WIT interface + 三权限位 `crypto:aead` / `crypto:asym` / `crypto:kdf` + ABI 25 → **26**（desktop
  独有，双端偏离）；原语只给中性算法、不给编排，宿主密钥不经原语暴露（非旁路红线）。③ **协商套件
  参数化（票 05）**：suite 可选字段，缺省 → 默认套件，未知名 fail-visible 拒绝；
  移动端旧端零改动。④ **enums 三分类处置（票 06-09）**：special_key 下沉插件、shell.rs 整文件删除、
  SessionStatus/Type 归位 protocol/session.rs（线协议形状）、动作词表 switch 声明式化（v17 详情）；
  `enums/` 终态 = 引擎级类型 + 传输面契约形状，业务语义零残留。实施与验收见
  `.scratch/2026-09-24-host-crypto-business-downsink/issues/01..09c`。

- **2026-09-25 v19**：websocket 业务下沉（ABI 27 → 28，破坏性；移动端明确不在
  兼容范围、零改动）。宿主 WS 面收为**通用 transport**（无业务内核红线 §5 / 裁剪线 ADR 0022）：
  旧 `/ws/event` 与 `/ws/terminal/session/{id}` 路由删除（只剩 `/ws/plugin/<id>/<path>`，旧路径
  通用 404 无 fallback）；`Message` 业务枚举 / `services/` / `subscription.rs` / `terminal_ws/` /
  `channel/{event,terminal}.rs` / `session.rs` / `src/events/`（AppEvent+publish+matcher+
  `HostSyncEvent`+`sync_handler`）整删，`WsSession` 收缩为连接认证态；宿主不再 import
  `SessionControlAction` / `TerminalAction` / `SyncPayload` / `DeviceConnectionEvent` 等产品概念。
  ① **新增 `host-websocket.connection-context` 原语**（返回脱敏连接事实：clientId / endpointId /
  owner / addr / authenticated / connectedAt / authContext{subject,deviceName,fingerprint}；
  永不返回 token；仅端点属主可调、权限 `ws:server`）。② **破坏性退役**：`host-events.broadcast-sync`
  （插件事件改 `host-bus.publish` + `host-events.emit`，`broadcast` 权限位保留但 `broadcast.sync`
  子项退役）、`host-pty.spawn` 的 `hostBroadcastSessionId` 字段（PTY 引擎不再知道 session id，
  插件经 `ring-fetch` 自持游标）。③ **旧产物 fail-visible**：v28 后旧产物（import `broadcast-sync`
  的 v27 产物）在实例化期失败，`stale_artifact_rebuild_hint` 判据扩展点名 `broadcast-sync`。
  ④ **SDK wire 收敛**：`wire/{sync,control}.rs` 删除、`SyncEvent`/`SyncPayload` 退役；HTTP 历史
  改经插件互调 `session-history`（插件自持 pty_id 调 `ring-fetch`）。实施与验收见
  `.scratch/2026-09-25-websocket-business-downsink/`（票 02-09；性能门禁含 WS 输出路径
  `ws_output_perf.rs` 探针：常态 196 KiB 全量到达 0 截断 / 压力 10 MiB 节流产出边产边拉 ≥ 4 MiB）。

### v32：认证中心显式注册（2026-09-29，ADR 0031）

本 ADR 的「授权策略 = 安全闸门」节与「双端偏离」节被 **ADR 0031** 修订；插件分类体系
（三层：基础服务 / 内部统一业务 / 业务应用 + worker 预留）另见 **ADR 0032**。

- `auth-policy` 能力发现的**注册语义**取代本 ADR 记载的「能力探测 + 排序取首个」——
  后者在候选 > 1 时会选中未实现策略的插件（2026-09-29 移动端 4001 日志风暴事故根因）
- **fail-closed 取代 fail-open**：「无中心 → 放行」与「传输失败 → 放行」两条降级删除
- `host-auth` 追加 4 函数（`auth-center-register` / `auth-center-unregister` /
  `auth-methods-list` / `auth-method-invoke`），**桌面独有**，移动端不跟演（ABI 11 不变）
- 认证记录真源归属不变（仍在认证中心插件私有库，宿主主库 `pairings` /
  `connection_history` / `session_configs` 仍退役）

### v33：插件分类三层（2026-09-29，ADR 0032）

插件分类（manifest `type`）是**机制学**（谁先加载、谁提供什么、谁依赖谁），不新增业务语义，
不改变本 ADR 的裁剪线（B1–B6）与四类薄壳口径。

- 新增 **L2 内部统一业务应用**类别：宿主内核**主动调用**该 wasm 组件做安全闸门裁决——
  本仓唯一的反向依赖类别。三条红线（白名单式登记 / 只做安全闸门 / 宿主只转发不解释）
  落成防回接锁 `internal_business_host_dependency_stays_gated`
  （`wasm_core/manager/host/tests/l2_gating_test.rs`）
- 加载顺序 `L1 基础服务 → L2 内部统一业务 → L3 业务应用` 是**显式分批**（原「一批
  `System` 组件 + 持久化批量」），宿主只按谓词判角色、层序真源在 SDK 常量
- **`lifecycle: ephemeral`（业务 worker）只预留类型**：声明在构建链与宿主加载期双侧显性
  拒绝，直到一次性实例机制与调度框架落地（ADR 0032 §6 启用清单）
- **双端偏离**：分类**桌面独有**（移动端 SDK 无 `PluginKind`、无角色驱动分批；移动端是
  自持业务 App 的客户端），移动端何时跟进由其首个需要分层的场景决定

### v34：认证中心持有入场签发密钥与验签执行（2026-09-29，ADR 0033）

ADR 0033 把设备入场 JWT 的**生成 / 签发 / 验签**从宿主移入认证中心（L2
`com.bedcode.terminal-session`），宿主 `utils/auth/jwt.rs` 整个退役。本条不新增
interface、不改裁剪线，只改归属与契约面。

- **契约面**：`host-auth` **删除 2 函数** `device-token-issue` / `device-token-verify`
  （ABI desktop 32 → **33**，破坏性）。旧产物（v32 SDK 构建）仍 import 这两个函数 →
  **实例化期**即被拒，`stale_artifact_rebuild_hint` 点名「按 v33 SDK 重建」
- **双端偏离**：本组 2 函数**桌面独有**（`host-auth` 自 v15 起即桌面独有；移动端 WIT/SDK
  本就不含它们，已 grep 核实），移动端不跟演不投影，**mobile ABI 保持 11**
- **归属依据**（§5.1.2 三问）：入场凭证是产品概念（设备名 / 指纹 / 撤销 / 7 天窗口），
  宿主没有第二个消费者 → 归插件。宿主侧只剩 ADR 0022 §5.1.3 允许的两类薄壳：**安全闸门**
  （`enforce_connection_policy` 只问中心一次 + `deny_kind` 三态分类，中心不可达即拒）
  与**零解析窄转发**（`auth-method-invoke` / 会话互调）
- **ADR 0032 的 L2 锁修订**：`l2_gate_returns_decision_only` 要求裁决门
  `-> Result<(), String>`。D1 之后宿主必须拿到认证身份才能填连接会话与转发给插件的
  `caller` 上下文——那不是新增产品事实，而是同一数据换了来源。故该锁**收紧**：成功态改
  为字段集**钉死**的 `AuthenticatedIdentity`（恰好 3 字段），仍禁止配对记录 / 信任列表 /
  设备档案等载荷，仍禁止 `&mut` 出参与额外上下文参数
- 论证、性能实测、迁移与发布原子性、fail-visible 三形态见 `docs/adr/0033-*.md`

### v35：生物凭证面下沉（2026-09-30，B-downsink，ADR 0033 修订）

生物凭证（P-256 公钥）的**托管与验签执行**从宿主移入认证中心（L2
`com.bedcode.terminal-session`）私有库 `auth_biometric_keys`，WASM 内 p256 验签。
与 v34 区块（ADR 0033 入场 JWT）同路线：认证中心自持凭证材料并自验，宿主不再持有
任何设备侧凭证材料。

- **契约面**（ABI desktop 33 → **34**，破坏性）：`host-auth` **删除 3 函数**
  `biometric-credential-bound` / `biometric-verify-signature` /
  `biometric-credential-bind`。旧产物（v33 SDK 构建）仍 import 这三函数 → **实例化期**
  即被拒，`stale_artifact_rebuild_hint` 点名「按 v34 SDK 重建」
- **宿主退役面**：`utils/auth/biometric.rs`（挑战管理器 + P-256 验签）整模块删除、
  `system/app_context.rs` 的 `biometric_challenges` 字段删除、`plugin_secrets` 的
  `biometric:*` 死行由 `db::run_migrations` 幂等清扫（v33 jwt.key 同款）
- **插件接管面**：`auth_records` 新增 `auth_biometric_keys` 表（`biometric_key_get/set/delete`
  端口）、`auth_http/biometric.rs` 挑战闸门改查私有库 + WASM 内 p256 验签
  （`p256` crate 已探针验证 wasm32-wasip3 可编译）、`auth_http/mod.rs` 的 bind 写私有库
- **双端偏离**：生物面**桌面独有**（移动端 WIT/SDK 本就不含 `biometric-*`，走 HTTP
  `/api/auth/biometric-*`，grep 已核实），移动端不跟演不投影，**mobile ABI 保持 11**；
  wire 流程与挑战-应答逐字节不变，移动端零改动
- **存量影响**：宿主旧 `biometric:<fp>` 行被清 → 已绑定生物认证的设备需**重新绑定**
  （配对记录 `auth_pairings` 在插件私有库，不受影响；配对码 / QR / JWT 认证不受影响）
- 依据（§5.1.2 三问）：生物凭证是产品概念（设备绑定事实），且 host-crypto 原语面
  只有 aead/kdf/x25519 无 ECDSA 验签——认证全归中心的纯粹性优先于「中性验签原语留
  宿主」的替代方案（用户裁定选 B）
