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

### 会话语义下沉批次（v18 + v19，2026-09-20 桌面端）

阶段 2 与阶段 3 的一部分在本仓库**首次合并执行**（原计划各自成阶段、各开一次 ABI 窗），
落地为单一内置插件 `com.bedcode.terminal-session`（终端会话中心：设备与配对 + 会话编排 + Agent
任务域；2026-09-22 票 06 起新 id，旧 id `com.bedcode.session` 的 HTTP 前缀与互调 api 名留双投窗口，
见下方 v9 条目），宿主侧只追加**既有 interface 的函数**，未新开任何 interface。
spec：`.scratch/2026-09-19-terminal-session-plugin/spec.md`（D2–D7），实施票 01–18。

| 面 | 追加 | 批次 | 权限位 |
| --- | --- | --- | --- |
| `host-auth` 记录面 | `trusted-devices-list` / `trusted-device-revoke` / `connection-history-list` / `auth-setting-set` | v18 | `auth` |
| `host-session` 配置面 | `config-upsert` / `config-get` / `config-delete` | v19 | `session:config`（新增位）
|
| → 配置面写原语 `config-upsert` / `config-delete` **已随 v23 退役**（真源在插件私有库，宿主写原语无调用者即死接口）；读取面 `config-list` / `config-get` 保留为一次性 legacy 迁移通道（迁移窗口结束随主库表退役），权限收编 `session:read` |
| `host-session` 创建与动作面 | `create-with-spec` / `remove` / `rename` / `resize`（原表的 `restart` 已于 v21 退役，见下「v21 收敛退役」） | v19（函数级追加不 bump） | `session:write` |
| `host-session` 事实面 | `annotate`（注解槽）/ ~~`connections-list`~~（**v14 起迁 `host-connection`**，此处只留同判据别名，随票 10 删） | v19 | `session:write` / `connection:read`（原 `session:read`） |
| `host-connection`（v14 新增 interface） | `connections-list`（宿主 server 在册连接原始记录） | v14（函数级搬迁，不 bump） | `connection:read`（新增位） |
| `host-platform` | `wsl-distros` | v19 | `platform` 现状 |
| `auth-policy` 导出 | `verify-device-token`（宿主中间件验签后取策略） | v17 | 能力导出，非宿主原语 |
| 前端贡献面 | `ui.registerSettingsSection`（设置分组扩展点） | 无 WIT（纯前端） | `ui:settings`（新增位） |

裁决要点：

1. **「映射决策归插件、执行留内核」的切口是 `create-with-spec`**：插件算好
   `{command, args, cwd, cols, rows, env, name}` 交给宿主，宿主只做 shell 包装 / WSL
   转换 / 尺寸缺省 / ID 预生成。会话配置真源同时从主库表迁入**插件私有库**
   （`host-plugin-database`），主库旧表保留一版只读退役，走一次性幂等迁移。
2. **注解槽取代内核任务字段（D5）**：内核只按 `session-id → map<string,string>` 搬运
   与透传，**绝不解释键名**；线协议里 `taskStatus` 等字段形状不变，值由插件经 `annotate`
   写入后由内核透传 → 移动端零改动。这是「内核去业务化」与「线协议不破」的唯一共存形态。
3. **`resize` 裁决分家**：谁是当前渲染端（正统端）的**事实登记**在内核，**裁决规则**
   （谁覆盖谁、何时提示）在插件。与 host-pty 第 2 条的「两张注册表」同一划界思路。
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
2. **`host-pty` 第 2 条的划界现状更正（重要，非措辞修订）**：该条写的「两张注册表、两套生命周期」
   在业务会话侧已经**合并为一张**——业务会话就是一个 `host-pty` 句柄，内核 `SessionComponents`
   与 `GlobalOutputManager` 对插件会话不再有内容（这正是移动端 M6/M7 受损的根因，P3 形态 B
   改由宿主 server 直读 `PtyRing` 恢复）。该条里"`host-session` 服务宿主业务会话线"的三方划界
   随之失效：`host-session` 已进入退役通道。注意**这不等于**该条第 2 点预留的措辞修订
   （「不注册业务输出总线 → 不默认注册；按 spawn 声明 opt-in 只读订阅」）——那句要等
   `host-pty` 的宿主广播声明（P3 子票）真的落地才改，本批不预支。
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

