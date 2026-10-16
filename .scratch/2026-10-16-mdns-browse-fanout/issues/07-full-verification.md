# 票 07 — 全量验证：spec §7 清单逐项「跑了/没跑」

Status: resolved
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

## Comments

### 门禁结果汇总（integration worktree `BedCode-mdns-int`，集成分支 tip `2db8de0ee` = T1–T6 全合入；2026-10-16 执行）

| # | 命令（各自 crate 根） | 结果 | 备注 |
| --- | --- | --- | --- |
| 1 | `packages/peer-net`：`CARGO_BUILD_JOBS=2 cargo test` | ✅ lib 85 + discovery_injection 1 + harness 9 + pause_symmetry 4 + shared_dirs 6 + transfer_session 13 = **118 passed / 0 failed** | 与票 03 交付一致 |
| 2 | `packages/bedcode-discovery-engine`：`CARGO_BUILD_JOBS=2 cargo test` | ✅ **45 passed / 0 failed**（含 type_hub 8 例 + 锁 3 例） | 与票 02/05 交付一致 |
| 3 | `packages/bedcode-server-peer-net`：`CARGO_BUILD_JOBS=2 cargo test` | ✅ **56 passed / 0 failed** | 与票 04/05 交付一致 |
| 4 | 桌面 `bedcode-desktop/src-tauri`：`CARGO_BUILD_JOBS=2 cargo test --no-fail-fast` | ⚠️ **37 个测试二进制全绿**，仅 1 失败：`capability_crates_unit_tests_only::governed_crates_have_no_crate_level_integration_test_surfaces`（`bedcode-host-api-core/tests/boundary_lock.rs`） | **既有基线红**，票面已预登记（dev 提交 `2d6ae198f` 引入，属 0047 在途工作面），非本改动 |
| 5 | 移动 `bedcode-mobile/src-tauri`：`CARGO_BUILD_JOBS=2 cargo test --no-fail-fast` | ✅ **37 个测试二进制全绿，395 passed / 0 failed** | 含 `retired_mobile_mdns_advertise_lock` / `peer_engine_consolidation_lock` / `shared_artifact_probe` |
| 6 | `cross-end-tests`：`CARGO_BUILD_JOBS=2 cargo test --test mdns_health_flow` | ✅ **1 passed / 0 failed**（M-001 桌面广播→移动旧链路发现 / M-002 /api/health / M-003 stop 清列表） | 票 04 门禁面再次确认，两端 wasm 产物在位 |
| 7 | `cross-end-tests`：`CARGO_BUILD_JOBS=2 cargo test` 全量 | ✅ **EXIT 0，无失败**（12 个测试二进制编译并运行；若干既有 ignored：`单进程 rig 装不下两端宿主域实现`——inventory 进程级注册冲突 / 无头无 AppHandle 弹窗，非本改动） | 两进程 rig 场景按既有忽略口径，未新增 |
| 8 | 锁变异自检复跑（`mdns_browse_singleton_lock`） | ✅ 变异 A（白名单条目改悬空）→ `browse_and_stop_browse...` + `service_daemon...` **红**；变异 B（engine.rs 加 `.browse(` 探测行）→ 红；均还原后 **3/3 全绿** | 变异注入经 git diff 确认零残留 |

### 环境处理（不入提交，记录备用）

- **跨端半程磁盘耗尽**：`/` 分区首次 100% 满导致移动端 `shared_artifact_probe` / `component_e2e` 与 cross-end `pairing_auth_flow` 等链接失败（`No space left on device`）。处置：清理 `/tmp` 旧日志/临时目录、`BedCode-mdns-t4/target`（票已合入的残留产物）、sccache 缓存、npm/gradle 缓存，恢复 ~71G 可用空间后重跑通过。属环境治理，无代码影响。
- **zip 4.6.1 yank 撞车**：fresh worktree 无 `src-tauri/Cargo.lock`（gitignore 面内）时在线解析 `zip = "^4.6"` 失败（4.6.0/4.6.1 均已 yanked）。处置：从主仓复制 `Cargo.lock`（钉 4.6.1，本地缓存在场）后 `cargo metadata` / `cargo test` 全通。已在本票执行时记录，未改任何依赖版本（按票面「不做上游升级」）。

### 未跑项（原因）

| 项 | 状态 | 原因 |
| --- | --- | --- |
| 真机清单 a–f（双端互见 / goodbye 消失与自动恢复 / purge 回收与 peer 不受影响 / 慢消费者隔离 / 移动旧链路发现页面回归 / 生命周期日志核对） | ⏳ 没跑 | **无连接设备**：需双端真机人工操作（桌面临近 + Android 真机同网段），交用户后续验收；代码侧等价面已由 cross-end `mdns_health_flow`（发现 + stop 清列表）与各 crate 单测覆盖 |
| 前端 vitest / eslint | 不跑 | 本改动**零前端**（无 vue/ts 文件变更），按票面理由跳过 |
| WIT `pnpm run check:wit` | 不跑 | 本改动**零 WIT / 零 ABI**（无契约变更、无 bump），按票面理由跳过 |
| `cargo fmt` / `cargo clippy` 全量 | 自查项（非 CI 门禁） | 各票已在票面记录既有漂移/warning 归属（peer-net 无 rustfmt.toml 存量、engine wire.rs 存量格式差异等），本票未重跑全量 |
