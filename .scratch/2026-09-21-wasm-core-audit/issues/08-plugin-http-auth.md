# 08: 插件 HTTP 端点的声明式认证（P1-1）

**What to build:** 插件经 HTTP 暴露的面获得与 WS 侧对等的**声明式认证策略**：端点在 manifest 声明 `auth`，未声明即默认最严；「未声明 httpEndpoints 的插件整前缀放行」这条零迁移策略退役；插件在 handler 里能拿到宿主判定的调用方身份（可信设备/会话或 `anonymous`）。

**Blocked by:** 06（调用方身份语义与前端通道一致后再定 `device` 口径）

**Status:** done（2026-09-22 关闭：none|jwt 两档、不动线协议档位；形态与兜底两处口径见「补充裁决」；实跑门禁与一次 flake 留痕见「门禁实跑」）

## 现状（已复核）

- `server/middleware/jwt_auth.rs:92-95`：`/api/plugin/**` 无 JWT 直接放行；`:9-11` 注释自陈「服务监听 0.0.0.0，局域网内任意设备均可无凭证调用已激活插件的 HTTP 端点（含写操作）」——文档诚实，但风险未处置；
- `server/controllers/plugin_controller.rs:71-80`：`declared.is_empty()` 时**前缀内全放行**（为零迁移保留，票 16 固化）；
- `plugin_controller.rs:135-138`：`/api/plugin/*` 路径 `device = None` → 插件拿不到任何调用方身份，只能自行从业务 token 判（宿主不提供原语）；
- 对照：`host-websocket` 服务端域有声明式 `auth: none | jwt`（code-map:156-157），并有首消息鉴权 + 超时 close 4001 的完整机制。

## 需裁决项

1. 默认策略翻转幅度：新插件「未声明即 401」，存量插件按 legacy 兜底到何时（现无一处遗留未声明的生产插件实现了 `_http_endpoint`——本票先核实这张表再定）；
2. `auth` 档位是否需要第三档（如 `pairing-scope` / `local-only`：仅环回可达，给 hook 脚本用），还是沿用 `none|jwt` 两档 + 环回豁免；
3. 是否给环回调用方一个可声明的独立身份（现 `link_crypto.rs:671-676` 已有环回豁免判定，但只服务链路加密，不服务授权）。

## 验收

- [x] 核实清单：桌面 4 个生产插件中哪些实现 `_http_endpoint`、各自的 `contributes.httpEndpoints` 声明状态、各自是否假设「仅本机调用」——结果写进本票 Comments，作为默认策略翻转的依据
- [x] manifest 增 HTTP 端点 `auth` 声明（与 WS 同形），宿主在 `plugin_http_endpoint` / `gateway.rs` 转发前强制；`auth` 缺失时的行为按裁决项 1
- [x] 转发给插件的参数携带宿主判定的调用方身份（可信设备 claims 派生标识或 `anonymous`），**凭据红线**：JWT 本体与指纹不透传（沿用 `PluginHttpRequest::device` 的现有约束与测试）
- [x] 未声明端点清单的插件不再「前缀内全放行」（删除 `plugin_http_path_allowed` 的 `declared.is_empty()` 短路），或把该短路限定到显式 legacy 白名单并标注退役条件
- [x] 环回/hook 场景（`claude code hook` 经 `/api/plugin/com.bedcode.session/...`，见 code-map 末尾）在收紧后仍可工作，且其身份可被插件区分
- [x] 协议红线：`/api/plugin/*` 响应形状与网关别名表行为不得破坏性变更（AGENTS §9「老端忽略未知字段」）；若加声明面，移动端 auto-task legacy 前缀（roadmap M1）的受影响状态同步登记
- [x] 门禁：`cargo test`（含 `plugin_controller` / `gateway` 双轨契约用例不回归）+ 新增认证档位用例 —— 实跑数字见「门禁」节

## Comments