- 移动端是**远程终端控制端**，不承载 PTY / mDNS 广播 / WS 服务端等主机侧引擎，故 `host-websocket`（desktop v14）、`host-auth`（v15 密钥托管；v18 追加认证记录面四函数）、`host-pty`（v16）、`auth-policy` 导出（v17，认证能力——宿主 server 中间件验签后取策略）、**会话语义下沉批次（v18 / v19：`host-session` 配置面 + 创建与动作面 + 注解槽 + 连接清单、`host-platform.wsl-distros`）** 均为**桌面独有接口**：mobile 的 WIT / ABI / SDK 不跟演也不投影（ADR 0018 双端各自演进的文档化偏离，同 wasmtime 桌面 48 / 移动 47 分叉先例）。当前 **desktop v25 / mobile 11**（v23 = host-session 配置面写原语退役，见 v10 登记；v24 认证记录下沉、v25 host-peer 节点生命周期原语；desktop 独有接口持续演进不要求移动端跟演）。
- **偏离不止 WIT 面**：本批次同时经用户 2026-09-19 授权**豁免 AGENTS.md §9「协议改动必须两端同步部署」**，豁免范围严格限于该 spec（`.scratch/2026-09-19-terminal-session-plugin/spec.md` D1）。自守边界：线协议**形状**（会话 DTO 字段、同步事件、WS 控制帧、认证握手报文）保持不变——保持它并不需要移动端改一行代码，且它是后置适配专项的成本基线。移动端受损面 M1–M5 已挂进路线图（`.scratch/2026-09-10-plugin-kernel-roadmap/spec.md`），桌面端不为其负责（spec Out of Scope）。
- **恢复条件**：当移动端需要同类能力（例如本地跑交互进程）时，再在该端 WIT 增补对应 interface 并对齐 ABI 计数；在此之前「改 WIT 必须双端同步」这一硬约束的适用范围限于**双端共有的接口**（host-peer / host-fs / host-http 等）。
- SDK 双端独立包（`plugin-sdk-desktop` / `plugin-sdk-mobile`），互不影响；宿主侧 `version > 当前 → 拒绝` 的兼容语义保证旧插件（≤v16）零迁移仍可加载。

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

## 抽象提取候选（登记，不在本期实施）
- **`PtyRing` ↔ 业务会话输出环（`session_output.rs::UnifiedOutputQueue`）的代码级合并**：二者形态同源（单生产者 + 全局偏移 + 游标拉取 + 字节/条目双上限），但生命周期不同（业务环随会话、插件环随 pty 句柄），且 2026-09-17 刚重构完的业务链路不背回归风险 → 本期刻意自持实现（约 130 行）。第二处同类形态出现时（或业务环新增维度需要插件侧同步时）再抽取。
- **PTY 引擎与业务会话线的进一步解耦边界**：票 01 已把「输出汇可注入」「终态门与退出码」下沉到 `PtySession`，票 02 追加「命令来源可注入」（`PtyCommandSource::Business | Raw`），`PtySlaveFdPolicy`（业务 `Hold` / 插件 `ReleaseOnSpawn`）仍是会话构造器的分支。若第三条消费线（如 AI 工具执行器）出现，应把「策略三元组（sink / command source / slave policy）+ 尺寸与 env」收敛为一个显式的会话装配参数结构，替代构造器家族。
  **（2026-09-23 后续）**：本候选的前两件已提前落定——`PtyCommandSource` 与 `pty_handler` 已退役、`PtySlaveFdPolicy` 票 3 已统一为 spawn 后释放 slave，`PtySession` 现只收调用方算好的 `CommandBuilder` + sink（见修订记录 v11）；「第三条消费线出现时再收敛策略三元组」的触发条件已不成立。
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
- **2026-09-21 v21**：**首次删除接口函数**——`host-session.create` / `restart` 退役（ABI desktop v20 → v21），宿主侧创建与重启执行器、命名/配置映射服务、`SessionStorage` 与主库配置投影写一并删除，内核不再读会话配置表。裁剪线依据：创建/重启的**映射决策**（命名唯一化 / config→launch / 何时启动 / 先 remove 再重建）全部是产品语义，宿主只保留 `create-with-spec` 执行端（shell 包装 / 发行版转换 / 尺寸缺省 / id 仲裁）与 `remove` 注册表清理。详见上方「v21 收敛退役」节与 `CHANGELOG.md`。
- **2026-09-21 v23**：**性能红线修订（roadmap 阶段 3 前置验证通过）**——「逐帧输出不进
  WASM」放宽为「输出字节禁止经 JSON 命令通道搬运，经 WIT 二进制原语（`list<u8>` 直传）
  可进 WASM」。依据：`.scratch/2026-09-21-terminal-output-consumer-perf/`（只读探针
  P1/P2/P2b/P3，release：真实插件路径 ≈40 µs/op / ~2.6 ms/MB，1 MB/s 风暴单核 0.26%；
  JSON 反例路径 ~75 ms/MB）。host-pty 纯拉取形态不变；`host-session-output` 若开设必须
  二进制直传。详见「终端输出消费插件化 · 性能红线修订」节。
