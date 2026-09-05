# `.vscode/` 目录说明

## `settings.json` 中的 rust-analyzer 限制

**背景**：2026-09-04 23:55，`rust-analyzer`（PID 345132）膨胀到 3.2 GB RSS，触发 `systemd-oomd` 约束式杀进程；系统随后被手动 reboot。根因是 VS Code 打开 BedCode 工作区根时，rust-analyzer 同时索引 `bedcode-desktop/src-tauri` 和 `bedcode-mobile/src-tauri` 两个独立 Tauri 项目（根目录无 Cargo workspace），内存叠加。

**做了什么**（3 件事）：

1. **`files.watcherExclude` + `files.exclude`**：排除 `target/` 编译产物、`node_modules/`、`gen/android/build/`、`gen/android/`、`/.git/`，减少索引内存占用。
2. **`memoryUsage.*Limit`**：把 context/syntaxTree/hover/completion 缓存从默认值压到 128–256 MB，硬上限卡住 RSS。
3. **`linkedProjects`**：默认注释掉，不强制——VS Code 打开工作区根时若同时识别到两个 `Cargo.toml`，需要手动通过命令面板 `Rust Analyzer: Switch LSPCargo target` 或在 `linkedProjects` 显式指定一个（参见 settings.json 内被注释的代码段）。

**没有改的**：

- 没改 `rust-analyzer.procMacro.enable` 为 `false`——Tauri 项目重度用 `tokio` `serde` 等宏，关掉会丢补全/跳转质量。
- 没强制 `linkedProjects`——工作区可能同时编辑 desktop 和 mobile，强制会切错。

## 复现验证

```bash
# 重启 VS Code 后看 rust-analyzer 内存
ps -eo pid,user,rss,comm | grep rust-analyzer
# 期望 RSS < 1.5 GB（之前峰值 3.2 GB）
```

如果仍超 2 GB，再加：

```jsonc
// settings.json 末尾追加
"rust-analyzer.cargo.features": "lean"
```

只编译运行时所需 features，砍掉 `tokio/full`、`serde/full` 等可选 features，进一步降索引量。
