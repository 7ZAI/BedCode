# 文件传输服务器归零迁移（v2.1 全手机发起传输）规格

Type: spec
Status: ready-for-agent
Related: `docs/adr/0021-mobile-file-server-zero-transfer.md`、`.scratch/lan-file-transfer-plugin/issues/13-服务器归零方案评审.md`、`.scratch/lan-file-transfer-plugin/v2.1-zero-transfer-implementation-plan.md`（施工图纸，wire 逐字定义与其为准）
Blocked by: 无（v2 批量批准/历史/队列已实现：v2-implementation-plan.md 两端单测全绿）

## Problem Statement

当前文件传输的拓扑是「两端各跑一个 HTTP server」：手机端运行一套独立的 actix-web 服务（随机端口 + Bearer Token + 挂载生命周期），桌面端用 reqwest 直连手机端点；桌面端自身另有一份文件服务（挂 /api + JWT）。由此产生四个用户可见/可运维的问题：

1. **手机端背负一套完整 HTTP server 栈**（actix 依赖、按核数起的 worker 线程、BearerTokenGuard、挂载启停生命周期）——APK 体积与运行时线程占用无谓地大。
2. **传输引擎两端双写维护**——list/download/upload 的断点续传、同名校验、指纹比对在手机 server 与桌面 controller 各实现一份，语义镜像、容易漂移，缺陷修复要改两处。
3. **断点真源不单一**——两端各自维护「已落盘字节」与 session 状态，断点语义接收端/发送端两套，重连恢复的决策各自为政。
4. **可靠性依赖面分散**——手机端做 server 时，桌面直连手机端口，手机切后台/Doze 时连接被系统掐断的窗口由两端任务各自兜底，传输体验分裂。

用户希望：传输功能与速度完全不缩水，但手机端更轻、断点续传语义单一、传输引擎只在桌面端维护一份。

## Solution

**服务器归零**：手机端不再运行、不再监听任何 HTTP 端口。所有大文件数据流由手机侧作为 **HTTP client 主动发起**，桌面端是**唯一的 HTTP 服务端**；WS 控制面承担意图通知 / 进度 / 取消 / 状态同步的全部协调职责。业务上的上传/下载语义原样保留，仅改变「谁是搬运字节流的一方」。

- 手机「从桌面拉」= 手机 `GET /api/file` + Range（206）→ 落 SAF/下载目录，本地游标记已写字节。
- 手机「推给桌面」= 手机 `POST /api/upload` 建会话 → `PUT` 从已收偏移 append → `complete` 原子改名。
- 桌面「发起」不再直连手机，而是经 WS 发 intent 指令，手机用**对应语意的动作**自行执行。
- 传输层加密维持现状（MVP 明文直通 `PassthroughCipher`，密钥协商未实现）——加密是未来缝，与方向无关，不构成迁移障碍。

用户感知的变化：手机端去掉一整条 server 依赖链（体积/线程下降）；断点续传更可靠（真源单一）；桌面端一处集中维护传输引擎；速度与功能不变（评审 13：速度等价、功能可全覆盖）。

## User Stories

