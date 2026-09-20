# 12: server 认证中间件（C3）

**What to build:** server 连接建立验签执行留在宿主中间件（密码学引擎不移动），认证策略取认证中心 capability 导出（06 框架：exported_capabilities 探测 / call_capability_export）；认证中心未激活时策略回退宿主。

**Blocked by:** 09（策略导出就绪）

**Status:** done（2026-09-19）

- [x] jwt_auth 中间件单测全绿（验签仍执行于宿主）
- [x] 连接建立流程集成测试通过（验签 → 策略取认证中心 → 放行/拒绝）
- [x] 认证中心未激活降级可用（策略回退宿主，无单点）

## 实施记录（2026-09-19）

### 设计定案

- **验签留宿主**：`JwtService::verify_token_with_expiry` 不动（密码学引擎不移动，
  spec §3「不动」表）；策略门在宿主验签**通过之后**执行——篡改/过期 token 连
  策略都到不了（单测锁定该前置性）。
- **策略取认证中心 capability 导出**：新 WIT 接口 `auth-policy`
  （`verify-device-token(token) -> result<string /*claims JSON*/, string /*拒绝原因*/>`，
  desktop 独有、双端偏离同 host-auth/pty 先例）+ 独立 world `plugin-auth-policy`
  （同 events-binary/events-ws「可选导出 + 宿主动态探测」模式，不进 plugin world
  必选列表）。SDK `wasm_entry!` 默认导出（默认**拒绝**——非认证中心插件不提供
  策略；宿主动态探测命中但中间件只对 `com.bedcode.devices` 实例调用）。
- **06 框架**：`capability.rs` 新增 `PROBE_CAPABILITIES`（可路由 + **仅探测**
  双表）——`auth-policy` 经 `probe_exported_capabilities` 探测进
  `exported_capabilities()`，但**不参与注册表路由**（消费方是宿主中间件而非插件
  import；注册为路由提供者会让任意 SDK 插件接管认证策略，语义错误）。宿主
  经 `PluginHost::call_plugin_capability_export`（新方法：直查实例 + 能力存在性
  前置校验）调用。
- **策略语义（devices 插件 `policy` 模块，迁移期）**：①结构（三段 base64url +
  claims 可解析，独立复检不信任宿主中间结果）②claims（iss == JWT_ISSUER、sub
  非空）③时效（exp 防御性复检）④信任（fingerprint 查 trust 镜像，`active=false`
  → 拒绝；未命中从宽放行——镜像非真源，撤销是唯一显式拒绝信号；镜像读取失败
  放行 + log_warn，防误杀全部连接）。note：插件密钥域与宿主不同，不重复验签。
- **降级（无单点）**：认证中心未激活（api 注册表无标记）/ 能力调用传输失败
  （实例缺失/trap）→ 宿主策略（验签通过即放行，迁移前行为）；策略拒绝 → 上抛
  原因（WS `POLICY_DENIED` 错误码 + 拒绝原因；HTTP 401 拦截）。

### 接线面

- `server/ws/conn.rs::authenticate_jwt`（WS 终端 + 事件通道首消息认证共享核心）
- `server/middleware/jwt_auth.rs::extract_and_verify_jwt`（HTTP /api 网关）
- 无 AppContext（无头/单测）→ 宿主策略；plugin WS 端点（`auth: none|jwt` 端点
  声明语义）与 reauth/biometric 端点不接（连接建立门在 WS 首消息 + API 网关，
  票 13/14 可扩展）

### WIT/ABI/SDK

- `bedcode.wit`：`interface auth-policy` + `world plugin-auth-policy`；abi 注释 v17
- SDK：`ABI_VERSION 16 → 17`；`wasm_auth_policy.rs`（新绑定模块，plugin-auth-policy
  world）；`wasm.rs` WasmPlugin 新增默认方法 `verify_device_token_policy`
  （默认拒绝）+ `wasm_entry!` 增 Guest impl + export!
