# 票 13 · 会话控制客户端迁插件（`session/http.rs` + `commands/session.rs` → `com.bedcode.terminal-session`）

> Status: **in-progress**（2026-10-08 实施）
> 前置：票 12（`host-connection.primary-target` / `host-websocket` 客户端域 / `plugins/terminal-session/` 骨架）、票 14B（`host-auth`）、票 16（任务域并入）。
> 全部事实取自 2026-10-08 工作区实测（行号 / 消费者清单可复现），不凭记忆。

## 1. 目标（spec 原文 + 实测校准）

spec 票 13 原文：「`session/http.rs` + `commands/session.rs` 迁入插件（list/start/stop/remove/input 经插件自有 HTTP 面，对齐桌面 `sessions_http` 模式）；JWT 注入语义保持（认证链路只走既有 auth 模块）。」

**实测校准（动手前消费者盘点）**：

| 对象 | 现状 | 实测消费者 | 结论 |
| --- | --- | --- | --- |
| `session/http.rs::SessionHttpClient` | 宿主 Rust HTTP 客户端（list/start/stop/remove/input 五方法，JWT Bearer 注入） | ① `session.rs::SessionManager`（三命令路径）② `host_impl/terminal.rs`（`host-terminal.send`，`terminal:input` 面） | 整体退役；② 内联解耦（票 15 退役整面） |
| `commands/session.rs`（`ws_start_session` / `ws_stop_session` / `ws_remove_session`） | 三命令经 `SessionManager` | **零调用**：全仓仅 4 文件命中（定义 / `lib.rs` 注册 / `commands.rs` re-export / `session.rs` 注释）——前端票 04 后改走 `http_request`，插件不走 Tauri 命令 | 死命令，随文件退役 |
| `session.rs::SessionManager` | 本地簿记（`active_session` / `sessions`）+ 会话名生成 | ① 三命令 ② `ws_disconnect` 的「停活跃会话」分支 | 退役：`active_session` 只在死命令路径写入 ⇒ `ws_disconnect` 分支恒 None（死逻辑，B2/B3 命中） |
| 前端会话控制真消费者 | `useMobileConnection`（loadActiveSessions / startSession / stopSession / removeSession）+ `SessionsView.vue`（stop/remove）+ `plugin/context.ts::session.list` | 经 `useHttpApi`（`http_request` = 宿主 HTTP 代理） | 改走插件命令面（否则插件实现无消费者） |
| 前端 input 真消费者 | `useTuiCompat`（TUI 滚轮）+ `plugin/context.ts::terminal.sendInput` + `useMobileConnection.sendInput` | 经 `useHttpApi::httpSendSessionInput` | 随本票迁插件（`send-http-input`，见 §3） |
| `host_impl/terminal.rs`（`host-terminal.send`） | 唯一 `SessionHttpClient` 残留消费者 | **零插件消费**（票 12 §2.6 / 票 15 §1.1 双实测） | 只解除依赖（内联直发），整面退役留票 15 |

## 2. 判据自查（ADR 0022）

- `SessionHttpClient` = **B4**（把 HTTP 结果翻译成产品 wire 形状）+ 会话控制语义 ⇒ 迁插件。
- `SessionManager` 本地簿记 = **B2/B3**（活跃会话状态机 / 业务真源）⇒ 退役（零实际消费者，非「下沉」——没有可迁往的对等物，桌面端真源早已是 HTTP + 事件）。
- 三命令 = **B2**（会话启停编排命令面）⇒ 退役。
- `ws_disconnect` 的「停活跃会话」= **B2**（宿主替上层决定断开时停会话）⇒ 删分支（恒 None 死逻辑；真编排随插件）。
- `PluginLifecycleEvent::SessionCreated/SessionStopped` 的 `session.rs:103/148` 发射点 = **B6**（宿主解释产品事件主动回调插件）⇒ 随 `SessionManager` 退役；其余 dispatch 点（connection/auth/router）不属本票。
- 宿主新增：仅 `execute_http_request` 的 `jwtAuth` 字段（**传输面认证代发**，对齐票 12 `host-websocket.jwt-auth` 先例——token 不落插件，C4）。
- `host_impl/terminal.rs` 内联 HTTP：**过渡期薄壳**（既有 WIT 面的等价实现，零语义变化），票 15 退役整面。

## 3. 范围裁决（硬边界，偏差点名）

