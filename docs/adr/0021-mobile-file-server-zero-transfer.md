# 移动端文件服务归零：全发起侧传输架构

文件传输的传输层改为「**全手机发起**」模型：移动端不再运行任何 HTTP 服务端（删除独立 actix-web server），大文件数据流全部由手机侧作为 HTTP client 主动发起，**桌面端为唯一的 HTTP 服务端**；WS 控制面承担意图通知 / 进度 / 取消 / 状态同步等全部协调职责。业务上的上传 / 下载语义保留不变，运输层统一收敛为两种原生动作——「手机从桌面拉（`GET + Range`）」与「手机推给桌面（`POST/PUT + session`）」。

这推翻了此前两处定案：① ADR 0016 与 `.scratch/lan-file-transfer-plugin/issues/01-传输协议选型.md` 隐含的「移动端做服务端、桌面端直连移动端 HTTP 端点」（01 明确列出『移动端做服务端 → 常规 Rust HTTP server』为备选）；② 当前已实现的「移动端独立 actix server（随机端口 + Bearer token）+ 桌面端 reqwest 直连」双栈形态。协议仍为 ADR 0016 与 01 选定的 **HTTP/1.1 + Range + upload session**，本 ADR 只改变服务端归属与发起权：桌面端不直连手机，手机端无监听、零端口、零 HTTP server 依赖。

## 动机

- **服务器归零**：手机端删除 actix-web 整条依赖链（HTTP 解析 / 路由 / 按核数起的 worker 线程池 / BearerTokenGuard / 挂载生命周期），APK 体积与运行时线程占用下降，手机端只剩 tungstenite client + reqwest client。
- **传输引擎单侧集中**：两端四个业务方向的传输能力全部落在桌面端唯一的 HTTP server（已有 `file_service_controller.rs`：NamedFile 原生 Range/206、`UploadSessionManager` received-offset 偏移校验与 409、HEAD size+mtime 指纹比对），断点真源、并发、审批模型不再双写维护。
- **语义单一化**：续传点一律以接收端「已落盘字节」为准，且接收端恒为桌面端；下载续传 = Range 206，上传续传 = session 已收偏移，两端不再需要镜像实现。

## 四方向映射（核心）

「桌面端发起」不再意味着桌面端直连手机拉 / 推，而是经 WS 指令手机用**对应语意的动作**自行执行；「发起权」从桌面端翻转给手机，业务语义原样保留：

| 业务语义 | WS 协调内容 | 手机执行的 HTTP 动作 |
|---|---|---|
| 手机下载桌面文件 | （手机自主发起，无需协调） | 手机 `GET /api/file?path=<挂载相对路径>` + Range → 206 → 落 SAF/下载目录 |
| 手机传文件给桌面 | （手机自主发起，无需协调） | 手机 `POST /api/upload` → `PUT /api/upload/{sid}` 从已收偏移 append |
| 桌面下载手机文件 | 桌面发 `intent{pull, file: 手机侧路径, batch}` | 手机 `POST /api/upload` 把本地/SAF 文件流给桌面（业务语义仍是「下载」，但数据流与上传方向共用同一引擎） |
| 桌面推文件给手机 | 桌面发 `intent{push, file: 桌面挂载内相对路径, dest: 下载目录}` | 手机 `GET /api/file?path=<挂载相对路径>` + Range → 落 SAF（业务语义仍是「上传」，数据流与下载方向共用同一引擎） |

关键事实：**WS 连接是手机主动连桌面的出站连接，桌面端往已建立的连接里发消息即可，不需要手机任何监听**。意图消息携带的路径语义在发起方侧解析（桌面路径 = 桌面挂载内相对路径，由桌面端控制器映射；手机路径 = SAF/本地路径，由手机 responder 解析）。

## 生命周期闭环（通知 → 执行 → 进度 → 取消）

