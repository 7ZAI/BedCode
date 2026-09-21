# 插件 HTTP 端点信任模型（/api/plugin/*）

> 本文固化 `/api/plugin/*` 端点代理的**现状信任模型**（审计 H4 文档化，票据 04，
> `.scratch/2026-09-18-db-http-base-service/issues/04-http-endpoint-surface-and-trust.md`）。
> 只描述与固化现状，**不改门禁行为**（移动端与脚本兼容红线）。

## 1. 现状模型（是什么）

插件 HTTP 端点代理 `ANY /api/plugin/{plugin_id}/{path:.*}`（桌面端 Actix server，
挂在 `/api` scope 下，随 HTTP + WS 统一端口监听）的信任边界：

| 事实 | 说明 |
| --- | --- |
| **无 JWT 放行** | 网关中间件对 `/api/plugin/*` 路径：有 JWT 则校验，无 JWT 的请求（如本机 hook 脚本、curl 调用）直接放行，**不要求凭证** |
| **仅激活检查** | handler 只校验插件是否处于 Activated 状态，不校验任何请求凭证（历史 BEDCODE_TOKEN 凭证从未被宿主校验，已移除） |
| **0.0.0.0 监听** | 服务监听 `0.0.0.0`，同一局域网内任意客户端的任意请求可达该端点 |
| **无速率限制** | 端点代理层不设请求频率上限（受底层 Actix 连接与 OS 限制约束） |

## 2. 推论（写端点必须插件自查认证）

在上述模型下，**任何声明为写操作（修改状态 / 数据 / 执行动作）的插件端点，
必须由插件自身校验调用方身份/授权**，宿主不替代、不提供开箱凭证。

- 插件侧可用的身份线索：`headers` 字段（票据 04 白名单透传 `content-type` /
  `accept` / `x-request-id`——**注意凭据头 authorization / cookie 一律不透传**，
  插件如需鉴权需自定义签名方案，如请求体携带调用方生成的临时 token）。
- 只读端点风险较低，但仍建议插件自查（幂等性 / 信息泄露面）。
- 插件自查的实现完全在插件层（`_http_endpoint` command 内校验），与宿主零耦合。

## 3. 现状治理的层次（票据 03/04 接线后的完整图景）

| 层 | 机制 | 状态 |
| --- | --- | --- |
| 激活门禁 | `is_activated(plugin_id)` 检查 | ✅ 既有 |
| 端点注册治理 | 已声明端点（manifest `contributes.httpEndpoints`，或既有的 `contributes.toolProviders`）精确匹配，未注册路径 **404**；未声明插件前缀内放行（未引入 `httpEndpoints` 声明面的既有插件零迁移过渡策略） | ✅ 票据 03 接线 / 票 16 补 `httpEndpoints` 声明面 |
| 请求头白名单 | 仅透传 `content-type` / `accept` / `x-request-id`，凭据头不透传 | ✅ 票据 04 |
| 响应 content-type | 插件可指定 `contentType`（默认 `application/json`） | ✅ 票据 04 |
| 路径冲突 | 同一路径被两个插件占用 → 后注册者声明拒绝（warn），首注册者保留 | ✅ 票据 03 |
| 旧前缀接管 | 请求前缀的插件**未激活**且其 id 在 `LEGACY_HTTP_PLUGIN_ALIASES` 里、接管方**已激活** → 转给接管方（`path` 段原样不变）。旧插件在位时绝不抢占（避免双写者与空库回包） | ✅ 票 16 落地 / 票 17 **判定为保留**（见下） |

> **旧前缀接管的审计含义**：`/api/plugin/com.bedcode.auto-task/*` 在 auto-task
> 未激活时由 `com.bedcode.terminal-session` 应答；**票 06 改名后 `com.bedcode.session`
> 前缀同理**（票 07 B2 双投窗口追加别名条目）；两条日志字段（`plugin_id` = 属主、
> `requested_plugin_id` = 请求前缀）据此区分。

**票 17 判定：保留别名兜底，本票不切断**（auto-task 工程与产物已彻底退役，旧前缀
仍在路由面可命中）。理由与代价：

- 切断只省掉一张常量别名表 + 一个纯函数（约 30 行 + 4 条用例），换来的是移动端任务
  面板 7 个调用面立刻 404（受影响清单 M1 从「延后」变成「当场」）。
- 用户项目里已部署的 hook 集成副本靠 `@bedcode-template-version` bump（票 17 已做）
  自动改指新前缀，但**重写发生在下一次会话启动注入时**——切断会让这段窗口内的状态
  推送静默丢失（agent 任务不报错、只是不再同步），比 404 更难排查。
- 长期维持不在本规格承诺内（spec Out of Scope：旧前缀是可牺牲的桌面侧兜底），
  **切断时机随移动端适配专项一并定**：那时移动端 api 基址常量改一处即可，两侧同批
  落地，兜底表与 M1 一起清零。

## 4. 明确不做（后置安全升级）

- **给 `/api/plugin/*` 加独立 token / 按插件粒度鉴权**：列为后置项，本期仅文档化现状。
- **TS-only 插件的端点桥接**（前端 Tauri event 桥）：现状语义 = 端点仍是
  Rust/WASM 插件能力，TS-only 桥接记为已知缺口，不动。

## 5. 插件开发建议

1. 写端点先做自查认证（§2）。
2. 优先用 `*_params` 参数绑定访问 DB，不要在端点里把用户输入拼进 SQL。
3. 端点路径在 manifest 声明（`contributes.httpEndpoints`；`toolProviders` 只用于
   真正的「外部工具提供者」语义），声明后可获得宿主侧精确匹配与冲突检测；未声明时
   按前缀放行（兼容期行为）。
4. 响应如需非 JSON 类型（文本 / 图片），返回 `{ status, body, contentType }`。