| 项 | 处置 | 理由 |
| --- | --- | --- |
| list / start / stop / remove | **迁插件**（4 命令） | spec 原文 |
| **input（HTTP 路径）** | **迁插件**（`send-http-input`） | spec 原文点名 input；前端 `httpSendSessionInput` 三消费者全路过宿主代理 ⇒ 收进插件；与票 12 的 WS 帧 `send-input` 是**两条覆盖不同场景的通道**（WS 依赖订阅连接；HTTP 绕过订阅、直发），双路保持、互不替代 |
| **resize** | **不迁**（留 `http_request`） | spec 未点名；票 15 §1.3 已定「终端 UI 的 resize 经 `mobileApi.httpRequest`」——越票改票 15 既定设计禁止 |
| `client.resize`（SessionHttpClient 无此方法） | — | 现状 resize 走前端 `httpResizeSession` 直调，不经宿主客户端 |
| `session.list` / `provider.session` 的 `context.session.list()` | **改走插件命令**（`session:read` 权限判定不变） | 会话查询产品语义收进插件；`context.terminal.sendInput` 同批改走 `send-http-input`（票 15 退役 TerminalAPI 时随删） |
| `host-terminal.send` 整面 | **不退役**（只解耦） | 属票 15 阶段 B（ABI 16→17），越票禁止；本票只把 `host_impl/terminal.rs` 的 HTTP 发送内联（不再依赖 `SessionHttpClient`） |

## 4. 落地面

### 4.1 插件（`plugins/terminal-session/`）

**新域 `rust/src/session.rs`**（会话控制域，与 terminal / auth / task 域并列）：

- 通道：`host-http.fetch`（`network:http` 权限），base URL 来自 `host-connection.primary-target`（票 12 已落，零新 WIT）。
- **JWT 注入**：请求 JSON 带 `jwtAuth: true` → 宿主代注 `Authorization: Bearer <global token>`（token 不落插件，C4；对齐票 12 `host-websocket.jwt-auth`）。
- 纯函数（native 可测）：
  - `build_request(method, url, body)` → `{method, url, headers:{Content-Type: application/json}, body?, jwtAuth: true}`
  - `classify_response(status, body)` → `ResponseOutcome::Envelope({code,message,data?}) | HttpStatusError {code: status, message} | MalformedBody`
  - 命令参数校验（`configId` / `sessionId` 必填，缺失显性 Err）
- 命令面（5 条，`contributes.commands` 由 manifest-gen 自动登记）：

| 命令 | 桌面端点 | wire 形状（与桌面 HTTP 契约逐字一致，零变化） |
| --- | --- | --- |
| `terminal-session.list-sessions` | `GET /api/sessions` | 透传 `{code, message, data:{sessions}}` |
| `terminal-session.start-session {configId, cols?, rows?}` | `POST /api/sessions/start` | body `{configId, cols, rows}`（camelCase）；透传 `{code,message,data:{sessionId,status}}` |
| `terminal-session.stop-session {sessionId}` | `POST /api/sessions/{id}/stop` | 透传 `{code,message}` |
| `terminal-session.remove-session {sessionId}` | `DELETE /api/sessions/{id}/remove` | 透传 `{code,message}` |
| `terminal-session.send-http-input {sessionId, data, specialKey?}` | `POST /api/sessions/{id}/input` | body `{data, specialKey}`；透传 `{code,message}` |

- 失败语义（对齐前端 `useHttpApi` 现状，调用点零改动）：
  - 非 2xx → `Ok({code: status, message: "HTTP {status}"})`（现状 `code: resp.status`）；
  - 业务码（200 + code!=0）→ 原样透传 `{code, message}`（前端按 code 分支）；
  - 非法 JSON body → `{code: -1, message: "invalid response JSON..."}`；
  - 网络故障 / 无 target / 权限拒绝 → 命令 Err（前端 wrapper catch → `{code:-1,message}`）。

**plugin.json**：`permissions` 增 `network:http`（Rust host 调用 `http_fetch` 亦会被 manifest-gen 幂等推导，手工同步避免首轮构建漂移）；`contributes.commands` 增 5 条。

### 4.2 宿主（`bedcode-mobile/src-tauri/`）

