# 01: 网关别名锁自校准前置（先让搬移变安全）

**What to build:** 那条守护「宿主业务路由不再长回来」的锁，今天在现状下**条件恒真**——把它改成「扫不到东西就红」。本票不搬任何文件、不改任何路由，只补锁；它是后续四票（04/05/06/07）每次重指向 `include_str!` 时唯一的安全网，所以必须最先落。

**Blocked by:** 无（可以立即开工）。**零撞车**：本票只动 `server/gateway.rs`，实测该文件今日干净，隔壁在途会话（`SessionConfig` 线协议退役 + 认证记录下沉）未持有它。

**Status:** done（2026-09-23 commit `ab118f41b`，dev）

## 现状（为什么这条锁今天是空的）

- 两条 `gateway.rs` 测试用 `include_str!("app.rs")` 扫宿主路由字面量，其中 `every_alias_still_has_a_host_route` 对 `FallbackPolicy::PluginRequired` 条目断言的是 `!APP_RS.contains(path)`；
- `BUSINESS_ROUTES` 现有 **17 条、全部 PluginRequired**（票 02/03/04 contract 已把宿主业务路由清零），所以该测试**只在做 17 次否定断言**——把它扫的目标换成任何一个不含路由字面量的文件，17 条全部通过，全绿；
- 这个空转窗口平时无害，但本任务（`server/` 三层化）恰好要连着四次移动 `app.rs` 及其路由体。锁一旦被指错文件而没人发现，后面谁把业务路由长回 `/api` 都不会红——正是本锁存在的唯一目的失效；
- `include_str!` 指向**不存在**的路径是编译期错误，编译器会抓；**危险的是指向一个存在但没有路由字面量的文件**，那半边只有自校准前置能抓。

## 验收

- [x] `every_alias_still_has_a_host_route` 开头加自校准前置：对扫描目标取 `configure_routes` 函数体，`grep`/字符串扫描命中的路由标识符数 **低于基线数即 `panic!`**，消息写明「锁已空转：扫描目标 X 只命中 N 条路由字面量，低于基线 M」
- [x] 基线数在本票钉为 **14**（现 `app.rs` 的 `configure_routes` 实测：12 条字面量 + `API_HEALTH_PATH` + `WS_EVENT_PATH` 两个常量标识符）。**两个常量必须纳入计数**——只匹配 `"/..."` 字面量的话，`/api/health` 与 `/ws/event` 两条整行删掉照样绿
- [x] 前置的取值范围用**函数体**而不是整文件（`awk '/^pub(\(crate\))? fn configure_routes/,/^}/'` 同形），否则 `use` 行里的标识符会混进来把基线数撑虚高
- [x] 第二条锁 `business_handlers_are_only_mounted_on_aliased_paths` 同步加「扫描目标非空」前置（它扫的是 `git_controller::` / `auth_controller::` 这两个早已退役的 handler 名，现状同样是零命中恒真）
- [x] **反向变异验证并贴出结果**：临时新建一个空的 `src/server/_lock_probe.rs`，把前置的扫描目标指过去 → 前置必须红；撤销 → 绿。不贴变异结果的「已加前置」不算过（做了三轮，见 Comments；含一轮「锁本身仍然咬得住」的正向变异）
- [x] 票 07 拆分后基线数改钉 **11**（HTTP 侧），该改动的判据与新旧清单写进票 07；本票在 Comments 里留一句提醒，防止下一个人把它当魔法数放宽成「≥1」
- [x] 本票不得改动 `BUSINESS_ROUTES` 表内容、`FallbackPolicy` 语义、`decide()` 判定或任何路由注册（diff 全部落在 `mod tests` 内：`+144 / -2`，`-2` 是两条锁各自的局部 `const APP_RS`）
- [x] `cargo test` 绿（含既有 `decide_*` 矩阵）；`cargo check --lib --tests` 0 error、本文件 0 告警（全 crate 既有告警 41/40 条无一指向 `gateway.rs`，判据见 Comments）
- [x] `gateway.rs` 行尾核一遍：本票若引入整文件重排说明误用了格式化，回退（CR 数 0 → 0，与 HEAD 一致）

## 归属与真源

来源：`../spec.md` §5.1（锁静默恒真）与 §5.2（路由多重集含常量这个坑）。行数以 spec §1.2 的实测表为准；本票开工时若与本处描述不符，以实际为准并顺手回写 spec。

## Comments

- 2026-09-23 立项：来源 spec §5.1。拆票时定为全局首票——它不属于「搬移」，而是让搬移可验证的前置 prefactor。
- 2026-09-23 done：**只动 `server/gateway.rs` 的 `mod tests`**（`+144 / -2`，`-2` 为两条锁各自的局部
  `const APP_RS`）。落法是把扫描目标收成一个取源点，前置挂在两条锁各自开头：
  `const APP_RS = include_str!("app.rs")` + `APP_RS_LABEL` + `HOST_ROUTE_IDENTIFIERS_BASELINE = 14` +
  `configure_routes_body()`（签名后首个顶格 `}` 之前，与票面 awk 同形）+
  `route_identifier_count()`（`"/…"` 字面量 + 两个常量名，与 spec §7 的 `grep -oE` 同形）+
  `assert_host_route_surface_is_live()` / `assert_host_route_surface_not_empty()`。
  另补 4 条永久用例钉前置自身（计数含常量、只取函数体、两个前置在死目标上必 `panic`）——
  否则「前置」本身又是一把只在变异验证当天有效的锁。