1. **意图通知**：桌面经 WS 发 `intent`（带 `expect_response`），手机 ACK 回执会话 ID 与起始 offset。
2. **执行**：手机拉起对应 HTTP 动作，大文件数据流走 HTTP（断点续传语义不变：Range 206 / UploadSessionManager received-offset）。
3. **进度**：手机每推进一段回推 `progress` 事件（WS），桌面端任务面板照常显示——进度上报本就是 WS 的职责，与现有 `TransferApproval` / `device-connected` 同一模式。
4. **取消/失败**：桌面发 WS `cancel` → 手机中止会话；手机中途失败主动发 `transfer_event{failed, batch, offset}`；重试 = 桌面重发 intent，手机从已收偏移续传——发送端无需记忆状态，接收端即断点真源。

## 断点续传保证

- **下载方向**：标准 HTTP Range（206 Partial Content），接收端本地记录已写字节，中断后以新 Range 重拉；续传有效性先用 HEAD size+mtime 指纹比对（源文件变更则放弃旧游标重传）。
- **上传方向**：`POST /upload` 创建会话 → `PUT /upload/{sid}?offset=` 从服务端已收偏移 append（offset 不符 409）→ `GET /upload/{sid}` 查询断点 → `complete` 原子改名。receiving 端恒为桌面端，断点真源在桌面端单侧集中维护。
- 桌面端 `file_service_controller.rs` 已实现全部所需端点，**无需新增端点**；手机端仅需补齐「本地已写字节游标 + 重查询」客户端逻辑。

## 并发

- 多文件并发 = 手机侧多个并行 HTTP 会话：上传 sessions 为独立 HashMap、下载 Range 无状态，并发天然成立，无需额外设计。
- 沿用 01 选型：并发默认 **3**（可配置，上限初拟 8）；单条 TCP 流即可打满 WiFi（01 已论证），多文件并发只需多路并行，不做文件内分片。
- 手机端 SAF 落盘注意锁粒度：每个 transfer 独立读/写句柄，避免共享一把文件锁把并发退化为串行。

## 可靠性要求

- **intent 必须 expect_response ACK**：没有回执就不确定动作是否启动；手机 WS 断线时任务暂停，重连后由状态同步触发 offset 查询续传（与既有断线重连逻辑一致）。
- **传输中的 HTTP 流不依赖 WS 存活**：WS 抖动只影响进度/取消控制面，不影响已在进行的数据流本身上下文。
- 手机 App 常驻 WS 客户端为通知即时性的前提（现状已满足）；切后台/锁屏的系统级限制沿用 01 策略——任务转「暂停-待续传」。

## Consequences

- **移动端移除**：`actix-web` 依赖、`BearerTokenGuard`、挂载生命周期（`ensure_started`/`stop`）、`file_service/server.rs` 全套 HTTP 服务实现；SAF 与 MediaStore 落盘逻辑（complete 落公共下载、SAF 中转、上传会话）从 server.rs 迁移为**载体无关的会话引擎**，挂到新增的 WS 传输 responder 下。
- **移动端新增**：`intent` 消息处理 + WS 传输 responder（pull：读取本地/SAF 文件流给桌面；push：接收桌面文件落盘），按 `transfer_id`/batch 分发并发任务。
- **桌面端**：唯一 HTTP server 增加 `intent` 处理（可复用现有 file_service WS 分发路径）；文件服务端点原样服务手机（已挂在 /api 下 + JWT 鉴权，无需改动）。
- **迁移顺序**：先抽载体无关会话引擎 → 建 intent/responder → 切流量 → 最后删 server.rs。
- **不受影响**：传输批 / 审批 / 钩子模型（ADR 0016）与协议选型（01）不变；两端需同步升级，无跨版本兼容负担。
- **已知限制**：若未来出现 AP 客户端隔离（同网设备间互不可达）场景，本架构整体失效，需回退全 WS 数据面承载（记录为已知边界，非当前目标）。

## 评审

可行性、性能与落地范围评审见 `.scratch/lan-file-transfer-plugin/issues/13-服务器归零方案评审.md`（结论：速度等价、功能可全覆盖，按明确定范围的迁移立项，核心新增量是手机端双向 HTTP client 传输栈与审批触发点重构）。