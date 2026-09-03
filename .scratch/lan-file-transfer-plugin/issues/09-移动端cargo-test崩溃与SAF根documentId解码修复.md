# 移动端 cargo test 启动崩溃（0xc0000139）与 SAF 根 document id 解码修复

Type: task
Status: resolved

## 问题

`cargo test --lib`（bedcode-mobile）测试二进制编译成功但**进程启动即失败**：

```
error: test failed, to rerun pass `--lib`
Caused by:
  process didn't exit successfully: ...\bedcode_lib-*.exe (exit code: 0xc0000139, STATUS_ENTRYPOINT_NOT_FOUND)
```

桌面端 `cargo test` 正常 → 移动端专属问题。0xc0000139 是加载器错误：**导入表里引用的某个 DLL 入口点在加载到的 DLL 副本中不存在**。

## 诊断

### 层 1：缺失入口点 `TaskDialogIndirect`

- 用 llvm-objdump 全量比对测试 exe 的 395 个导入与 System32 各 DLL 的导出表，唯一真实缺失：`TaskDialogIndirect`（comctl32.dll）
- 该符号来自 `rfd 0.16` 的 `win_cid` message dialog 后端（经 `tauri-plugin-dialog` 引入）
- 本机 `C:\Windows\System32\comctl32.dll` 是 **5.82 兼容桩**（FileVersion 5.82，WinBuild.160101.0800），**不导出** TaskDialogIndirect（v6 API）；WinSxS 中 6.0.26100.x 的 v6 副本有导出
- 测试 exe **无应用级 manifest** → 加载器选中 System32 的 5.82 桩 → 启动即崩
- 桌面端不崩的原因：依赖图 feature unification 让桌面端 rfd 走了另一后端（winxp，MessageBoxW），测试 exe 根本不导入该符号 —— 所以只有移动端爆

### 层 2：崩溃掩盖了一个真实生产 bug

测试能跑后，`file_service::server::tests::list_dir_lists_saf_root_and_traverses_tree` 失败（SAF 树根列表 0 条目）：

- `list_saf_dir` / `resolve_saf_download_source` 用 `saf_tree::tree_document_id()` 取根 document id，返回的是 URI 路径段的**百分号编码形态**（`primary%3ADownload`）
- 而 SAF 契约两端都是**解码形态**（`primary:Download`）：Kotlin `DocumentsContract.getTreeDocumentId` / `SafEntry.document_id`（COLUMN_DOCUMENT_ID）/ `buildChildDocumentsUriUsingTree`（appendPath 会再编码，传已编码 id 会变成 `primary%253ADownload` 查不到）
- 结果：根级 `list_tree` 永不命中 → **真机上 SAF 共享目录根列表/下载同样为空**；子目录遍历用子条目解码 id 不受影响
- 该 bug 一直存在但测试二进制在这台机器上从未能启动，从未被发现

## 修复

### 1. 链接产物统一注入 common-controls v6 manifest

- 新增 `src-tauri/app.manifest`：`Microsoft.Windows.Common-Controls` v6.0.0.0 依赖（内容与 tauri-build 默认 Windows manifest **完全一致**）
- `src-tauri/build.rs`：`CARGO_CFG_TARGET_OS == "windows"` 时（判定必须用该 env，build.rs 编译于宿主，`#[cfg]` 不反映交叉编译目标）经 `cargo::rustc-link-arg` 注入 `/MANIFEST:EMBED` + `/MANIFESTINPUT:<绝对路径>`
- bin 改用 `tauri_build::WindowsAttributes::new_without_app_manifest()`：避免 tauri-build 的 `resource.lib`（含默认 manifest）与 lld 生成的 RT_MANIFEST **资源重复**（链接错误 duplicate resource）
- 作用域：`cargo::rustc-link-arg` 只影响本包 target（测试 exe / cdylib / bin），不泄漏到依赖的 build script；Android 交叉编译目标不受影响

踩坑记录（cargo 1.95.0 实测）：

| 尝试 | 结果 |
|------|------|
| `.cargo/config.toml` rustflags `/MANIFESTINPUT` | 泄漏到所有依赖 build script 的链接（它们也走 rust-lld），且 `/manifestinput requires /manifest:embed` |
| `cargo::rustc-link-arg-tests` / `-lib` / `-bins` | cargo 1.95 **全部不支持**（invalid instruction），只有通用 `rustc-link-arg` |
| 只加 `/MANIFESTINPUT` 不加 `/MANIFEST:EMBED` | lld 报 `/manifestinput: requires /manifest:embed` |

### 2. SAF 根 document id 解码

- `saf_tree::tree_document_id()`：末尾路径段返回前先 `percent_decode`（对齐其文档声明的 `DocumentsContract.getTreeDocumentId` 等价语义）
- `saf_tree::tree_alias()`：去掉自身的二次解码（tree_document_id 已解码，二次解码会破坏含字面 `%XX` 的罕见 id）
- `tree_uri_parsing` 测试同步更新（`primary%3ADownload` → `primary:Download`，新增 `%2F` 用例）

## 验证

- `cargo test --lib`：**128 passed; 0 failed**
- `cargo build`：bin + cdylib 链接干净
- 两个 exe（`bedcode-mobile.exe`、`bedcode_lib-*.exe`）`.rsrc` 中均含 `Microsoft.Windows.Common-Controls` v6 依赖

## 备注

- **系统侧根因**：本机 System32 的 comctl32 为 5.82 兼容桩（WinSxS 同时存在 5.82.26100.x 与 6.0.26100.x 两个组件），manifest 缺失的二进制拿 5.82 —— 所有真实 Windows 应用都带 v6 manifest，此为正常防御面而非系统损坏
- **桌面端同款潜在地雷**：桌面测试 exe 同样无 manifest，当前只是依赖图恰好不导入 `TaskDialogIndirect`；若 rfd/插件升级引入该导入会以同样方式崩溃（未处理，需要时同方案移植）
- 相关文件：`src-tauri/app.manifest`、`src-tauri/build.rs`、`src/file_service/saf_tree.rs`
