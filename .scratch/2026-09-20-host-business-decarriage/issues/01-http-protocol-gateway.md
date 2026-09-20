# 01: HTTP 协议网关框架（双轨契约锁）

**What to build:** 宿主新增平台基础服务「HTTP 协议网关」：一张业务 URL 别名路由表（`/api/configs`、`/api/quick-actions`、`/api/file-tree|file-content|diff-tree|file-diff`、`/api/git/*` → 各目标插件端点），转发复用既有插件动态 HTTP 端点代理内核，移动端 JWT 验签仍在宿主统一中间件完成。双轨起步：别名目标未实现/未激活时降级宿主旧实现，**全部业务端点对外形状与今天逐字节一致**，契约对照测试先于任何域名切换落地。

**Blocked by:** None（可立即开始）

**Status:** done（2026-09-20；桌面 `cargo test --lib` **1099/0**、闭环 `[skip]` 计数 0；`server::` 210/0，其中新增 `server::gateway` 20/0；`cargo clippy --lib` 对 gateway 零告警；`rustfmt --check` 对新文件与本次改动行零 diff（既有文件的历史格式偏差未顺手改）；本票零前端改动，移动端零改动）

- [x] 网关别名路由表覆盖 02/03/04 票涉及的全部业务端点，条目带归属插件声明
- [x] 转发复用既有插件动态 HTTP 端点代理的同一转发内核与信任判定，不发明新传输机制
- [x] 鉴权语义不变：移动端 JWT 验签仍在宿主中间件，转发前完成，插件收到可信上下文
- [x] 双轨降级成立：别名目标未激活/未实现 → 宿主旧实现，响应形状与今天逐字节一致
- [~] 全部业务端点「双轨对照」契约测试通过（同请求新旧两路输出一致）——**本票只能交付一半**，见 Comments ⑤
- [x] 桌面既有回归全绿（`cargo test --lib` 1096/0、闭环 `[skip]` 计数 0；本票零前端改动，vitest/eslint 无适用面）
## Comments

### ① 落点

- 新增 `src-tauri/src/server/gateway.rs`：别名路由表 `BUSINESS_ROUTES`（10 条 = 配置 1 + 快捷指令 1 + 文件浏览 5 + git 3）、
  纯判定 `decide`、`FallbackPolicy::{HostImplementation, PluginRequired}`、中间件 `business_gateway`。
- `controllers/plugin_controller.rs`：转发内核抽出为 `PluginHttpRequest` + `build_plugin_http_args` + `forward_to_plugin`
  （原 handler 与网关调同一函数）；`_http_endpoint` 入参新增可选 `device` 字段（字段级追加，老插件忽略未知字段）。
- `middleware/jwt_auth.rs`：`/api` 的 JWT 闭包提为具名中间件 `jwt_gateway`——为了让「先验签后网关」可被真实 actix 栈测到。
- `app.rs`：`/api` scope 挂两个中间件（注册顺序由内到外，故网关写在验签之前）。

### ② 「未声明就不切」是票 02/03/04 的落地前提

`/api/plugin/*` 那条路由对**未声明 httpEndpoints 的插件**是「前缀内放行」（既有插件零迁移）；
网关反过来：目标插件必须在 manifest `contributes.httpEndpoints` 里**逐字声明**该业务端点才切过去。
否则 session 插件今天已激活（只声明了任务域端点），一上线路由就会把 `/api/configs` 等活端点
从宿主手里抢走、把移动端打进空实现。空清单在网关侧一律判「未声明」→ 降级宿主。

### ③ 转发前置：必须有已验签的设备上下文

`decide` 的第一个参数是 `verified`（宿主 claims 是否已注入）。未验签一律 `HostFallback`，
于是「未验签请求不得进插件」成为网关自身性质，不依赖中间件注册顺序；顺序错乱时最多多一次降级，
由验签中间件把请求 401 掉。顺序语义另用 `scope_wrap_registration_puts_jwt_outermost`
把 actix「最后注册 = 最外层」这条承重假设钉住。

### ④ 载荷纪律

判定与降级分支都不碰 payload（只有 Forward 分支 `into_parts` 读 body），
否则宿主 handler 会读到空 body 而 400——形状漂移最隐蔽的一种。用例
`fallback_branch_leaves_the_payload_intact` 直接断言宿主 handler 拿得到完整 JSON。
请求体三态分开：无载荷 → Null（与 `Option<Json>` 同口径）；合法 JSON → 原样透传（不重序列化宿主 DTO，
避免未知字段被丢）；畸形 JSON → 显式拒绝（HTTP 200 + `code:1003`，与宿主这些端点的错误口径一致）。

### ⑤ 「双轨对照」本票只能交付一半，剩余部分归 02/03/04

同请求「新路径 vs 旧路径输出一致」要求新路径**存在**。票 01 阶段插件业务端点还不存在，
所以本票交付的是可先行锁死的那一半：

- 形状 golden：10 个端点的宿主回包 JSON 逐字段钉死（`business_endpoint_shapes_are_locked_for_dual_track`），
  含「可选字段是显式 null 还是省略」这类细节（`ConfigItem.wslDistro` / `GitBranches.currentBranch` 是显式 null，
  `FileTreeNode.path/children` 是省略——写 golden 时按真实 DTO 校正过，不是凭直觉）。
- 转发入参同源：两条路径共用 `build_plugin_http_args`，用例锁形状。
- 中间件行为：未验签不转发、验签后降级到宿主、payload 不被消费、方法/路径不命中时与「不挂网关」逐项等值。

真正的双轨对照（真 wasm 插件 vs 宿主 handler）随 02/03/04 各自落地补测；网关在 02/03/04
期间的行为差异只有 `Forward`/`HostFallback` 一格，判定已被纯函数用例覆盖。

### ⑥ 顺手纠正的两条票面前提（影响票 02 范围）

领域审计把快捷指令记作「主库表 + HTTP + WS 服务 + 桌面命令面 + 同步广播全链路活跃」，实装核对：

- **桌面快捷指令命令面不存在**：前端 `listQuickActions/createQuickAction/updateQuickAction/deleteQuickAction`
  四个 wrapper 指向的 Tauri 命令在 `src-tauri` 里没有任何实现、也不在 `invoke_handler` 清单里；
  `useQuickActionStore` 只做 pendingInput 中转，与 `quick_actions` 表无关。
- **快捷指令变更广播不存在**：`SyncPayload` / `DesktopSyncEvent` 无快捷指令变体；
  `SessionConfigAction::ListQuickActions` 只有线协议类型与一个无调用点的
  `services/session_config.rs::list_quick_actions_response`（桌面 WS 侧没有 `session_config` 帧分派）。
- 两端均无快捷指令 UI；活的出口只有 `GET /api/quick-actions`（且移动端 `httpListQuickActions` 亦无调用方）。

结论：票 02 的「命令面形状不变」与「WS 变更广播形状不变」两格没有可保持的现状；补广播需要新增
`SyncEvent` 变体（动 SDK/WIT），与 spec 决策 6「不触发 ABI bump」直接冲突。已就此向用户确认范围。