| # | 文件 | 改动 |
| --- | --- | --- |
| 1 | `src/session/http.rs` | **删除**（`SessionHttpClient` + `parse_session_list` / `parse_start_session_id` / `parse_ok_envelope` / `session_base_url`/`format_base_url` 别名 + 3 个结构锁测试——锁迁 §4.2#7） |
| 2 | `src/session.rs` | **删除**（`SessionManager` / `SessionInfo` / `SESSION_MANAGER` 构造面）；`src/lib.rs` 去 `pub mod session;` |
| 3 | `src/commands/session.rs` | **删除**（三死命令 + `StartSessionResponse`）；`src/commands.rs` 去 `pub mod session;` 与 re-export |
| 4 | `src/state.rs` | 删 `SESSION_MANAGER` 单例 + `get_session_manager`（imports 同步） |
| 5 | `src/commands/connection.rs` | `ws_disconnect` 删「停活跃会话」死分支（连同 `get_session_manager` import） |
| 6 | `src/plugin/wasm_runtime/host_impl/terminal.rs` | 解除 `SessionHttpClient` 依赖：内联直发 `POST /api/sessions/{id}/input`（`no_proxy` + Bearer + `timeouts::SESSION_CONTROL` + 信封解析），行为等价；注释点名票 15 退役整面 |
| 7 | `src/plugin/wasm_host.rs`（`execute_http_request`） | 增 `jwtAuth?: bool` 支持：`true` → `bearer_auth(get_global_token())`；**token 为空 → 显性 Err**（fail-visible，禁匿名代发） |
| 8 | `src/lib.rs` | `invoke_handler!` 注销 3 项（`ws_start_session` / `ws_stop_session` / `ws_remove_session`）；模块声明同步 |
| 9 | `src/enums/control.rs` / `src/connection/request.rs` 等 | 注释里的 `session::http` 指针更新（文档字面 ≠ 事实：改引用） |
| 10 | `src/system/constants/terminal.rs` | `SESSION_NAME_ID_PREFIX_LEN` 零消费者 ⇒ 删（消费者唯一是已删的 `SessionManager`） |
| 11 | `tests/session_http_flow.rs` | 改造：不再引用已删 crate 项——改为「宿主 `execute_http_request(jwtAuth)` + mock actix 桌面 HTTP」的 wire 形状 / JWT 头 / 错误映射测试（会话控制的真实组件闭环见 §4.3） |
| 12 | `tests/retired_mobile_session_control_face_lock.rs`（新，4 例） | 见 §4.3 |

### 4.3 新锁与结构锁迁移（`retired_mobile_session_control_face_lock.rs`）

- L1 全 `src/**/*.rs` 零退役符号（`SessionHttpClient` / `SessionManager` / `session_base_url` / `ws_start_session` / `ws_stop_session` / `ws_remove_session` / `parse_ok_envelope` / `parse_session_list`，跳过纯注释行 + 锁文件自身）；**迁入**原 `session/http.rs` 两个全 src 结构锁（旧信封命令名零残留 / WS 帧级加密零残留）+ `migrated_control_plane_has_no_envelope_usage` 收缩到存续文件（`host_impl/terminal.rs`）。
- L2 `invoke_handler(` 真实块零 3 个退役注册项（判据写 `invoke_handler(` / `generate_handler![`——票 14 教训）。
- L3 宿主前端 `src/**` 零退役函数字面量（`httpListSessions` / `httpStartSession` / `httpStopSession` / `httpRemoveSession` / `httpSendSessionInput` / `ws_start_session` 等）。
- L4 反向断言（新面在场）：插件 `session.rs` 五命令字面量 + `plugin.json` commands/permissions（`network:http`）+ 插件 `http_fetch` 调用 + 宿主 `jwtAuth` 判据 + 前端 `sessionCommands.ts` 封装调用 4 字面量。
- 变异自检（旁路 → 转红 → 还原）≥4 例。

### 4.4 前端（`bedcode-mobile/src/`）

| # | 文件 | 改动 |
| --- | --- | --- |
| 1 | `src/plugin/sessionCommands.ts`（新） | 会话控制命令面封装（5 函数：`listSessions` / `startSession` / `stopSession` / `removeSession` / `sendHttpInput`），返回 `ApiResult` 形状（与 `useHttpApi` 逐字段一致：成功透传 / 插件 Err → `{code:-1,message}` catch 归一）；复用 `pluginInvoke` + `TERMINAL_PLUGIN_ID` |
| 2 | `src/composables/useHttpApi.ts` | 删 `httpListSessions` / `httpStartSession` / `httpStopSession` / `httpRemoveSession` / `httpSendSessionInput` 五函数 + `useHttpApi()` 返回表对应项（**保留** `httpResizeSession` / `httpListConfigs` 等） |
| 3 | `src/composables/useMobileConnection.ts` | 4 处会话控制 + `sendInput` 改 import `sessionCommands`（函数体一行换调用，返回语义不变） |
| 4 | `src/views/SessionsView.vue` | stop / remove 两处 import 换 `sessionCommands` |
| 5 | `src/plugin/context.ts` | `session.list()` / `terminal.sendInput()` 改走 `sessionCommands`（权限判定 `requirePermission` 不变） |
| 6 | `src/__tests__/integration/session-flow.test.ts` + `src/__tests__/plugin/pluginContextHttp.test.ts` | mock 面从 `http_request` 换 `plugin_invoke`；断言改插件命令形状（permission 判定断言保留） |

