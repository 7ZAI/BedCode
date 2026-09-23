# 02: prefactor — 会话对外视图类型迁出内核 session 目录

**What to build:** 让「宿主不再持有会话」与「对外协议形状不能变」这两件事**解耦**：
把内核会话目录里那批**其实是对外 wire 契约**的视图类型（会话记录、对外视图、
尺寸裁决结果与来源、会话状态的 serde 形状）搬到中立协议域，并给它们补形状锁。做完之后，
票 11 删整个内核会话目录时不会再顺手删掉移动端与前端还在读的协议定义——这是 11 的前置，
也是「先让改动变容易」的那张票。

**Blocked by:** 无（可以立即开始）。

## 与并发批次的分工（2026-09-24 对齐，别抢同一块肉）

`enums/` 目录下的会话相关类型**不归本票**——`.scratch/2026-09-24-host-crypto-business-downsink/`
的 §4.2 三分类清单已经把处置权分给它自己的票：终端转义（按键组合 → 转义字节）下沉插件是它的 06，
会话状态/类型业务视图收窄是它的 08，环境选择与启动配置下沉是它的 07。
**本票只动内核会话目录内部**那批「实现与协议混住」的视图类型（会话记录、对外视图、
尺寸裁决结果与来源），把它们从待删目录里搬出来；跨真源对齐锁继续保证插件登记域视图
与宿主类型逐字段相等。

**推论（要写进票末的判据）**：如果本票发现某个待搬类型的**定义就在 `enums/`**、
且对侧批次对它另有处置计划 → 不搬、不改，只在本票末登记「等对侧 X 票落地后由 11 一并处理」，
避免同一类型被两条线各搬一次。特殊键路径同理：对侧 06 落地前，网关的特殊键翻译照旧可用，
本票不预先把它挪位置。

**Status:** done（2026-09-24 落地：`src/protocol/` 中立域 + 5 把新形状锁；
宿主 `--lib` 1153/0、集成 8 target 全绿、变异自检三处全咬；
vitest 唯一红属并发 crypto 批次的权限位文案，非本票改动）

## 为什么先做它

P1-b 之后内核会话线已经没有生产流量，但它的**类型**仍是对外真源：窄转发层反序列化插件视图时
按 `SessionInfo` 的字段形状解析、HTTP/WS 响应体外嵌 `SessionInfoView`、尺寸裁决返回
`ResizeOutcome`。这些类型今天和「内核登记实现」住在一个目录里，删实现必然连带删协议——
所以必须先把协议搬出去。

## 验收标准

- [x] 上述对外类型全部离开内核会话目录，落在只放「跨端 wire 契约 / 引擎级转义」的中立域；
      内核会话目录里只剩登记与输出环**实现**（且实现内部仍可从新位置 import 类型）
      → `src/protocol/session.rs`；`session.rs` 的重导出直接删除，不留兼容转发
- [x] serde 形状逐字不变：新增/迁移**形状锁**用例，把会话状态八个取值（含 `Error` 的
      `{"error": …}` 形态）、对外视图的字段集合与 camelCase、尺寸裁决四态回执逐一钉死
      → 见 Comments「形状锁」+「变异自检」两段（5 把新锁，三处源级变异全部咬住）
- [x] 跨真源对齐锁继续有效：插件登记域视图（`session-list` 同源）与迁出后的宿主类型
      逐字段相等（P1-b 前置 C 已建立该锁，迁位置不得让它退化成自比较）
      → `session_e2e::` 11/11 绿，锁仍是插件 JSON → 宿主 `SessionInfo` 反序列化比对
- [x] 全仓引用改指新位置后：宿主 `cargo test --lib` 全绿、集成 8 target 全绿、
      桌面前端 `pnpm run test:run` 全绿、根 `pnpm exec eslint .` 0 error
      → 1153/0 + 8 target 全绿 + eslint 0 error；**vitest 唯一红属对侧在途权限位**（见 Comments）
- [x] 本票**零行为变化**：响应体、事件载荷、错误文案逐字不变（用 `--ignore-cr-at-eol` 核 diff，
      确认没有整文件行尾抖动混进来）
      → 90 行搬移码逐字比对（仅 2 行 derive 全限定名等价差）、行尾计数 now=HEAD

## 边界与不做

- 不删任何内核会话实现（那是 11）。
- 不动 WIT / ABI（本票纯宿主内部模块归属，插件侧零改）。
- 不顺手格式化既有 rustfmt 偏离（按文件核自己那几行）。

## Comments

### 2026-09-24 · 落地（票 11 的前置已就位）

