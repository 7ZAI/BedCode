# 04: 宿主四处验签改调中心 + `utils/auth/jwt.rs` 退役

**What to build:** 宿主不再做 JWT 密码学。四处验签调用点统一改为调认证中心，`JwtService` 整个退役。**这是本专项的主体。**

**Blocked by:** 02, 03

**Status:** todo

## 7.1 四处调用点

- [ ] `server/http/middleware/jwt_auth.rs`（6 处引用）—— `verify_token_with_expiry` 删除，只保留 `enforce_connection_policy`（中心内部先验签再裁决）
- [ ] `server/http/controllers/plugin_controller.rs`（3 处，`:531/:564/:572`）—— 端点级认证档位闸改为复用中间件已注入的 claims，或调中心
- [ ] `server/http/gateway.rs`（2 处，`:253/:465`）—— 同上
- [ ] `server/websocket/channel/plugin.rs`（3 处）—— `verify_endpoint_jwt` 改为只调中心，close 4001 带 `deny_kind` 原因
- [ ] 顺序语义核对：今天「宿主验签 → 问中心策略」两步；改后「问中心一次，中心内部先验签再裁决」一步。**失败面收窄为一次调用**

## 7.2 退役

- [ ] `utils/auth/jwt.rs` 整个退役（`JwtService` / `generate_device_token` / `verify_device_token` / `resolve_secret` / 全部 `#[cfg(test)]`）
- [ ] `utils/auth/host_secrets.rs` 的 `JWT_SECRET_KEY_ID` 用途清理（`host_secrets.rs` 仍服务生物公钥寄主，**模块本身不删**）
- [ ] `utils/auth/auth_center.rs::enforce_connection_policy` 保留，改为只调中心；`deny_kind` 三态（`no_center` / `unavailable` / `policy`）**不变**

## 16. 附带清理（spec §16，同批）

- [ ] 删 `utils/auth/auth_center.rs::format_device_display_name` —— **死代码**，全仓（含 `tests/`）除自身外零引用，唯一「引用」是 `l2_gating_test.rs:76` 的锁表字符串；其注释「WS 重认证路径仍在宿主使用」已过期（HTTP 同构实现早已下沉到插件 `auth_http`）
- [ ] 同步删 `l2_gating_test.rs:76` 的 `BRIDGE_PUBLIC_SURFACE` 条目（加删导出项必须同改该表）
- [ ] `call_api` 归属错位：被 `utils/session_gateway.rs:44` 当**通用互调 JSON-RPC 客户端**用（会话 API，不只认证中心），却住在 `auth_center.rs` 且注释写「调用认证中心互调 api」——上提到中性位置（如 `wasm_core/` 下），或明确拆名
- [ ] `session_active(host_ctx)` 的 `host_ctx` 参数已无用（`let _ = host_ctx;`）——清理或注释说明为何保留签名
- [ ] `call_api` 上提后同步 `l2_gating_test.rs` 的 `L2_CONSUMER_ALLOWLIST`（`src/utils/auth/auth_center.rs` 条目可能要移）

## 关键实现事实

- 退役面（生产路径）共 5 文件：`jwt_auth.rs` / `plugin_controller.rs` / `gateway.rs` /
  `channel/plugin.rs` / `host_api/auth.rs`
- 测试面（票 06 处理）：`host/tests/system_component_test.rs`（8 处）·
  `runtime/tests/ws_e2e.rs`（9 处）· `ws_output_perf.rs`（2 处）· `auth_center_perf.rs`（6 处）
- `enforce_connection_policy` 是**唯一**保留的裁决入口，三态语义与结构化字段
  `deny_kind` 一律不动（AGENTS §8）
- 宿主**不得**新增任何 JWT 密码学（防止回接的静态锁由票 06 建）

## 验收

- `cd bedcode-desktop/src-tauri && cargo test` 全量绿（集成 target 一并）
- 全仓 grep `JwtService|verify_token_with_expiry|generate_device_token|verify_device_token`
  在 `src-tauri/src/` 生产路径下**零命中**（仅票 06 的回归锁可含字样）
- 配对 → 签发 → HTTP `/api/*` 准入 → WS 端点准入 → 撤销后拒绝，端到端可用
- `deny_kind` 三态各有正例 + 反例
- 变异自检：把某处验签改回本地/跳过中心调用，对应用例必须转红