i18n：零新增用户可见文案（命令面无 UI 文案）；若触发双语同步要求则写明理由。

## 5. 同步点清单（漏了会怎样）

| # | 文件 | 改动 | 漏了会怎样 |
| --- | --- | --- | --- |
| 1 | 插件 `plugin.json` | permissions + `network:http`；commands +5 | 权限门拒绝（host fn fail-closed） |
| 2 | 插件 `rust/src/session.rs` + `commands.rs` + `lib.rs` | 新域 + 分派 + `mod session` | 命令未知 |
| 3 | 宿主 `execute_http_request` | `jwtAuth` 分支 | 桌面 401（无 Bearer） |
| 4 | 宿主 `host_impl/terminal.rs` | 内联 HTTP | 编译红（引用已删客户端） |
| 5 | 前端 `sessionCommands.ts` | 新封装 | 编译红 |
| 6 | 打包产物 | `pnpm run plugins:build -- --plugin com.bedcode.terminal-session` | APK 内旧产物（无新命令） |

ABI：**零变更**（`host-http.fetch` 是 config JSON 兼容增强，WIT/ABI 不动；v16 产物照常加载）。
跨端协议：**零 wire 变更**（五个端点请求/响应形状逐字保持，仅发起方从宿主 Rust 换为插件 host-http——宿主代执行）。

## 6. 门禁（AGENTS §10 两段式）

- 插件 native：`cd plugins/terminal-session/rust && cargo test`（session 域纯函数 + 既有 40 例回归）。
- 宿主：`cargo test`（新锁 4 例 + 改造后的 `session_http_flow` + 全量集成目标；lib 6 处 `egress.rs` 在途基线照旧）。
- **真实组件闭环**：`component.rs` 新增「session 域 + host-http + jwtAuth + mock 桌面 HTTP」端到端用例（真实 WASM 组件 → 插件 session 域 → 宿主 host-http 执行 → mock actix 桌面断言 wire/JWT）。
- 前端：`pnpm run test:run` 全量 + 根 `pnpm exec eslint .` 0 error。
- wasm32 真门禁：`pnpm run plugins:build -- --plugin com.bedcode.terminal-session`（产物刷新进 `resources/plugins/mobile/`）。
- 变异自检 ≥4 例（旁路 → 转红 → 还原）。
- `cross-end-tests`：不跑（零 wire 变更，同票 12/14/16 口径；留票 21 全量）。
- 真机：会话列表 / 起停删 / TUI 滚轮输入，留票 21。

## 7. 风险与偏差

| 风险 / 偏差 | 说明 |
| --- | --- |
| input 双通道并存 | WS 帧 `send-input`（票 12）与 HTTP `send-http-input`（本票）语义不同（订阅依赖 vs 直发）；前端按场景各走一条，文档点名 |
| resize 未迁 | 尊重票 15 §1.3 既定设计（终端 UI 经 `mobileApi.httpRequest`）；本票点名 |
| 插件未激活时会话控制不可用 | 内置插件默认启用；对齐桌面「插件未激活时前端命令面显性报错」口径（spec §7 风险表） |
| `host-terminal.send` 内联实现 | 过渡期残留（零消费 + 票 15 退役），注释点名活不过一票 |
| 结构锁迁移 | 原 `session/http.rs` 内 3 个全 src 结构锁迁入新锁文件，覆盖不缩水（变异保留） |

## 8. 实施记录（2026-10-08）

### 8.1 落地清单（对照 §4）

**插件（`plugins/terminal-session/`）**：
- 新增 `rust/src/session.rs`（会话控制域）：`http_call`（host-http + `read_primary_target`）+
  五命令（`list_sessions` / `start_session` / `stop_session` / `remove_session` / `send_http_input`）+
  纯函数锚（`build_request` / `session_path` / `build_start_body` / `build_input_body` /
  `classify_http_response`）+ native 单测 10 例
