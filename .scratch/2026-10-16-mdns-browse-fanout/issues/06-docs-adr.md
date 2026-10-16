# 票 06 — 文档：ADR 0054 + ADR 0042 增补 + 注释同步复核

Status: resolved
Type: task
Blocked by: 05

## 内容

- **新开** `docs/adr/0054-mdns-single-browse-per-type-and-fanout-hub.md`（跨 crate 机制不变量，spec §6 D6）：
  - 背景：根因（mdns-sd 每服务类型单 listener、后到覆盖；重传链保活旧 listener）；ADR 0042 只做了守护级共享，listener 级缺位
  - 决策：全进程每类型唯一 `daemon.browse()` 只由类型中枢代持；订阅者进程内 fan-out；requery 收敛中枢；锁白名单（`type_hub.rs` 唯一 browse / `engine.rs` 唯一创建守护）
  - 后果与不做项（spec §8）；与 ADR 0042 / 0022 的关系（零 WIT / ABI 变更，不触发 bump）
- `docs/adr/0042-*.md`「验证」或「与既有 ADR 的关系」节**增补一行执行记录**指向 0054
- **注释同步复核**（spec §8；票 02/03/04 已改，此处逐条复查）：peer-net `discovery.rs` 模块头、engine.rs `shared_daemon` 注释、移动 `plugin/host_domains/mdns.rs:16` 红线、ADR 0042 关联段落
- CHANGELOG **不写**（AGENTS §2：发布前一次性汇总）

## 门禁

- 文档与最终代码逐条核对；引用的路径 / 符号 / 行号真实可对照（AGENTS §0「文档字面 ≠ 事实」）
- 两端 code-map 若票 05 已登记，此处仅核对

## Comments

### 改动摘要

- **① 新 ADR** `docs/adr/0054-mdns-single-browse-per-type-and-fanout-hub.md`（约 190 行）：
  状态（已实施 2026-10-16，票 01–05 已合入；全量回归见票 07）+ 背景（症状 / mdns-sd 单
  listener 槽根因 / 依赖方向约束）+ 决策 D1–D9（中枢唯一 browse 代持 / fan-out 64 与惰性剔除 /
  100ms 合并窗前缘吞并 + 后缘补齐 / 守卫退订与归零立即 stop / 引擎浏览原语切换 / peer-net
  纯值注入 / 端口扩展与双端宿主接线 + 移动 `register_process_ports()` 装配时序 / 锁白名单与
  依赖方向 / 零 WIT 零 ABI）+ 后果与不做项 + 验证表 + 与 ADR 0042 / 0022 / 0018·0019 的关系。
- **② ADR 0042 增补**：`docs/adr/0042-dual-end-mdns-engine-shared.md` 文末新增
  「## 执行记录（后续收口）」段，一行执行记录指向 ADR 0054（说明 listener 级不变量已收口、
  D1–D7 与守护唯一创建点口径维持不变）。
- **③ 注释同步复核**：四个目标逐条对照最终代码，**全部与实际一致，无需修正**（详见下）。
- **④ 登记检查**：新 ADR **不登记**（仓内无 ADR 索引惯例——ADR 0053 同样未被任何索引登记，
  `CONTEXT.md` 仅 glossary 一处提 ADR 0022，无 ADR 列举表）；两端 code-map 防回接锁索引
  由票 05 登记，本次核对一致，无需改动。

### 逐条核对结论（③ + ④）

| 目标 | 核对结论 |
| --- | --- |
| peer-net `src/discovery.rs` 模块头「浏览面走类型中枢订阅（ADR 0054）」段 | ✅ 与实际一致：值形态 `PeerMdnsBrowse`（:520）、5 参 `spawn_peer_mdns_daemon`（:577）、`request_requery` 转调闭包（:503）、`REQUERY_INTERVAL` 留 peer-net（:112）均与文中描述逐条对得上；全文无 `swap_tx/swap_rx`/`receiver swapped` 残留 |
| `packages/bedcode-discovery-engine/src/engine.rs` `shared_daemon` 注释（listener 级不变量 + 中枢位置） | ✅ 一致：`ServiceDaemon::new` 唯一于 `init_daemon`（:60）、`shared_daemon` 指向 `type_hub::subscribe_type`（:135-138）、`browse`/`stop_browser_inner` 已删直调（:151/:285） |
| 移动 `plugin/host_domains/mdns.rs` 红线注释（:13-27 第二层不变量 + `register_process_ports`） | ✅ 一致：`register_process_ports()`（:124）与 `lib.rs` setup 调用（:96）实际在场；「未装即 `subscribe_type` fail-visible panic」与 `ports::ports()` 未装即 panic 的实现一致 |
| ADR 0042 关联段落 | ✅ 已按票面增补执行记录（本票改动） |
| 两端 code-map 防回接锁索引（票 05） | ✅ 核对一致：`mdns_browse_singleton_lock.rs`（源码锁，落 `src/`）+ `dependency_direction_lock.rs`（依赖方向，FORBIDDEN_DEPS 含 `bedcode-discovery-engine`）；锁名 / 文件 / 判据与最终事实逐条相符，无需改动 |
| ADR 登记惯例 | ✅ 无此惯例（0053 未登记）⇒ 0054 不登记 |

### 事实核对（写前用 ls / grep 逐一验证，AGENTS §0）

- 锁路径：`packages/bedcode-discovery-engine/src/mdns_browse_singleton_lock.rs` **确实**（src 内模块，
  `lib.rs` 以 `#[cfg(test)] mod mdns_browse_singleton_lock;` 接线）——ADR 引用即此路径（非 spec 原定
  `tests/`），与实施修订一致。
- 唯一 `daemon.browse`/`stop_browse` 代码行全仓只在 `type_hub.rs`（其余命中为注释散文，锁判据已跳过）；
  `spawn_peer_mdns_advertiser` 全仓仅剩 `discovery.rs` 模块头的历史说明文字（非符号引用）。
- 验证表数据逐条取自票 01–05 的 Comments（实际跑过的门禁），未把未跑的写成跑了；
  票 04 桌面端 `capability_crates_unit_tests_only` 的 1 失败如实标注为「base 既有、与本改动无关」。
- 移动装配时序（`register_process_ports()` 早于任何 `subscribe_type`）已在 ADR D7 记录。

### 偏差与理由

- 无生产代码 / 测试改动（纯文档票）。注释复核四个目标全部一致，未产生注释修正项。
- 未跑 cargo / vitest：本票无代码面改动（AGENTS §10 完成定义按改动面裁剪）；文档一致性靠
  grep / ls 逐条核对，结论记于上表。
- 新 ADR 状态写作「已实施（票 01–05 已合入）；全量回归验证见票 07」，与票 07（全量验证）尚未执行
  的事实一致，不预写其结果。

### 未决问题

- 无。

