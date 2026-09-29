# 08: 中心未就绪 / 需重新配对的显式 UI 信号

**What to build:** 补两处用户可见信号。**第一处是 ADR 0031 的欠账**（已核实未落地），第二处是本专项迁移带来的新可见面。

**Blocked by:** 04

**Status:** todo

## 8.1 ADR 0031 欠账：中心停用时无任何提示

**已核实**：`bedcode-desktop/src/`、`wasm-apps/terminal-session/src/`、
`bedcode-mobile/src/` 三处搜 `no auth center registered` / `no_center` / `auth center`
**零命中**。ADR 0031 Consequences 明写「认证中心插件停用 = 本机认证面全断
（有意为之，**但需要 UI 侧给明确提示**）」—— 该提示从未实现。

**现象（今天）**：中心一停用，连接直接被拒，界面上没有任何解释，用户看到的是
「连不上」而不是「认证中心没起来」。

- [ ] 桌面端：`deny_kind = no_center` 走 `frontend.*` 错误码（宿主/基础设施域）
- [ ] 移动端：close code 4001 携带原因时给出可读提示（**不是** toast 风暴 —— 移动端
      已在 2026-09-29 修过致命化 + 退避 + 同因熔断，见 ADR 0031 M1/M2 票）
- [ ] i18n key 两端 zh-CN / en 同步（AGENTS §6）

## 8.2 本专项新增：存量 token 失效需重配

迁移后存量已配对设备调 `handle_reauth` 会拿到 `invalid` 而非网络错误。用户需要知道
**「重新配对」是正确动作**，而不是以为网络坏了反复重试。

- [ ] 移动端区分「凭证失效（需重新配对）」与「网络错误」，**不同文案**（ADR 0030 错误码口径）
- [ ] 桌面端设置/连接页给出「检测到旧版本凭证，需重新配对」的显式信号
- [ ] 建议在 ADR 0033 §失败模式 F3 落账

## 关键实现事实

- `deny_kind` 三态是**结构化字段**（AGENTS §8 红线）：`no_center` / `unavailable` 属
  部署故障域，`policy` 属产品语义域。UI 文案必须体现这个区分——排障路径完全不同
- **禁止**把 JWT / claims / 密钥片段写进任何用户可见文本（凭据红线）
- 移动端已修的「认证类关闭码致命化」（`bedcode-mobile/.../connection/ws_client.rs`）
  是本票的前置——否则中心停用会再次变成 6 Hz 日志/toast 风暴

## 验收

- 中心停用 → 桌面与移动端都给出点名原因的提示（截图或 CDP 实测留证）
- 旧 token 重配 → 文案明确指向「重新配对」，非「网络错误」
- i18n key 在 zh-CN 与 en 两文件同步
- 桌面 vitest 全量 + 移动 vitest 全量绿
- 端到端复核：拔掉/停用中心插件不再出现无解释的静默失败
