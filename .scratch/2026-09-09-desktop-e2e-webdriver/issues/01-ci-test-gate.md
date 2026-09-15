# 01: CI 测试门禁上线（Rust + vitest）

**What to build:** 合并到 master/uat 时，CI 自动跑桌面端 + 移动端的 Rust 测试（`cargo test`）与两端前端测试（`pnpm run test:run`），任一层失败即阻断合并，让回归测试从「本地 Done When 自觉」升级为「合并门禁」。

**Blocked by:** None（可立即开始）

**Status:** ready-for-agent

- [ ] CI 在合并 master/uat 时跑桌面端 `cargo test`，失败阻断
- [ ] CI 在合并 master/uat 时跑移动端 `cargo test`，失败阻断
- [ ] CI 跑两端 `pnpm run test:run`，失败阻断
- [ ] 保留原有 eslint 门禁不变
- [ ] 测试按分层独立 job（Rust 桌面 / Rust 移动 / 前端两端），失败信息能定位到具体层