**新域**：`bedcode-desktop/src-tauri/src/protocol.rs` + `protocol/session.rs`（入口文件与目录同名，
仓库无 `mod.rs` 规矩）。

**为什么是新建而不是塞进 `enums/`**：票面要求落点是「**只放**跨端 wire 契约 / 引擎级转义」的中立域，
而 `enums/` 今天还装着 `auth.rs`（加密协商）、`shell.rs`（WSL / 环境选择）这类待裁决的业务类型，
且正是并发批次 `2026-09-24-host-crypto-business-downsink` 票 05/07/08 的重构对象——
塞进去等于把本票的落点变成对侧的撞车点。该批 §4.2 确实把 `enums/` 登记为「线协议形状」的家，
**若后续两域要合并，判据按本票域头注释里那条红线走（对外契约才可入域）**，不预先合并。

**搬了什么**（`session/` → `protocol/`）：`SessionInfo`、`task_fields_from_slot`、`SessionInfoView`
（含 `from_session`）、`RendererSource`（含 `is_desktop`）、`ResizeOutcome`。
内核目录里的实现（`DefaultSessionInfoRegistry` / `CanonicalRendererRegistry` /
`resolve_initial_size` / 输出环）留在原地，改从新位置 import。
`session.rs` 的重导出**直接删掉**，不留兼容转发——消费方 9 处（`commands.rs`、
`utils/session_gateway.rs`、`events/sync_handler.rs`、`server/http/controllers/session_controller.rs`、
`server/websocket/subscription.rs`、`server/websocket/services/session_control.rs`、
`wasm_core/host_api/session.rs`、`session/session_components.rs`、`session/session_manager.rs`）
+ 测试（`session_e2e.rs`）全部改指 `crate::protocol::*`。

**刻意没搬（三条判据，写下来免得 11 又搬一遍）**：

| 类型 | 处置 | 判据 |
| --- | --- | --- |
| `SessionStatusEvent` | 留 `session/session_event.rs` | 不是跨端 wire 契约的搬运目标：其消费面（`events/forwarder.rs`、`websocket/channel/terminal.rs` 的内核状态订阅转接）**由票 09 删除**，之后随 11 一起消失，搬它等于搬一具要埋的尸体 |
| `SessionStatus` / `SessionType` 的**定义** | 留 `enums/session.rs`，本票只锁其 wire 形态 | 定义在 `enums/` 且对侧票 08 对它另有处置计划 → 票面推论「不搬、不改、只登记」 |
| `KeyCombo` 转义表 | 原地不动（实际住 `enums/special_key.rs`，非内核会话目录） | 对侧票 06 裁决其下沉；本票不预先挪位置（§3b-1 撞车点表） |

**形状锁**（`protocol/session.rs` 的 `mod tests`，共 11 项 = 6 迁移 + 5 新增）：
`shape_lock_session_info_field_set_is_exact`（记录精确键集）、
`shape_lock_session_info_view_field_set_is_exact`（视图 = 记录 + 四任务字段的完整键集）、
`shape_lock_session_status_wire_forms`（**八个 wire 形态**：七变体，`Error` 带值 / 带 `null` 两形态——
票面「八个取值」按可出现的 JSON 形态数解）、
`shape_lock_resize_outcome_four_receipts`（`Applied`/`NeedsConfirmation` × `Desktop`/`Mobile`
四态回执**整体 JSON 相等** + 反序列化回环，不只校字段存在）、
`shape_lock_renderer_source_tag_is_kind_not_status`（tag 键 `kind` 与 `status` 不混）。

**变异自检**（证明锁会咬，不是恒真断言）——注入三处源级变异后 `cargo test --lib protocol::`：

| 变异 | 咬住的用例 |
| --- | --- |
| `ResizeOutcome` 去掉 `rename_all_fields` | `…json_shape_is_camel_case` + `four_receipts` |
| 视图 `task_status` 去掉 `skip_serializing_if` | `view_without_slot` + `treats_empty_slot` + `view_field_set` |
| `SessionInfo.started_at` 改名 | `info_field_set` + `view_field_set` |

结果 `5 passed; 6 failed`，命中面与预期完全一致；还原后 `11 passed; 0 failed`。
`SessionStatus` 形态锁未做直接变异（要改 `enums/session.rs`，属对侧在途文件），
其咬合性由同型的 tag/大小写变异（第一行）与插件侧 `session/model.rs` 的同形锁共同保证。

