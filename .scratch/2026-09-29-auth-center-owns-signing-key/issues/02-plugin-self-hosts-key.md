# 02: 插件侧自持密钥 — 生成 / 签发 / 验签全部本地化

**What to build:** 让 `com.bedcode.terminal-session` 成为入场 JWT 密钥的唯一持有者：生成、签发、验签全走本地实现，删掉对宿主 `device-token-*` 原语的依赖，并删除宿主那把死密钥。

**Blocked by:** 01

**Status:** todo

- [ ] `pairing/keys.rs::get_or_create_jwt_key` 从「activate 探活兼生成」变为**唯一密钥源**（已是 guest 内 `getrandom::fill` 生成，见 `keys.rs:42-43`）
- [ ] `lib.rs:782` activate 里的 `jwt_key_from_host_auth()` 探活改造：调用 `get_or_create_jwt_key`，**失败阻断 activate**（凭据不可用时显性失败；禁止降级为进程随机密钥后仍对外服务——重启即全灭，比拒绝更糟）
- [ ] `auth_http/jwt.rs::issue_device_token` / `verify_device_token` 从调 `host.auth_device_token_*` 改为调本地 `pairing::jwt::JwtService`
- [ ] `auth_http/mod.rs:311` `handle_reauth` 的验签改走本地实现
- [ ] `policy/mod.rs::evaluate` 前置**密码学验签**（现只有结构 / claims / 时效 / 信任四类检查，缺签名验证）
- [ ] `pairing/jwt.rs` 的 `sign` / `verify_token`（`:78` `sign_hs256` / `:224` `verify_token`）从死代码转活，补生产路径测试
- [ ] 删除宿主侧 `plugin_secrets` 中 `('host','jwt.key')` 行的写入路径（`utils/auth/jwt.rs:33` `resolve_secret`），并清理存量行
- [ ] token wire 格式**逐字节不变**：HS256 + 三段 base64url + claims `{sub, iss, iat, exp, device_name?, fingerprint?}`

## 关键实现事实

- **`auth_device_token_issue` 退役后插件必须零调用**。当前唯一生产调用点是
  `wasm-apps/terminal-session/rust/src/auth_http/jwt.rs:27`
- `policy::verify_device_token`（`policy/mod.rs:88`）内部会调 `host.records()` →
  `pairings_all` **二次跨界拉取全部配对记录**。验签下沉后这是**每请求两次跨界往返**，
  性能影响见 spec §4（crypto 只占 6%，可接受）
- 中心已有的 HS256 实现与宿主 `jsonwebtoken` **逐字节兼容**，由
  `bedcode-desktop/src-tauri/src/utils/auth/jwt.rs:337` 的跨实现锁证明
  （`host_jsonwebtoken_matches_plugin_fixed_vector`）。该锁在票 04 退役宿主实现后
  失去意义，**票 06** 处理
- `keys.rs:79` native 单测路径 `jwt_key_from_host_auth()` 显性返回 Err（无 host-auth），
  单测经 `MockSecretStore` 注入 —— 该形状保持
- 删除 `('host','jwt.key')` 是**删写入路径 + 清存量行**，不是 DROP 表（该表还有生物
  公钥寄主 `plugin_secrets` key=`biometric:<fp>`，语义不变）

## 验收

- `cd bedcode-desktop/wasm-apps/terminal-session/rust && cargo test` 全绿
- 全仓 grep `device_token_issue|device-token-issue` 在 `wasm-apps/` 下**零命中**
- 中心自签的 token 能被中心自验（往返用例）
- 错误 key / 过期 / 结构畸形三类拒绝各有正反例断言
- activate 在密钥不可用时**失败**而非降级（变异自检：把失败分支改回 warn 后必须转红）
