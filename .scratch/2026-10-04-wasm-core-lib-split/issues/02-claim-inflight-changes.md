# 02: 认领 wasm_core 在途改动，定开工顺序

**What to build:** 工作区里存在一批**非本 spec 的未提交改动**，正在对 `wasm_core` 做目录化拆分（两个大文件被大幅删减并各自新增了 `tests/` 子目录），改动方向与本 spec 高度重叠。按 AGENTS §11「非本任务的改动一律不碰、不回滚」，本票先把这批改动认领清楚，再决定后续 8 票何时开工。

本票交付的是一个**决策 + 边界**，不是代码改动。

需要判定并回答的四个问题：

1. **归属**：这批改动是谁/哪条工作线的产物？是否已完成、是否打算提交？
2. **重叠面**：它改的两个大文件分别落在本 spec 哪几票的射程内？（其中一个是 ws 域绑定层 ⇒ 04；另一个属 security 网关区 ⇒ 与 03 的 mdns 同区）
3. **合并还是串行**：若那批工作本身也在做 crate 化 / 目录化，则本 spec 的对应票应与之合并而不是并行改同一批文件；否则保持串行、待其落定后开工。
4. **量测基准刷新**：本 spec 全部行数取自 HEAD。若在途改动会被提交，本 spec §1 的量测基准与 spec 的规模结论需按新 HEAD 复核一遍。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 在途改动的归属有明确结论（谁做的、是否在途、是否计划提交）
- [x] 与本 spec 各票的重叠面逐项列出，明确哪几票会与它撞同一批文件
- [x] 给出「合并 / 串行等待」的明确决定与理由，写进本票 Comments 并同步 spec 的风险登记
- [x] 期间**未修改、未回滚、未 stash** 任何在途改动（`git diff` 可证）
- [x] 若结论是「等待」，则在本票注明预计可开工的前置条件；若是「合并」，则在 spec 里标出被合并掉的票号
- [x] 结论确定后，spec 的量测基准标注（HEAD / 新 HEAD）已相应更新

## Comments

- **Q1 归属：另一个并发会话的「内联 Rust 测试目录化拆分」工作线，不是本 spec 的 crate 化。**
  - 物证：两个**未跟踪**的新脚本 `scripts/audit-rust-tests.mjs`（度量内联 `#[cfg(test)]` 块规模）/ `scripts/split-rust-tests.mjs`（按分隔注释把内联测试拆成 `src/<mod>/tests/<name>.rs`，或 `<crate>/tests/<name>.rs` 公共 API 形态）。该批次同时做了 `mod.rs` → 同名 `.rs` 扁平化（AGENTS §6）与测试锁文件外迁（`capabilities_test.rs` → `tests/capabilities_lock.rs`、`hot_path_logging_test.rs` → `tests/hot_path_logging_lock.rs`，两者已 staged）。
  - 两者目标正交：它拆的是**测试落点**，本 spec 拆的是**实现归属**（域 → crate）。不是同一件事的两半，因此**不存在合并的可能**。
- **该批次已于 16:55~16:56 自行消失。** 时序证据（本会话实测）：
  | 时刻 | 事件 |
  | --- | --- |
  | 16:53:28 | `scripts/split-rust-tests.mjs` mtime |
  | 16:54:22 | `host_api/ws.rs` + `security/network_auth.rs` + 两组 `tests/` 子目录 mtime（拆分产物落盘） |
  | 16:55:07 | 本会话观测到两个 `tests/` 目录仍在，`ws.rs` 46,728 字节 |
  | 16:56 | `ws.rs` 回到 HEAD 的 1,960 行、`network_auth.rs` 回到 1,830 行、两个未跟踪 `tests/` 目录**已删除** |
  即并发会话自己回滚了这轮拆分，只剩两个脚本未跟踪。`git diff` 对这两个文件现为空。