**零行为核对**：
- 搬移逐字性——从 HEAD 版两个源文件抽出被移出的 90 行非注释代码，对新文件比对，
  **仅 2 行不同**且是同一行 `serde::Serialize/Deserialize` 全限定名 → 简名（新文件顶部已 `use`，
  derive 等价）。
- 行尾——`raw` 与 `--ignore-cr-at-eol` 的 numstat 对全部触及文件一致；被点名的文件 CRLF 行数
  now=HEAD=0，新文件纯 LF。**无整文件行尾抖动**。
- 我在收尾时**还原了自己引入的 24 处格式化重排**（对 HEAD 原文跑 rustfmt 得到「我会提的 hunk」，
  再逐 hunk 逆向）。票面「不顺手格式化既有 rustfmt 偏离」是硬约束，而
  `rustfmt --check HEAD:src/events/sync_handler.rs` 证明 `sync_handler.rs:77/84` 那类重排是**我**
  的 per-file rustfmt 造成的，不是对侧在途改动——已改回原样。对侧文件（`session_gateway.rs`、
  `session_controller.rs`、`commands.rs`、`session_manager.rs`）的在途重排按 diff 计数逐处核过，
  一律未动。

**门禁实跑（2026-09-24 01:44–01:52）**：

| 门禁 | 结果 |
| --- | --- |
| `cargo check --lib --tests` | 0 error；lib 告警数 42 = 改前基线，`src/protocol` 零告警 |
| `cargo test --lib` | **1153 passed / 0 failed** |
| 集成 8 target | `broadcast_shutdown` 1 / `build_manifest_smoke` 1 / `http_auth_biometric` 1 / `link_crypto_http` 4 / `pty_session_chain` 1 / `server_integration` 1 / `ws_auth_rules` 1 / `ws_session_route` 1，全 ok |
| 跨真源对齐锁 | `cargo test --lib session_e2e::` **11 passed / 0 failed**；锁仍是「插件 `session-list` 的 JSON → 宿主 `SessionInfo` 反序列化 + 网关视图比对」，未退化成自比较 |
| 根 `pnpm exec eslint .` | **0 error**（120 warning，不计入门禁） |
| `pnpm run test:run`（vitest，桌面） | **81 files / 794 tests：793 passed、1 failed** |

vitest 需 **`NODE_OPTIONS=--max-old-space-size=6144` + `--pool=forks`** 才跑得完：
13GB 机器上默认堆连跑两次都在 `Ineffective mark-compacts near heap limit` 处崩
（第二次已确认不是与 cargo 抢资源——单独跑仍崩）。提堆后完整出数：
`Test Files 1 failed | 80 passed (81)`、`Tests 1 failed | 793 passed (794)`。

**中途红与归属（都是并发批次在途 / HEAD 自带，不是本票改动）**：
1. 首轮 `cargo test --lib` 出 8 红：6 项是 `plugin-wasip3-test` 夹具的
   `error[E0063]: missing field pty_quota`（字段来自已 landed 的 `c5e3d86d3`，夹具与 SDK
   `types.rs` 当时都在 HEAD 态 ⇒ HEAD 即红）→ 已立
   [`issues/14-wasip3-fixture-pty-quota-stale.md`](14-wasip3-fixture-pty-quota-stale.md)；
   另 2 项 `pty_e2e` PTY 计数断言在 `--test-threads=1` 下 5/5 绿，且夹具被对侧修好后一并转绿。
   **对侧在 01:35 自行给夹具跟演了 `pty_quota`**（其工作区改动），所以票 14 现只剩
   「HEAD 上仍红、合并前须由对侧带入」这一层含义，票面已按实测改记。
2. vitest 唯一红：`permissionMeta.test.ts > 词汇表每一条权限都有文案` 报
   `crypto:aead, crypto:asym, crypto:kdf` 缺 zh-CN 文案——正是对侧 crypto 票新加权限位、
   尚未跑 SDK `pnpm run gen:permissions` + 补 i18n（AGENTS §7 的「拆分后同步点必须同步落」）。
   本票前端零改动，不代改对侧文件。
3. vitest 的 OOM **不是并发抢资源**：单跑（`--pool=forks`）仍在同一位置崩堆，提堆到 6GB
   才完整出数。上面门禁表已按实测口径写死跑法；此前「与 cargo 并发导致」的猜测作废。

**顺带记账**：起跑人工基线（票 01）时撞到的跨插件 api 防漂移编译红已立为
[`issues/13-plugin-api-drift-trait-stale.md`](13-plugin-api-drift-trait-stale.md)，
两票（13/14）与本票改动面无交集，均已登记进 spec §3b 拓扑表。
