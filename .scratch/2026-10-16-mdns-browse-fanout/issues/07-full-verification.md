# 票 07 — 全量验证：spec §7 清单逐项「跑了/没跑」

Status: ready-for-agent
Type: task
Blocked by: 06

## 清单（在 integration worktree 执行；重命令串行 + `scripts/adaptive-run.mjs`，防 OOM）

- `packages/peer-net`、`packages/bedcode-discovery-engine`、`packages/bedcode-server-peer-net` 各自 `cargo test` 全量
- 桌面 `bedcode-desktop/src-tauri`、移动 `bedcode-mobile/src-tauri` 各自 `cargo test` 全量
- `cross-end-tests`：先 `cargo test --test mdns_health_flow`，后 `cargo test` 全量（两端 wasm 产物已在位；两进程场景前置缺失则记录）
- 锁变异自检复跑（票 05 两条）
- 真机清单 a–f（spec §7）：无连接设备 → 逐项「没跑 + 原因：需双端真机人工操作」，交用户后续验收
- 前端 / WIT 面：本改动零前端、零 WIT → 不跑 vitest / `pnpm run check:wit`，写明理由

## 产出

本票 `## Comments` 内以表格汇总：命令 / 结果 / 未跑项原因