- **基线实测一致**：Rust 侧 `HOST_ROUTE_IDENTIFIERS_BASELINE` 与 shell 侧门禁命令取到同一个数——
  `awk '/^pub(\(crate\))? fn configure_routes/,/^}/' src/server/app.rs | grep -oE '"/[^"]*"|API_HEALTH_PATH|WS_EVENT_PATH' | wc -l` → **14**。
  别名表构成同步复核：17 条全 `PluginRequired`、`HostImplementation` 0 条（票面「只做 17 次否定断言」成立）。
- **反向变异三轮（贴原文）**：
  1. 空 `src/server/_lock_probe.rs` + 两条锁同指过去（取源点是一行，故一次改两处生效）→ 两条锁全红：
     `锁已空转：扫描目标 server/_lock_probe.rs 里没有 configure_routes 函数体`
     （`every_alias_still_has_a_host_route` FAILED / `business_handlers_are_only_mounted_on_aliased_paths` FAILED）
  2. 探针改成「有 `configure_routes`、体内只 1 条路由」→ 第一条锁红在**计数**判据上，第二条锁的前置**过**
     （它验的是「非空 + 有 `.route(`」，不是数量）——两条前置判据不同各有理由，不是重复：
     `锁已空转：扫描目标 server/_lock_probe.rs 的 configure_routes 只命中 1 条路由标识符，低于基线 14（12 条路径字面量 + API_HEALTH_PATH + WS_EVENT_PATH）`
  3. **正向变异（证明补前置没把真锁做成装饰）**：临时往 `app.rs` 的 `/api` scope 里塞回
     `.route("/configs", …)`（此时计数 15 ≥ 基线，前置放行）→ 锁本身仍红：
     `/api/configs 已 contract（PluginRequired），宿主路由必须注销（真源在插件，宿主不该再挂同路径 handler）`
     三轮的探针文件与 `app.rs` 那行均已撤销：`git diff -- server/app.rs` 空、`_lock_probe.rs` 已删。
- **门禁实跑**（对侧票 02 落定后的稳定态）：`cargo test` = lib **1147 通过 / 0 失败** + 集成与 doc-test
  合计 **1159 通过 / 0 失败**、`[skip]` 计数 **0**（插件产物为今日 03:35 重出，本票未动插件链）；
  `cargo check --lib --tests` **0 error**，`gateway.rs` 相关告警 **0** 条（全 crate 既有 41/40 条逐条指向
  `peer_net.rs` / `wasm_runtime*` / `http_filter.rs` / `session_components.rs` 等，非本票 diff）；
  `rustfmt --check src/server/gateway.rs` clean——先实测 HEAD 版该文件在**同目录**下 rustfmt-clean，
  故单文件 rustfmt 只会动我的行；行尾 CR 数 0 → 0。测试后无残留进程/监听端口。
- **给票 07 的提醒（勿当魔法数）**：`HOST_ROUTE_IDENTIFIERS_BASELINE` 是**从现 `app.rs` 实测钉出来的数**，
  不是随手写的阈值。拆分后收窄为 HTTP 侧时改钉 **11**（判据与新/旧清单已写在票 07 验收第 3 条），
  **禁止**为了「先让它绿」放宽成 `>= 1` 或 `>= 0`——那样等于把本票整把锁退回立项前的空转态。
  重指向时只改两行：`const APP_RS` 的 `include_str!` 路径 + `APP_RS_LABEL`（改名不改判据）。
- **与 spec / 票面的偏离（按票面更严实施）**：spec §5.1 写的是「命中数为 0 即 panic」，票面钉的是
  「低于基线 14 即 panic」。取票面：0 命中只是 14 判据的一个特例，而「目标文件还在、路由已被搬空」
  这种半死不活态（变异 2 那一轮）只有数量判据能抓。§5.1 的表述随票 09 文档同步时对齐即可。
- **现状漂移登记**：① spec §1.4 与 §5.1 里的 `gateway.rs:726`/`:728`/`:756`/`:758` 行号在本票后已移动
  （取源点 `const APP_RS` 现在 `:724`，两条锁各在 `:869` / `:899`，且不再是各自的 `include_str!`）——
  票 04/05/06 重指时按「搜 `include_str!("app.rs")`」定位，别按行号。② 本票开工时判定「零撞车」成立，但**票 02 由对侧
  会话先行落地**（commit `580507975`，删 7 项死码 + facade 收窄），其文件清单与 `gateway.rs` /
  `server/app.rs` 逐条不重叠，故本票直接骑在其上。③ 对侧删除动作**尚未收口的中间态**跑出过一次
  1 项失败（`cargo test` 1146 通过 / 1 失败），复跑与 `580507975` 之后均为全绿——登记以免下一个人
  误读本票的「cargo test 绿」。
- 2026-09-23 提交范围：工作区此刻有另两条线的大量在途改动（`commands.rs` / `db/` / `plugin/` 等约 200 文件），
  本票按文件归属只提交 `src/server/gateway.rs`（`ab118f41b`）+ 本票票面，**未 push**。
  **另**：本票顺带回写了 spec §5.1 的「票 01 已落」接线说明（取源点已合并、判据取基线而非「命中数为 0」），
  但 `spec.md` 与本目录其余 issue 文件此刻仍是**未跟踪**状态（仅 `issues/02` 被对侧提交），故该回写留在工作区，
  随 spec 的首次提交一起入库——票 04/05/06 开工前若看到 §5.1 与本票 Comments 不一致，以本票为准。
