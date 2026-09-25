# 传输编排整体下沉（Peer Transfer Orchestration Downsink）

Status: **done**（2026-09-25 用户裁决开放点 1–6 全部定案，见 §6；**票 1–4 全部落地**，见 §5）
Date: 2026-09-25
范围: **桌面端为主**（`bedcode-desktop/src-tauri/src/server/peer_net/peer_engine_{transfer,receive,remote}.rs`、
`server/peer_net.rs`、`wasm_core/host_api/peer.rs`、SDK `bedcode-desktop/packages/plugin-sdk-desktop/`（WIT + abi.rs）、
`wasm-apps/file-transfer/`）；移动端 `plugins/file-transfer/` + `packages/plugin-sdk-mobile/` 同构存在——
双端策略见开放点 5（会话下沉先例 = 允许损坏 + 受损清单记账）。
决策依据: 用户 2026-09-25 审查指令（peer_net 业务耦合审查）+ 审查结论（编排面可下沉 / 数据面不可下沉 /
「仅经宿主 HTTP 通用函数」不成立）+ 用户选择立项目录 2；AGENTS §5（无业务内核）、ADR 0022（裁剪线）、
`.scratch/2026-09-23-session-engine-downsink/spec.md`（方法论先例：真源迁移 + fail-visible + 防回接锁）。

---

## 0. 审查结论（2026-09-25，作为验收基线）

1. `packages/peer-net` crate = 纯引擎（mDNS `_bedcode-peer._tcp` + TLS 1.3 直连 + 自定义线协议），
   与 HTTP/WS 终端链路完全独立互不感知——**宿主 HTTP 通用函数路径不适用**（无 mTLS 对等握手 /
   mDNS 发现 / 断点续传落盘能力，硬走 HTTP = 重写传输底座）；正确对应物 = WIT `host-peer` 原语。
2. `server/peer_net.rs` 主文件与 `wasm_core/host_api/peer.rs` 基本干净（属主记账 / 信任面 /
   consent 桥 / 权限门+句柄表+转发），保留。
3. **业务残留集中在 `peer_engine_*` 三文件**（§1 清单）——本专项标的。
4. 数据面（TLS/磁盘/事件通道/oneshot 回执/句柄表）**不可下沉**（WASM 无 socket/FS/长连接，
   ADR 0022「离宿主无法实现」）；编排面可下沉。

## 1. Problem Statement：现状与终态的差额

### 1.1 现状（2026-09-25 实测，行号为当日基准）

**宿主残留业务清单（`peer_engine_*`）**：

| 残留 | 位置 | 性质 |
| --- | --- | --- |
| 活跃任务状态机（send 侧） | `peer_engine_transfer.rs:109-121` `PeerTransferState.tasks` + `pick_pending_to_start:627-644` | 产品语义（迁移规则/队列调度） |
| 并发闸门 | 1..=8 默认 3（`DEFAULT_CONCURRENCY`/`validate_concurrency`，receive.rs:73-94） | 产品调度策略 |
| 历史封顶 | `HISTORY_CAP=200`（transfer.rs:37）/ `RECEIVE_TERMINAL_CAP=100`（receive.rs:37） | 产品规则 |
| 快照 payload = 业务任务视图 | `publish("peer-transfer-changed")` transfer.rs:1139-1142；`publish("peer-receive-changed")` receive.rs:546-556 | 宿主状态机产物，经 `bus_topic_for` 推 `peer:transfer`/`peer:receive` |
| serve 双端记账 | `drive_serve_events`/`register_serve_task`（transfer.rs:962-1126） | 「两端各自展示同一次传输」产品需求 |
| 发送源收集 + 同名去重 | `collect_outgoing_files`/`unique_remote_path`（transfer.rs:1157-1300） | 产品交互语义 |
| 取消原因码映射 | `"cancelled-by-sender"`/`"cancelled-by-receiver"` 等 kebab-case（transfer.rs:916-928、receive.rs:424-436） | 前端 i18n 约定 |
| peer_name 展示名解析 | `resolve_peer_name`（receive.rs:244-253） | UI 展示语义 |
| pull 编排（并发信号量 + 任务行预登记） | `run_pull_queue`（remote.rs:360-454）+ `register_remote_pull`（receive.rs:257-291） | 产品编排 |

**口径漂移**：`peer_engine_transfer.rs:1-20` 模块头自述「只做 WIT send-files/pause/resume/close 原语的
引擎接入」，实际持有上述全部业务态——与「宿主回查内核拿会话」漂移同型（session-engine-downsink 先例）。

**插件侧已自持（`wasm-apps/file-transfer/rust/src/`）**：