- 2026-09-21 立项：来源 spec §5-P1-1。

### 核实清单（验收 1，2026-09-22 实跑，默认策略翻转的依据）

桌面 4 个生产插件（`bedcode-desktop/plugins/`）：

| 插件 | 实现 `_http_endpoint` | 声明 `contributes.httpEndpoints` | 是否假设「仅本机调用」 |
| --- | --- | --- | --- |
| `com.bedcode.terminal-session` | **是**（`task/` + `file_browse/` + `auth_http/` 三域，`lib.rs` 单一入口） | **是**（34 条） | 是——hook 面 `task-status` / `session-mode` 只由本机脚本按环回地址调用 |
| `com.bedcode.file-transfer` | 否 | 否（无该键） | — |
| `com.bedcode.agent-hub` | 否 | 否 | — |
| `com.bedcode.ai-chatbox` | 否 | 否 | — |

调用方三类，逐一核过：

1. **移动端**：`bedcode-mobile/src/composables/useHttpApi.ts` 走 `/api/plugin/com.bedcode.auto-task/*`
   （旧前缀，经别名表转给接管方）共 9 个函数；请求经 `http_request` Rust 代理，
   `should_inject_jwt` 对**除 `/api/auth/*` 之外**的全部路径注入 Bearer token →
   收紧到 `jwt` 档后移动端不受影响（这是「存量零迁移」里最大的一条，必须先确认）。
2. **本机 hook 脚本**：`plugins/terminal-session/scripts/{auto_task_hook.py, codex_task_hook.py,
   pi_task_hook.ts, opencode_task_hook.ts}` 只打两条端点——`task-status`（GET 查询 + POST 终态推送）
   与 `session-mode`（GET），全部无凭证、环回地址。**这两条必须声明 `auth: "none"`**，
   否则票 09 之后部署在项目里的 hook 全部静默失效。
3. **桌面前端**：`bedcode-desktop/src/` 内 `/api/*` HTTP 调用为**零**（全走 Tauri 命令）→
   网关别名与 `/api/plugin/*` 的收紧不会打到桌面 UI。

「未声明清单 → 前缀内 ANY 放行」这条零迁移短路的消费者清点：

- 生产插件：无（上表只有 terminal-session 有 HTTP 面且已声明）；
- 宿主测试：仅 `plugin_controller.rs` / `registry.rs` 两处单测断言过它（本轮随之翻转）；
- 集成测试（`src-tauri/tests/**`、`wasm_runtime/tests/**`）、e2e、fixture 插件
  （`packages/plugin-*-test`）：**零** 引用 `/api/plugin/`，也没有 fixture 实现 `_http_endpoint`；
- ⇒ 硬 404 在仓库内无人依赖；**仓外 zip 安装的第三方插件**若从未声明清单则会被判 404，
  按用户裁决接受（列 Breaking）。

两处「查过但不属于本票」的登记：

- `contributes.toolProviders` 与 `httpEndpoints` 共用注册表，但其 `endpoint` 是**外部 URL**
  （拼出的 `/api/plugin/<id>/http://x` 永不匹配真实请求），宿主侧 `find_http_endpoint`
  无生产调用者 → 无派发面；本票给它缺省最严档，不为它开「无 auth 字段故豁免」的口子。
- 前端 TS-only 插件的 HTTP 面（`src/plugin/registry.ts` 的 `findHttpEndpoint`）**只登记不消费**，
  没有任何宿主派发点 → 收紧不覆盖它，也不因本票变坏。
- `caller` 的插件侧消费者：terminal-session 本轮不读 `caller`（hook 面本就 `none`）；
  按身份自行收紧属插件产品决策，不在内核面。

### 补充裁决（2026-09-22 用户裁决，开工前）

4. **声明形态 = 混合条目**：`httpEndpoints: (string | { path, auth? })[]`
   ——与 WS 端点注册面 `{path, auth}` 同形；旧 `string[]` 产物照常解析（零迁移只落在形态上）。
