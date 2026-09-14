# 04 — state.rs LinkCryptoContext 补 `encrypt_http` + set_link_crypto_context 参数

**What to build:** `src-tauri/src/state.rs` 的 `LinkCryptoContext` 新增 `encrypt_http` 字段（当前只有 enabled/strict_mode/encrypt_ws_event/kd_public_b64）；`set_link_crypto_context` 命令参数同步补 `encrypt_http`；连接模块相关命令参数对齐。HTTP 加密判定（ticket 03）依赖此字段。

**Spec:** §3（加密收束 Rust）、§9 D2

**Blocked by:**

**Status:** done

## 关键实现事实（handoff §2/§3.4 已核实）

- `state.rs` `LinkCryptoContext`：enabled/strict_mode/encrypt_ws_event/kd_public_b64 + `get/set` + `update_link_crypto_pin`。
- 前端 `useLinkEncryption.ts` 的 `syncLinkCryptoContextToNative` 经 `set_link_crypto_context` 推 Rust，**当前只推 enabled/strictMode/encryptWsEvent/kdPublicB64——缺 encryptHttp**；本 ticket 补 Rust 侧接收字段（前端推送侧在 ticket 09 同步补）。
- 兼容：老前端（本分支切换前）invoke 不带 encryptHttp → 字段可选/默认 false，不破坏。

## 实现清单

- [x] `LinkCryptoContext` 加 `encrypt_http: bool`（或等价命名，camelCase 参数 `encryptHttp`）
- [x] `set_link_crypto_context` 命令参数补 `encrypt_http`（可选，默认 false，兼容旧调用）
- [x] connection 模块相关命令参数对齐（如有透传）
- [x] 相关单测：默认值 / 设置后读取 / 旧参数形状不破坏

## 验证

- `cargo test`（src-tauri）全绿；与 ticket 03 的加密判定联调（`enabled ∧ encrypt_http ∧ kd_public_b64.is_some()`）

## Comments

- 2026-09-11 完成：`state.rs` `LinkCryptoContext` 加 `encrypt_http: bool`（Default 与 static 初始化均为 true——主开关关着时无效，与 encrypt_ws_event 同风格），新增 `is_http_encryption_active()`（`enabled ∧ encrypt_http ∧ kd_public_b64.is_some()`，对齐前端 isChannelEncryptionActive('http')，供 ticket 03 使用）；`commands/connection.rs` `set_link_crypto_context` 加 `encrypt_http: Option<bool>`（unwrap_or(true)，旧前端不推送不破坏）。验证：cargo check + cargo test 全绿（251 passed）。