- `rust/src/commands.rs`：分派 5 命令（会话控制域段落）；`rust/src/lib.rs`：`mod session`
- `plugin.json`：permissions +`network:http`（manifest-gen 构建幂等核对一致）+ contributes.commands 5 条

**宿主（`src-tauri/`）**：
- 删 `src/session.rs`（`SessionManager` / `SessionInfo`）+ `src/session/http.rs`（`SessionHttpClient`
  + 解析函数 + 3 结构锁测试）+ `src/commands/session.rs`（三死命令）
- `src/state.rs`：删 `SESSION_MANAGER` 单例 + `get_session_manager`；`src/commands/connection.rs`：
  `ws_disconnect` 删「停活跃会话」死分支（原分支恒 None）；`src/commands.rs` / `src/lib.rs`：
  模块声明与 3 注册项注销
- `src/plugin/wasm_host.rs`：`execute_http_request` 增 `jwtAuth`（`resolve_jwt_auth_header`
  三向纯函数：关=不注入 / 开+有 token=注入 / 开+空 token=显性 Err）+ 单测 2 例（三向 + 端到端 Bearer）
- `src/plugin/wasm_runtime/host_impl/terminal.rs`：解除 `SessionHttpClient` 依赖（内联直发
  input，行为等价；注释点名票 15 退役整面）；`host_impl/http.rs` 补权限门 fail-closed 单测 1 例
- `src/enums/control.rs` / `src/connection/request.rs`：`session::http` 失效指针更新；
  `src/system/constants/terminal.rs`：`SESSION_NAME_ID_PREFIX_LEN`（唯一消费者已删）删除
- `tests/session_http_flow.rs`：改造为「宿主 host-http 执行器 + mock actix 桌面」wire 契约测试
  （四端点形状 / Bearer 注入 / 业务码与非 2xx 透出形态，四场景单入口串行）
- `tests/retired_mobile_session_control_face_lock.rs`（新，7 例）：退役符号 12 needle /
  注册面零 3 项 / 前端零退役字面量 / 新面在场 + **结构锁随迁**（原 session/http.rs 的旧信封
  命令名零残留 / WS 帧级加密零残留 / 控制面无信封引用——收缩到存续的 `host_impl/terminal.rs`）

**前端（`src/`）**：
- 新增 `plugin/sessionCommands.ts`（5 函数，`ApiResult` 形状与退役前逐字段一致，Err → `{code:-1}` 归一）
- `composables/useHttpApi.ts`：删 5 会话函数（保留 resize——票 15 口径）；`useMobileConnection.ts`
  5 处 / `views/SessionsView.vue` 2 处 / `composables/usePresetTasks.ts` 2 处 / `plugin/context.ts`
  2 处（`session.list` / `terminal.sendInput`）换 `sessionCommands`；`useMobileCommands.ts` 注释更新
- 测试改造：`integration/session-flow.test.ts`（mock 面 http_request → plugin_invoke，断言改插件
  命令形状 + 「会话控制零 http_request」反断言）、`integration/connection-flow.test.ts`（3 用例：
  事件通道就绪对账 / sendInput 成功与业务错误 → 插件命令面）、`plugin/pluginContextHttp.test.ts`
  （mock `@/plugin/sessionCommands`，权限判定断言保留）

### 8.2 偏差记账（不隐藏）

1. **并行会话碰撞（票 15 阶段 A 同日实施）**：终端 UI 域已由并行会话迁入插件前端
   （`src/terminal/**` 大量迁移 + `terminal/api.ts` 的 resize/input 经 `mobileApi.httpRequest`）。
   本票据此调整：① 插件前端终端域的 TUI 输入与 resize 保留 `mobileApi.httpRequest` 路径
   （票 15 既定设计，不碰在途）；② 宿主前端 `httpResizeSession` 已零消费者，保留现状（票 15
   收口时处置）；③ `connection-flow.test.ts` / `useMobileCommands.ts` / code-map 为共享文件，
   按「写前重读 + 写后复查」操作，变异自检后 git diff 复核零漂移。
