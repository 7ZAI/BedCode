# 05：统一集成测试执行 + 全量回归（收尾）

**Type:** task
**Spec:** `../spec.md`（§5 验收标准）；契约单一事实源 `docs/adr/0030-error-envelope-and-user-prompt-boundary.md`
**Blocked by:** 01, 02, 03, 04（全部）
**Status:** ready-for-agent

**What to build:** 按用户指令（2026-09-27）：票 01–04 **只编写不执行**的集成测试在本票**统一执行**；同时按 AGENTS §3/§10 跑全量回归（Rust 全量 + 前端全量 + lint），任一红即修复到绿。产出「界面零技术详情」最终状态的验收证据：集成测试清单结果 + 全量套件结果 + grep 复核。

**Acceptance:**

- [ ] 票 01–04 编写的全部集成测试在本票统一执行（含宿主 wasm 闭环 fixture 用例；按 AGENTS §3 用 rustup shim 的 cargo，禁止绕过 shim；测试后检查并关闭残留后台进程/端口）
- [ ] Rust 全量：`cargo test` 通过（桌面端 `bedcode-desktop/src-tauri`）
- [ ] 前端全量：`pnpm run test:run` 通过（对应端；禁止 `pnpm run test` watch 挂起）
- [ ] `pnpm exec eslint .` 0 error（warning 不计入门禁）；`cargo fmt` / `cargo clippy` 自查
- [ ] i18n：新增键 zh + en 同步存在；无 `{error}` 残留
- [ ] 契约回归防线全绿（信封序列化契约 / i18n 扫描 / toast 消费层断言）
- [ ] 收尾证据：集成测试清单与结果、全量通过截图/输出、`src/` grep 零技术原文直显；`lens_diagnostics` 无 blocker
- [ ] 测试后进程清理：无 mock server / vitest worker / gradle daemon 等残留进程占用端口或 CPU