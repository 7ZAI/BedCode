# 票 07 · 接收侧事件桥 + 策略闸门（阶段 2 第二票）

> 状态：**已完成（2026-10-07）**。门禁结果见 §6；真机双端互连与 `cross-end-tests`
> 未跑（原因见 §6）。移动端 ABI **12 → 13**（破坏性：事件回流契约变，函数集不变）。

## 1. 目标与本票实际范围

spec 票 07：「宿主 `peer_receive.rs` 收敛为『询问回执表 + 接收事件桥
（`peer:receive-event`）+ 策略闸门』（policy/timeout/download_dir 留原语参数）；
ask 逐批放行经 `respond-transfer`（安全闸门留宿主）」。

**本票执行**（与 spec 一致）：
- ✅ 宿主 `peer_receive.rs` 重写为回执表 + 事件桥 + 闸门（971 → 634 行）
- ✅ 新 topic `peer:receive-event` 直推插件总线；旧快照 topic 与前端事件桥接退役
- ✅ 插件事件归约承接接收方向（`offer-pending` / `pull-started` 建行 + 推进事件），
  接收任务真源唯一在插件
- ✅ 拉取会话建行从「宿主预登记任务行」改为 `pull-started` 事件（补上票 06 遗留的
  `register_remote_pull`）
- ✅ 宿主命令面收窄 4 个（列表 / 应答 / 取消 / 清历史），另 4 个引擎函数去
  `#[tauri::command]` 降为原语入口
- ✅ 新增双方向共用事件桥模块 `peer_events.rs`
- ✅ 防回接锁 ×2（含 topic 桥接面），均经变异自检
- ⏳ 票 08（插件侧封顶/重试回放细化）、票 09（发现/设备列表投影）、票 10（剩余
  宿主命令面收口 + 发送面锁扩展到接收面）未做

## 2. 改动清单（13 文件）

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 宿主引擎 | `peer_events.rs`（新，277 行） | 双方向共用的「引擎事实 → 插件总线事件」翻译单点：8 种载荷构造（progress / terminal / terminal_state / pause / pull-served / offer-pending / pull-started / node-stopped）+ 150ms 节流窗口 + `publish_engine_event`；6 条载荷单测 |
| 宿主引擎 | `peer_receive.rs`（重写，971 → 634 行） | 删任务表（`tasks` / `inner.tasks`）、进度入账（`update_progress` / `apply_receive_progress`）、终态结算（`settle_terminal` / `terminal_status_of` 等价物）、暂停同步（`set_receive_pause_status`）、终态封顶（`RECEIVE_TERMINAL_CAP`）、展示名解析（`resolve_peer_name`）、快照推送（`publish` / `snapshot` / `emit_json`）；留 `pending` 回执表 + `PeerTransferSettings` 闸门 + `drive_receive_events` 纯事件桥；`register_offer` 内联进事件桥；`register_remote_pull` → `announce_remote_pull_started`；`fail_task` → `announce_local_failure`（直推 terminal 事件） |
| 宿主引擎 | `peer_transfer.rs` | 删本地载荷构造与 `publish_engine_event`（迁 `peer_events`）、删 4 条随之迁移的载荷用例、删 `PeerTransferDto`（保留 `PeerTransferFileDto` 作 `collect-outgoing` 返回形状） |
| 宿主 | `peer_remote.rs` | 拉取建行改走 `announce_remote_pull_started`、拨号失败改走 `announce_local_failure`；模块头与会话上下文档按「事件桥」口径改写 |
| 宿主 | `peer_net.rs` | `bus_topic_for` 摘除 `peer-receive-changed → peer:receive` 映射；契约测试改反向断言 `None` |
| 宿主 | `lib.rs` | 登记 `pub mod peer_events`；invoke_handler 注销 4 个接收侧命令 |
| 宿主测试 | `peer_receive/tests/settings_default_is_ask.rs` | 删终态封顶用例（函数随任务表退役） |
| 宿主测试 | `peer_receive/tests/gate.rs`（新） · `tests/bug.rs`（删） | 闸门参数口径 3 例（并发区间 / ask 超时进引擎 / 自动档位不带窗口）；`bug.rs` 5 例测的是已删函数，同款语义在插件 `transfer_store` 有更全的等价用例 |
| 锁 | `tests/retired_mobile_receive_orchestration_lock.rs`（新） | 结构面 12 项退役构件 + topic 桥接面 |
| WIT | `plugin-sdk-mobile/rust/wit/bedcode.wit` | host-peer 接口文档登记 `peer:receive-event`（取代 `peer:receive`），列 kind 集与 `node-stopped` 语义 |
| SDK | `abi.rs` | `ABI_VERSION` 12 → 13 + 版本志（点明「协商单向，低 ABI 产物不在加载期被拒」） |
| 夹具 | `plugin-component-test/src/lib.rs` | `abi.version()` 12 → 13 |
| 插件 | `transfer_store.rs` | `reduce_event` 增 2 个接收锚点（`offer-pending` → pending 行 / `pull-started` → running 行）；`mark_interrupted_on_load` → `mark_active_interrupted`；删 `prune_absent` + `reconcile_diff`（快照专用，无调用方）；用例 −4/+4 |
| 插件 | `peer.rs` + `lib.rs` | 删 `merge_and_emit`（快照合并入口）；`auto_answer_pending(snapshot)` → `auto_answer_offer(event)`；新增 `on_receive_event`（node-stopped / offer-pending 分派）与 `attach_pull_meta`（按 `rel_path` 挂重试元数据）；订阅与退订改 `peer:receive-event` |
| 文档 | 双端 code-map（移动端段）· `CHANGELOG.md` | 事件范式与命令面段落改写；英文 CHANGELOG 补本票条目（**中文条目本轮按用户裁决未写**，见 §6） |