2. **真实组件闭环未做**：夹具组件（`plugin-component-test`）无法承载真实插件 session 域代码
   （跨 crate），端到端由三层拼接覆盖：宿主执行器 wire 测试（真实 reqwest + mock 桌面 HTTP，
   含 jwtAuth 注入）+ 插件纯函数单测（请求构造/响应分类）+ 权限门/egress 单测；真机（列表 /
   起停删 / TUI 滚轮）留票 21。
3. `session-flow.test.ts` 与 `connection-flow.test.ts` 的 mock 面换插件命令后，
   `useHttpApi` 的 `setApiBaseUrl` 前置不再是会话用例依赖（插件命令经宿主端解析 base）。
4. 既有 `keys.rs` 2 个 dead_code warning 为票 12 在途遗留，非本票引入。
5. **票 15 阶段 B 交叉（实施末端发现）**：本票收尾时并行会话开始实施票 15 阶段 B
   （host-terminal / terminal-hooks 整面退役，ABI 16→17），删除了 `host_impl/terminal.rs`。
   本票锁已适配：① KEEP_HOST_FACE 移除 `host_impl/terminal.rs` 条目（该面归票 15 锁）；
   ② 随迁结构锁 `migrated_control_plane_has_no_envelope_usage`（目标即该文件）随之移除
   （锁对象消亡，信封零残留由另两把全 src 扫描锁覆盖）。适配后锁验证因宿主编译处于
   并行在途态（SDK `terminal.rs` 删除 vs `traits.rs` 引用未同步的 E0432 中间态）暂未复跑，
   待票 15 阶段 B 编译收敛后由其会话或后续会话复跑（锁文件独立于并行改动面）。

## 9. 门禁结果（2026-10-08 实测）

| 门禁 | 结果 |
| --- | --- |
| 插件 native `cargo test`（terminal-session/rust） | ✅ **50 passed / 0 failed**（既有 40 + session 域 10） |
| **wasm32 真门禁**（`pnpm run plugins:build -- --plugin com.bedcode.terminal-session`） | ✅ 组件化产物刷新（wasm 522 KB / index.js 911 KB）入 `resources/plugins/mobile/com.bedcode.terminal-session/`；manifest-gen 权限并集核对一致（`network:http` 在场）；2 warning 为 keys.rs 既有 |
| 宿主定向（新锁 + wire 测试） | ✅ 新锁 **7/7** + `session_http_flow` **1/1** |
| 宿主 `cargo test --no-fail-fast` 全量 | ✅ lib **314 passed / 6 failed**（6 个全为 `egress.rs` 在途基线，与票 12/14/16 同数同款；passed 数较票 14 的 383 下降系票 15 并行迁移终端域测试所致）+ **16 个集成 target 全绿**（含本票新锁、三把既有票锁、`ws_protocol_integration`、`mock_plugin_ws_fixture` 等） |
| 前端定向（3 个改造文件） | ✅ 31/31（session-flow / connection-flow / pluginContextHttp） |
| 前端全量 `pnpm run test:run` | ✅ **66 文件 / 720 用例全绿**（含票 15 并行迁移的插件终端域 20 文件） |
| 根 `pnpm exec eslint .` | ✅ **0 error**（110 warning 不计入门禁，均为既有 / 票 15 在途） |
| **新锁变异自检** | ✅ **4/4**（① state.rs 注入退役符号字面量 → 锁1 红 ② stub `commands/session.rs` + 注册回接 → 锁1+锁2 红 ③ `sessionCommands.ts` 换退役函数名 → 锁3+锁4 红（首版注释含 needle 使锁4 免疫已修正）④ plugin.json 删 `network:http` → 锁4 红；全部还原后复跑 7/7 绿 + git diff 零漂移。**末端适配**：票 15 阶段 B 删除 `host_impl/terminal.rs` 后锁降为 6 例（KEEP_HOST_FACE 移除该条 + 信封引用结构锁移除），复跑因宿主编译处于并行在途态暂缓——见 §8.2#5） |
| ABI / 跨端协议 | ✅ 零变更（WIT 不动，v16 产物照常加载；五端点 wire 逐字保持，仅发起方宿主 Rust → 插件 host-http） |
| `cross-end-tests` | ⚠️ 未跑（零 wire 变更，同票 12/14/16 口径；列票 21 全量回归） |
| 真机 | ⚠️ 未跑（会话列表 / 起停删 / TUI 滚轮输入，列票 21 验收） |
| 文档联动 | ✅ 本票 + spec 状态行 + 移动端 code-map（终端链路段会话控制域条目 + 前端命令面描述 + 锁索引）+ 双语 CHANGELOG |
