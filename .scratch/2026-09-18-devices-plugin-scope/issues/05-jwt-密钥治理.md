# 05: JWT 密钥治理（A3）

**What to build:** 宿主 JWT 密钥由硬编码常量改为 secret-store 托管：首启随机生成 + 持久化；既有签发/验签调用面不变，行为零回归。

**Blocked by:** 04

**Status:** done（2026-09-19）

- [x] 首启随机生成密钥并持久化；重启后密钥稳定（同一 token 重启前后验签均通过）
- [x] 既有 jwt 单测全绿（无行为回退）
- [x] 密钥明文不出宿主、不进日志（只记长度）；失败路径带操作上下文（AppError）

## 验证证据（2026-09-19）

- `cargo test --lib` **954 passed / 0 failed**（全量）；jwt 单测 5/5 绿（含新增 `all_service_instances_share_same_key` / `convenience_fns_share_key_with_service`）；host_secrets 单测 3/3 绿（首启生成+幂等、重启稳定、未初始化报错）
- 旧硬编码常量 `JWT_SECRET` 全仓 grep 无残留（移动端无 JWT_SECRET，不受影响）
- 前端未改动（纯 Rust 票）

**产物**：

- `utils/auth/host_secrets.rs`（新）：宿主侧密钥托管——与插件 secret-store 同表（`plugin_secrets`）不同域，保留属主 `host`，不经插件权限门（宿主是仲裁者；插件 SQL 层隔离天然读不到 host 行）；read-through 缓存 + 首启 `OsRng` 随机生成 32B（hex 落库）+ UPSERT 持久化；日志只记 key/value_len
- `jwt.rs`：`JWT_SECRET` 常量 → `resolve_secret()`（OnceLock 进程内只解析一次，所有 JwtService 实例共享同一密钥）；未托管环境（单测）回退进程内随机密钥并 `tracing::error!` 告警
- `lib.rs` setup：`host_secrets::init(db)` + 预生成 JWT 密钥（`?` 上抛阻断启动——避免静默降级导致重启后 token 全部失效）

**遗留风险**：回退路径（无主库/单测）token 重启后失效——仅测试环境命中，生产在 setup 预生成落库；密钥轮换/吊销机制未实现（超出本票范围，后续票可评估）。