5. **未声明 `httpEndpoints` 的插件 = 硬 404**：直接删除 `plugin_http_path_allowed` 的
   `declared.is_empty()` 短路，不留空 legacy 白名单（「禁止保留无人走的分层」优先于「留个阀门」）；
   对仓外第三方插件是破坏性变更，写进 CHANGELOG Breaking。
6. **档位词汇单点**：`none | jwt` 提升到桌面 SDK `EndpointAuth`，WS 注册面（缺省 `none`）
   与 HTTP 声明面（缺省 `jwt`）共用同一枚举 + `parse_with(raw, default)`；
   manifest-validate.js 的合法集注明真源指向该枚举（同权限词汇的处置口径）。

### 实施记录（2026-09-22）

- [x] 验收 1 核实清单：见上表。
- [x] 验收 2 声明面：SDK `rust/src/types.rs` 加 `EndpointAuth` + `HttpEndpointContribution`
      （serde untagged，两形态并存），`PluginContributes.http_endpoints` 换类型；
      SDK `src/types.ts` / 宿主 `src/plugin/types.ts` 同形；`bin/manifest-validate.js` 逐条校验
      形态 / path 判据 / `auth ∈ {none,jwt}` / 未知字段 / 跨形态重复。五同步点齐
      （打包 CLI 无 httpEndpoints 模板，故 manifest-gen 无需改；`dist/` 不入库）。
- [x] 验收 3 调用方身份：`HttpCaller{Device,Localhost,Anonymous}` + `caller_identity()`
      （claims 优先、其次环回 peer_addr、否则匿名），转发入参新增 `caller` 字段；
      `device` 语义不变（只在已验签时出现），gateway 内私有 `device_context` 收编到这一处
      （两轨共用一条判据，杜绝「网关给身份、插件路由不给」的分叉）。
- [x] 验收 4 前缀放行退役 + 转发前强制：`plugin_http_endpoint` 按属主端点声明档位判
      `jwt`→要求已验签，否则 401 + 业务码 1007；`gateway::decide` 改为
      「宿主条目 `RouteAuth` 与插件声明档位**取较严者**」，新增 `GatewayDecision::AuthRequired`
      （401，不再把「没登录」报成「插件未激活」）；`decide` 收编原先散在中间件调用点的
      「未验签不转发」前置。
- [x] 验收 5 环回/hook 可用：terminal-session `NO_AUTH_HTTP_ENDPOINTS`（9 条：hook 两条 +
      `auth/*` 七条）落进 plugin.json 对象条目，产物已重建；Rust 与前端两份契约用例锁死
      「档位集合 == 常量清单」，两个方向各有一条判据（少标→hook/配对 401，多标→写端点敞开）。
- [x] 验收 6 协议红线：`/api/plugin/*` 响应形状未变（401 用的是与 JWT 中间件同一 `{code:1007,message}`）；
      别名表逐条 golden 未动；`caller` 为字段级追加（老插件忽略未知字段）。
      移动端 auto-task legacy 前缀：**不受影响**（其请求带 JWT，见核实清单第 1 条）。
- [ ] 门禁数字：见下。

**行为变更（Breaking）清单**：① 未声明 `httpEndpoints` 的插件 HTTP 面整体 404；
② 已声明但未写 `auth` 的端点要求 JWT 验签（此前免凭证可达）；③ `decide` 对「未验签 +
已声明」的处置由 `HostFallback`/`PluginRequired` 改为 `AuthRequired`（生产链路被中间件
更早拦住，仅在中间件顺序错乱时生效，方向更严）。
**移动端**：`plugin-sdk-mobile` 的 manifest 类型无 `httpEndpoints` 面 → 本票桌面独有，
移动端跟演时再补（同 `host-websocket` 的双端偏离口径）。

