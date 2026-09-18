# 01 — host-http 补 `network:http` 权限仲裁

**What to build:** 修复审计发现 H1（最高优先，Rust 端权限断链）。`host-http.fetch` 宿主入口补充权限检查：未声明 `network:http` 的插件调用返回明确权限拒绝错误。完全复用其他域既有 `check_permission` 模式（签名/语义/测试形态），无新机制、无配置项。TS 路径已在前端 fast-fail，本票只补 WASM/Rust 最终仲裁。

行为收紧是**有意的安全修复**，但必须核对存量：全仓检查已装插件 manifest（重点是 ai-chatbox / agent-hub 这类实际调用 host-http 的）是否声明 `network:http`，漏声明的一并处修（属权限声明修正，不算破坏兼容）。对「未声明的存量违规插件」与「存量新调用」的边界测试要覆盖两类错误消息：权限拒绝 vs 请求错误。

**Blocked by:** None — can start immediately.

**Status:** `ready-for-agent`

- [x] `host-http.fetch` 入口补 `check_permission(PERMISSION_NETWORK_HTTP)`（对齐 host_impl/mod.rs 既有模式 + 操作名 `host_http_fetch`）
- [x] 全仓核对存量插件 manifest 的 `network:http` 声明（ai-chatbox / agent-hub / auto-task / file-transfer），漏声明处随票修正
- [x] 单测：未声明权限拒绝 / 声明后放行（正反例，参照 host_impl/mod.rs 既有 check_permission 测试）；拒绝时的错误消息含清晰原因
- [x] 既有依赖 host-http 的插件测试全绿；桌面 cargo test 全绿 + eslint 0 error
- [x] SDK 注释补充：host-http 需要 `network:http` 权限（guest 侧文档同步）

## Comments