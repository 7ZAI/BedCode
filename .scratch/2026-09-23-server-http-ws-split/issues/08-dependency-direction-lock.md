# 08: 钉死「http 与 websocket 零互依」结构锁 + 真机六链路冒烟

**What to build:** 把票 06/07 达成的分层从「此刻成立」变成「改坏了会红」。一条测试断言三件事：两个传输面双向零 import、旧路径不得复发、`core` 侧认识传输面的文件**有且只有一个**（`core/app.rs`）且其引用清单不膨胀。再加一次真机端到端冒烟，证明整个搬移对用户可见行为零影响——这是九票里唯一无法由编译器担保的部分。

**Blocked by:** 07（分层最终形状落地才有东西可锁）。

**Status:** done（08a 锁面 2026-09-23，见 Comments；六链路冒烟拆至 [`08b-machine-smoke.md`](08b-machine-smoke.md)，ready-for-human）

> **拆票记录（按票面 Comments 预案 + 用户裁决）**：本票拆为 **08a**（依赖方向结构锁，agent 可做，已完成）+ **08b**（真机六链路冒烟，ready-for-human，见 `08b-machine-smoke.md`）。08a 已把本机可独立完成的 HTTP 子集（health / terminal-bg 404 / sessions 401 / configs 401）跑进 Comments 作部分证据；需真机配对、加密开关双跑、插件 WS 的链路归 08b。

## 锁的实现口径

放在 `server/websocket.rs` 的 `#[cfg(test)]`（一处锁两侧）。手法沿用本仓自家先例——`gateway.rs` 的两条路由锁与 `wasm_runtime/tests/pty_e2e.rs` 都用 `include_str!` 扫自身源码，源码文本断言在本案是既有做法，不是新发明。

- 清单**必须动态枚举**：用 `env!("CARGO_MANIFEST_DIR")` + `std::fs::read_dir` 递归取 `src/server/http/**/*.rs` 与 `src/server/websocket/**/*.rs`。硬编码文件清单会让下一个新增文件绕过锁——那等于锁了个寂寞。
- 断言 A：`http` 侧任一文件正文不含 `server::websocket`；`websocket` 侧任一文件不含 `server::http`。
- 断言 B：两侧都不含旧路径复发形态 `crate::server::{controllers,dtos,gateway,middleware,services,ws,app,message,connection_types,filter,metrics,link_crypto,supervisor,port_checker}::`（这些现在都必须带 `core::` / `http::` / `websocket::` 中间层）。
- 断言 C：`core/` 侧除 `core/app.rs` 外不含 `server::http::` / `server::websocket::`；`core/app.rs` 的传输面引用**逐条枚举比对**（白名单 = 两侧 `configure_routes` + `http::middleware::http_filter::TrafficFilter`），多一条即红——这是把 D7 的 I3 豁免钉成可审计的清单，而不是「core 可以随便引」。
- 失败消息必须打印 `文件:行` 与命中内容，让下一个人在不读票的情况下能自己定位。

## 变异自检（这条锁自己的价值证明）

- [ ] 临时在 `http/gateway.rs` 加一行 `use crate::server::websocket::message::Message;` → 断言 A 必须红；撤销
- [ ] 临时在 `core/metrics.rs` 加一行 `use crate::server::http::dtos::ApiResponse;` → 断言 C 必须红；撤销
- [ ] 在 `http/` 里新建一个空临时文件，确认它**被动态枚举覆盖**（否则清单是假的）；撤销
- 三段变异结果贴进 Comments，无变异证据不算过

## 真机六链路冒烟（用户已裁决归本票，不单开 ready-for-human 票）

桌面端起 `pnpm run tauri:dev`，同局域网移动端/浏览器逐条验：

- [ ] `GET /api/health` 返回 `{status,port,uptime_secs}`（组合物 bootstrap 完好）
- [ ] `GET /static/terminal-bg` 设过背景图时返回图片、未设时 404（公开 HTTP 端点不经 JWT）
- [ ] `GET /api/sessions` 带 JWT 正常、不带 401（`/api` scope 与验签中间件层级未变）
- [ ] 移动端配对后 `/ws/event` 连接建立、设备在线判定与同步广播正常（事件通道 + 首消息认证）
- [ ] `/ws/terminal/session/{id}` 输出帧可见、resize / 输入可达（终端链路 + TB v3 二进制帧未变）
- [ ] 一条插件 WS 端点 `/ws/plugin/com.bedcode.terminal-session/{path}` 连接与收发正常，且未注册路径 404、超上限 503（票 07 的拒绝时序未变）
- [ ] **链路加密开关开/关各跑一遍**上述 WS 两条（`filter` / `link_crypto` 进了 `core/`，两域共用责任链是这次搬移里最容易被静默改坏的共享面）
- [ ] 一条业务别名 `GET /api/configs`（PluginRequired 档）在插件激活时转发、未激活时明确报错——顺带证明票 01/04/05/07 四次重指向后的网关锁仍在真干活
- [ ] 冒烟结果与首次实测日期写进 Comments；跑完清理测试起的后台进程与监听端口（AGENTS §3）

## 验收（代码面）

