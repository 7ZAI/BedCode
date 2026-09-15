# 03 — 配对/认证端点下发 Kd 公钥与指纹

**What to build:** `auth_controller` 三个端点的响应 DTO 扩展两个可选字段：`kdPublicB64`（Kd 公钥 base64，32B 解码可验）与 `kdFingerprint`（SHA-256 前 16 hex）——`POST /api/auth/verify`、`POST /api/auth/qr-connect`、`POST /api/auth/reauth`（以 `app.rs` 实际路由为准）。移动端据此在配对/重认证时建立或刷新 pin（消费侧在 05/06）。字段为新增可选输出，老客户端忽略多余字段不受影响。`/auth/pairing`（配对请求发起，尚未到信任建立点）不下发。

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] 三端点响应均含两新字段，值与 01 持久化的 Kd 一致
- [ ] 控制器/DTO 测试：字段存在、base64 可解码为 32 字节、指纹与 Kd_pub SHA-256 匹配
- [ ] 老客户端兼容回归：既有移动端对响应的解析不因多余字段破坏
- [ ] `/auth/pairing` 响应不含新字段
- [ ] `cargo test --lib` 全绿
