# 票 01 · 基线收口（认领在途 + 确证基线绿）

> 输入：`spec.md` §5 T-01。前置：无（唯一前置——所有后续 ticket 依赖本票交出的干净基线）。
> **依赖：T-01 → T-02 → T-03 → {T-04 ∥ T-05} → T-06**（T-01 无并行者）。
> **测试节奏**：本票只做 `cargo check` 针对性自验（全量测试统一在 T-06 后集中跑）。
> 目的：工作区三层在途改动（0047 staged rename + codebuddy engine.rs 第一批 + docs/adr 4 份删除）认领提交，把后续批次建立在干净基线上。

## 1. 在途改动认领（已核实，2026-10-11 04:00）

| 来源 | 内容 | git 状态 | 归属 |
| --- | --- | --- | --- |
| 0047 会话 | `mobile→bare`、`desktop→common` 形态中立化 rename（38 文件）+ `host_impl/{auth,connection,notify,terminal_stream}` 删除 + WIT 生成物重拼（03:16，三处 core.wit sha256 一致） | staged（index） | 上一任务已核账，保留 |
| codebuddy 第一批 | `engine.rs` 723 行新建（**untracked**）+ `common.rs`/`bare/component.rs` 改调 `engine::trapped` 等 + `common/component/tests/stale_hint.rs` | untracked + unstaged | 本任务第一部分，保留并纳入 T-03 前置 |
| 本会话凌晨 | docs/adr 4 份过时决策删除（0015/0021/0025/0026） | unstaged（D） | 本任务独立项，一并提交 |

**红线**：`git status` 若出现本表之外的改动（#114 之外），先停下核实来源再动；严禁对他人文件整文件回滚（§11 文件回滚规范）。

## 2. 提交组织（按 AGENTS.md §11 纪律——路径限制提交）

- **提交 A（docs）**：docs/adr 4 份删除（机因记录见 memory daily 2026-10-11）。
- **提交 B（rename 中立化）**：0047 staged 的 desktop→common / mobile→bare rename + host_impl 4 域删除 + WIT 重拼生成物。**注意当日教训**：`git commit -m ... -- <paths>` 是工作树提交，若同一文件含他人改动须先 edit 逐段还原再提交。
- **提交 C（engine.rs 第一批）**：untracked `engine.rs` + common/bare 壳改动 + 相关测试。按功能拆为「engine 配置面合一」「导出判定合一生」两个提交也可，但**不拆分跨文件混改**。
- 全部用 `git commit -- <paths>` 路径限制；提交前 `git status` 核对 staged 区（第一列非空格）不能夹带他人内容。

## 3. 门禁（实跑贴结果）

- 提交后工作区 `git status` 清零（除本票自身的记录文件）。
- `packages/bedcode-wasm-core`：双形态 `cargo check --lib`（默认 + `--no-default-features --features bare-host`）0 error。
- `bedcode-mobile/src-tauri`：`cargo check` 0 error（codebuddy 未复跑的移动 `cargo test` 延期到 T-02 补，本票只做 check 收敛）。
- `bedcode-desktop/src-tauri`：`cargo check --lib` 0 error。

## 4. 交付物

- 干净工作区 + 3 个提交（docs / rename / engine 第一批）+ 本票记录。
- 遗留项登记：移动端 `cargo test` 未跑（交 T-02）；wasm 应用产物未重构建（交 T-05）。
## 5. 状态：完成（2026-10-11 06:20，会话 01a127db 提交 + 本会话核验）

- 提交链（dev，d36c4a1a4 → 8ae2384a3，平行会话 06:09-06:10 落地三提交）：
  - `2d6ae198f` refactor(wasm-core): 形态中立化收口 + engine 机制合一（0047 staged rename + codebuddy engine.rs 第一批 + host_impl 4 域删除 + 锁同步）
  - `693f64266` docs(wasm-core): 形态中立化 spec/ticket 记录 + ADR 0047 + CHANGELOG/code-map/AGENTS 同步
  - `8ae2384a3` refactor(wasm-core): 删除 rename 源（desktop/mobile→common/bare）与已迁宿主的移动独有域残余
- 门禁核验（本会话实跑，全 0 error）：
  - wasm-core `cargo check --lib`（默认 common-host）：✓（9 warning）
  - wasm-core `--no-default-features --features bare-host`：✓（10 warning）
  - bedcode-mobile/src-tauri `cargo check`：✓（9 warning）
  - bedcode-desktop/src-tauri `cargo check --lib`：✓
- 逻辑遗留：移动端 `cargo test`（含 codebuddy 未复跑面）交 T-02 补；wasm 应用产物未重构建交 T-05。
- 当前 cfg 计数：common-host 79 处 + bare-host 50 处（src/，无 feature 表计数见 T-02）。
