# 04 — 端点请求面透传 + 无 JWT 门禁信任模型文档化

**What to build:** 两个独立但同属「服务端点完善」的工作：

1. **请求面增强**（审计 H5 前半）：向插件端点透传白名单内的调用方请求 headers（白名单避免透传全部头；与 WS spec 阶段 A 的过滤链/链路加密通道保持区分，本期不做 WS）；响应侧支持插件指定 content-type（默认保持 application/json）。请求参数对象增量加字段（增量演进原则，老插件忽略未知字段）。
2. **信任模型固化**（审计 H4）：`/api/plugin/*` 对无 JWT 请求放行（hook 脚本场景）+ 仅激活检查 + 服务监听 0.0.0.0 → 局域网任意客户端可达的现实，以 ADR 或 knowledge 文档固化：写类端点必须插件自查认证，宿主不替代。**不改门禁行为**（移动端与脚本兼容红线）。

**明确不做：** 给 `/api/plugin/*` 加独立 token / 按插件粒度鉴权（安全升级列为现状文档化之外的后置项）。

**Blocked by:** None.

**Status:** `ready-for-agent`

- [x] headers 白名单透传（确定白名单字段集）+ 单测（白名单内外正反例）
- [x] 响应 content-type 支持 + 单测；默认行为保持 application/json（回归）
- [x] 信任模型 ADR/knowledge 文档（含 0.0.0.0 监听、无 JWT 放行、激活检查、插件自查建议）
- [x] TS-only 插件端点桥接记为已知缺口（现状语义，插件端点 = Rust/WASM 能力）
- [x] 桌面 cargo test 全绿 + eslint 0 error

## Comments