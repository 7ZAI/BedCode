# 票 01 — 加载期闸门（validation.rs）

**状态**：resolved · 2026-09-30

## 落点

- `bedcode-desktop/src-tauri/src/wasm_core/manager/validation.rs`：
  - 新增 `validate_preopen_category(manifest)`：`wasi_preopen_dirs` 非空且
    `lifecycle != ephemeral` → 显性拒绝，文案点名 `host-fs` 替代 + ADR 0034；
  - `validate_manifest_required` 末尾挂接（在 `validate_lifecycle` 之后——ephemeral
    + preopen 由既有 lifecycle 闸门先拒，两类缺口不混同）；
  - 入口覆盖：`parse_manifest_json` 是 loader（`loader.rs:292`）与 downloader
    （`downloader.rs:90`）的统一入口，闸门两条路径生效。

## 回归用例

`wasi_preopen_dirs_on_non_worker_is_rejected_at_load`：
- 正例：无声明 / 显式 persistent 无声明 → 照常加载（既有 manifest 零迁移）；
- 反例：缺省 lifecycle 声明 preopen → 拒且文案含 `wasiPreopenDirs` + `host-fs` + `ADR 0034`；
  显式 persistent 声明 preopen → 拒；
- `ephemeral` + preopen → 由 `validate_lifecycle` 拒，文案点名 `ADR 0032`（不是 0034）。
- 变异判据（注释写明）：摘掉闸门 → 反例全红。

## 验证

`cargo test --lib manager::validation` → 10 例全绿；全量宿主 `cargo test` → 1072 绿。
