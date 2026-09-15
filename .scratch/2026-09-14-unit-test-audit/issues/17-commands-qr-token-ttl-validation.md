# 17-commands-qr-token-ttl-validation

> Status: `done`（2026-09-15 修复）
> Blocked by: 无
> 关联 spec: [commands-spec.md](../commands-spec.md) §6 风险 A、§8 P0

## What to build

为 `qr::set_qr_token_ttl` 补 TTL 边界校验 + 单元测试，防止 `ttl = 0` 或超大值导致 QR 配对失败。

## 根因

`qr.rs:91-95`：
```rust
pub async fn set_qr_token_ttl(db: State<'_, Arc<Mutex<Database>>>, ttl: u64) -> Result<()> {
    let db = db.lock().await;
    db.set_setting("qr_token_ttl", &ttl.to_string())
        .map_err(|e| crate::AppError::Config(e.to_string()))
}
```

无任何边界校验。`ttl = 0` 时 `generate_qr_code` 用 `qr_manager.generate(0)` 生成立即过期的 token，移动端扫码后连接立即失败。

现有测试：`qr.rs` 零测试。

## 修复方向

1. **校验 `ttl > 0`**：`ttl = 0` 返回 `AppError::InvalidInput`。
2. **校验 `ttl <= 86400`**（24 小时上限，防超大值）：超限返回 `AppError::InvalidInput`。
3. **补单测**：
   - `ttl = 0` → `Err`
   - `ttl = 1` → `Ok`
   - `ttl = 86400` → `Ok`
   - `ttl = 86401` → `Err`

## 影响面

- 前端设置页 TTL 输入框需同步加前端校验（但 Rust 端是最终仲裁）。
- `get_qr_token_ttl` 读取时可能读到历史 0 值——需考虑 fallback（已有 `unwrap_or(300)` 兜底）。

## 验收清单

- [ ] `set_qr_token_ttl` 校验 `ttl > 0 && ttl <= 86400`
- [ ] 4 个边界单测通过（0 / 1 / 86400 / 86401）
- [ ] `cargo test --lib commands::qr` 通过

## Comments

2026-09-14 审计创建。P0：0 TTL 导致 QR 配对完全失败。
