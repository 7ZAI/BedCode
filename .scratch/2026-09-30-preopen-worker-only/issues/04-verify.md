# 票 04 — 验证

**状态**：resolved · 2026-09-30

## 针对性（开发中两段式）

- `cargo test --lib wasi_preopen_dirs_on_non_worker_is_rejected_at_load` ✓
- `cargo test --lib manager::validation` → 10 例 ✓
- `pnpm exec vitest run packages/plugin-sdk-desktop/__tests__/manifest-validate.test.ts` → 39 例 ✓
- `cargo test --lib preauthorize` → 5 例（preauth 机制未破坏）✓
- `cargo test --lib wasi_e2e` → 3 例（preopen 机制守门测试未破坏）✓
- SDK crate `cargo test wasi_preopen` → 9 例 ✓
- 4 个 wasm-app plugin.json 实测过新构建闸门 → errors: none ✓

## 全量回归（§10）

- 宿主 `cargo test` → **1072 passed / 0 failed / 2 ignored**，exit 0（含集成 target + doctest）✓
- 前端 `pnpm run test:run` → 112 文件 / **1424 passed** ✓
- 根目录 `pnpm exec eslint .` → **0 error**（118 warning 不计入）✓
- `rustfmt --check`（仅本任务 Rust 文件）→ 本任务新增代码零 diff；activation.rs:409/636
  与 fixture_build.rs 的 fmt 漂移为分支既有状态，非本任务改动，不碰 ✓

## 未纳入自动化门禁的手工验证项（§10 逐项说明）

- wasm 应用完整构建（含 wasmHash 注入）：**没跑**——本任务零 wasm 业务代码改动（仅
  README/注释），4 应用 manifest 已实测过新闸门；完整构建产物与本任务无关。
- gen/android gradlew：**没跑**——未触碰移动端 Kotlin。
- 真机/浏览器核验：**没跑**——无 UI 行为改动（usePluginManager 仅注释）。

## 进程清理

测试无残留后台进程（cargo/vitest 均正常退出）。
