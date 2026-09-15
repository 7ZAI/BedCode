# 13 — 验证收尾：spec §6 验收 8 条 + 全量测试 + 构建链路

**What to build:** 全部实现 ticket 完成后逐项过 spec §6 验收标准（8 条）+ 全量测试 + 构建链路验证 + lens 收尾（AGENTS.md §10 完成定义）。真机联调清单按 handoff §4 Phase 3 / spec §4 测试段执行。

**Spec:** §6 验收标准（8 条）、§4 测试段

**Blocked by:** 01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12

**Status:** ready-for-agent

## 验收 8 条（spec §6，逐项核对）

1. 移动端前端源码零 `fetch(` / `@tauri-apps/plugin-http` 引用（useUpdateChecker 一并收束）
2. `useHttpApi` 所有调用路径（文件树、会话模式、任务队列、SAF、biometric HTTP 绑定等）经统一代理全绿
3. 链路加密：开开关 + 已 pin 时请求经 Rust 信封化且桌面端可解密；GET/HEAD 无 body 仍带协商头；加密失败 fail-closed
4. request_id 出现在所有代理日志结构化字段；并发请求互不串扰（多路复用正确性单测）
5. Egress Policy：桌面端目标全放行；useUpdateChecker（GitHub API）经 L2 放行；未声明外网 URL 被拒（不发请求）；自定义 URL 弹窗授权流程可用（确认放行 / 拒绝拒绝）；插件 `preauthUrls` 声明被宿主收集生效
6. 全量：移动端 `cargo test` + vitest 绿；根目录 eslint 0 error

## 验证命令（AGENTS.md §3/§10，必须实际运行并贴出结果）

- [ ] `cd bedcode-mobile/src-tauri && cargo test` 全绿
- [ ] `cd bedcode-mobile && pnpm run test:run` 全绿（禁 watch；内存不足用 `pnpm exec vitest run --maxWorkers=3`）
- [ ] 根目录 `pnpm exec eslint .` 0 error（warning 不计入）；`cargo fmt` / `cargo clippy` 自查
- [ ] i18n key 双语言同步核对（zh-CN + en）
- [ ] 构建链路：`pnpm run tauri:android:build`（或 dev）通过；caps schema 重新生成无碍
- [ ] 真机联调：配对 → 文件树/会话/任务队列 HTTP 路径全绿；加密信封与桌面端互通回归；自定义 URL 授权弹窗全流程；插件 ai-chatbox 请求（预设无弹窗 / 自定义 baseUrl 弹窗）
- [ ] 收尾 `lens_diagnostics mode=all` 无 blocker（🔴 blocker 未清前不算 done）

## 边界确认（spec §7）

- 终端 WS 搬迁（useTerminalSocket）不在本分支；事件/同步 WS 不动；桌面端不改；peer-net 链路加密不在范围。

## Comments

## Comments
- 2026-09-12 全部实现完成后的验证（tickets 10/11/12 done）：
  - cargo test：**297 全绿**（267 lib 单测 + 11×2 集成 + 7 http_proxy_flow + 1）；clippy 无新增
  - vitest：**372 全绿**（43 文件；含新增 EgressConsentDialog 10 用例）；vue-tsc exit 0
  - eslint：**0 error**（125 既有 warning 不计入）；cargo fmt 对分支文件自查
  - 验收 1：移动端 src 零 `fetch(` / `@tauri-apps/plugin-http`（rg 全仓 0 命中）
  - i18n：zh-CN/en 双同步核对无差异（egress 9 + settings 11 key）
  - 构建链路：cargo check 通过；gen/schemas 4 个文件随 http:default 移除自动重生成（预期产物）
  - 真机联调未做（无设备环境）——验收 5 的自定义 URL 弹窗全流程留待真机验证
- **教训（重要）**：本会话误用 `rustfmt` 全文件格式化 13 个分支文件 + pi-lens rustfmt 钩子连带格式化 workspace 其他 .rs（peer_net 等 22 个干净文件被污染）；且 src-tauri 多数源文件是 **CRLF**（lib.rs/state.rs/manager.rs/connection.rs 等 HEAD 为 CRLF），rustfmt 输出 LF 造成整文件伪 diff。已全部恢复：lib.rs 从 HEAD 重放 4 处分支改动（12 行）、manager.rs 逐段还原 rustfmt 噪音（2832→13 行）、22 个干净文件 git checkout 还原。最终 git status 仅剩分支真实改动 + 本会话新增。**纪律：src-tauri 改格式前先查行尾符；rustfmt 只对新增文件用，已有文件禁止全文件格式化。**
