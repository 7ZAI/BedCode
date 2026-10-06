# 06: contract 独立验收（crate / src-tauri / cross-end-tests 三全量）

**What to build:** 见 spec §6（行内）与对应 spec 章节。

**Status:** done

**Type:** task

## Comments

- 2026-10-06 接盘：票 05 完成后进入独立验收。验收清单见 spec §6 票 06 行内 + 本票
  How：crate 根 `cargo test` 全量、src-tauri `cargo test` 全量、cross-end-tests 全量、
  与基线零回归；lens_diagnostics 无 blocker；手工验证项（wasm 应用完整构建含
  wasmHash 注入——本票未动 wasm-apps 应不受影响）。

## Blocked by
- 05（已完成）

## 验证
- （填写中）

- 2026-10-06 验收结果：
  - **crate 根 `cargo test`**：784 passed / 2 failed / 1 ignored——两个失败均为
    既有基线（§4）：`session_e2e::test_session_task_domain_closed_loop`（9-30 起红）
    + `terminal_output_perf::perf_p2_guest_ring_fetch_batch_curve`（5ms 墙钟 flake，
    本轮全量 / 单跑均 ~6.1ms，负载 2.8+、swap 7G 用、两 pi agent + GUI 并发；
    与 handoff 记录的 6.1ms 一致）。**零回归**：数字与票 04 全量（784/2）逐字一致。
  - **src-tauri `cargo test` 全量**：lib 76 passed / 0 failed + 12 个集成测试全绿
    （broadcast_shutdown / build_manifest_smoke / capabilities_lock / empty_dir_lock /
    hot_path_logging_lock / http_auth_biometric / pty_session_chain / server_integration /
    wasm_core_whole_crate_lock / ws_auth_rules + wasm_bridge_bench 编译绿，harness=false
    无运行时用例）。
  - **cross-end-tests**：用户指示不需要运行（代码侧 `cargo check --tests` 已在票 04
    验收绿；本票无协议改动）。
  - **lens_diagnostics mode=all**：无 blocker（仅 warning：CHANGELOG 既有 markdown
    lint 漂移 + 锁测试的 expect() 既有模式）。
  - **手工验证项**：wasm 应用完整构建（含 wasmHash 注入）**没跑 + 原因**——本票
    未动 wasm-apps / SDK / WIT（ABI 零变动），且集成测试（pty_session_chain /
    broadcast_shutdown / ws_auth_rules）已内置构建 + 加载真 wasip3 fixture 并全绿，
    证明 wasm 工具链经新 crate 形态可用；gen/android 未动 Kotlin 不跑；真机/浏览器
    未跑（后端结构迁移，不影响前端）。
