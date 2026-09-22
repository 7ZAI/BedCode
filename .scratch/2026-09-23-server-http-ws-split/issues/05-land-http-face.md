# 05: HTTP 面收进 `server/http/`

**What to build:** HTTP 那套（业务别名网关、REST 控制器、DTO、中间件）从 `server/` 根搬进 `server/http/`，`server/http.rs` 作模块入口。做完后「改 HTTP 只进这个目录」成立，且 HTTP 侧对外部 WS 模块的引用实测为零这一事实被钉死——它是三层化里**唯一一趟零跨面代价**的搬移。

**Blocked by:** 04（`core` 路径必须先定稿，否则 HTTP 侧对 `crate::server::core::*` 的引用要改两遍）。

**可与票 06 并行**，但见下面「`server.rs` 归属划分」——两票唯一会物理撞车的文件是 `server.rs`，已预先划清各改哪几行。

**Status:** done（2026-09-23，commit 25e9ba3a1 与票 06 联合交付）

## 搬移面（`git mv`，内容零改，只改 `use`）

| 旧 | 新 |
| --- | --- |
| `server/gateway.rs` | `server/http/gateway.rs` |
| `server/controllers.rs` + `controllers/{plugin,session}_controller.rs` | `server/http/` 同名同层 |
| `server/dtos.rs` + `dtos/{common,session,config,file,git}_dto.rs` | `server/http/` 同名同层（票 02 已删 `auth_dto` / `plugin_dto`） |
| `server/middleware.rs` + `middleware/{http_filter,jwt_auth}.rs` | `server/http/` 同名同层（票 02 已删 `cors.rs`） |
| —— | `server/http.rs`（新建模块入口，声明四个 `pub mod`） |

**不在本票**：`app.rs` 里的 HTTP 路由（health / terminal-bg / `/api` scope）→ `http/routes.rs` 是票 07；本票 HTTP 面**暂时没有自己的 `configure_routes`**，这是预期中间态，别顺手提前抽。

## `server.rs` 归属划分（并行前提，务必遵守）

- **票 05 只动这些行**：删除 `pub mod controllers; / pub mod dtos; / pub mod gateway; / pub mod middleware;` 四行，新增 `pub mod http;` 一行。
- **票 05 不得触碰**：`pub mod ws;`、`pub mod connection_types;`、`pub mod services;`、facade 的 `pub use` 段——它们归票 06 与票 02/03。
- **票 06 拥有 `server.rs` 的最终形态**。
- 撞车处理（沿用本项目既有做法）：若对侧正在写盘，**不 `git add -A`**、不整文件覆盖；用 plumbing（`git hash-object` / `update-index`）只提交自己那几行并刷新真索引，或按声明段各提交一次。禁止 `git checkout --` 逆向对侧未提交内容。

## 引用改写

- `server/` 内：`dtos` 9、`middleware` 5、`controllers` 3、`gateway` 1，共约 18 处；
- crate 内外部：`plugin/manager/wasm_runtime/tests/session_e2e.rs` 的 5 处 `crate::server::dtos::config_dto::*`（形状契约锚点断言）；
- **HTTP 侧自身对 `core` 的引用不改**（票 04 已定稿）：`middleware/http_filter.rs` 用 `core::filter` 与 `core::link_crypto::NEGOTIATION_HEADER`，`gateway.rs` 用 `core::*`。这两组是 I2（传输面 → core）的合法边。

## 锁跟随

`gateway.rs` 一搬家，它内部两条 `include_str!("../core/app.rs")`（票 04 落的）路径深度不变（`http/gateway.rs` 与 `server/gateway.rs` 相对 `core/app.rs` 同为两级… 实际是一级到 `server/`、再进 `core/`），**必须重新核算并当场由票 01 的自校准前置验证**。相对路径算错是编译期错误（`include_str!` 直接报），算「对但指到别的存在文件」才是前置负责抓的那一半。

## 验收

- [ ] `server/http/` 存在且含四组文件 + `http.rs` 入口；`server/` 根不再有 `gateway.rs` / `controllers*` / `dtos*` / `middleware*`；无 `mod.rs`
- [ ] `grep -rn "crate::server::\(controllers\|dtos\|gateway\|middleware\)::" src/ tests/` == 0
- [ ] **HTTP 面对 WS 模块的引用为 0**：`grep -rn "server::ws" src/server/http/` == 0（立项实测本就是 0；`EndpointAuth` 两侧都直连 SDK `bedcode_plugin_api::`，WS 侧那个只是再导出，**不要**为了「复用」而引入 HTTP→WS 边）
- [ ] `gateway.rs` 两条 `include_str!` 重指向后，票 01 的自校准前置仍绿 + 故意指错一次会红（贴变异结果）
- [ ] 中间件顺序原样保留：`/api` scope 里「网关写在验签之前」的 `Scope::wrap` 语义与那段解释注释**整块搬动、不重排、不改写**（AGENTS 与 `app.rs` 原注释都把它当硬约束）
- [ ] 17 条 `BUSINESS_ROUTES` 条目、`decide()` 判定、`FallbackPolicy` 取值**零变化**（本票 diff 里只应出现 `use` 行与路径字符串）
- [ ] 三个形状契约锚点 DTO 文件与 `session_e2e.rs` 的断言完整存活（`git diff` 里它们只该出现路径前缀变化）
- [ ] `cargo check --lib --tests` 0 warning；`cargo test` 绿（产物重出、`[skip]=0`）
- [ ] `cargo doc --no-deps` 无新增 broken link（基线来自票 04 存的输出）；`core/filter.rs` 里那条指向 `http_filter` 的绝对内链在本票后改成 `crate::server::http::middleware::http_filter`
- [ ] **CRLF 纪律**：`dtos.rs`、`dtos/{common,file,session,git,config}_dto.rs`、`middleware.rs` 都是 CRLF，`dtos/session_dto.rs` 行尾混合（cr=71/93）；只用 Edit 工具改 `use` 行，逐文件核 `tr -dc '\r' | wc -c` 等于 HEAD
- [ ] 与票 06 的 `server.rs` 分段改动核对：`git diff` 里本票对 `server.rs` 只增一行 `pub mod http;`、只删那四行

## 归属与真源

来源：`../spec.md` §3.2 HTTP 各行、§4 引用计数表、§5.1、§5.4；`server.rs` 归属划分是 2026-09-23 拆票时用户裁决（票 05/06 并行）。

## Comments

- 2026-09-23 立项。拆分策略裁决：本任务走「按目录原子进」，**不走** skill 默认的 expand–contract 兼容壳——理由是这是互不相交的分区搬移，每票自己就能编译全绿，加壳只会留下只活两三票的别名（正是票 03 要删的那个 `message.rs` 壳那种债）。


## Comments

- 2026-09-23 立项。
- 2026-09-23 done：与票 06 联合提交（用户指示「完成后再和 05 一起检测」）。验证全绿：cargo check 0 error、cargo test lib 1147 + 集成全绿 [skip]=0、cargo doc 断链全为 HEAD 基线、锁变异自检指错→红→恢复。
- 已知限制：commands.rs / lib.rs 的 ws→websocket 行在未提交的 commands 合并线文件内，本提交不暂存（随合并线提交）；本提交独立编译依赖合并线落地（与票 04 半断状态同型）。