1. 作为手机用户，我想浏览并下载桌面共享目录中的文件，以便把办公文件拷到手机上——下载**不需要任何审批**（我即操作者与落盘者），直接执行、进度实时。
2. 作为手机用户，我想把手机里的文件（SAF 可见范围）发给桌面，以便备份到电脑——桌面端按「接收策略」决定是否询问我（的桌面端用户）：每次询问时桌面弹出批卡，**接受后才开始传输**；直接拒绝时我这边第一时间看到拒绝原因。
3. 作为手机用户，我想中断一个进行中的上传/下载再继续，以便网络波动时不必重来——**断点真源 = 落盘方已写字节**：下载续传重发 Range、上传续传查桌面 session 偏移。
4. 作为手机用户，我想在批量发送（如 50 首歌）时对端只问我一次，以便不被逐文件询问打断——批量请求（批 ID + 清单 + 总大小）一次应答，批准后批内免问直传；等待答复期间我这边任务显示「等待对方同意」，可取消。
5. 作为手机用户，我想在锁屏/后台收到桌面推送文件的请求，以便不错过对方发来的文件——通知带「接受/拒绝」action，点击后经系统通知通道路由回应用内命令；传输开始时我确认保存位置**即视为批准**（上传方向的落盘方审批）。
6. 作为手机用户，我想在桌面拉取我的文件时知道此事，以便掌握我的数据被谁拿走——收到**信息性通知**（无按钮、不阻塞、不作为审批门）：拉取是下载方向，免审批，但知情权保留。
7. 作为手机用户，我想一次同时传输多个文件，以便提高效率——并发默认 3（可配置 1–8），每个任务独立文件句柄，互不串行（不共享一把锁）。
8. 作为手机用户，我想走开一会儿、手机被系统挂起，传输不会把文件搞坏——沿用「暂停-待续传」策略，回到前台/重连后从断点继续，不重复写字节。
9. 作为桌面用户，我想把文件推送到手机上，以便随时把电脑文件带去手机——我在桌面点「发送到手机…」，经 WS 发 intent，手机按接收策略确认后执行下载动作并落盘；桌面面板实时看到进度。
10. 作为桌面用户，我想下载手机里的文件，以便把手机照片备份到电脑——我在桌面浏览手机共享目录并选择，经 intent 让手机把文件推送给我（数据流与上传同一引擎），**免审批**（发起人即落盘者），批上下文由桌面内部自批准，不弹冗余确认。
11. 作为桌面用户，我想中止某次传输，以便发现选错文件时及时止损——经 WS 发 cancel，手机中止对应 HTTP 会话；已写字节保留，重试从断点续传。
12. 作为桌面用户，我想在手机退后台/离线时知道传输停了，以便决定是否等待——任务卡显示「对端离线，任务挂起」；心跳超时（30s 无回传）判定失联，重连后自动恢复进度。
13. 作为两端用户，我想升级后传输互通无差别，以便各自用最新版本——两端同步升级，无跨版本兼容负担。
14. 作为手机用户，我想手机 App 更轻、更快启动，以便日常使用更顺滑——手机端整条 actix-web 依赖链被移除，APK 体积与运行时线程占用下降。
15. 作为两端用户，我想传输速度与之前一样快，以便大文件（几十 GB 级）迁移不焦虑——单条 TCP 流即可打满 WiFi（01 论证），数据路径与现实现完全同构，WS 不插入字节流。
16. 作为手机用户，我想在某台桌面拉取文件时无法拒绝的前提下仍可事后查看记录，以便审计我的数据流——桌面拉取动作进入手机端传输历史（只读归档，封顶 200 条）。

## Implementation Decisions

### 架构

- **桌面为唯一 HTTP 服务端，端点原样复用**：现有 `/api/file`（NamedFile Range/206 + HEAD 指纹）、`/api/upload` 会话三件套（POST 建会话 / PUT 偏移 append / GET 查偏移 / complete 原子改名）、`transfer-request` 批端点全部不动，**零新增端点**（评审 13 确认）。
- **桌面端点发现零新增消息**：手机 WS 客户端本就持有桌面地址与已认证 JWT（认证已 HTTP 化），HTTP base 与 Authorization 直接从现有连接态派生，不引入「桌面公告」消息、不新增端口。
- **手机端新增 client 传输栈**（wasi 沙箱隔离之外、宿主侧原生实现）：三个模块——下载（Range→SAF 落盘）、上传（session 编排）、游标（本地已写字节，纯函数）。手机端「发起」与「响应 intent」共用同一套 client 栈，不复制两份引擎。
- **断点真源语义单一化**：续传点一律取「接收端（落盘方）已写字节」。下载方向 = 手机本地游标（push 场景）/ Range 起始（自主下载）；上传方向 = 桌面 session 已收偏移（手机自主上传、pull 场景）。两端各维护一份游标实现仅因落盘方物理位置不同，**语义同构**。

### 线协议（wire，施工图纸 §3.1 为准，两端逐字一致）

