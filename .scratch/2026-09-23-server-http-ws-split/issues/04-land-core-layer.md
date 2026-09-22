# 04: 传输无关六文件收进 `server/core/`

**What to build:** `server/` 第一次有了明确的「内核层」：`app / supervisor / port_checker / filter / metrics / link_crypto` 六个传输无关文件进 `core/`，`server/core.rs` 作模块入口。做完后，读代码的人看目录名就知道哪些是「引擎与共享面」、哪些是传输面——而 HTTP 与 WS 都只能向下依赖 `core`，不许横向够对方。这一步把后面两票（05 http / 06 websocket）的目标路径钉死，让它们不必再改一次 `crate::server::core::…`。

**Blocked by:** 01（本票要重指向 `gateway.rs` 的 `include_str!`，必须有自校准前置兜着才知道指对没指对）、02（`client_info` / `cors` / facade 声明段先落地，避免同一段改两遍）。

**Status:** done（2026-09-23 commit `36428e0a5`，dev，见 Comments）

## 搬移面（`git mv`，内容零改，只改 `use`）

| 旧 | 新 | 备注 |
| --- | --- | --- |
| `server/app.rs` | `server/core/app.rs` | **整文件搬**，一拆为三是票 07，本票不动它的内部结构 |
| `server/supervisor.rs` | `server/core/supervisor.rs` | 其 `use super::metrics::{…}` 同目录 → **零改** |
| `server/port_checker.rs` | `server/core/port_checker.rs` | 顺带把文件头「在 WebSocket 服务器启动前检查端口」改成「服务器启动前」——它管的是 HTTP+WS 共用的那一个端口，旧文案是历史遗留 |
| `server/filter.rs` | `server/core/filter.rs` | 修两处死链（见下） |
| `server/metrics.rs` | `server/core/metrics.rs` | |
| `server/link_crypto.rs` | `server/core/link_crypto.rs` | **不拆** HTTP 信封 / WS 帧两分支（见票面末「为什么不拆」） |
| —— | `server/core.rs` | 新建模块入口，声明六个 `pub mod` |

## 引用改写

- `server/` 内约 27 处（`metrics` 14、`filter` 5、`link_crypto` 4、`app` 3、`supervisor` 1）；
- crate 内外部 13 处：`lib.rs`（`supervisor` / `link_crypto` / `port_checker`）、`system/lifecycle.rs`、`commands.rs`（`supervisor` / `metrics` / `link_crypto`）、`plugin/.../host_impl/config.rs`、`plugin/.../host_impl/auth.rs`；
- 集成测试 7 个文件里指向这六文件的部分：`tests/{broadcast_shutdown,pty_session_chain,server_integration,http_auth_biometric,ws_auth_rules,ws_session_route}.rs` 的 `server::app::start_http_server` 与 `tests/server_integration.rs` 的 `server::supervisor::ServerSupervisor`、`tests/link_crypto_http.rs` 的 `server::{filter,link_crypto}`。
- **例外**：`crate::server::app::ws_frame_limit` 本票**不改**（它随 `app.rs` 一起进 `core/`，迁到 `websocket/routes.rs` 是票 07 的 D5）。

## 锁跟随（本票的关键副作用）

`gateway.rs` 的两条 `include_str!("app.rs")` 在本票变成 `include_str!("../core/app.rs")`——**路径仍指向承载路由的文件**，票 01 加的自校准前置当场验一次：前置绿 = 指对了；前置红 = 指错了。禁止为了过编译把前置的基线数调低。

## 文档内链同步

- `core/filter.rs` 头部「接线点」两条：`[`super::http_filter`]` 内链在 `http_filter.rs` 尚未搬家时**会断**（`filter` 在 `core/`、`http_filter` 在 `middleware/`）——本票先把它写成绝对路径 `crate::server::middleware::http_filter`，票 05 落地后再改成 `crate::server::http::middleware::http_filter`。这条链 `cargo check` 不报、只有 `cargo doc` 报，**是最典型的静默项**。
- 同两条注释里的文字「`server/app.rs` 最内层 wrap_fn」「WS 接线点 `server/ws/terminal_ws.rs`」：后者**现状已过期**（阶段 A 后实际接线在 `ws/conn.rs`），按 AGENTS §12「描述与实际不符时以实际为准并顺手修正」改成 `ws/conn.rs`。
- `system/constants/plugin.rs` 里提到 `server::app::ws_frame_limit()` 的文档字眼：本票**不动**（路径在票 07 才变），但在 Comments 里挂一句，防止票 07 漏掉。

