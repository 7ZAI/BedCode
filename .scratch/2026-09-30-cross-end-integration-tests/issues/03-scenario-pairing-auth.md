# 票 03 — 场景 1：配对 / 认证闭环（HTTP 六端点真实往返）

**状态**：resolved · 2026-09-30
**类型**：task

## 落点

`cross-end-tests/tests/pairing_auth_flow.rs`，8 条契约（C-001…C-008）全部落地。

| 契约 | 覆盖 |
|---|---|
| C-001 | pairing 正例（码非空 + 有效期为正） |
| C-002 | 错配对码 → 1005，**且不消耗有效码**（副作用断言，随后仍能换到 token） |
| C-003 | 有效码换 JWT |
| C-004 | reauth 回炉验签 + 跨秒换发必产出新 token |
| C-005 | 篡改 token → 1001（fail-closed） |
| C-006 | QR 正向：桌面 `qr-code-generate` 互调生成 → 移动端 `qr_connect` 换 token，且该 token 同样可复验 |
| C-007 | QR 一次性：二次使用 → 1006 |
| C-008 | 未绑定生物凭证 → 1008（端点存在且失败显性） |

## 两处「初稿契约写错、实测行为更有价值」

1. **C-004「reauth 必须产出新 token」不成立**：JWT `iat` 是**秒级**粒度，同一秒内
   reauth 会得到**逐字节相同**的 token。改为：同秒 reauth 断言「新 token 同样可复验」，
   跨过秒边界再 reauth 断言**必须不同**（后者才杀死「reauth 原样回显入参」的变异）。
2. **互调 params 形状**：`plugin_api_call` 的 params 是 **api 声明的入参值本身**，
   不是带参数名的对象（SDK 宏 `deser_code`：单参直接 `from_value(params)`）。
   `qr-code-generate(ttl: u64)` 要传 `json!(300)`，传 `{"ttl":300}` 会让插件侧报
   `invalid params`——**而错误走 guest 的 on_message 失败通道，宿主侧表现为 5s 超时**，
   排查成本高。已把该约定写进 `desktop_ctx::plugin_api_call` 的文档注释。

## 未覆盖（诚实边界）

- 生物认证正向路径：移动端私钥在 Android Keystore，无头进程构造不出真设备密钥。
- QR 的「桌面扫码确认」UI 步骤：直接驱动插件 `qr-code-generate`（即桌面 UI 的同一入口）。

## 验证

`cargo test --test pairing_auth_flow` → 1 passed。
