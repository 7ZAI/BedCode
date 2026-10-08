# 票 08 · 插件事件归约状态机收口（重试回放细化 + redial / 历史持久化集成）

> 状态：**已完成（2026-10-07）**。门禁结果见 §6。**零宿主改动、零 ABI / WIT / 协议变更**
> （事件总线契约沿用票 06 / 07）。

## 1. 目标与本票实际范围

spec 票 08：「插件事件归约状态机」——file-transfer 插件以事件归约为唯一任务真源
（建行 / 推进 / 终态 / 原因码映射 / 封顶 / 重试回放单点，真源在插件私有库
`transfer_store` 扩展）；历史 / 设置 / 注册表读面迁移。门禁：断点续传（redial）
集成 + 终态历史持久化测试。

**本票执行**：
- ✅ 补齐 spec 列举但票 06 / 07 未覆盖的最后一格：**回放与节流判据**（重试可重试性、
  发送闸门、拉取意图队列）收进 `transfer_store` 纯函数单点
- ✅ 重试编排**次序重排**：判据与闸门前置到调引擎之前（修「无主会话 + 槽位泄漏」）
- ✅ 排队发送批派发失败不再静默丢弃（重排队 → 落带凭证的终态行）
- ✅ 拉取意图入队竞态消除（入队先于 `peer-pull-files`）+ 队列封顶移到入队点
- ✅ redial 前置条件（`node-stopped` 清句柄保留 memo）+ 停用清进程内意图
- ✅ `restore_entries` 抽出可测 seam；redial 场景链与历史持久化往返集成用例
- ⏳ 票 09（发现 / 设备列表投影）、票 10（剩余宿主命令面收口 + 锁扩展到调度面）未做

### spec「历史 / 设置 / 注册表读面迁移」的核实结论（无需改动）

逐项核实，三条读面**票 06 / 07 之后已在插件侧**：

| 读面 | 真源 | 状态 |
| --- | --- | --- |
| 历史 | 插件 `transfer_store` + storage 键 `transfer_entries` | ✅ 已在本票补持久化往返门禁 |
| 设置 | 插件 storage 键 `transfer_settings`（`settings_store`），宿主只收配置推送 | ✅ 插件持有；本票把并发缺省值收成 `DEFAULT_CONCURRENCY` 单点（原先闸门与设置各写一个 `3`） |
| 共享目录注册表 | `roots_registry`（插件 storage + `peer-set-shared-roots` 全量推送） | ✅ 已在插件 |

宿主侧残留读面（`get_peer_receive_settings` 等，前端零消费者）属**票 10 命令面收口**，
本票不动（票 07 §5.4 已记录）。

## 2. 改动清单（7 文件，其中 2 新增）

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 插件 store | `transfer_store.rs`（+130行） | 新增 `PULL_INTENT_CAP`；重试判据 `RetryRefusal`（三类拒绝 + 文案）+ `retry_source`；闸门判据 `send_slot_open`（下限 1）；拉取意图队列 `push_pull_intent`（入队点封顶）/ `take_pull_intent`（node × rel_path 匹配 + 收窄单文件）；`apply_retry` 补注「retryMeta 保留」；模块头补票 08 段；用例组 `tests/retry_and_gate.rs`（7 例） |
| 插件编排 | `peer.rs`（+180 行） | `retry_task` 重排（判据 → 闸门 → 回放，删冗余 `slot.retry_meta` 回写）；`pull_files` 与 retry pull 分支**意图先入队**；`PendingSend` 加 `attempts` + `MAX_SEND_ATTEMPTS` 重排队 + 用尽落终态行（`send_row` / `insert_failed_send_entry` / `next_local_failure_id` / `insert_send_row` 四个新函数）；`attach_pull_meta` 收窄为「凭证取出」；`node-stopped` 增 `drain_sessions`；`reset_volatile_intents`；`restore_entries` + `persist_entries` / `load_entries` 泛型到 `&(impl HostStorage + HostLog)`；`send_slot_available` 走纯函数判据；`MockHost` 补 `HostLog` + `logs` |
| 插件会话 | `device_bridge.rs`（+10 行） | `statics_lock` 上提为 `#[cfg(test)] pub(crate)`（跨模块共用同一把串行锁）；内联测试改用它 |
| 插件设置 | `settings_store.rs`（+7 行） | 新增 `DEFAULT_CONCURRENCY`（闸门与设置的并发缺省单点） |
| 插件入口 | `lib.rs`（+9 行） | `deactivate` 调 `reset_volatile_intents` 并留痕；模块头补票 08 段 |
| 用例 | `peer/tests/redial_and_history.rs`（新，7 例） | redial 场景链 / redial 前置条件 / 回放判据前置 / 终态历史持久化往返 / 封顶往返 / 本地终态行形状 / 拉取意图挂载与不挂载 |
| 文档 | `bedcode-mobile/docs/code-map.md` · `CHANGELOG.md` | 新增「回放与节流判据单点（票 08）」段；英文 CHANGELOG 条目（中文按前票裁决未写，见 §5.3） |