## 验收

- [x] `server/core/` 六文件 + `server/core.rs` 存在；`server/` 根不再有这六个文件；无 `mod.rs`（`find src/server -name mod.rs` = 0）
- [x] 六文件除 `use` 行、`port_checker` 与 `filter` 的注释外，**diff 里无逻辑改动**——`git show --stat -M` 逐文件：`supervisor.rs` **0 行变更**（纯 rename）、`metrics.rs` **0 行变更**、`app.rs` +6/-3、`filter.rs` +8/-4、`port_checker.rs` +5/-2、`link_crypto.rs` +20/-9（其中 2 行是 use 被路径拉长后按 rustfmt 折行）
- [x] 票 01 的自校准前置在锁重指向后仍绿，且**故意指错一次会红**（贴变异结果，见 Comments）
- [x] `grep -rn "crate::server::\(app\|supervisor\|metrics\|filter\|link_crypto\|port_checker\)::" src/ tests/` == 0（另补该 grep 抓不到的裸路径形态与 doctest 形态，见 Comments「票面清点口径漏了两类」）
- [x] `cargo check --lib --tests` **0 error**；告警 49 条**无一指向 `core/` 或 `gateway.rs`**（逐文件归属核对，全部是 `session_components.rs` 23 / `http_filter.rs` 测试块 6 / `wasm_runtime*` 等既有项）；`cargo test` **1159 通过 / 0 失败 / `[skip]`=0**（产物为当日 03:35 重出，本票不涉插件链）
- [x] `cargo doc --no-deps` 相比开工前**无新增** broken intra-doc link：基线 **29** 条 → 现 **28** 条，`diff` 唯一差异是 `unresolved link to super::http_filter` 被本票修掉（基线原文存 `/tmp/t04_doc_baseline_keep.txt`，逐条清单见 Comments）
- [x] **CRLF 纪律**：`supervisor.rs` 本票**零内容改动**（纯 `git mv`，票面预判的 5 处 `crate::server::` 实测不存在——它只有一行 `use super::metrics`，搬家后同目录仍然成立）；实际需要改写的是 `websocket_manager.rs`(430/430) / `system/lifecycle.rs`(490/490) / `tests/{broadcast_shutdown,http_auth_biometric,server_integration,ws_session_route}.rs` 四个全 CRLF 测试文件——逐文件改写前后 CR 数**全等**（`diff` 空），未跑 `cargo fmt`
- [x] `server/` 内 HTTP 侧与 WS 侧文件对本票六文件的引用全部收敛到 `crate::server::core::`：HTTP 侧 `gateway.rs`(1) + `middleware/http_filter.rs`(3)，WS 侧 `ws/conn.rs`(8) / `ws/endpoint.rs`(2) / `ws/channel/event.rs`(1) / `ws/channel/terminal.rs`(1) / `ws/websocket_manager.rs`(1)——票 06/08 锁的前提成立

## 为什么不拆 `link_crypto`

它 1792 行里 HTTP 信封分支与 WS 帧分支边界其实清楚，但**身份与落盘、单一 `LinkCryptoConfig`（同时携带 `encrypt_http` / `encrypt_ws_terminal` / `encrypt_ws_event` 三档）、`LinkEncryptionFilter` 的 `should_process` 通道表**三处是共享的。拆开要么 fork 配置状态、要么再造第四个共享模块——都属行为面重构，另票（spec §9.4）。本票只把它整体收进 `core/`，承认它「必须共享」而不是「没拆开」。

## 归属与真源

来源：`../spec.md` §2 D2 + §3.2 表前 6 行 + §4 引用计数表 + §5.3 + §5.4。

## Comments

