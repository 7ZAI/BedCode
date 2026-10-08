# 票 06 · 发送侧事件桥 + send-files 语义收窄（阶段 2 首票）

> 状态：**已完成（2026-10-07）**。门禁结果见 §6；真机双端互连与 `cross-end-tests`
> 未跑（原因见 §6）。移动端 ABI **11 → 12**（破坏性）。

## 1. 目标与本票实际范围

spec 票 06：「宿主 `peer_transfer.rs` 收敛为『batch_id → 发送会话句柄表 + 引擎事件桥』
（`peer:transfer-event`，150ms 进度节流）；`send-files` = 一次调用即发一会话，
并发闸门/队列泵删除（插件自控 `PENDING_SENDS`）；`current_concurrency`/`pump_*` 退役」。

**本票执行**（与 spec 一致，另含票 02 规划的同批破坏性变更）：
- ✅ 宿主 `peer_transfer.rs` 重写为句柄表 + 事件桥（1,909 → 约 900 行）
- ✅ `send-files` 收窄 + `concurrency` 载荷字段运行期显性拒绝（点名 ABI v12）
- ✅ 新 topic `peer:transfer-event` 直推插件总线；旧快照 topic 与前端事件桥接退役
- ✅ 插件事件归约状态机（唯一任务真源）+ 插件侧并发闸门 + `resume-all` 编排归插件
- ✅ host-peer 删 `resume-all-transfers`（票 02 规划随票 06 执行）
- ⏳ 票 07（接收侧事件桥）、票 08（插件侧封顶/重试回放细化）、票 09（发现投影）、
  票 10（剩余宿主命令面收口 + 锁扩展）未做

## 2. 改动清单（11 文件）

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 宿主引擎 | `peer_transfer.rs`（重写） | `SendSessionHandle` 句柄表 + `drive_send_session` 事件桥（Progress 节流 / Terminal 代次防护 / Paused·Resumed 直推）+ `drive_serve_events` 纯直推 + `send_files_to_peer_with_policy` 收窄返回句柄 + pause/resume/cancel 按句柄表路由（serve → 接收方向逐级回落）；删任务表、历史文件、并发闸门、批量恢复、serve 记账 |
| 宿主绑定 | `host_impl/peer.rs` | `peer_send_files` 删并发脉冲 + 显性拒绝 `concurrency`；删 `peer_resume_all_transfers` |
| 宿主绑定 | `component.rs` | 删 `resume_all_transfers` 的 bindgen impl |
| 宿主 | `lib.rs` | invoke_handler 删 5 个退役命令 |
| 宿主 | `peer_net.rs` | `bus_topic_for` 摘除 `peer-transfer-changed → peer:transfer` 映射 + 测试改断言 None |
| 宿主 | `peer_receive.rs` | 删 `resume_all_peer_receiving`（唯一调用方是已删除的宿主批量恢复编排） |
| WIT | `plugin-sdk-mobile/rust/wit/bedcode.wit` | host-peer 删 `resume-all-transfers`；接口文档登记 `peer:transfer-event` 与 send-files 收窄 |
| SDK | `host/peer.rs` + `wasm_host.rs` + `abi.rs` | 删 trait 方法与转调；`ABI_VERSION` 11 → 12 + 版本志 |
| 夹具 | `plugin-component-test/src/lib.rs` | `abi.version()` 硬编码 11 → 12（改 WIT 后必须同步，否则宿主 ABI 协商测试红） |
| 插件 | `transfer_store.rs` | 新增 `terminal_status_of` / `reduce_event` / `insert_active_projections` / `reconcile_diff` + 11 条用例 |
| 插件 | `peer.rs` + `lib.rs` | 新增 `reduce_and_emit` / `rebuild_from_active_transfers` / `dispatch_pending_sends` / `launch_send` / `running_send_count` / `resolve_peer_name`；`send_payload` 去并发字段；`resume_all_tasks` 遍历自身 paused 条目；订阅 + 归约 `peer:transfer-event`；activate 调首屏兜底 |
| 锁 | `tests/retired_mobile_send_orchestration_lock.rs`（新） | 结构面防回接锁 ×2（含快照 topic 桥接退役） |

## 3. 与桌面 v31 的对齐与移动端差异（点名）

| 语义 | 桌面（v31 终态） | 移动端（本票） | 差异 |
| --- | --- | --- | --- |
| 会话句柄表 | `SendSessionHandle{cancel, epoch, pause, sources, node_id, encrypt, total, transferred, paused}` | 同字段集 | 无 |
| `active-transfers` 字节 | 句柄表持有 total/transferred | **本票起句柄表同样持有**（旧实现恒给 0） | 差异消除（票 04 遗留的显性 0 已去掉） |
| `send-files` 载荷 | `{ path, encrypt? }`，`concurrency` 运行期拒绝 | 同款，错误文案点名 ABI v12 | 仅版本号 |
| pause/resume 路由 | send 会话 → serve handler → false | send 会话 → serve handler → **接收方向回落**（保留移动端既有行为） | 移动端多一跳回落：push 接收批 / 拉取会话的暂停经同两个原语，票 07 收口后与桌面同形 |
| `pull-served` 载荷 | 带 files + totalSize | 同款 | 无 |
| 事件 topic | `peer:transfer-event` | 同名同形状 | 无（ADR 0018 下同名词义对齐） |
| `resume-all` | 插件遍历自身 paused 批 | 同款 | 无 |
| serve 供流记账 | 插件自建 send 行 | 同款 | 无 |