## 3. 本票修掉的三个真实缺陷（非重构 tidy-up）

| # | 缺陷 | 触发条件 | 后果 | 修法 |
| --- | --- | --- | --- | --- |
| D1 | **重试先发后校验** | 对进行中条目或非本端发起行点重试 | send 方向无引擎建行事件 → 铸出永不入店的**无主会话**；进度事件打不中行；**永久占死一个发送槽位**（闸门再不放行） | `retry_source` 判据前置到调引擎之前；`apply_retry` 未命中即显性报错 |
| D2 | **拉取意图入队晚于引擎调用** | 任意 `pull-files` / 拉取重试 | 引擎铸 `batch_id` 后 `pull-started` 事件即回流，事件处理早于函数返回 → `retry_meta` 永挂不上 → **该行永久不可重试** | 意图先入队（两处调用点）；封顶移到入队点 |
| D3 | **排队发送批失败静默丢弃** | 槽位满时排队，随后对端离线 / 句柄陈旧 | 排队批在 store 里**没有行**，失败只写日志 → 用户意图凭空消失，且「从历史重试」对它无效 | `MAX_SEND_ATTEMPTS` 重排队（重拨常能救活），用尽后落带 `retry_meta` 的终态失败行 |

顺带修掉的连带问题：`node-stopped` 不清 session 句柄 → 节点重启后陈旧句柄让
`ensure_target` 永远走不到重拨分支（`peer:connection{connected:false}` 只摘单个节点，
节点整体下线时无人摘）；`retry` 不过并发闸门 → 静默超并发发送。

## 4. 与桌面 v31 的对齐与移动端差异（点名）

| 语义 | 桌面（v31 终态） | 移动端（本票） | 差异 |
| --- | --- | --- | --- |
| 重试判据位置 | 内联在 `retry_task`（先发后校验，同款隐患） | `transfer_store::retry_source` 纯函数单点 + 前置校验 | **移动端更严**（桌面同款次序，本票未动桌面——超出本专项范围，留待桌面同批） |
| 重试过闸门 | 桌面 retry 直发 | 移动端显式拒绝（槽位满） | 移动端闸门自控（票 06），语义差异点名 |
| 排队失败可见性 | 无排队批概念 | 重排队 + 终态失败行 | 移动端独有形态 |
| 拉取意图入队 | 入队在 `peer-pull-files` 之后（同款竞态） | 入队先于调用 | **移动端更严** |
| `node-stopped` 清句柄 | 桌面靠快照对账 | 显式 `drain_sessions` | 移动端无快照通路，必须显式 |

## 5. 已知遗留（点名，非静默）

1. **桌面端同款隐患未修**：`bedcode-desktop/wasm-apps/file-transfer` 的 `retry_task`
   仍是「先发后校验」，也有拉取意图入队竞态。本专项 Out of scope 含桌面端改动，故
   只在此点名；如需对齐应与桌面同批立项。
