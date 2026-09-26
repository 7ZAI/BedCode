# 移动端适配桌面端 WS 硬切 — 票据索引

上游 spec：[`../spec.md`](../spec.md) ｜ 排查报告：[`../survey.md`](../survey.md)

本目录把 spec §5 的 P0–P6 拆成 7 张可独立交付的票。**桌面宿主零改动**是本专项硬约束
（`bedcode-desktop/src-tauri/src/**`、`packages/plugin-sdk-desktop/**`、WIT/SDK/ABI/权限、
`plugin.json` 零 diff）；唯一允许的桌面改动是 `terminal-session` wasm 应用内的最小补广播。

## 票据

| 票 | 阶段 | 内容 | Blocked by | 主要落点 |
| --- | --- | --- | --- | --- |
| [01](01-baseline-and-fixture.md) | P0 | 基线与假插件端点夹具 | 无 | `bedcode-mobile/src-tauri/tests/` + 前端 vitest mock |
| [02](02-plugin-broadcast-events.md) | P1 / D1 | 桌面插件补广播（唯一出口 + 7 事件） | 无（帧壳定稿解锁 03） | `wasm-apps/terminal-session/rust/src/**` |
| [03](03-mobile-event-channel.md) | P2 / M1+M2 | 移动端事件通道（session-control 常驻 + 对账） | 02 | `src-tauri/src/connection/**`、`handler/sync.rs` |
| [04](04-mobile-control-plane-http.md) | P3 / M3 | 控制面迁 HTTP + 旧信封退役 | 03 | `src-tauri/src/{session,commands}/**` |
| [05](05-mobile-terminal-stream.md) | P4 / M4 | 终端流新协议重写 + 前端 TB v3 退役 | 01, 03 | `src-tauri/src/terminal_link.rs`、`src/stores/terminalBuffer.ts` |
| [06](06-encryption-retire-and-cleanup.md) | P5 / M5+M6 | WS 加密退役 + 清理 + 文档 | 03, 04, 05 | `src/composables/useLinkEncryption.ts`、locales、docs |
| [07](07-integration-run-and-final-gates.md) | P6 | **集成测试统一运行** + 全量门禁 + 真机联调 | 01–06 | 全仓 |

依赖序（建议执行序）：`01 → 02 → 03 → 04 → 05 → 06 → 07`。
并行安全点：02 可与 01 并行；04 与 05 在 03 完成后可并行；06 需 03/05 两条连接路径重写落地。

## 测试节奏（本专项强制）

- 每个实现票**只跑针对性单元测试**自验（AGENTS §3 测试两段式：过滤命令，红了立即修）。
- **单个票不运行集成测试**；但**可以写**集成测试（Rust `bedcode-mobile/src-tauri/tests/`、
  前端 `bedcode-mobile/src/__tests__/integration/`），写好后在该票末尾的
  「集成测试（待票 07 运行）」小节列出**文件路径 + 用例名**，不在本票执行。
- 全部实现票（01–06）完成后，由 **[07](07-integration-run-and-final-gates.md) 一次统一运行**
  所有集成测试 + 全量回归 + 真机联调。
- 跑测后清理残留进程/端口；cargo 一律走 rustup shim（`~/.cargo/bin/cargo`，AGENTS §3）。
- 单元测试的开发/审查先加载 `unit-test-discipline` skill；前端 UI 改动先加载 `frontend-styles` skill。

## 完成定义（专项级）

见 spec §10。票据级完成 = 该票验收清单全勾 + 单测绿 + 集成测试已写待运行 + 无宿主 diff。
