# 03: E2E 无头化 + CI job

**What to build:** 让 E2E 在 CI 里无头自动跑——CI 安装 `webkit2gtk-driver` + `xvfb` + `tauri-driver`，新增独立 E2E job（`xvfb-run` 无头运行），按路径变更触发、失败阻断合并。

**Blocked by:** 02（E2E 基础设施搭建）

**Status:** ready-for-agent

- [ ] CI 安装 `webkit2gtk-driver`、`xvfb`，`cargo install tauri-driver --locked`
- [ ] 新增 E2E job，用 `xvfb-run` 无头跑通最小 smoke 断言
- [ ] E2E job 按相关路径 filter 触发，避免无谓开销
- [ ] E2E 失败阻断合并
- [ ] `tauri-driver` / `WebKitWebDriver` 依赖在 CI 里可缓存或稳定安装，流水线耗时可控
