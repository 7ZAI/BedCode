# `.vscode/` 目录说明

## `settings.json` 中的 rust-analyzer 内存限制

**背景**：2026-09-04 23:55，`rust-analyzer`（PID 345132）膨胀到 3.2 GB RSS，触发 `systemd-oomd` 约束式杀进程；系统随后被手动 reboot。根因是 VS Code 打开 BedCode 工作区根时，rust-analyzer 同时索引 `bedcode-desktop/src-tauri` 和 `bedcode-mobile/src-tauri` 两个独立 Tauri 项目（根目录无 Cargo workspace），内存叠加。2026-09-07 优化时发现上一轮用的键在 rust-analyzer **0.3.3033 已移除**（`rust-analyzer --print-config-schema` 确认），本轮换成当前版本有效键。

**各文件职责**（内容一致，路径基准不同）：

- `.vscode/settings.json`（仓库根）——打开工作区根、双项目同时加载时生效
- `bedcode-desktop/.vscode/settings.json`、`bedcode-mobile/.vscode/settings.json`——单独打开子项目时生效

**做了什么**（当前 rust-analyzer 0.3.3033 有效键）：

1. **`files.watcherExclude`（VSCode 原生键，支持 glob）**：排除 `**/target/**`（两项目合计约 43G，不排除则启动遍历即吃爆内存）、`**/gen/**`、`**/node_modules/**`、`**/.git/**`。替代已移除的 `rust-analyzer.files.watcherExclude`。
2. **`rust-analyzer.files.exclude`（注意：0.3.3033 起不再支持 glob，须为相对路径）**：排除 `target`、`gen`（同时列根相对与工作区相对两种写法，兼容不同解析基准），让 RA 的 VFS 不加载编译产物与 Android 生成目录。
3. **`rust-analyzer.cachePriming.enable: false`**：关闭启动时全工作区缓存预热——内存最大头，替代已移除的 `memoryUsage.*Limit`；代价是首次 hover / 跳转略慢。
4. **`rust-analyzer.lru.capacity: 64`**：语法树 LRU 缓存默认 128 条 → 64 条（单位是条目数，不是 MB）。
5. **`rust-analyzer.procMacro.processes: 1`**：proc-macro 服务进程 2 → 1（tauri+serde+tokio 宏栈每进程占用可观）。
6. **`rust-analyzer.checkOnSave: false`**：不再后台跑 `cargo check`，省一个常驻 rustc 进程；编译错误改由 `cargo check` / `cargo test` 自查。
7. **`rust-analyzer.numThreads: 4`**：主循环并行线程默认全核（16）→ 4，压低双工作区并行索引峰值内存。
8. **`linkedProjects`**：保持注释掉不强制——VS Code 打开工作区根时若同时识别到两个 `Cargo.toml`，需要手动通过命令面板 `Rust Analyzer: Switch LSPCargo target` 或在 `linkedProjects` 显式指定一个。若只编辑单项目，也可用 `linkedProjects` 锁定单个 `Cargo.toml` 直接减半内存。

**没有改的**：

- 没改 `rust-analyzer.procMacro.enable` 为 `false`——Tauri 项目重度用 `#[tauri::command]`、`generate_context!`、`serde` 派生宏，关掉会丢补全/跳转质量且满屏假报错。`procMacro.enable` 隐含要求 build script 开启，故 `cargo.buildScripts.enable` 保持 `true`。
- 没强制 `linkedProjects`——工作区可能同时编辑 desktop 和 mobile，强制会切错。

## 复现验证

```bash
# 重启 VS Code（或 Reload Window）后看 rust-analyzer 内存
ps -eo pid,user,rss,comm | grep rust-analyzer
# 期望 RSS < 1.5 GB（之前峰值 3.2 GB）
```

如果仍超 2 GB，再加：

```jsonc
// settings.json 末尾追加
"rust-analyzer.cargo.features": "lean"
```

只编译运行时所需 features，砍掉 `tokio/full`、`serde/full` 等可选 features，进一步降索引量。
