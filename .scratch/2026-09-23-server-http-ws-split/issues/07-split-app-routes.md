# 07: `app.rs` 一拆为三（组合物 + 两侧 `routes.rs`）

**What to build:** 把那个「一个文件装两种传输」的熔接点拆开：`core/app.rs` 只剩单端口组合物，HTTP 的路由与公开端点进 `http/routes.rs`，WS 的三个握手处理器与帧上限进 `websocket/routes.rs`。做完后每个传输面的路由改动都只落在自己目录里，`core/app.rs` 成为全仓**唯一**被允许同时认识两面的文件——这个例外必须是显式的、可枚举的，否则零互依不变量就是假的。

**Blocked by:** 04、05、06（三面路径全部定稿后才能抽路由；提前抽会让 `use` 改写做两遍）。

**Status:** done（2026-09-23 commit `2796f9f63`，dev，见 Comments）

## 拆法

| 从 `core/app.rs` 抽出 | 落到 | 内容 |
| --- | --- | --- |
| `ws_frame_limit()` | `websocket/routes.rs` | 保持 `pub(crate)`。**D5**：它是 WS 帧上限（读 `AppConfig::network.ws_*`），寄居 `app.rs` 是历史错位 |
| `session_terminal_ws` / `event_ws` / `plugin_endpoint_ws` / `endpoint_owner_activated` | `websocket/routes.rs` | 三条 WS 握手 + 属主激活闸门 |
| `health_check` / `terminal_bg_content_type` / `terminal_bg_image` | `http/routes.rs` | 两个公开 HTTP 端点（均不经 JWT，理由在各自注释里） |
| `/api` scope 及其 `wrap` 链 | `http/routes.rs` | **整块搬、不重排**（见下） |
| 留在 `core/app.rs` | —— | `start_http_server` + `App` 级 wrap（CORS / Logger / metrics 计数 / `TrafficFilter`）+ 一个只调两侧 `configure_routes` 的组合物 |

**为什么 `app.rs` 不整个归 `http/`**：HTTP 与 WS 共用一个端口、一个 `HttpServer`，组合物天然同时认识两面。把它塞进任何一侧都会凭空造出一条 `http → websocket`（或反向）的假边。

## `core/app.rs` 的例外必须是显式白名单

拆完后 `core/app.rs` 会 import：`http::configure_routes`、`websocket::configure_routes`、`http::middleware::http_filter::TrafficFilter`。**第三条不是疏忽**——`TrafficFilter` 现状挂在 `App` 级，覆盖 WS 升级请求与公开路由；把它收进 `/api` scope 是**覆盖范围收窄的安全边界变更**，本票禁止做（spec §9.3 另票论证）。票 08 的锁据此把白名单钉成一个文件、三条引用，多一条即红。

## 三条必须原样搬走的语义

1. **`/api` 的 `wrap` 相对次序**：注册顺序 = 由内到外，`Scope::wrap` 后注册者先执行，故代码里「网关写在验签之前」。那段解释注释连同 `app.rs` 里「中间件顺序即请求顺序」的注释一起整块搬，**禁止改写或重排**——它是 `gateway.rs` 那条 `unverified_requests_never_reach_gateway` 真实栈测试之外的第二道说明。
2. **插件 WS 端点的拒绝时序**：未注册 / 属主未激活 → 404，入站连接数超限 → **协议升级前** 503（不产生连接事件）。这条时序是 host-websocket 原语契约的一部分，搬动时 `frame_size(entry.max_message_bytes)` 与各分支的返回码一字不改。
3. **两条 WS 路由共用 `ws_frame_limit()`、插件端点用端点声明值**：D5 迁走函数后，`websocket/endpoint.rs` 与 `host_impl/ws.rs` 两处消费者改指 `websocket::routes::ws_frame_limit`（面内边 + 一处外部边）。

## 验收

- [x] `core/app.rs` 只剩组合物与 bootstrap；`http/routes.rs`、`websocket/routes.rs` 各自导出一个 `configure_routes`
- [x] **路由面守恒（本票唯一的行为证明，编译器管不着）**：拆前存基线、拆后比对——
      `configure_routes` 函数体内的路由标识符多重集 **拆前 14 项 = HTTP 侧 11 + WS 侧 3**。扫描式必须同时纳入 `API_HEALTH_PATH` / `WS_EVENT_PATH` 两个常量：只匹配 `"/…"` 字面量的话，`/api/health` 整行删掉照样绿（spec §5.2 实测的这个坑是本验收的存在理由）。`diff` 结果贴进 Comments
- [x] 票 01 的自校准前置在本票**第三次重指向**（`include_str!("../core/app.rs")` → `include_str!("routes.rs")`，即 HTTP 侧那份），前置的**基线数从 14 改钉为 11** 并在票里写明判据；禁止把它放宽成「≥1」当过关
- [x] `HttpServer` 的全部参数（`bind(BIND_ADDRESS:port)` / `keep_alive` / `client_request_timeout` / `max_connections` / `backlog` / `tcp_nodelay` / `shutdown_timeout` / `workers`）逐项与拆前一致——`git diff` 里这些行不该出现
- [x] 常量归属：`API_HEALTH_PATH` / `BIND_ADDRESS` / `CORS_MAX_AGE_SECS` 留 `http/routes.rs` 与 `core/app.rs` 各自引用处；`PLACEHOLDER_PEER_ADDR` 改由 `websocket/routes.rs` 引入；**不动** `system/constants/server.rs` 本体
- [x] 三个端点常量的语义分工在票面注释里写清（`WS_EVENT_PATH` 归 WS 侧、`API_HEALTH_PATH` 归 HTTP 侧），别留在组合物里造成「core 知道具体路由」的错觉
- [x] `system/constants/plugin.rs` 里提到 `server::app::ws_frame_limit()` 的文档字眼改指新路径（票 04 Comments 挂的那条）
- [x] `cargo check --lib --tests` 0 warning；`cargo test` 绿（产物重出、`[skip]=0`；`ws_e2e` / `test_session_*` 必须真跑）
- [x] `grep -rn "crate::server::app::ws_frame_limit" src/` == 0
- [x] 桌面端起服务后 `/api/health` 手工可达（票 08 做完整六条链路冒烟，本票只需这一条证明组合物没拆坏 bootstrap）

