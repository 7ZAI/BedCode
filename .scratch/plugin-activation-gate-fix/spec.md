# Spec — 插件激活门禁与 WASI 预打开修复（2026-09-05）

## 背景

桌面端 `tauri:dev` 日志（`~/.local/share/com.bedcode.app/logs/runtime.2026-09-04.log`）报告两个插件启用失败：

```
ERROR [plugin:com.bedcode.ai-chatbox] activate failed: WASI 预打开目录未就绪：/home/binblink/.bedcode/ai-chatbox 的授权已保存，请停用后重新启用插件完成初始化
ERROR [API] plugin_activate(com.bedcode.file-transfer) failed: Plugin error: Please configure shared directories in plugin settings first
```

完整排查过程见交接文档（/tmp 会话产物，本 spec 为持久记录）与 `.pi/sessions/` 会话日志。

## 目标

1. **Bug A**：file-transfer 启用预授权门禁的 `preauth_paths` storage key 无写入方——插件侧 `mount_local` / `update_roots` 补上写入/剔除，恢复「先配置共享目录 → 再启用」契约。
2. **Bug B**：ai-chatbox WASI 预打开目录不存在导致 preopen 失败、激活自检死循环——宿主 `build_wasi_ctx` 幂等创建目录；激活前按「当前授权目录 ⊄ 实例已预打开」判定重建实例，使「停用再启用」重试真正生效（无需重启应用）。

## 验收

- Rust 单测全绿：桌面宿主 `cargo test --lib`、两端 file-transfer 插件 `cargo test`
- 新增用例覆盖：`build_wasi_ctx` 创建缺失目录并纳入 preopened 列表；不可创建路径跳过不 panic；`mount_local` 追加去重、`update_roots` 剔除、未知 id 幂等
- 人工路径（tauri:dev）：
  - ai-chatbox 首次启用：已授权（父目录前缀）→ 直接成功；全新未授权 → 弹窗同意后按提示「停用再启用」即成功（重建实例，无需重启）
  - file-transfer：设置页添加共享目录（写入 preauth_paths）→ 启用成功；移除全部目录 → 启用恢复「请先配置共享目录」拒绝

## 涉及文件

| 端 | 文件 | 改动 |
|----|------|------|
| 桌面宿主 | `src-tauri/src/plugin/wasm_runtime/component.rs` | `build_wasi_ctx` 返回 `(WasiCtx, Vec<String>)` + `create_dir_all`；`LoadedWasmPlugin` 增加 `preopened_dirs` 字段/访问器；`resolve_preopen_dirs` 提升 `pub(crate)`；2 个新单测 |
| 桌面宿主 | `src-tauri/src/plugin/wasm_runtime.rs` | re-export `resolve_preopen_dirs` |
| 桌面宿主 | `src-tauri/src/plugin/host.rs` | `ActivatePlan` 携带 `declared_preopen_dirs`；激活前重建判定；`rebuild_wasm_instance` 助手；`reload_wasm_plugin` 复用助手 |
| 桌面插件 | `plugins/file-transfer/rust/src/peer.rs` | `mount_local`/`update_roots` 写删 `preauth_paths`（含 3 新单测） |
| 移动插件 | `plugins/file-transfer/rust/src/peer.rs` | 同上（2 新单测） |

## 决策点

- **目录创建放宿主 `build_wasi_ctx`（方案 A）**而非插件 activate 内自检：preopen 发生在加载期（早于 activate 的 fs_request_auth），授权即代表用户同意在该路径写数据，宿主创建职责最贴合现状；对 file-transfer 等未来 WASI 插件同样成立。
- **重建实例仅触发于「声明了 wasiPreopenDirs 且当前授权目录未全覆盖」**：当前仅 ai-chatbox 声明（`${home}/.bedcode/ai-chatbox`），其余插件零开销；重建复用 `load_plugin_from_file` + map 替换（与 dev 热重载同一机制，语义已被验证）。
- **plugin 侧 storage 写失败如实上抛**：共享目录已落库但预授权缺失会再次锁死启用，宁可让用户看到错误重试（重试幂等：upsert 命中 + push 去重）。

## 注意

- ~~桌面端存在流程断点（预授权门禁与「配置页需先启用插件」互为前置，设置 UI 在 FileTransferView 内、路由守卫会先 auto-activate），本 spec 未处理~~ → 已由 issue 03（启用先行）解决（2026-09-05）。
- 移动端 ai-chatbox 数据目录用 `{AppDownloadsDir}/ai-chatbox`（存在），无 Bug B；移动 file-transfer 的 Bug A 已同步修复。