- `transfer_store.rs`：快照驱动的自有存储——按 batchId merge 进店 + **200 封顶** + 重启 interrupted
  标注 + `retryMeta` 回放（send 记源路径 / pull 记共享根+文件清单）；
- `settings_store.rs`：接收策略真源 + auto 分支自动应答；
- `roots_registry.rs`：共享根 CRUD 真源（`set-shared-roots` 全量推镜像）；
- `device_bridge.rs` + 前端 `deviceState.ts`：设备缓存自持（found/lost/TTL/cap）。

**核心矛盾（双真源重叠）**：插件 `transfer_store` 头注释自述「以引擎 `peer:transfer`/`peer:receive`
全量快照事件为**进度真源**」——即**进度真源实际在宿主状态机**，插件只是投影 + 历史持久化。
宿主与插件各持一份 200 封顶、各持一份终态判定（`is_terminal` 双实现）。这违反 ADR 0022 终态
（业务真源在插件）。

### 1.2 终态（微内核划界）

宿主 peer 域只剩四样**无业务语义**的资产：

1. `packages/peer-net` 引擎 crate（不动）；
2. `PeerNetState` 节点生命周期 + 属主记账 + 信任面（`peer_net.rs`，不动）；
3. **会话句柄表**：`batch_id → { CancelToken, PauseSlot, epoch, sources }`（现混在 tasks 里的纯引擎控制面）；
4. `host_api/peer.rs` 原语投影（权限门不变）。

`peer_engine_*` 三文件收敛为**引擎事件桥 + 句柄表管理**：不再有 tasks 状态表、并发闸门、封顶常量、
peer_name 解析、原因码映射、serve 记账、源收集。编排全归插件。

**防回接锁**：`retired_peer_transfer_orchestration_is_not_reintroduced`（结构锁：peer_engine_* 不出现
任务状态表 + 调用点接线 + 复用 helper，参照 `retired_kernel_session_domain_is_not_reintroduced`）。

## 2. 引擎事件流现状（改造对象）

```
SharedDirHandler（crate）
  ├─ transfer_tx ──→ drive_receive_events（receive.rs:197-236）
  │                    OfferPending → pending 表（oneshot）+ 任务行 pending
  │                    Progress → update_progress + throttle_publish(150ms)
  │                    Terminal → settle_terminal（状态映射 + 封顶 + publish）
  │                    Paused/Resumed → set_receive_pause_status
  ├─ serve_tx ─────→ drive_serve_events（transfer.rs:968-1006）→ send 记账任务
  └─ send_batch 会话通道 → drive_send_session（transfer.rs:734-825）
                       Progress/Terminal/Paused/Resumed → tasks 状态机 + publish
总线：publish_bus_only → peer:transfer / peer:receive（全量快照）→ 插件 transfer_store merge
```

插件消费侧（`lib.rs` activate 时订阅 `peer:transfer`/`peer:receive`/`peer:consent`/`peer:connection`/`peer:devices`）。

## 3. P0 裁决定案（2026-09-25 用户裁决）

> **裁决结果**：事件回流 = 方案 A；send-files 收窄；collect-outgoing 原语；active-transfers 原语；
> 150ms 节流留宿主；**移动端不变，先做好桌面端再说**（= 开放点 5 选「允许损坏 + 受损清单记账」，
> session-engine-downsink 先例）。其余按 spec 倾向执行。

### 方案 A：引擎原始事件直推总线（已裁决采纳）

宿主把 `TransferEvent` 逐条 JSON 化直推总线（新 topic 或重构现有 topic），插件自建状态机做**事件归约**。

- 宿主桥接层只做：事件序列化 + **进度节流（150ms 窗口保留在宿主——纯性能无业务）** +
  OfferPending 的 oneshot 回执登记（pending 表本质是回执通道，非业务态，保留）；
- 插件：事件归约建活跃任务视图 + 自持封顶/原因码/peer_name（从自持设备缓存解析）+ serve 记账
  （收 PullServed 自建 send 视图）+ 并发闸门（调用 send-files 前自控节奏）；
- 首屏一致性：插件激活晚于事件时需要「当前活跃会话清单」——见开放点 4。

### 方案 B：快照语义退化（不推荐）

宿主状态机只追踪「引擎会话级事实」、不持封顶/方向/原因码——但 `PeerTransferDto` 字段
（direction/peer_name/reject_reason）本身就是业务形状，「去业务化」治标不治本，双真源依旧。

## 4. 阻塞链勘察（2026-09-25 实测）