## 归属与真源

来源：`../spec.md` §2 D4 + D5 + D7（I3 白名单）、§3.2 表首行、§3.3 接缝代码草图、§5.1、§5.2、§9.3。

## Comments

- 2026-09-23 立项：前身是 spec 旧 §6 的「C3」，拆票后独立成票。本票是全部九票里唯一带行为风险的（其余都是 move-only 或加锁），故单独成票、独立可 revert。
- 2026-09-23 done（`2796f9f63`，10 文件 +367/-305）：路由面守恒 + 锁重指向 + 手工冒烟全过，三处与票面/草图不同，按实际办理并回写如下：

  **1. 路由面守恒（diff 原文）**——spec §7 命令跑在 HEAD 的 `core/app.rs` 与拆后两侧 `routes.rs`：
  ```
  /tmp/routes_before.txt: 14     /tmp/routes_after.txt: 14
  （diff 空）===> 路由面一致
  HTTP 侧 11 / WS 侧 3（合计 14 = 拆前基线）
  ```
  HTTP 侧 11 = 10 条字面量（`/api`、`/plugin/{plugin_id}/{path:.*}`、`/sessions`、`/sessions/start`、
  `/sessions/{id}/{stop,resize,input,history,remove}`、`/static/terminal-bg`）+ `API_HEALTH_PATH`；
  WS 侧 3 = 2 条字面量（`/ws/terminal/session/{session_id}`、`/ws/plugin/{plugin_id}/{path:.*}`）+ `WS_EVENT_PATH`。

  **2. 锁第三次重指向 + 变异验证**——`include_str!("../core/app.rs")` → `include_str!("routes.rs")`（`gateway.rs` 在
  `http/`，与 `http/routes.rs` 同目录）；`APP_RS_LABEL` → `"server/http/routes.rs"`；基线 14 → 11，判据清单写死在
  常量注释（10 字面量 + `API_HEALTH_PATH`）。变异：把取源点指到**最像的错文件** `../websocket/routes.rs`
  （存在、编译过、有 configure_routes 但只 3 项）：
  ```
  thread '…every_alias_still_has_a_host_route' panicked at src/server/http/gateway.rs:784:9:
  锁已空转：扫描目标 server/http/routes.rs 的 configure_routes 只命中 3 条路由标识符，低于基线 11（10 条路径字面量 + API_HEALTH_PATH）
  test result: FAILED. 0 passed; 1 failed  → 撤销后 25/25 绿
  ```
  未放宽成「≥1」：数量判据是「文件还在、路由已被搬空」半死不活态的唯一抓手。

  **3. 与 spec §3.3 草图的三处偏离**（均按票面验收优先处理）：
  - `websocket.rs` **没有** `pub use routes::ws_frame_limit`——票面 D5 明确两个消费者（`endpoint.rs` ×2 /
    `host_impl/ws.rs` ×1）改指 `websocket::routes::ws_frame_limit` 直接路径，re-export 成死代码
    （`pub(crate) use` 会触发 unused_imports，违反 0-warning 验收）。若未来要短路径再加。
  - `http/routes.rs` 的 `business_gateway` wrap 行**保持单行**（rustfmt 想折行）——HEAD 的 `app.rs` 同一行
    在项目配置（max_width=120 + small_heuristics）下**同样不干净**，票面「整块搬、不重排」优先于格式化。
  - 顺带修 `jwt_auth.rs:77` 文档字眼：`server/core/app.rs` → `server/http/routes.rs`（/api scope 已不在组合物里）。

  **4. 手工冒烟**——真实二进制起服（端口 8767 自动选定，port file 读取）：`/api/health` →
  `{"port":8767,"status":"ok","uptime_secs":52}`；`/api/sessions` 无 JWT → **401**（/api scope wrap 链完好）。
  坑：`kill $!` 杀不掉 GTK 子进程（二进制 fork 出 `:4102087`），首次起服残留占 8767，已 `kill -9` 清理并删除
  `bedcode-port.txt`（AGENTS §3 测试后清理）。六条链路全量冒烟留给票 08。

  **5. 验证证据**——`cargo check --lib --tests` 0 error（41 条警告全为 HEAD 既有，本票文件零新增：
  `host_impl/ws.rs:81/960` 的 ClientEntry 字段 / dropped_frame_count 死码为 HEAD 状态，本票只改 :979 一个路径字符串）；
  `cargo test` lib **1147 通过 / 0 失败**，集成全绿，**`[skip]=0`**（产物 `node plugins/terminal-session/scripts/build.js`
  重出后 `test_session_*` ×7 / `ws_e2e` ×3 真跑）；`cargo doc --no-deps` 10 条 unresolved 全部落在本票未触碰文件
  （plugin/manager.rs、fs_auth.rs、task_data_migration.rs、metrics.rs、supervisor.rs、gateway.rs 未改行），零新增断链；
  `grep -rn "crate::server::app::ws_frame_limit" src/` = 0；CRLF 保持（websocket.rs 29/29、plugin.rs 193/193，
  `git diff --ignore-cr-at-eol` 只显目标行）。