## 3. 与桌面 v31 的对齐与移动端差异（点名）

| 语义 | 桌面（v31 终态） | 移动端（本票） | 差异 |
| --- | --- | --- | --- |
| 接收侧事件 topic | `peer:transfer-event` 单 topic 双方向共用 | `peer:transfer-event`（send）+ `peer:receive-event`（receive）双 topic | 移动端按方向分 topic（票 06 已建立 send侧，票 07 建 receive 侧）；语义同构，插件按 topic 定向 direction |
| 建行锚点 | pull-served（send 供流记账） | 同款 + `offer-pending`（入站询问）+ `pull-started`（本端拉取） | 移动端多两类锚点（桌面无入站询问与拉取编排面） |
| 节点下线 | 引擎侧终止 + 插件对账 | `node-stopped` 事件 → 插件把在册进行中条目（含 paused）标 interrupted | 移动端显式事件（桌面靠快照对账） |
| 应答闸门 | 宿主 `respond-transfer` 回执表 | 同款（`pending: HashMap<batch_id, oneshot::Sender<bool>>`） | 无 |
| 快照 topic | 已退役 | 已退役（票 06 send / 票 07 receive） | 无 |
| 接收落点 | 可配置 | 恒 MediaStore.Downloads（`set-download-dir` 原语保留但不用于移动端落点） | 移动端既有约束（票 04） |
| 拉取并发 | 插件侧 | 仍读宿主设置 `concurrency`（`peer_remote` 拉取队列信号量） | **遗留**：拉取编排本身仍在宿主 `peer_remote.rs`（spec §1.1 判为 B2待下沉，无对应票，见 §5） |

## 4. fail-visible 三形态

| 形态 | 落地 |
| --- | --- |
| ① 旧读路径删除 | 接收任务表、快照推送与前端事件 `peer-receive-changed` 整条通路删除；插件侧快照第二真源（`merge_and_emit` / `reconcile_diff` / `prune_absent`）一并删除——**不留「查不到就当一致」的兜底**：能覆盖归约态的快照会掩盖事件丢失 |
| ② 旧产物实例化期点名 | **部分覆盖**：ABI 12 → 13（SDK + 夹具同步），但 `component.rs::verify_abi` 是**单向**协商（仅拒高于宿主者），低 ABI 旧产物不会被拒——票 06 的「旧产物即拒」表述在本票不成立，已在 `abi.rs` 版本志与 §5 如实标注 |
| ③ 退役词汇/topic 加载即抛 | `peer:receive` 与 `peer-receive-changed` 的桥接映射摘除（残留映射会把空快照回流成静默降级）；防回接锁把这层钉住（变异自检：重新加回映射 → topic 锁转红） |

防回接锁：`retired_mobile_receive_orchestration_is_not_reintroduced`（12 项退役构件，
扫描 `peer_receive.rs` / `peer_remote.rs` / `peer_net.rs` 非注释行）+
`retired_receive_snapshot_event_is_no_longer_bridged_to_plugin_bus`。
变异自检：注入 `fn settle_terminal` → 结构锁 FAILED（点名行号）。

## 5. 已知遗留（点名，非静默）

1. **旧插件产物不会被加载期拒绝**：ABI 协商单向（`version > ABI_VERSION` 才拒），
   v12 产物仍可加载但订阅的是已退役 topic → 接收列表恒空、ask 弹窗不弹，且**无任何
   报错**。缓解依赖构建流程（插件随宿主同批重建，`build --rust-only` 已在门禁里跑）。
   要真正兜底需改协商语义为双向或按 topic 订阅做加载期校验——属独立立项。