- **Q2 重叠面（逐票）：**
  | 票 | 射程文件 | 与该批次是否撞 |
  | --- | --- | --- |
  | 03 | 新建 `packages/bedcode-host-kit` · `component.rs` · `host_api/mdns.rs` · `server/ports_impl.rs` | **否**（该批次未触这四个） |
  | 04 | `host_api/ws.rs`（整体搬进 `bedcode-server-websocket`） | **是** —— 唯一真重叠 |
  | 05 | `host_api/peer.rs` | 否 |
  | 06 | `host_api/http.rs` | 否 |
  | 07/08 | `src/db/**` + `host_api/{database,storage}.rs` | 否 |
  | — | `security/network_auth.rs` | 不在本 spec 任一票射程内（属留 core 的安全闸门，与 mdns 同区但不同文件） |
- **Q3 决定：不等待、不合并，直接开工；带一条开工前自检。**
  理由：重叠的那批改动已经不存在，树回到 HEAD 形状，票 04 无实际阻塞；强行「等」等于为一个已消失的阻塞付等待成本。**开工 04 前必须先 `git status --short bedcode-desktop/src-tauri/src/wasm_core/host_api/ws.rs` 确认干净** —— 若并发会话重启扫描并再落一次拆分，票 04 应改为「先提交那批拆分、再在其新形状上搬迁」，不得在同一文件上双写（AGENTS §11）。
- **Q4 量测基准：不变，仍是 HEAD `dbe50d229`。** 并发会话未提交任何东西，HEAD 未前进；且 `ws.rs` / `network_auth.rs` 已回到 HEAD 形状 ⇒ spec §1 的 **† 声明（行数已变）可以撤销**，恢复为「全部量测取自 HEAD」。R6 保留但降级为「开工 04 前的一次自检项」。
- 附带：本票期间**未修改、未回滚、未 stash** 任何在途改动；本会话对工作区的写入只有票 01 的两个注释文件 + 本票/ spec 的 markdown。

### 2026-10-04 17:34 追记：并发会话**又回来了**，本票结论需按「反复」而非「已消失」读

票 02 结案时（16:56）判定那批改动已自行回滚。执行票 03 期间（17:00:29）实测：

```
bedcode-desktop/src-tauri/src/wasm_core.rs
bedcode-desktop/src-tauri/src/wasm_core/security/strategy.rs
bedcode-desktop/src-tauri/src/wasm_core/security/network_auth.rs
bedcode-desktop/src-tauri/src/wasm_core/security/network_auth/tests/*.rs   ← 未跟踪目录重现
```

即那个「内联 Rust 测试目录化拆分」的并发会话**再次启动并在扫 `wasm_core`**。
`git status` 里宿主侧待改文件也比票 02 时多了一组（`pty/pty_process.rs`、`pty/wsl.rs`、
`server/ports_impl.rs`、`system/config.rs`、`system/lifecycle.rs`、
`utils/auth/test_tokens.rs`、`utils/session_gateway.rs`）。

**修正本票结论的读法**：不是「那批改动消失了、无并发风险」，而是
**「存在一个反复在 `wasm_core` 与宿主若干文件上工作的并发会话」**。它当前的目标是
测试拆分（非 crate 化），但它会持续扫过本票链要改的同一批文件。

**对本票链的操作含义**：

| 票 | 风险 | 处置 |
| --- | --- | --- |
| 03 | 要改 `component.rs` / `host_api/mdns.rs` / `server/ports_impl.rs` / `config.rs` / `monitor.rs` / `runtime.rs` | 动手前逐个 `git status` 该文件；若并发会话正在改同一文件，**先等它落定**，不得同文件双写（AGENTS §11） |
| 04 | 要整体搬 `host_api/ws.rs` | 同上；票 02 已把这条列为开工自检项 |
| 05-08 | `host_api/{peer,http,database,storage}.rs` / `src/db/` | 同上 |

**本会话自身的写入**（可 `git status` 核验）：仅新增 `packages/bedcode-host-kit/`
与本票链的 markdown；**未触碰任何宿主文件**。