- 2026-09-23 立项：来源 spec §2 D2（用户选定收进 `core/` 而非留根，代价 40 处引用随改，本票即该代价的兑现）。
- 2026-09-23 done（`36428e0a5`）：六文件 `git mv` 进 `server/core/`，新增 `server/core.rs` 入口，
  29 文件 / +97 / -77。`server::core::` 全仓 56 处。**四处与票面不符，按实际办理并回写如下**：

  1. **锁的重指向路径不是 `../core/app.rs`**。票面写的是 `include_str!("../core/app.rs")`，那是
     `gateway.rs` 已在 `http/` 时的形态（票 05 才搬它）；本票 `gateway.rs` 仍在 `server/` 根，
     正确相对路径是 `include_str!("core/app.rs")`——写成 `../core/` 时编译器直接报
     `couldn't read src/server/../core/app.rs`，属 §5.1 说的「编译器会抓的那一半」，实测踩到一次。
     票 05 搬 `gateway.rs` 进 `http/` 时同批改成 `../core/app.rs`，**这一条随票 05 兑现**。
  2. **票面清点口径漏了两类引用，都不在 §4 的「27 + 13 + 7」计数里**：
     ① `lib.rs` 里的**裸路径** `server::port_checker::` / `server::supervisor::` / `server::link_crypto::`
     （3 处，crate 根文件可省 `crate::`）——票面的验收 grep 以 `crate::server::` 起头，**抓不到它们**，
     只有编译器会报；② `filter.rs:26` 的 **rustdoc doctest** `use bedcode_lib::server::filter::{…}`——
     既不是 `crate::` 形态也不是「死链」，改漏了不会让 `cargo check` 红，而是 `cargo test --doc` 红
     （`cargo test` 全量里第 11 个测试块）。两者本票都已改，复跑证据：
     `test src/server/core/filter.rs - server::core::filter (line 24) ... ok`。
     **票 05/06/07 的验收 grep 建议统一改成**
     `grep -rnE "(^|[^:a-zA-Z_])server::(app|supervisor|metrics|filter|link_crypto|port_checker)::" src/ tests/`
     并额外扫 `bedcode_lib::server::`。
  3. **`commands.rs` 本票没动**（票面「外部 13 处」里列了它的 `supervisor / metrics / link_crypto`）。
     那 3 个 import 只存在于隔壁「命令面合并」线**未提交的 +740 在途版**，HEAD 版 `commands.rs`
     对六模块**零引用**（判据：`git show HEAD:…/commands.rs | grep -nE "server::(app|supervisor|metrics|…)::"` 空）。
     所以本票若提交 `commands.rs` 就是把对侧 740 行卷进自己的 commit。处理：本票不碰该文件；
     工作区里对侧那份已带 `server::core::` 路径（本票 sed 过），他们提交时自然带走，不会断链。
  4. **`supervisor.rs` 无需改内容**（票面预期「5 处 `crate::server::` 前缀要改 + 它是唯一既 CRLF 又要改内容的文件」）。
     实测它只有一行 `use super::metrics::{MetricsCollector, ServerMetrics}`，搬家后仍是同目录 → 零改，
     代价是全 CRLF 文件保持**纯 rename**（0 行变更）。真正需要改的 CRLF 文件是
     `websocket_manager.rs` / `system/lifecycle.rs` / 4 个测试文件，改写前后 CR 数逐文件相等。

- **锁跟随的变异验证（贴原文）**：前置重指 `core/app.rs` 后 `cargo test --lib server::gateway::` **25 绿**，
  shell 侧独立复扫 `configure_routes` 函数体仍是 **14 项**（与 `HOST_ROUTE_IDENTIFIERS_BASELINE` 双轨对上）。
  然后把取源点故意指到**新布局里最像的错文件** `core/supervisor.rs`（存在、编译通过、无路由面）：

  ```
  thread '…every_alias_still_has_a_host_route' panicked at src/server/gateway.rs:768:32:
  锁已空转：扫描目标 server/core/app.rs 里没有 configure_routes 函数体   → FAILED
  thread '…business_handlers_are_only_mounted_on_aliased_paths' panicked at …:768:32:
  锁已空转：…                                                            → FAILED
  ```

  撤销后两条锁绿。**顺带量到一处新漂移**：那次变异只改 `include_str!` 未改 `APP_RS_LABEL`，于是失败消息
  点的是假名 `server/core/app.rs`——重指向必须**同批改两行**，已把这条写进 `gateway.rs` 的取源点注释。
  票 08 若把该锁换成 `env!("CARGO_MANIFEST_DIR")` + `fs::read` 形态，可让路径与消息同源、彻底消掉这个漂移。