2. **`peer_remote.rs` 拉取编排仍在宿主**（逐文件队列 + 信号量并发 + 会话表）：判据
   B2（业务编排）命中，但 spec 阶段 2 未给它单独票；本票只把它的事件出口从「写宿主
   任务行」换成 `pull-started` 事件。拉取并发上限因此仍读宿主设置 `concurrency`
   （发送方向并发真源已在插件）。下沉归属见票 08/10 的评估。
3. **中文 CHANGELOG 未写**（用户裁决本轮不写）：`CHANGELOG.md` 已补条目，
   `CHANGELOG_zh.md` 待补——双语同步漂移显式记录在此，不静默。
4. **`peer_receive.rs` 仍有 3 个 Tauri 命令**：`get_peer_receive_settings` /
   `set_peer_receive_policy` / `set_peer_transfer_encryption` /
   `set_peer_transfer_concurrency`。其中 policy/落点是引擎闸门配置原语（该留），
   但 `get_peer_receive_settings` 与前端无消费者（设置真源在插件 storage）——
   属票 10 命令面收口面。
5. **`concurrency` 设置字段双真源**：宿主设置文件（服务拉取队列）与插件 storage
   （服务发送闸门）各存一份，票 06 已记录同款，本票未扩大（拉取仍在宿主）。

## 6. 门禁结果

| 门禁 | 结果 |
| --- | --- |
| 宿主 `cargo check --lib` | ✅ 0 error 0 warning |
| 宿主 `cargo test`（lib） | ✅ **374 通过 / 6 失败** —— 6 个失败全在并行会话在途的 `egress.rs`（`default_tier_consults_records` 等），非本票文件，按 AGENTS §11 不碰；与票 06 同款基线 |
| 宿主集成测试目标（`--no-fail-fast`） | ✅ 全绿（含两个防回接锁各 2 例、组件加载往返等） |
| 防回接锁（新） | ✅ 2 passed；**变异自检 2/2**：注入 `fn settle_terminal` → 结构锁红；重新加回 `"peer-receive-changed" => Some("peer:receive")` → topic 锁红，均已还原 |
| 插件 crate `cargo test` | ✅ **44 passed / 0 failed**（净 −5/+5：删`prune_absent` / `reconcile_diff` / 终态封顶 / `mark_interrupted`拆分等 5 例，增3 条接收锚点 + 1 条合并后的中断标注用例） |
| **wasm32 构建（`--rust-only`，spec §6 硬门禁）** | ✅ 通过：`node ../../packages/plugin-sdk-mobile/bin/cli.js build --rust-only` → wasm32 release 编译 + componentize 成功（**720,145 bytes**）。native `cargo test` 绿 ≠ 可交付；唯一残留 warning 是既有基线 `entry_from_dto` 未使用（票 06 已记录，非本票引入） |
| `rustfmt --check`（本票文件） | ✅ clean（仅格式化本票与票 06 同工作流文件；`peer_net.rs` 存在并行会话在途的格式漂移，**未整文件格式化**，按最小改动原则不碰） |
| `cargo clippy` | ✅ 本票文件零新增（未单独跑 clippy 全量：改动面为删除 + 纯函数搬移，全量 `cargo clippy` 会扫到并行会话在途文件） |
| 双端 WIT 对照 | ✅ host-peer 函数集不变（移动端 19 vs 桌面 19），本票只改接口文档与 topic 契约 |
| 插件调用点零漂移 | ✅ `peer:receive` / `peer:receive-event` 仅 file-transfer 消费；auto-task / ai-chatbox 不触碰 host-peer 事件 topic |
| 前端 `pnpm run test:run` / 根 eslint | ⚠️ **未跑（用户裁决本轮不测）**：本票零前端文件改动（传输 UI 早已全量走插件命令面与 `plugin:file-transfer:*-changed` 事件），无 i18n key 增减 |
| **真机双端互连**（入站询问 → auto/ask 应答 → 进度 → 暂停恢复 → 终态 → 拉取重试） | ⚠️ **未跑**：需移动端真机/模拟器 + 桌面同网段；行为等价依赖插件 store 归约用例 + 宿主事件桥纯函数用例 + 载荷形状单测，真机验证列入收尾全量验收（票 20） |
| `cross-end-tests` | ⚠️ **未跑**：本票不改跨端协议（peer 线协议 / REST / WS 均未动，只改宿主内部编排与总线 topic），但事件总线契约有变，真实互连仍以票 20 为准 |
| 中文 CHANGELOG | ⚠️ **未写**（用户裁决），漂移已记入 §5.3 |