- **2026-09-21 v22**：`host-platform.reveal-in-dir` 原语化（ABI desktop v21 → v22；desktop 独有，双端偏离同 `host-platform`）：把「在系统文件管理器中定位并选中文件/目录」从「宿主 Tauri 命令 `plugin_reveal_in_dir` + `system:open` 权限 + 前端 `context.system.revealInDir` 桥」改为内核原语。裁剪线依据：定位是**平台交互动作、不读取任何数据**（路径本就由调用方提供），与 `pick-files` / `pick-folder` 同口径——因此**不叠加权限门**（宿主 `host-platform` 域保持「无权限门」的一致性）。`system:open` 权限随宿主命令面与前端插件 API 一并退役，五同步点全落：SDK 常量与 API 映射 / 前端合法集合 / 宿主命令面（`require_system_open` 门） / 唯一消费方 file-transfer 的 manifest 与调用点（改经自身命令 `file-transfer.reveal-in-dir` 走 SDK 原语） / 打包侧校验。实现本体（Windows Shell COM / macOS `open -R` / Linux `xdg-open`）归引擎模块 `system/opener.rs`，宿主 `open_log_dir` 与插件原语共用一份。实施记录见 `.scratch/2026-09-21-host-rust-residue/issues/04`。
- **2026-09-15 v4**：host-mdns 升级为 mDNS 基础能力服务（见「host-mdns v2」节）：新增 advertise / stop-advertise / is-advertising 三原语（config-json 纯引擎参数）、浏览事件定向投递 `mdns:found.<owner>` / `mdns:lost.<owner>`（payload 增 serviceType/browserId）、单守护收敛（全局唯一 ServiceDaemon，peer-net 与插件共享）、双表属主仲裁与按属主回收、宿主身份广播登记（owner=host，零业务代码红线 D3）、Android 多播锁随单守护常驻获取；全局发现桥接与缓存重发通道退役（D1），file-transfer 双端一期迁移（D2）。ABI desktop 12→13 / mobile 10→11（ADR 0019 双端同版）。实施验收后落 ADR（D5 定案）。
- **2026-09-18 v5**：新增 host-websocket（见「新增 host-websocket」节）：客户端域（connect / send-text / send-binary / close / is-connected）+ 服务端域（register-endpoint / 收发 / 广播 / 踢出 / 注销 / 清单）共 14 函数 + 可选导出 `events-ws`（宿主动态探测，未导出则帧丢弃 + 首次 warn + 计数）；状态事件改 **owner 作用域 topic**（`ws:<event>.<owner>`，标识在 payload，D3）；插件端点挂载 `/ws/plugin/<plugin-id>/<path>`（命名空间由宿主注入，D5）；权限按域拆 `ws:client` / `ws:server`（D6）；插件端点帧过流量过滤链但不参与链路加密（`TrafficChannel::WsPlugin`，D9）；本期仅 `ws://`（D7）。ABI desktop 13→**14**（mobile 11 不变，ADR 0019 双端各自演进）。
- **2026-09-19 v6**：新增 host-pty（见「新增 host-pty」节）：6 函数（spawn / write / resize / kill / ring-fetch / is-running）+ 唯一生命周期事件 `pty:exit.<owner>`；输出面定为**纯拉取**（否决 push 回调），限额四项按「创建类失败可见 / 数据面读侧截断」分级，环容量改为 spawn 的插件声明参数（宿主上下限仲裁）。同时首次把**桌面独有接口的双端偏离**成文（「双端偏离」节：host-websocket v14 / host-auth v15 / host-pty v16，mobile 不跟演 + 恢复条件），并登记两条抽象提取候选（「抽象提取候选」节）。ABI desktop 15→**16**（v15 由认证中心线 `host-auth` 占用；mobile 不跟演）。实施与验收见 `.scratch/2026-09-19-pty-base-service/`（票 01-07）。
- **2026-09-19 v7**：`host-auth` 追加**认证记录面**四函数（`trusted-devices-list` / `trusted-device-revoke` / `connection-history-list` / `auth-setting-set`）：读**内核原始记录**（`pairings` 全表含软删行、`connection_history`、`settings` 白名单键），排序 / `is-active` 过滤 / 展示组织与派生视图一律归插件（裁剪线：宿主不解释「什么算已连接设备」）；`pairings` 的凭据列（session token / public key）不出内核；属主说明——记录是宿主全局数据、无句柄表，故无属主段校验，权限门 `auth` 即授权边界。ABI desktop 17→**18**（mobile 不跟演，同「双端偏离」节）。实施与验收见 `.scratch/2026-09-19-terminal-session-plugin/`（票 05）。
- **2026-09-20 v8（当前）**：会话语义下沉批次落成（见「会话语义下沉批次」节）：ABI desktop 18→**19**（`host-session` 配置面 + `create-with-spec` + 动作四项 + `annotate` / `connections-list` + `host-platform.wsl-distros`，同批次函数级追加不再 bump），新增两个权限位 `session:config` / `ui:settings`，设置分组扩展点与「内置入口按贡献插件运行态让位」两条内核 UI 改动落地，`com.bedcode.devices` 与 `com.bedcode.auto-task` 两个桌面插件退役并合并进 `com.bedcode.session`（旧 HTTP 前缀由宿主别名表兜底、切断时机并入移动端专项）。移动端零改动，其受损清单与 §9 同步豁免一并记入「双端偏离」节与路线图。实施与验收见 `.scratch/2026-09-19-terminal-session-plugin/`（票 01–18）。
- **2026-09-22 v9（当前）**：**插件 id 变更登记 + 终端窗口域下沉收口 + 输出原语落地**。① **id 变更**：
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
- **2026-09-22 v10（当前）**：**host-session 配置面写原语退役（ABI desktop 22→23）**：
  删 `config-upsert` / `config-delete`（业务配置真源自票 08 起在插件私有库，宿主写原语无调用者——
  死接口删除，行为零变化）；读取面 `config-list` / `config-get` 保留为**一次性 legacy 迁移通道**
  （`terminal-session` 激活时读主库 `session_configs` 迁入私有库，marker 幂等；/api/sessions/start 已
  走插件编排不再读主库——票 09 起的退役绑定条件已满足）；权限位 `session:config` 同步退役
  （config-get 改挂 `session:read`，五同步点全落，`gen:permissions` 重出）；`SessionConfigManager`
  收缩为只读迁移通道（写路径 + Config 事件发布删除，引擎层 SQL 写接口保留为基础服务）；
  主库 `session_configs` 表保留为迁移源，观测信号（启动 `legacy_rows` 计数）归零后作 contract 删除
  （迁移窗口结束再删读面与表）。桌面独有接口，移动端零改动。实施见
  `.scratch/2026-09-22-pty-business-downsink/spec.md`（阶段 1）。
- **2026-09-23 v11（当前）**：**PTY 引擎去业务化收口（ABI 不变，desktop 仍 v25；移动端零改动）**。
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
- **2026-09-23 v12（当前）**：**`host-app.plugin-resource-dir` 原语（ABI 不变，desktop 仍 v25；
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

- **2026-09-24 v14（当前）**：**`host-connection` 独立原语落成（票 04，ABI 不变；移动端零改动）**。
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