- [x] 锁测试命名清晰（如 `http_and_websocket_are_independent`），`cargo test http_and_websocket_are_independent` 绿
- [x] 三段变异自检结果齐备
- [x] 锁文件本身无反向依赖问题（`websocket.rs` 里的测试不 import `http`，走文本扫描——若为省事改成类型系统引用，等于自毁断言 A）
- [x] `cargo check --lib --tests` 0 warning（本票文件）；`cargo test` 全量绿（产物重出、`[skip]=0`）
- [x] 零前端改动 → `pnpm run test:run` / `pnpm exec eslint .` 不适用，在 commit message 里写明理由

## 归属与真源

来源：`../spec.md` §2 D7、§6.1、§7 的六条链路清单与「测试后清理进程」。真机门禁历史上就挂在 `.scratch/2026-09-18-ws-base-service/spec.md` 的 §6 未收口项，本票把它做实。

## Comments

- 2026-09-23 立项：拆票时用户裁决「真机冒烟归进本票」，不单开 human 票。若冒烟需等真机环境，把本票拆成 08a（锁，agent 可做）+ 08b（冒烟，ready-for-human）并在 Comments 记录。
- 2026-09-23 **08a done**（锁面，本票 Status 对应锁；冒烟半张票拆 08b）：

  **1. 实现**——锁落在入口文件 `server/websocket.rs` 的 `#[cfg(test)] mod dependency_direction_lock`（扫描范围是 `websocket/` **目录**，入口在目录外，锁正文里的 `"server::http"` 字面量不会被断言扫回自锁）。动态枚举 + 三断言 + **非空哨兵**（各面文件数下限 + 已知哨兵文件，防空转恒真）+ 白名单**双向比对**（多一条红、死条目也红）。附 `face_path_extraction_follows_uppercase_stop_and_boundaries` 钉提取规则（大写段截断 / `http_filter` 粘连边界 / UTF-8 不 panic）。

  **2. 与票面断言 C 的偏离（用户裁决：扩白名单钉已知豁免）**——票面字面「`core/` 除 `app.rs` 外零引用」落地即红，因两处**先于本票存在**：
  - `core/supervisor.rs` 5 处活代码 `crate::server::websocket::{WebSocketManager,ServerEvent,registry::WsSessionRegistry}`——spec 病灶 4 / §9.2 登记的 bootstrap 倒挂，本任务 move-only 明确不修。白名单按**定义项**收 3 条路径（`::Started`/`::Stopped`/`::global` 经大写段截断归并到类型名），注释挂 §9.2：抽 `core::server_runtime` 后整段删。
  - `core/filter.rs:53` rustdoc 绝对路径内链 `crate::server::http::middleware::http_filter`——票 04 修 `cargo doc` 断链故意写死，非代码 import，白名单收 1 条。
  `core/app.rs` 白名单严格按票面 3 条。断言 A/B 与「多一条即红」语义未放宽；白名单本身也是可审计清单，不是「core 可以随便引」。

  **3. 三段变异自检（贴原文）**：
  ```
  M1 http/gateway.rs + use crate::server::websocket::message::Message;
  → I1 违反：http/ 侧出现指向 websocket 面的路径…
      http/gateway.rs:46: use crate::server::websocket::message::Message;   FAILED
  M2 core/metrics.rs + use crate::server::http::dtos::ApiResponse;
  → I3 违反：core/ 传输面引用超出白名单…
      core/metrics.rs:8: 非白名单传输面引用 `crate::server::http::dtos::ApiResponse`   FAILED
  M3 新建 http/_mutation3_probe.rs（含跨面 use，非零字节——零字节文件无禁用内容，
     对 contains 型断言不可观测；探针必须带违禁行才能证明「新文件进清单」）
  → I1 违反… http/_mutation3_probe.rs:2: …   FAILED
  三段撤销后 2/2 绿。
  ```

  **4. 门禁数字**——`cargo test` **1161 通过 / 0 失败 / `[skip]=0`**（lib 1149 = 基线 1147 + 本票 2 锁测试；产物四插件 `bedcode-desktop/plugins/*/scripts/build.js` 重出）；`cargo check --lib --tests` 0 error、本票 `websocket.rs` 0 warning（lib test 总告警 41 = 票 07 基线）；`cargo doc --no-deps` unresolved **10** = 票 07 基线、零新增、无 `websocket.rs`；CRLF `websocket.rs` **550 CR / 550 lines** 纯 CRLF，`git diff --ignore-cr-at-eol` 只 +521 行新测试；测试后无残留进程/端口。

  **5. 本机 HTTP 子集冒烟（2026-09-23，debug 二进制起服 :8767）**——`GET /api/health` → 200 `{"port":8767,"status":"ok","uptime_secs":17}`；`GET /static/terminal-bg` → **404**（未设背景图，符合票面「未设时 404」）；`GET /api/sessions` 无 JWT → **401** `code:1007`；`GET /api/configs` 无 JWT → **401**（PluginRequired 档验签在网关前，符合「网关挂验签之后」）。**剩余链路（WS 三条 + 加密双跑 + configs 带插件激活正向）拆 08b**。GTK 子进程按票 07 坑清干净（`kill -9` + 端口复核 free）。

  **6. 零前端改动**——本票 diff 仅 `src/server/websocket.rs`（+521 测试）+ 本 `.scratch` 票据文档；`pnpm run test:run` / `pnpm exec eslint .` 不适用，理由写入 commit message。工作区另有隔壁「命令面合并」线在途的 `commands.rs`/`lib.rs`/plugin/* 改动，**本票一概不碰、不入 commit**。