## 门禁实跑（2026-09-22）

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| 宿主全量 | `~/.cargo/bin/cargo test`（lib + 8 集成 target + doctest） | **lib 1147 passed / 0 failed**，各集成 target 全绿，`[skip]` 计数 **0**，EXIT=0 |
| 宿主类型面 | `cargo check --lib --tests` | 0 error |
| 前端全量 | `pnpm run test:run -- --pool=forks` | **79 files / 756 tests passed**（含 SDK 新用例 + 插件契约用例） |
| Lint | 根 `pnpm exec eslint .` | **0 error**（120 warning 全是既有 `vue/attributes-order`，不计入门禁） |
| 桌面 SDK | `cargo test`（`plugin-sdk-desktop/rust`） | **105 passed / 0 failed**（+3 ignored，与基线同） |
| 插件工程 | `cargo test`（`terminal-session/rust`） | **214 passed / 0 failed** |
| 打包 CLI | SDK `vitest run __tests__/manifest-validate.test.ts` | 8 passed（已含在上面 756 内） |
| 产物链 | `pnpm run plugins:build` | EXIT=0；产物 manifest 与源语义相等（`json.load` 比对 True），9 条对象条目已随产物出 |
| 格式 | `rustfmt --check` 逐文件 | 改动的 5 个 src-tauri 文件按 `src-tauri/rustfmt.toml` 归零；SDK `types.rs` 回到基线 9 处（不新增偏离）；插件 crate 未整表格式化（该 crate 无 rustfmt 配置、HEAD 即非 clean，禁止顺手重排他人代码） |

### 一次红跑留痕（既有 flake，不属本票）

第二轮 `cargo test` 出现 1 红：`wasm_runtime::tests::engine_limits::
test_component_trap_emits_host_error_log`（`captured: []`）。判据三条：

- 首轮与第三轮全量各 1147/1147 全绿；该用例单跑连 3 次 3/3 绿；
- 它的 `capture()` 用 `tracing::subscriber::with_default`（**线程本地**订阅者），宿主日志
  若落在工作线程上就捕不到——与隔壁线在途的 A0-3 宿主 async 化改动面
  （`wasm_runtime.rs` / `component.rs` / `host_impl/*`）同源；
- 本票未触碰 `engine_limits` 与 trap 日志路径。

⇒ 记为**既有 flake（1 红 / 3 全量跑）**，不阻塞本票；捕获取决于线程的做法本身不稳，
需单独立项（显式注入订阅者或加串行闸）。

### 变异自检（每锁一条，实跑）

| 注入 | 预期转红 | 实际 |
| --- | --- | --- |
| 恢复 `declared.is_empty() \|\|` 前缀放行 | `declared_paths_match_exactly_and_undeclared_grant_nothing` | 红 ✓ |
| `decide` 认证前置 `\|\|` → `&&`（任一档即可） | `decide_takes_the_stricter_of_route_and_endpoint_auth` | 红 ✓ |
| HTTP 声明缺省档 `Jwt` → `None`（照抄 WS） | registry 4 条 + `registered_http_endpoints_carry_declared_auth_tier` | 5 条全红 ✓ |
| plugin.json 漏标一条 hook 的 `none` | 插件 Rust `none_auth_endpoints_match_manifest_and_cover_public_surface` | 红 ✓（213 passed / 1 failed） |
| `EndpointAuth::parse_with` 未知档位静默回落缺省 | SDK `test_endpoint_auth_parse_with_default_and_unknown` | 红 ✓（104 passed / 1 failed） |
| 401 构造器误用 `HttpResponse::Ok()`（沿用宿主业务面 200 + 业务码的习惯） | `unauthenticated_response_is_http_401_with_both_remedies_named` | 红 ✓（断言点明「免凭证拒绝必须是 HTTP 401」） |

### 首提交后补的一处真实缺口（`89fcf57f2` 之后）