1. **WIT `host-peer` 现状 = 18 原语**（`bedcode.wit:97-161`）：dial-peer / close / respond-consent /
   list-trusted / revoke-trusted / send-files / respond-transfer / set-receive-policy / pause-transfer /
   resume-transfer / resume-all-transfers / set-shared-roots / list-shared-roots / browse-directory /
   pull-files / set-download-dir / start-node / stop-node。pause/resume/close 已按 batch_id 寻址
   （句柄范式），形态不变。
2. **ABI = v29**（`abi.rs:167`；v28 = websocket 下沉、v29 = host-http 服务端路由域）。
   **开工前必须实读 abi.rs**——本 spec 立项时 MEMORY 曾记 v23，实读修正为 v29。
3. **send-files 语义收窄（开放点 2）**：现状「一次调用 = 入队 + 宿主闸门调度（可能排队 pending）」；
   下沉后闸门归插件 → send-files 应收窄为「一次调用 = 一个会话立即发起」——函数签名不变但行为
   变化，属破坏性，需 ABI bump + 旧产物 fail-visible（参照 v28 `stale_artifact_rebuild_hint` 先例：
   实例化期点名「按哪个版本重建」）。
4. **resume 的 redial 分支依赖 sources**：`resume_via_redial` 现从 tasks 取源清单（transfer.rs:391-414）；
   下沉后 sources 必须随句柄表保留（会话重启必需，非业务）。
5. **OfferPending oneshot 回执**：不可序列化，宿主桥接层必须保留 pending 回执表 + respond-transfer
   原语回执（现状已是，形态不变）。
6. **PullServed → send 记账**：serve 记账任务行（direction=send、sources 空）纯宿主产物；下沉后插件
   收 PullServed 自建视图，宿主 serve 通道只透传事件。
7. **双端**：移动端 `plugins/file-transfer/` 存在且同构；移动 SDK `plugin-sdk-mobile/rust/wit/bedcode.wit`
   独立（13 KB，移动不跟演桌面 ABI 序列，memory：mobile 11）。桌面 bump v30 时移动端对齐策略见开放点 5。
8. **前端联动**：`PeerTransferDto` wire 形状变化（peer_name / reason 码消失或语义变化）会波及插件前端
   组件与 `plugin-contract.test.ts` 四处 pin（plugin.json / 插件 Rust 契约用例 / 插件前端契约测试 /
   宿主 session_e2e 同类 pin）。

## 5. 分票与进度（按「先增量后破坏」原则实际执行）

- **票 1 ✅（2026-09-25 落地）**：WIT **v30 纯增量**（host-peer 追加 `active-transfers` 会话表投影查询 +
  `collect-outgoing` 发送源枚举）+ 宿主**引擎原始事件桥**（`peer:transfer-event` / `peer:receive-event`
  双写直推；Progress 复用快照 150ms 节流窗口；terminal_state_payload 直译不做原因码映射）+
  SDK `HostPeer` trait/WasmHost 扩展 + 4 处 native mock 同步（E0046 连带）。
  验证：宿主 lib check ✅；宿主 peer 域单测 56+1 全绿；SDK `test_abi_version_is_v30` ✅；
  file-transfer 插件 wasm 编译 ✅（RUSTUP_TOOLCHAIN=nightly-2026-09-16）；terminal-session 插件 wasm ✅；
  两插件 native 测试（59 / 330）全绿；fmt 自查（新引入 diff 已修，`host_api/peer.rs` 文件级存量签名
  风格漂移不动，留专项 fmt 票）。
- **票 2 ✅（2026-09-25 落地）**：插件侧事件归约状态机——`transfer_store.rs` 新增 `reduce_event`
  （offer-pending/pull-served 建行锚点 + progress/terminal/paused/resumed 推进，方向按 topic 定向；
  原因码映射单点收编插件）+ `reconcile_diff`（归约态 vs 快照对账，偏差 warn 留痕）+
  `insert_active_projections`（首屏兜底）+ `lib.rs` 订阅 `peer:transfer-event` / `peer:receive-event`
  （事件归约主写、快照 merge 退化为校正 + 对账）+ **插件侧并发闸门自控**（`PENDING_SENDS` 队列 +
  `dispatch_pending_sends`，running ≥ concurrency 时排队）+ peer_name 自持（device_snapshot 缓存解析，
  短指纹兜底）+ activate 首屏 `rebuild_from_active_transfers`。宿主配套：引擎事件载荷补 `tsMs`
  （wasm32 无时钟）。**验收达成**：事件归约测试矩阵 9 项（建行幂等/进度补正/原因码按方向映射/
  paused 保留/full_completed 例外/兜底占位/对账偏差）+ 归约视图 67/0 全绿。
