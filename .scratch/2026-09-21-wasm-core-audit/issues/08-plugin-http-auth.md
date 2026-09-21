# 08: 插件 HTTP 端点的声明式认证（P1-1）

**What to build:** 插件经 HTTP 暴露的面获得与 WS 侧对等的**声明式认证策略**：端点在 manifest 声明 `auth`，未声明即默认最严；「未声明 httpEndpoints 的插件整前缀放行」这条零迁移策略退役；插件在 handler 里能拿到宿主判定的调用方身份（可信设备/会话或 `anonymous`）。

**Blocked by:** 06（调用方身份语义与前端通道一致后再定 `device` 口径）

**Status:** ready-for-human（涉跨端协议形状，需用户确认「是否动线协议」）

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

- [ ] 核实清单：桌面 4 个生产插件中哪些实现 `_http_endpoint`、各自的 `contributes.httpEndpoints` 声明状态、各自是否假设「仅本机调用」——结果写进本票 Comments，作为默认策略翻转的依据
- [ ] manifest 增 HTTP 端点 `auth` 声明（与 WS 同形），宿主在 `plugin_http_endpoint` / `gateway.rs` 转发前强制；`auth` 缺失时的行为按裁决项 1
- [ ] 转发给插件的参数携带宿主判定的调用方身份（可信设备 claims 派生标识或 `anonymous`），**凭据红线**：JWT 本体与指纹不透传（沿用 `PluginHttpRequest::device` 的现有约束与测试）
- [ ] 未声明端点清单的插件不再「前缀内全放行」（删除 `plugin_http_path_allowed` 的 `declared.is_empty()` 短路），或把该短路限定到显式 legacy 白名单并标注退役条件
- [ ] 环回/hook 场景（`claude code hook` 经 `/api/plugin/com.bedcode.session/...`，见 code-map 末尾）在收紧后仍可工作，且其身份可被插件区分
- [ ] 协议红线：`/api/plugin/*` 响应形状与网关别名表行为不得破坏性变更（AGENTS §9「老端忽略未知字段」）；若加声明面，移动端 auto-task legacy 前缀（roadmap M1）的受影响状态同步登记
- [ ] 门禁：`cargo test`（含 `plugin_controller` / `gateway` 双轨契约用例不回归）+ 新增认证档位用例

## Comments

- 2026-09-21 立项：来源 spec §5-P1-1。

### 裁决（2026-09-22 用户裁决，开工前已定）

1. **默认策略翻转幅度**：新插件「未声明 `auth` 即最严（要求 JWT）」，存量走**显式 legacy 名单**兜底并标注退役条件
   （不是无条件保留 `declared.is_empty()` 的前缀内全放行）。核实清单仍按票面第一条先跑一遍再定名单内容。
2. **档位**：**沿用 `none | jwt` 两档**，与 WS 侧同形；**不加** `local-only` / `pairing-scope` 第三档。
   环回可达性不靠档位表达，靠下面第 3 条的身份给出。
3. **环回身份**：转发给插件的调用方身份除可信设备 claims 派生标识与 `anonymous` 外，
   **加一个固定的 localhost 标识**（本机 hook 可被插件区分），沿用 `PluginHttpRequest::device` 现有约束与测试：
   JWT 本体与设备指纹一律不透传。