401 那条分支起初只有纯函数被锁（`plugin_http_auth_allowed`），**响应形状本身没用例**——
「判定对但回错状态码 / 文案含糊」测不到。补法：把 `/api/plugin/*` 的拒绝响应收成一个具名
构造器 `plugin_http_unauthenticated_response`（与网关既有的 `unauthorized_response` 对称），
两边各加一条真实 actix body 读取的形状用例（401 + 1007 + 文案点名端点与两条出路，
且网关那条反向断言「不得把未登录报成插件未激活」）。补后 `plugin_controller` + `gateway`
两模块 32 passed，新增 2 条各自命中；上表第 6 行即其变异自检。

**仍未覆盖的（如实登记）**：`plugin_http_endpoint` 整条 handler 链（属主解析 → 声明匹配 →
档位判定）没有端到端用例——它需要 `AppContext::global()` 与一个已激活插件，现有测试面里
没有这个夹具（网关侧有真实 actix 栈用例 `unverified_requests_never_reach_gateway`，
但它锁的是中间件顺序，不穿到 handler 的档位分支）。要做成套需先给 handler 建可注入的
AppContext 夹具，属独立工程，不在本票夹带（列遗留 7）。

前三处在同一次 `src-tauri` 跑内并行生效、各自命中预期用例；五处全部还原后复绿（58 passed）。

## 遗留与后续（本票不做）

1. **整插件粗档无声明面**：收紧粒度到端点后，「本插件全部端点仅本机可达」这类策略仍写不出来；
   需要时按裁决 2 另立档位，不在本票夹带。
2. **`caller` 的消费方**：内核只负责给出可信身份，terminal-session 本轮不读 `caller`
   （hook 面本就 `none`）。若将来要「非本机不得写队列」，判据在插件侧。
3. **环回判据 = TCP `peer_addr`**：与 `link_crypto` 的环回豁免各自实现（同语义两处代码）；
   桌面若挂本地反代，两个判据都要重审。
4. **handler 里两次查表**（`list_http_endpoint_paths` + `find_http_endpoint`）：两次 await 之间
   插件被停用是真实竞态，已按最严档兜底；将来合一查询时必须保住「未声明 → 404」的断言方向。
5. **移动端跟演**：`plugin-sdk-mobile` 无 `httpEndpoints` 声明面（grep 证实），该端接同类能力时
   补自己的 SDK 类型 + 打包链校验，桌面结果不构成其正确性依据（ADR 0022「双端偏离」）。
6. **CHANGELOG 只补英文**：沿用本线票 01–06 的既有一致口径（审计票条目未落 `CHANGELOG_zh.md`）。
7. **handler 端到端夹具**：`plugin_http_endpoint` 要 `AppContext::global()` + 已激活插件才能整链测，
   现有夹具（`host_impl::tests::build_host_ctx` 造的是 `WasmHostContext`）覆盖不到。补好后应测三格：
   未声明 → 404、`jwt` + 无凭证 → 401、`none` + 环回 → 真的到达插件。

### 裁决（2026-09-22 用户裁决，开工前已定）

1. **默认策略翻转幅度**：新插件「未声明 `auth` 即最严（要求 JWT）」，存量走**显式 legacy 名单**兜底并标注退役条件
   （不是无条件保留 `declared.is_empty()` 的前缀内全放行）。核实清单仍按票面第一条先跑一遍再定名单内容。
2. **档位**：**沿用 `none | jwt` 两档**，与 WS 侧同形；**不加** `local-only` / `pairing-scope` 第三档。
   环回可达性不靠档位表达，靠下面第 3 条的身份给出。
3. **环回身份**：转发给插件的调用方身份除可信设备 claims 派生标识与 `anonymous` 外，
   **加一个固定的 localhost 标识**（本机 hook 可被插件区分），沿用 `PluginHttpRequest::device` 现有约束与测试：
   JWT 本体与设备指纹一律不透传。