- 新增 `FileTransferIntent`（桌面→手机，Sync 通道）：字段 `intentId / direction("pull"|"push") / semantics("download"|"upload") / batchId? / relativePath / size / deviceName / expectResponse`。
- 新增 `IntentAck`（手机→桌面，FileService 通道）：字段 `intentId / decision("accepted"|"rejected") / offset / sessionId?`。
- 新增 `FileTransferCancel`（桌面→手机）：字段 `intentId`。
- 进度/心跳：复用既有 progress 通道，载荷扩展 `intentId`（`#[serde(default)]` 兼容旧载荷）；新增 heartbeat 语义（10s 静默心跳，桌面 30s 无回传判失联）。
- 双向通道：桌面发 SyncPayload 变体、手机回 FileServicePayload 变体——遵循 v2 既有双向映射模式，方向语义清晰；任一变体改动须两端 `enums` 双写 + wire 单测同步。

### 审批规则（用户已拍板，四场景）

**只有「上传」方向需要落盘方审批；「下载」一律免审批。** 自洽性：下载场景发起人 = 落盘方本人（自批准无意义）；上传场景落盘方是对方用户，才需要授权。

| 场景 | 协议方向 | 落盘方 | 审批 |
|---|---|---|---|
| 手机自主下载桌面文件 | 下载 | 手机（本人） | 免审批，直接执行 |
| 手机自主上传给桌面 | 上传 | 桌面（对方） | 桌面批卡（v2 机制完整保留：ask→批卡→approve/reject→超时 TTL） |
| 桌面推文件给手机（push） | 上传 | 手机（对方） | 手机确认：ask→前台对话框/后台通知 action；**用户接受后才回 ACK 并执行数据流** |
| 桌面下载手机文件（pull） | 下载 | 桌面（本人） | 免审批；手机仅信息性通知（无按钮、不阻塞） |

- **pull 的批上下文自批准**：pull 数据流 = 手机 POST 桌面 upload 引擎，会撞桌面 ask 策略的「无已批准批 ID 的 session 创建 403」。桌面发 intent 前在桌面宿主内部自建 approved 批（审批人即桌面用户本人，零交互），batchId 随 intent 下发，手机 POST 时携带。
- **push 的防绕过门**：ask 策略下手机在用户确认前不得执行下载动作（responder 状态机插入 Approved 门）；push 数据流（手机 GET）无 session 创建，不需要批上下文。
- 手机端 v2 批状态机随 server 删除自然移除；桌面端批 gating 保留（接收端恒为桌面），语义从「防手机用户绕过」变为「桌面自批准 + 对端监督」。

### 迁移方式

按四阶段，每步独立可验证：① 手机 client 传输栈 + 断点游标（手机自主发起两方向）→ ② intent 线协议与手机 responder → ③ 桌面发起方向切 intent 驱动 + 审批落地 + 桌面协调者化（乐观更新/心跳/超时）→ ④ 删除手机 server、actix 依赖、BearerTokenGuard，收口公告逻辑。①完成后手机端既有 server 仍运行（双轨观察期），④是唯一行为断裂点。

### 可靠性

- intent 必须 `expect_response` ACK（无回执不启动动作）；手机 WS 断线时任务暂停，重连后经状态同步查 offset 续传。
- 传输中的 HTTP 流不依赖 WS 存活：WS 抖动只影响进度/取消控制面。
- 手机退后台/Doze 沿用「暂停-待续传」；前台服务常驻 + 心跳保活。
- 响应器按 intentId 分发并发任务（并发 3 可配置）；每个 transfer 独立 SAF 句柄，避免共享文件锁退化为串行。

### 删除/保留清单

- 删除：手机 actix-web 依赖、FileServiceServer/ensure_started/stop、BearerTokenGuard、Announce（端口/token 公告）语义、手机接收上传会话（引擎迁为 client 上传编排）。
- 保留：SAF 中转与落盘逻辑（迁移到 client 下载模块）、桌面 controller 全套、通知栏通道（扩展 intent 应答 action）、传输历史、队列四 tab、Kotlin 通知基础设施。

