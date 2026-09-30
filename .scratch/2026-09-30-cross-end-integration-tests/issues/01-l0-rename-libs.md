# 票 01 — L0：两端 lib 改名（`bedcode_lib` → 端专用名）

**状态**：resolved · 2026-09-30
**类型**：task

## 落点

`[lib] name` 两端各改为 `bedcode_desktop_lib` / `bedcode_mobile_lib`；代码引用
（两端 src / tests / bench 的 `bedcode_lib::`）机械替换；字符串常量与文档手工同步。

## 与 spec 清单的两处偏差（实查为准）

1. **spec 漏项**：`bedcode-mobile/src-tauri/android-backup/app-java/generated/Rust.kt`
   的 `System.loadLibrary("bedcode_lib")` —— spec §6-1 写「Android 工程无
   `libbedcode_lib.so` 硬编码」，**实查为假**：该文件是**手工保留副本且入库**，
   `gen/android` 下同名文件被 gitignore（tauri 重建自动跟随）。已同步改为
   `bedcode_mobile_lib`，并在 `docs/knowledge/build-process.md` 写明这条恢复陷阱。
2. **spec 文档口径本身是陈旧的**：三份文档写的插件日志 target
   `bedcode_lib::plugin::plugin_log` **两端都不对**——桌面是**硬编码常量**
   `bedcode_lib::wasm_core::plugin_log`（`wasm_core/host_api/log.rs`），
   移动是 `tracing!` 宏自动取模块路径得到的
   `bedcode_lib::plugin::commands::plugin_log`。已按实测分端改写（AGENTS §0
   「文档字面 ≠ 事实」：先修文档）。

## 验证

- 桌面 `cargo test` 全量绿（lib 1058 passed / 0 failed / 1 ignored + 8 个集成 target 全绿）
- 移动 `cargo test` 全量绿
- `rg -n bedcode_lib` 全仓复查：仅剩 build-process.md 里描述「曾同名」的一行说明