- **文档内链同步**：`filter.rs` 的 [`super::http_filter`] 断链改绝对路径（票面说票 05 落地后再改成
  `server::http::middleware::http_filter`，届时是第二次改这一行，别忘了）；同段两条接线点文字一并修正——
  `server/app.rs` → `server/core/app.rs`，WS 接线点 `server/ws/terminal_ws.rs` → `server/ws/conn.rs`
  （实测 `TrafficFilterChain::global()` 在 `conn.rs:371/410/446`，`terminal_ws/` 只剩订阅/转发）。
  **另修两处票面未列的文本路径**（本票搬移造成的失真）：`middleware/http_filter.rs:4`、
  `middleware/jwt_auth.rs:77` 的 `server/app.rs` 字样。`system/constants/plugin.rs:89` 的
  `server::app::ws_frame_limit()` 按票面**不动**——**票 07 记得连带它**（D5 把 `ws_frame_limit` 迁去
  `websocket/routes.rs` 时，这行文案的目标路径与 `ws/endpoint.rs:123,318` 一起改）。
  spec §5.3 列的「20 处 / 13 文件」文本路径大头仍在 `host_impl/ws.rs`、`session/session_output.rs`、
  `utils/auth/auth_center.rs`，属票 09 范围，本票未动。

- **并发与提交法**：开工时 `gateway.rs` 干净，但 `lib.rs`(59 行对侧 hunk) 与 `commands.rs`(740 行) 载着
  隔壁「命令面合并」线在途内容，7 个测试文件与 `ws/*` 是认证记录下沉线的撞车面。落 commit 时用
  **临时 `GIT_INDEX_FILE` + `commit-tree` + 带 old-value 的 `update-ref`（CAS）**，全程不碰工作区：
  `lib.rs` 取「HEAD blob + 自己那 3 行」，其余 21 文件的工作区 diff 经逐行归属扫描确认 100% 是本票改写；
  落完 `git reset -q -- <本票路径>` 刷真索引，对侧 59 行 / 740 行回到未暂存态继续归他们所有（已复验）。
  **一次现场事故登记**：本票 `git mv` 之后、提交之前，对侧 04:23 落了 `727041862`，其提交过程把索引里
  我那条 staged rename 冲掉了（六文件里五个的旧路径退回「未暂存删除」、`core/` 一度变未跟踪）。
  工作区内容**零损失**（`git show HEAD:…ws/conn.rs | grep -c 'server::core::'` = 0 证明我的改写也没被卷进他们的 commit），
  重新 `git add` 新路径即恢复 rename 识别。教训：共享 worktree 里 `git mv` 之后不要长时间把 staged 状态
  留在索引里，**要么尽快提交，要么按本票一样从工作区重建**。
- **门禁数字**：`cargo check --lib --tests` 0 error / 本票文件 0 告警；`cargo test` 1159 通过 0 失败 `[skip]`=0
  （提交后又复跑一次，同为 1159/0）；`cargo doc --no-deps` 断链 29 → 28（零新增）；
  rustfmt 只对被路径改写顶破 `max_width` 的三行手工折行（`core/link_crypto.rs` 的 use、
  `middleware/http_filter.rs` 的 use 顺序、`ws/endpoint.rs` 的 assert_eq），其余 rustfmt 偏离经行号比对
  确认是既有（`ws/conn.rs:697`、`tests/pty_session_chain.rs:86`、`host_impl/auth.rs` 8 处、
  `tests/ws_auth_rules.rs:69`），**未整文件格式化**；测试后无残留进程与监听端口。
