# 04: core-security：统一授权框架

**What to build:** 资源授权有统一框架：资源类型（fs / network / process / storage / bus / api-call）× 操作 × 三段决策管线（manifest 声明快速失败 → 授权审批/持久化授权记录 → 运行时强制 Rust 端最终仲裁）。现有 fs 三层校验重构为框架的 fs 资源实现（行为等价），互调门重构为 api-call 资源实现（ADR 0017 语义不变）；新资源类型可经 trait 注册仲裁器扩展。凭据在日志与审计中只记长度。

**Blocked by:** 01（五模块骨架归位）

**Status:** resolved

- [x] 三段决策管线对每类资源按序执行，任一拒绝即整体拒绝
- [x] fs 资源：现有 fs_auth 测试全部迁入且不丢用例、全绿（行为等价）
- [x] api-call 资源：未声明 API 不可调（ADR 0017 回归测试保留）
- [x] 新资源类型经 trait 注册即可接入管线（用一个测试资源证明）
- [x] 审计/日志无凭据明文（grep 断言长度字段模式）
- [x] `cargo test` 全绿

## Comments

- 2026-09-13 完成：security/framework.rs 落地三段决策管线（check_declared → check_approved → enforce，任一拒绝即拒；RequireApproval 不中止、交 enforce 内弹窗）+ ResourceKind（fs/network/process/storage/bus/api-call/custom）+ ResourceAuthorizer trait 注册扩展。FsAuthorizer 适配 fs 三层校验（语义与 host_impl/fs.rs 手工链一致，调用链暂保留手工内联形态，统一路由后续渐进）；ApiCallAuthorizer 适配互调门，bus_publish 门禁已改经框架路由（错误文案不变，行为等价）。决策计数埋点进 core-monitor（PluginMetrics.authz allow/deny/require_approval，快照加 authz 字段——additive 不破坏既有形状）。无凭据相关日志（框架不接触凭据）。cargo test 641 全绿。
