# 02: E2E 基础设施搭建

**What to build:** 落地 WebdriverIO + `@wdio/tauri-service` 的 E2E 框架骨架——引入 `tauri-plugin-wdio`（`cfg(debug_assertions)` 隔离，不进 release）、前端注入 `@wdio/tauri-plugin`、capabilities 授权、`wdio.conf` 配置，并用一个最小 smoke 断言证明「`browser.tauri.execute()` → Tauri IPC → Rust 命令」链路在真实应用内跑通。**不含任何业务测试用例**（会话/配对/UI 路径用例留到下一版本）。

**Blocked by:** None（可立即开始）

**Status:** ready-for-agent

- [ ] `tauri-plugin-wdio` 以 `cfg(debug_assertions)` 条件注册，release 构建不含该插件且能正常编译
- [ ] 前端入口注入 `@wdio/tauri-plugin`，仅测试环境生效
- [ ] capabilities 增加 wdio 权限（`wdio:default` 或最小化 `wdio:allow-execute`）
- [ ] `wdio.conf` 配置 `@wdio/tauri-service`，driverProvider 用 external `tauri-driver`
- [ ] 一个最小 smoke 断言本地跑通（如 `execute` 读 `window.location.href` 或调 `ping`），证明框架链路可用
- [ ] 本地跑 `tauri:build`（或 release 编译）确认测试插件未泄漏进生产产物
