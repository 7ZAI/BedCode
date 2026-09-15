# 桌面端 Plugin 实现模块单元测试审查报告

> 状态: **审计完成，1 张修复票据待处理**（2026-09-14，23:55）
> 范围: `bedcode-desktop/src-tauri/src/plugin/wasm_runtime/host_impl/` + 剩余插件文件
> 测试规模: **~100 个**（host_impl 系列 ~86 + fs_auth 7 + approval 4 + downloader 6 + types 9 + loader 3 + watcher 5）
> 分支: `dev`

---

## 1. 摘要（Verdict）

**host_impl 系列测试密度尚可，但 commands.rs 和 services.rs（共 711 行）零测试。**

- `host_impl/fs.rs`（404 行，18 测试）：文件系统操作覆盖全面。
- `host_impl/session.rs`（267 行，12 测试）+ `host_impl/bus.rs`（243 行，12 测试）：会话/总线覆盖较好。
- `host_impl/database.rs`（458 行，9 测试）+ `host_impl/log.rs`（421 行，8 测试）+ `host_impl/process.rs`（379 行，8 测试）：中等覆盖。
- `host_impl/http.rs`（614 行，4 测试）+ `host_impl/peer.rs`（499 行，4 测试）：低覆盖。
- `fs_auth.rs`（614 行，7 测试）：权限校验有覆盖。
- **零测试**：`commands.rs`(359)、`services.rs`(352)。
- **低覆盖**：`api.rs`(331行5测试)、`wsl_fs.rs`(224行2测试)。

---

## 2. 审查基线

```bash
cargo test --lib plugin::wasm_runtime::host_impl   # → ~86 passed
cargo test --lib plugin::fs_auth plugin::approval plugin::downloader plugin::types plugin::loader plugin::watcher   # → ~34 passed
```

---

## 3. 总判定表

| 文件 | 行数 | 测试 | 判定 |
|---|---|---|---|
| `host_impl/fs.rs` | 404 | 18 | 🟢 有效 |
| `host_impl/session.rs` | 267 | 12 | 🟢 有效 |
| `host_impl/bus.rs` | 243 | 12 | 🟢 有效 |
| `host_impl/database.rs` | 458 | 9 | 🟡 部分 |
| `host_impl/log.rs` | 421 | 8 | 🟡 部分 |
| `host_impl/process.rs` | 379 | 8 | 🟡 部分 |
| `host_impl/api.rs` | 331 | 5 | 🟡 部分 |
| `host_impl/http.rs` | 614 | 4 | 🟡 部分 |
| `host_impl/peer.rs` | 499 | 4 | 🟡 部分 |
| `host_impl/wsl_fs.rs` | 224 | 2 | 🟡 部分 |
| `fs_auth.rs` | 614 | 7 | 🟡 部分 |
| `approval.rs` | 340 | 4 | 🟡 部分 |
| `downloader.rs` | 299 | 6 | 🟡 部分 |
| `types.rs` | 280 | 9 | 🟢 有效 |
| `loader.rs` | 280 | 3 | 🟡 部分 |
| `watcher.rs` | 220 | 5 | 🟡 部分 |
| `commands.rs` | 359 | 0 | 🔴 零测试 |
| `services.rs` | 352 | 0 | 🔴 零测试 |

---

## 4. 修复优先级

| 优先级 | 票据 | 内容 |
|---|---|---|
| P1 | 32 | commands.rs + services.rs 补测试 |

---

## 5. 审计纪律记录

- 测试计数：~120（grep 精确匹配）
- 零测试文件：2（commands.rs + services.rs，共 711 行）
