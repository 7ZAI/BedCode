# `.vscode/` 目录说明

## 两种入口，先确认一件事

**rust-analyzer 1.98 起不再递归发现子目录 Cargo 工程**：仓库根没有 `Cargo.toml`，若不显式声明工程，打开仓库根直接报 `failed to find any projects`。因此**所有 RA 工程声明都走 `rust-analyzer.linkedProjects`**（schema 原文：*"Disable project auto-discovery in favor of explicitly specified set of projects"*）。两端各含约 14 个独立 Cargo 工程（宿主 `src-tauri` + 插件 `plugins/*/rust` + SDK `packages/plugin-sdk-*` + 测试组件 `packages/plugin-*-test`，合计约 28 个 `Cargo.toml`）——只声明宿主工程，插件/SDK/测试组件不随宿主加载（单独开发时直接打开对应目录，见下）。

| 入口 | 配置位置 | RA 实例 | 内存 |
| --- | --- | --- | --- |
| **直接打开仓库根**（默认习惯） | 根 `.vscode/settings.json` 的 `linkedProjects` 声明两端 `src-tauri/Cargo.toml` | **单实例**加载两端宿主（+ path 依赖 `packages/peer-net`、`packages/link-crypto`、各自 `plugin-sdk-*`） | 中等；远小于旧递归发现（28 工程），因显式声明禁用了自动发现 |
| **打开 `BedCode.code-workspace`**（内存隔离优先） | 各文件夹 `.vscode/settings.json` 的 `linkedProjects` 锁定各自 `src-tauri/Cargo.toml` | **每文件夹独立实例**，各自只编译一个宿主 | 单实例最低；两端合计与方案一相近 |

单实例 RSS 若超过 ~2 GB，改用多根工作区拆分。

**插件 / SDK / 测试组件的 Rust 工程**在两种入口下都不随宿主加载；需要单独开发时直接打开对应目录（如 `code bedcode-desktop/plugins/terminal-session/rust`）——该目录是单 Cargo 工程，无需 linkedProjects。

**调试与任务**：

- 打开仓库根：用根 `.vscode/tasks.json` + `.vscode/launch.json`（`${workspaceFolder}` = 仓库根，路径带 `bedcode-desktop/` 前缀）
- 打开多根工作区：用各文件夹 `.vscode/tasks.json` + `.vscode/launch.json`（`${workspaceFolder}` = 文件夹本身，无需前缀），与根版本等价

## `settings.json` 中的 rust-analyzer 内存限制

**背景**：2026-09-04 23:55，`rust-analyzer`（PID 345132）膨胀到 3.2 GB RSS，触发 `systemd-oomd` 约束式杀进程；系统随后被手动 reboot。根因是当时 RA 递归发现全部 ~28 个 Cargo 工程并在单实例内索引。2026-09-07 优化时发现上一轮用的键在 rust-analyzer 0.3.3033 已移除，换成有效键；2026-09-25 以当前版本 **1.98.1**（`rust-analyzer --print-config-schema`）复核全部键仍然有效，并确认 1.98 起需 `linkedProjects` 显式声明工程。

**各文件职责**：

- `.vscode/settings.json`（仓库根）——打开仓库根时生效：`linkedProjects` 声明两端宿主工程 + 下方内存键
- `BedCode.code-workspace`（仓库根）——可选的多根入口：每文件夹独立 RA 实例（内存隔离）
- `bedcode-desktop/.vscode/settings.json`、`bedcode-mobile/.vscode/settings.json`——多根工作区中按文件夹生效，也支持单独打开子项目；含各自的 `linkedProjects` + 下方内存键

**做了什么**（rust-analyzer 1.98.1 有效键）：

1. **`rust-analyzer.linkedProjects`**：禁用项目自动发现，只加载显式列出的 `Cargo.toml`（根：两端宿主；各文件夹：各自宿主）。
2. **`files.watcherExclude`（VSCode 原生键，支持 glob）**：排除 `**/target/**`（两项目合计约 43G，不排除则启动遍历即吃爆内存）、`**/gen/**`、`**/node_modules/**`、`**/.git/**`。
3. **`rust-analyzer.files.exclude`（不再支持 glob，须为相对路径）**：排除 `target`、`gen`（同时列根相对与工作区相对两种写法，兼容不同解析基准），让 RA 的 VFS 不加载编译产物与 Android 生成目录。
4. **`rust-analyzer.cachePriming.enable: false`**：关闭启动时全工作区缓存预热——内存最大头；代价是首次 hover / 跳转略慢。
5. **`rust-analyzer.lru.capacity: 64`**：语法树 LRU 缓存默认 128 条 → 64 条（单位是条目数，不是 MB）。
6. **`rust-analyzer.procMacro.processes: 1`**：proc-macro 服务进程 2 → 1（tauri+serde+tokio 宏栈每进程占用可观）。
7. **`rust-analyzer.checkOnSave: false`**：不再后台跑 `cargo check`，省一个常驻 rustc 进程；编译错误改由 `cargo check` / `cargo test` 自查。
8. **`rust-analyzer.numThreads: 4`**：主循环并行线程默认全核（16）→ 4，压低并行索引峰值内存（多根工作区下两个实例合计 8 线程，仍在预算内）。

**没有改的**：

- 没改 `rust-analyzer.procMacro.enable` 为 `false`——Tauri 项目重度用 `#[tauri::command]`、`generate_context!`、`serde` 派生宏，关掉会丢补全/跳转质量且满屏假报错。`procMacro.enable` 隐含要求 build script 开启，故 `cargo.buildScripts.enable` 保持 `true`。

## 复现验证

```bash
# 打开仓库根（或 Reload Window）后看 rust-analyzer 内存
ps -eo pid,user,rss,comm | grep rust-analyzer
# 单实例（两端宿主）期望 RSS < 2 GB（旧递归发现峰值 3.2 GB）；超过则改用 BedCode.code-workspace
```

如果单实例仍超 2 GB，再加：

```jsonc
// 对应 settings.json 末尾追加
"rust-analyzer.cargo.features": "lean"
```

只编译运行时所需 features，砍掉 `tokio/full`、`serde/full` 等可选 features，进一步降索引量。