## Testing Decisions

**什么是好的测试**：只测外部行为——HTTP 契约（请求/响应形状与状态码）、wire JSON 逐字形状、状态机合法迁移——不测内部实现细节（不 mock 内部函数、不断言私有结构）。任何「两端 wire 不一致」必须在单测层被抓住，而不是等真机联调。

**Seam（唯一）**：桌面 FileService HTTP 端点契约。两端各自对该契约独立验证：

- **手机侧**：测试内起一个轻量契约 mock server（仅实现 206 Range + session 三件套 + 批 gating 语义），client 栈对 mock 跑全链路：下载 Range/206、游标恢复、HEAD 指纹不符放弃旧游标、上传会话创建→append→complete、偏移重查、409、404 重建、批 gating 403。**不依赖真实桌面进程**。
- **桌面侧**：对 file_service_controller 的契约测试（现有基础），补 batch gating 下「pull 自批准批」路径与 transfer-request 三路分流。
- **wire**：新增 4 个变体两端 serde 往返 + action 字符串断言（先例：现有 `test_announce_wire_format` 模式）。
- **responder**：intent 分发（并发 3）、ACK 回执、push+ask 的 Approved 门（无确认不执行）、cancel 中止、fail 偏移上报（纯状态机部分做纯函数测试）。

**先例**：mobile `tests/file_server_http.rs`（HTTP 集成测试）、`tests/ws_protocol_integration.rs`、两端 enums 内嵌 wire 测试、v2 批状态机纯函数测试（`validate_batch_transition` 模式）。

**验收分层**：单测/契约测试在两端 CI（cargo test + npm test:run + gradlew Kotlin 编译）全绿；真机双端联调按 §8.3 清单（吞吐基准 ≥ 80%×min(T,D)、30/60/90% 中断续传哈希一致、审批四场景、删除后全量回归）作为最终验收，不属于单元测试接缝。

## Out of Scope

- **AP 客户端隔离场景**（同网设备互不可达）：ADR 0021 已知边界，本架构整体失效，需回退全 WS 数据面——记录为已知限制，非当前目标。
- **文件内分片并发**：01 基准不达标时的升级路径，MVP 不做。
- **MediaStore 直接写入**：SAF 中转落盘不变（评审 8 的 M3 直传不在本方案范围）。
- **传输层加密**：MVP 明文直通保留；未来 AES-GCM 接入时两端同源、方向无关，不构成迁移障碍。
- **真机弱网/多设备联调**：spec §10 与验收清单 12 的独立事项，本 spec 只规定单测接缝与验收入口。
- **限速/定时/历史长期留存**：沿用 v2 决策（不做）。
- **移动端插件化改造**（WASM 传输引擎下沉为插件能力）：服务器归零只做宿主层，插件 SDK host fn 暴露必须的下载/上传/应答接口，引擎本身留在宿主。

## Further Notes

- 审批规则（上传需落盘方审批、下载免审批）已由用户明确定案，实现前无需再讨论；四场景表是本 spec 的权威语义。
- 本 spec 是 v2 的**增量迁移**：spec.md（v1/v2 全量规格）与 v2-implementation-plan.md（批量批准施工图纸）仍然有效，凡本 spec 未提及的 v2 行为按其执行。
- 施工详图（wire 逐字 JSON、文件级改动清单、SDK 四处同步、测试矩阵、双 subagent 任务划分）见 v2.1-zero-transfer-implementation-plan.md——本 spec 面向验收与任务拆分，施工图面向执行，两者须保持同步。
- 已知实现风险：progress 载荷扩展需 `#[serde(default)]` 兼容旧版；Kotlin 通知 action 与 v2 批应答 action 并存（PendingIntent extra 需区分 kind）；阶段 ④ 删除 actix 后现有 `file_server_http` 集成测试需同步改写为 client 栈契约测试。
- 观察期：阶段 ① 双轨并存期间两端二进制互操作按旧路径，④ 之后才要求同步升级。