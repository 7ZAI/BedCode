# 23 — auth.rs + control.rs + shell.rs 补 serde 往返测试

**What to build:** 为 `auth.rs`、`control.rs`、`shell.rs` 三个零测试枚举文件补 serde 往返测试。

**Blocked by:** 无

**Status:** done（2026-09-15）

- [ ] `auth.rs`：`AuthStage`（7 variant）+ `AuthPayload` serde 往返
- [ ] `auth.rs`：未知 variant 反序列化拒绝
- [ ] `control.rs`：`SessionControlAction`（含 `session_id` 字段）serde 往返
- [ ] `shell.rs`：`ShellType` + `ShellConfig` serde 往返
- [ ] `shell.rs`：`ShellConfig` 默认值（`default()` / `Default` trait）
- [ ] 未知字段拒绝（serde deny_unknown_fields 行为）
- [ ] `cargo test --lib enums::` 通过

## 证据

3 个文件共 331 行零测试。这些枚举是跨端协议表面（移动端 TS 必须逐字节一致），serde 格式变更无回归锁。

对比：`special_key.rs`（828 行）有 32 个 serde 测试，证明该模块风格支持 serde 测试。

## 修复方向

参考 `special_key.rs` 的 `test_serde_roundtrip` / `test_serde_bare_key` 模式，为每个枚举补：
1. 正例：每个 variant 的序列化 → 反序列化 → 断言等值
2. 反例：未知 variant 拒绝
3. 默认值：`Default` trait 行为

## 影响面

仅新增测试，零生产代码改动。

## Comments

- 2026-09-14 审计发现，见 `../enums-spec.md` §3