## 4. fail-visible 三形态

| 形态 | 落地 |
| --- | --- |
| ① 旧读路径删除 | `transfer_history.json` 读路径（`ensure_history_loaded` / `read_history_file`）与写路径一并删除；宿主任务表、serve 记账、批量恢复编排整体删除；`peer-receive` 侧「全部继续」编排删除（无「查不到就返回空」式静默降级） |
| ② 旧产物实例化期点名 | ABI 11 → 12：`plugin-component-test` 与 SDK `ABI_VERSION` 同步；宿主加载按 `abi.version()` 协商，旧产物（声明 11）与宿主期望 12 不匹配即拒绝 |
| ③ 退役词汇/载荷加载即抛 | `send-files` 载荷携带 `concurrency` → 显性报错并点名「rebuilt plugin artifact with current SDK」；`peer-transfer-changed` 总线桥接映射摘除（残留映射会把空快照回流成静默降级） |

防回接锁：`retired_mobile_send_orchestration_is_not_reintroduced`（13 项退役构件 +
快照 topic 桥接，源码扫描只扫非注释行）+ 载荷检测（行为面）。变异自检：注入
`pick_pending_to_start` → 结构锁转红，还原后恢复绿。

## 5. 已知遗留（点名，非静默）

1. **排队中的发送批不可见/不可取消**：`PENDING_SENDS` 里的批尚未调 `send-files`、没有
   batchId，UI 上不可见（`enqueue` 返回 `{ queued: true }`）。
   **用户裁决（2026-10-07）：与桌面同批决策**——保持现状记为遗留，不在本专项给排队批
   发临时 id。要改两端一起改（避免双端行为分叉），列入票 20 收尾评估。
2. **宿主 `set_peer_transfer_concurrency` 命令与 settings 字段仍在**：并发节流真源
   已迁插件，宿主侧字段暂留（持久化兼容 + 旧命令面），**属票 10 退役面**。当前它
   只写设置、不参与任何闸门判定——插件闸门读的是插件 storage 里的 `concurrency`。
3. **接收方向未动**：`peer_receive.rs` 的任务表 / `peer-receive-changed` 快照 /
   原因码映射仍在宿主，属票 07。
4. **`peer_remote` 的 pull 任务预登记**（`register_remote_pull`）属接收方向，票 07。

## 6. 门禁结果

| 门禁 | 结果 |
| --- | --- |
| SDK `cargo check` | ✅ 0 error（crate 单编译单元，wit-bindgen 重跑读新 WIT） |
| 宿主 `cargo check --lib` | ✅ 0 error 0 warning（26.86s） |
| 宿主 `cargo test --lib` | ✅ **376 passed / 6 failed** —— 6 个失败全在并行会话在途的 `egress.rs`（`default_tier_consults_records` 等），非本票文件，按 AGENTS §11 不碰；本票改动面 0 失败 |
| 防回接锁（新集成测试） | ✅ 2 passed；**变异自检**：注入 `pick_pending_to_start` → 结构锁 FAILED，还原后绿 |
| 插件 crate `cargo test` | ✅ **44 passed / 0 failed**（此前被票 04 遗留的 MockHost 缺 5 方法阻塞，本票补齐 `peer_set_download_dir` / `peer_start_node` / `peer_stop_node` / `peer_active_transfers` / `peer_collect_outgoing`；删 `peer_resume_all_transfers`） |
| **wasm32 构建（`--rust-only`，spec §6 硬门禁）** | ✅ 通过：`node ../../packages/plugin-sdk-mobile/bin/cli.js build --rust-only` → wasm32 release 编译 + componentize 成功（734,957 bytes）。**native `cargo test` 绿 ≠ 可交付**，`#[cfg(target_arch="wasm32")]` 面只有这条路径覆盖；`plugin.json` 自动填充后零改写（manifest 与源码一致） |
| `cargo fmt --check` | ✅ 本票文件干净（全仓 diff 仅并行会话在途文件：auth/manager、connection/*、commands/dev_logs、egress.rs、peer_net.rs 历史漂移区——均非本票改动行） |
| `cargo clippy` | ✅ 本票文件零新增（剩余 4 条为插件侧既有基线：`entry_from_dto` 未使用、`cloned_ref_to_slice_refs`、`unnecessary_sort_by`、`ptr_arg`；桌面同款同形，未顺手改） |
| 双端 WIT 对照 | ✅ host-peer 移动端 **19** 函数 vs 桌面 **19** 函数，函数名集合逐项 `diff` **完全一致**（`resume-all-transfers` 已删，双端 host-peer 首次完全对齐） |
| 插件调用点零漂移 | ✅ file-transfer 是唯一消费者，同批迁移完毕；auto-task / ai-chatbox 不触碰 host-peer |
| **真机双端互连**（发送闭环 / 断点续传 / 暂停恢复 / 全部继续） | ⚠️ **未跑**：需移动端真机或模拟器 + 桌面同网段；行为等价依赖插件面 44 测试 + 宿主 376 测试覆盖的纯函数与句柄表契约，真机验证列入收尾全量验收（票 20） |
| `cross-end-tests` | ⚠️ **未跑**：本票不改跨端协议（peer 线协议 / REST / WS 均未动，只改宿主内部编排与总线 topic），但事件总线契约变化仍以票 20 的真实互连为准 |