- **票 3 ✅（2026-09-25 落地，破坏性）**：WIT **v31**——① `resume-all-transfers` **退役删除**
  （「全部恢复」编排归插件遍历自身暂停批逐个调 resume-transfer，插件 `file-transfer.resume-all`
  命令保留实现改自编排）；② `send-files` 语义收窄「即发即会话」+ 载荷 `concurrency` 脉冲字段
  **运行期显性报错**（fail-visible 双保险之行为级）；③ 旧快照 topic（peer:transfer/peer:receive）
  映射摘除退役；④ `active-transfers` 实现改**三处句柄面投影**（send 会话句柄表 + receive pending
  询问表 + pull 会话表）；⑤ `pull-files` 删并发信号量与任务行预登记 → 逐文件即发 + `pull-started`
  引擎事实事件；⑥ 宿主残面删除：`peer_engine_transfer.rs` 整体重写（任务状态机/封顶/serve 记账/
  peer_name/原因码/并发闸门全删 → `SendSessionHandle` 句柄表 + 引擎事件桥；源收集剥离
  `source_collect.rs`）、`peer_engine_receive.rs` 重写（任务表/终态结算/并发设置全删 → 回执表 +
  纯直推；`PeerTransferSettings` 删 concurrency）、`peer_engine_remote.rs` 收窄；
  ⑦ `stale_artifact_rebuild_hint` 增 v31 判据（实例化期点名重建，fail-visible 三形态②）；
  ⑧ 防回接锁 `retired_peer_transfer_orchestration_is_not_reintroduced`（15 符号源码扫描，
  `HISTORY_CAP:` 带冒号精确匹配避让 `METRICS_HISTORY_CAPACITY`）+ **变异自检通过**
  （注入 `register_remote_pull` → 转红 → 还原回绿）；⑨ 两插件 wasm 产物重建
  （file-transfer + terminal-session，wasmHash 重注入）。
  验证：宿主 lib 887/0 + 集成 10/0 全绿；file-transfer 67/0、terminal-session 330/0、SDK 146/0；
  前端 vitest 814/0；eslint 0 error（120 warning 不计门禁）。
  **连带修复两处前序在途漂移**：`permission.rs` 词汇锁目录 `plugins` → `wasm-apps`
  （rename 迁移遗留）、`session_e2e` terminal-session manifest 断言补 `network:http`
  （v29 路由注册所需）。
- **票 4 ✅（2026-09-25 落地）**：文档——code-map 对等网络节 / ADR 0022 增补（v30/v31 记账）/
  CHANGELOG / `peer_engine_transfer.rs` 模块头口径修正（随票 3 重写完成）/
  **移动端受损清单**（`mobile-impact.md`，开放点 5 裁决：移动端 SDK 11 不跟演，
  移动 file-transfer 在桌面 v31 宿主下的行为差异逐条列出）。

## 6. 开放点（2026-09-25 用户裁决，全部定案）

1. **事件回流形态**：A（原始事件直推，推荐）/ B（快照语义退化）？——**裁决：A**。
2. **send-files 语义**：收窄为「即发即会话」（插件自控并发，推荐）vs 保留宿主排队闸门？——**裁决：收窄**。
3. **发送源收集**：新增 host-peer `collect-outgoing` 枚举原语（目录递归 + 大小，纯文件系统事实）vs
   插件经 host-fs 自行递归（大目录逐条原语调用开销大，不推荐）？——**裁决：新增原语**。
4. **首屏一致性**：新原语 `active-transfers: func() -> result<string, string>`（宿主句柄表投影，
   仅 batch_id/字节/终态事实，无业务字段）vs 周期重发？——**裁决：新增原语**。
5. **移动端策略**：跟随 v30 同步改造 vs 允许损坏 + 受损清单如实记账（session-engine-downsink 先例）？
   ——**裁决：移动端不变，先做好桌面端再说**（允许损坏 + 受损清单记账，票 4 落清单）。
6. **进度节流位置**：宿主桥接层保留 150ms 窗口（推荐，纯性能）vs 插件自节流？——**裁决：留宿主**。

## 7. 验收基线

- `cargo test`（宿主 lib + 集成）/ `pnpm run test:run`（桌面）/ 插件 251 项级全绿；
- 防回接锁 `retired_peer_transfer_orchestration_is_not_reintroduced` 存在且变异自检通过；
- 宿主 `peer_engine_*` grep 无 `HISTORY_CAP` / `RECEIVE_TERMINAL_CAP` / `pick_pending_to_start` /
  `collect_outgoing_files` / `peer_name` 解析 / 原因码字面量；
- 插件 `transfer_store` 成为唯一任务真源（封顶/终态/重试回放单点）；
- 旧 ABI 产物实例化期收到点名重建错误（fail-visible 三形态②）。