- AGENTS.md §7 双端偏离清单 + ADR 0022「双端偏离」节同步（auth-policy v17 桌面独有）
- 旧插件（≤v16 产物）零迁移：`version > 当前 → 拒绝` 兼容语义，auth-policy 缺失
  → 探测未命中 → 无策略面（中间件只对 devices 实例调用，实例无该导出 → 调用
  失败 → 宿主策略回退）

### 测试与验证（全绿）

- **插件单测**：devices 71/0（policy 模块 11 个：正例放行/已信任放行/撤销拒绝/
  iss 违反拒绝/sub 空拒绝/过期拒绝/结构非法拒绝×3/未知指纹从宽/指纹缺失放行/
  撤销优先于合法签名；native 路径显性失败）。**顺手修一个存量 flake**：四个配对/
  QR 测试共享 `CURRENT_CODE`/QR_MANAGER 静态状态，并发线程互相踩（A 生成 → B
  清除 → A 读 status None），加 `PAIRING_STATE_LOCK` 串行化锁（`--test-threads=1`
  复现，锁后稳定 71/0×3）
- **中间件单测**（jwt_auth.rs +4）：篡改签名拒绝（验签留宿主前置性）、乱串拒绝、
  无 Authorization/非 Bearer → None、合法 token + 无 AppContext（认证中心不可判定）
  → 宿主策略回退放行且 claims 注入
- **capability 单测**：`is_routable(auth-policy) == false`（仅探测不路由；register
  拒绝），host-storage 仍可路由
- **宿主闭环** `test_server_auth_policy_closed_loop`（host.rs +1）：真实 devices
  wasip3 产物加载激活 → 宿主验签（无效 token 到不了策略）→ 未激活回退 Ok →
  激活+空镜像放行 → `trust.add-pairing` 放行 → `trust.revoke` 拒绝（原因可读）→
  其他设备 token 放行 → 实例移除后能力调用失败回退 Ok（认证中心故障不误杀连接）
- **全量**：宿主 lib **1009 passed / 0 failed**（含 pty 线全部翻绿）；SDK 85/0；
  集成测试 ws_auth_rules（真实 WS 链路 + 认证中心缺省回退）✓ / http_auth_biometric
  ✓ / server_integration ✓ / link_crypto_http ✓ / ws_session_route ✓ /
  build_manifest_smoke ✓；eslint 0 error（123 warning，1 个为并发线
  TerminalWindowView.vue 新增，非本票）；fmt/clippy 自查涉本票区域零新增
- **已知存量失败（非本票）**：集成测试 `broadcast_and_shutdown_flow` =
  在途 auth 线 utils/auth/jwt.rs 未提交 ERROR 日志（secret store unavailable 回退），
  与 handoff §3「非本票注意」一致；本票改动不涉及该 ERROR（fallback 走 warn!）
- **产物**：`resources/plugins/desktop/com.bedcode.devices/bedcode_plugin_devices.wasm`
  重建（v17 SDK，wasip3 Component，导出 auth-policy）

## 产物

- `plugins/devices/rust/src/policy/`：`mod.rs`（evaluate 纯函数核心 + wasm/native
  cfg 分流 + 11 单测）；lib.rs 增 `mod policy` + WasmPlugin 覆盖
  `verify_device_token_policy`
- `packages/plugin-sdk-desktop/`：WIT（auth-policy + world + abi 注释）；`abi.rs`
  （17）；`wasm_auth_policy.rs`（新）；`wasm.rs`（trait 默认方法 + wasm_entry!）
- 宿主：`capability.rs`（CAP_AUTH_POLICY/EXPORT 常量 + PROBE_CAPABILITIES +
  仅探测不路由 + 组件导出日志措辞中性化）；`host.rs`（call_plugin_capability_export
  + 闭环测试）；`auth_center.rs`（enforce_connection_policy 三态降级）；
  `conn.rs` / `jwt_auth.rs`（接线 + 单测）
- 文档：AGENTS.md §7 + ADR 0022「双端偏离」节（auth-policy v17 桌面独有）

## 后续

- 票 13 命令面退役（数据源整合：trust 镜像转真源，策略 ④ 从宽→权威）
- 票 14 文档同步（ADR 0022 认证语义可下沉列 / AGENTS.md §8 / code-map / roadmap）