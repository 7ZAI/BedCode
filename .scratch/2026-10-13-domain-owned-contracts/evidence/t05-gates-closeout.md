# T-05 收口证据（2026-10-13）

三把契约结构锁 + 文档口径全扫 + 全量门禁结果。逐项记录见
`../issues/05-locks-docs-gates-closeout.md` 的 Comments。

## 新锁与变异自检（3/3：改坏即红、点名地点、还原 sha 一致）

| 判据 | 注入形态 | 现象 | 还原 |
| --- | --- | --- | --- |
| `core_wit_stays_mechanism_only` | `core.wit` 追加 `interface host-evil` | 红，点名 `["host-evil"]` | `053c55e3…` 一致 |
| `compose_caps_map_to_the_right_contract_houses` | 移动 caps 注入 `pty` | 红，点名「移动 caps 不得含 `pty`」 | `6d5ccd5f…` 一致 |
| `capability_domains_stay_end_neutral` | `mdns.wit` 追加 `interface host-fs-mobile` | 红，点名白名单外 interface | `ac735e60…` 一致 |

## 全量门禁（实际运行）

| 面 | 命令 | 结果 |
| --- | --- | --- |
| WIT 漂移锁 | `pnpm run check:wit` | 19/19 ok（双端生成物） |
| 内核 | `packages/bedcode-wasm-core`: `cargo test --lib` | 577/577 |
| 能力域 | `discovery-engine --features common-host` 35/35 · `file-transfer-core` 76/76 | 绿 |
| 桌面宿主 | `bedcode-desktop/src-tauri`: `cargo test --no-fail-fast` | 35 个二进制绿；仅 2 个基线红二进制（见下） |
| 移动宿主 | `bedcode-mobile/src-tauri`: `cargo test --no-fail-fast` | 全绿（lib 226 + 全部集成；probe 4/4 · composition 3/3 · purity 7/7） |
| wasm 应用 | file-transfer `cargo test` 83/83（mount_local 5/5） | 绿 |
| SDK | 移动 `--features wasm` 全绿；桌面 `--features wasm` 158 passed / 4 failed（`wire/key.rs` 基线红） | 无新增红 |
| 前端桌面 | `bedcode-desktop`: `pnpm run test:run` | 116/117 文件；唯一红 = `terminalPreview` resize 重试（HEAD 既有，T-03 同记录） |
| 前端移动 | `bedcode-mobile`: `pnpm run test:run` | 78/78 文件 / 895 用例 |
| 前端 lint | 根 `pnpm exec eslint .` | 0 error / 97 warnings |

## 基线红（非本票引入，均已证据化）

| # | 面 | 证据 |
| --- | --- | --- |
| 1 | `capability_crates_no_product_ids`（1 例）+ `capability_crates_unit_tests_only`（2 例） | `bedcode-host-api-core` 在 HEAD 即有 crate 根 `tests/boundary_lock.rs`（`git ls-tree HEAD` 在场）；两 crate 未登记 `SCANNED_CRATES`/`PENDING_SCAN_CRATES`（`git show HEAD:…` 同缺）。属并行线登记面，本票只补登 `bedcode-notify-engine` |
| 2 | 桌面 SDK `wire/key.rs` 4 例遗留格式 | 临时 worktree（HEAD）同判据同红 |
| 3 | 桌面 vitest `terminalPreview` resize 重试 | HEAD 既有（T-03 记录同款） |
| 4 | `cross-end-tests` 不编译 | 在途 API：`PluginHost::new` 6→8 参（host-facts 单形态）、`AppContext::db()` tokio→std Mutex、移动 `session::http`/`terminal_link` 退役；均来自 domain-owned 之外的并行线，本票未触碰其源 |

## 连带修复（锁/文档事实性）

- `empty_dir_lock.rs` 源码根约定 `bedcode-mobile/plugins/*/src` → `wasm-apps/*/src`
  （2026-10-08 目录改名的旧路径）；清理遗留空目录 `bedcode-mobile/src/__tests__/{config,views}`。
- `packages/bedcode-discovery-engine/src/routing.rs` 的 `EXPORTS` 用后缀形态
  `bedcode:plugin/host-mdns.browse@0.2.0`（与前缀形态 `bedcode:plugin@0.2.0` 不同），
  bump 时由 `wit_package_version_lock` 抓出并修正。
- 内核 `contract.rs` 的版本字面量断言改为由真源 `PACKAGE_VERSION` 派生（同类漏改的根治法）。