2. **`peer_remote.rs` 拉取编排仍在宿主**（逐文件队列 + 信号量并发）：判据 B2 命中，
   spec 阶段 2 未给它单独票（票 07 §5.2 已记录，票 08 未改变其归属判断）。
3. **中文 CHANGELOG 未写**（延续票 07 用户裁决，本轮只写英文）：`CHANGELOG_zh.md`
   待补——双语同步漂移显式记录在此。
4. **宿主命令面残留未清**：`get_peer_receive_settings` 等前端零消费者的读面属票 10。
5. **本地终态行时间戳为 0**：wasm32 无本地时钟（禁 `std::time`），排队失败落的那条
   终态行 `updatedAtMs = 0`，在 `history_view` 降序排序里排最后（如实可解释，但真机
   上「刚失败的排在历史最末」略反直觉）。彻底解法是引擎事件盖章，属独立立项。
6. **`resume_task` 仍不做本地乐观改写**：状态推进完全交给引擎事件归约（票 06 口径），
   若引擎 resume 失败且不回事件，UI 会停在 paused。本票未改（与桌面同形）。

## 6. 门禁结果

| 门禁 | 结果 |
| --- | --- |
| 插件 crate `cargo test` | ✅ **58 passed / 0 failed**（净 +14：判据分类 3 + 终态变体 + 闸门钳制 + 意图队列 2 + redial 场景链 + redial 前置 + 判据前置 + 历史持久化往返 + 封顶往返 + 本地终态行 + 意图挂载/不挂载） |
| **变异自检 3/3** | ✅ ① `send_slot_open` 去掉 `limit.max(1)` 下限钳制 → `send_slot_open_uses_clamped_limit` 红；② `push_pull_intent` 去掉入队点封顶 → `pull_intent_queue_is_capped_on_push` 红；③ `restore_entries` 不回写 → `terminal_history_survives_restart_and_active_rows_are_marked` 红。均已还原并复跑全绿 |
| **wasm32 门禁（`--rust-only`，spec §6 硬门禁）** | ✅ `node ../../packages/plugin-sdk-mobile/bin/cli.js build --rust-only` → wasm32 release + componentize 成功（**725,757 bytes**；票 07 为 720,145）。唯一 warning 是既有基线 `entry_from_dto` 未使用 |
| `rustfmt` | ✅ 本票**新增/改写块**已按 rustfmt 风格修正（两个新测试文件整体 rustfmt）；插件 crate 存在**大量既有格式漂移**（`peer.rs` / `transfer_store.rs` / `device_bridge.rs` / `roots_registry.rs` 等），按最小改动原则**未整crate 格式化** |
| `cargo clippy` | ✅ 本票新增代码零新增 warning（修掉自查发现的 `restore_entries` `&mut Vec` → `&mut [_]`）；既有 4 warning（`entry_from_dto` dead_code、`save_and_push`/`load_or_migrate` `&mut`、`history_view` `sort_by`、`merge_snapshot` `cloned_ref_to_slice`）为票 06/07 及更早基线 |
| 行尾纪律 | ✅ 脚本批量改写曾把 `peer.rs` / `transfer_store.rs` 的 CRLF 洗成 LF（raw diff 由 +485 膨胀为 +1369）→ 按 HEAD 逐行还原行尾，`git diff --numstat` 与 `--ignore-cr-at-eol` 现已一致；两个新测试文件按 crate 主流（CRLF）落盘 |
| 宿主 `cargo test` | ⚠️ **未跑**：本票**零宿主文件改动**（改动面全在插件 crate + 文档），宿主基线仍是票 07 记录的 `egress.rs` 6 例在途失败 |
| 前端 vitest / 根 eslint | ⚠️ **未跑**（延续票 07 裁决）：零前端文件改动，无 i18n key 增减 |
| 真机双端互连 | ⚠️ **未跑**：引擎交互面（`peer-send-files` / `peer-pull-files` / 真实重拨）需真机宿主，列入票 20 全量验收 |
| `cross-end-tests` | ⚠️ **未跑**：本票不改跨端协议（peer 线协议 / REST / WS / 总线载荷均未动） |