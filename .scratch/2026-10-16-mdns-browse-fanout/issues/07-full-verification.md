# 票 07 — 全量验证：spec §7 清单逐项「跑了/没跑」

Status: ready-for-agent
Type: task
Blocked by: 06

## 清单（在 integration worktree 执行；重命令串行 + `scripts/adaptive-run.mjs`，防 OOM）

- `packages/peer-net`、`packages/bedcode-discovery-engine`、`packages/bedcode-server-peer-net` 各自 `cargo test` 全量
- 桌面 `bedcode-desktop/src-tauri`、移动 `bedcode-mobile/src-tauri` 各自 `cargo test` 全量
- `cross-end-tests`：先 `cargo test --test mdns_health_flow`，后 `cargo test` 全量（两端 wasm 产物已在位；两进程场景前置缺失则记录）
- 锁变异自检复跑（票 05 两条）
- **环境前提（fresh worktree 缺省没有，跑前从主仓复制，均在 gitignore 面内）**：`bedcode-desktop/src-tauri/resources/plugins/desktop` 与 `bedcode-mobile/src-tauri/resources/plugins/mobile`（随包 wasm 产物）；`bedcode-desktop/target/fixtures/wasm32-wasip3/release/*.wasm`（桌面测试夹具）
- **已知基线红（非本改动，勿误报）**：桌面 `src-tauri` 的 `capability_crates_unit_tests_only::governed_crates_have_no_crate_level_integration_test_surfaces` 在 dev 即红（`packages/bedcode-host-api-core/tests/boundary_lock.rs` 未登记例外桶，dev 提交 `2d6ae198f` 引入）——记录「既有红 + 证据」即可，不修（属 0047 在途工作面）
- 真机清单 a–f（spec §7）：无连接设备 → 逐项「没跑 + 原因：需双端真机人工操作」，交用户后续验收
- 前端 / WIT 面：本改动零前端、零 WIT → 不跑 vitest / `pnpm run check:wit`，写明理由

## 产出

本票 `## Comments` 内以表格汇总：命令 / 结果 / 未跑项原因
