# 票 04 — 收尾：全量回归 / 文档同步 / 诚实边界复核

**状态**：resolved · 2026-10-01
**类型**：task
**依赖**：01、02、03（全部 resolved）

## 改动面（最终）

| 类别 | 文件 |
|---|---|
| 新增场景 | `cross-end-tests/tests/{event_channel_flow,http_proxy_flow,mdns_health_flow}.rs` |
| 共享装配 | `cross-end-tests/tests/common/{desktop_ctx,mobile_ctx}.rs` |
| 依赖 | `cross-end-tests/Cargo.toml`（+3 个 **dev**-dep：`bedcode-server-websocket` / `bedcode-server-core` / tauri `test` feature 已由移动端统一开启） |
| 唯一生产代码改动 | `bedcode-mobile/src-tauri/src/mdns/discovery.rs`（+27/−4，`start_headless` 无头装配缝，行为不变） |
| 既有测试修正 | `cross-end-tests/tests/session_http_flow.rs` C-003 轮询谓词（等终态而非「不再 running」，见下方诚实记录第 2 条） |
| 文档 | `docs/knowledge/mobile-desktop-auth.md`（场景表 +3 行、诚实边界 +3 条、装配约束三条）、`cross-end-tests/README.md`（结构图 + 装配约束 + 只读观测依赖）、`CHANGELOG.md` / `CHANGELOG_zh.md` |
| 桌面端生产代码 | **零改动**（`git diff bedcode-desktop/` 为空） |

`AGENTS.md` §3 黄金命令**未改**：默认仍是 `cd cross-end-tests && cargo test`，示例过滤命令
`--test terminal_ws_flow` 依然成立。

## 回归证据（全部实际运行）

| 项 | 结果 |
|---|---|
| `cross-end-tests` 全量 | **11/11 绿**，连跑 3 次（另修既有 C-003 竞态后再验 3 次） |
| 桌面 `cargo test --no-fail-fast` | **884 lib 绿 / 0 红** + 全部集成 target 绿 |
| 移动 `cargo test --no-fail-fast` | **337 绿 / 0 红** + 集成 target 绿（唯一生产改动由此验证） |
| 移动 `pnpm run test:run` | **53 文件 / 524 用例**绿 |
| 桌面 `pnpm run test:run` | **112 文件 / 1422 用例**绿（见下方环境说明） |
| 根 `pnpm exec eslint .` | **0 error / 117 warning**（与改前同数），exit 0 |
| `cross-end-tests` `clippy --tests` | 本包 **0 诊断**，exit 0（14 条 warning 全在未改动的既有 crate） |
| `cross-end-tests` `cargo fmt` | 本轮**新增/修改**的文件全部 fmt clean；`cargo fmt --check` 仍报 `terminal_output_pressure.rs` 一处，因为**该文件与 HEAD 逐字一致**（HEAD 本身就不是 fmt-clean）——按最小改动原则未顺手格式化别人的文件（此前 `cargo fmt` 误碰过，已逐段还原，`git diff --quiet` 通过） |
| 残留 | 无测试进程 / 端口 / `/tmp/bedcode-*` 目录 |

## 诚实记录（不粉饰）

1. **桌面 vitest 在本机默认 worker 数下会 Node 堆 OOM**（`FATAL ERROR: Ineffective
   mark-compacts near heap limit`）：本机 13GB 内存、112 个测试文件。压到
   `--pool=forks --poolOptions.forks.maxForks=2` + 5GB 堆后 **112/1422 全绿、exit 0**。
   判定为**环境/资源配置**，非代码回归的依据：本会话**未改任何桌面前端文件**
   （`git status bedcode-desktop/(src|packages)` 为空），且失败项随 pool 配置漂移
   （单 fork 串行时 Button 等出现 5000ms 超时级联，而单跑 Button **7/7 绿**）。
2. **`session_http_flow` 曾两次失败——根因是**测试自己的轮询谓词**，已修**：
   全量跑时红，单跑不复现（单跑 12 次全绿）。第二次特意抓到了断言原文：

   ```text
   C-003 停止后会话终态必须是 stopped（pty:exit 收尾）
     left: Some("stopping")   right: Some("stopped")
   ```

   桌面端行为**是对的**（同一份日志里，紧随断言之后就是 `pty:kill → pty:exit →
   note_status(Stopped) → publish session:stopped`）。错的是测试：它轮询到「不再
   `running`」就断言，而 `stopping` 是 `SessionStatus::Stopping` ——插件模型里明写的
   「`Stopped` 之前的**过渡态**」。于是断言跑在 PTY 收尾之前，负载高时必现。

   **判据改为等终态**（与插件 `SessionStatus::is_terminal()` 同源：`stopped` / `error`），
   而不是「不再 running」。这是本轮唯一的**既有文件**改动（`session_http_flow.rs` 的
   C-003 轮询谓词），不在原票据范围内，故在此显式记录。修后：单跑 10 次 + 全量连跑
   3 次，**11/11 全绿、0 失败**。

   教训再次生效：**首跑红就定性缺陷太早**。第一次没抓到断言原文时我拒绝下结论；抓到
   之后才看清「红的是测试的谓词，不是产品」——若当时按「偶发抖动」放过或按「产品缺陷」
   上报，两种都是错的。
3. **磁盘曾打满（100%，剩 18MB）**导致宿主 `cargo test` 链接期报
   `No space left on device`（症状伪装成编译错误）。按既有做法删三个
   `target/debug/incremental`（合计约 16GB）后重跑全绿。

## 未跑项（预期，写明原因）

- `pnpm run tauri:build` / 真机核验：本任务无 UI 改动、无协议/ABI/版本号变动。
- `./gradlew :app:compileUniversalDebugKotlin`：`gen/android` 下 Kotlin 无改动。
- 范围外（沿用 spec §8）：peer-net 文件传输、生物认证正向路径、QR 扫码 UI。