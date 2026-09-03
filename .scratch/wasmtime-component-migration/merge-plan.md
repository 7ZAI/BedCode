# 迁移分支 → dev 合并冲突分析与解决预案

> 生成时间：2026-08-08 · 分析对象：`feat/wasmtime-component-migration`（HEAD `7b668a6b`）合入本地 dev（`0664e213`）

## 1. 合并概况

| 项 | 值 |
|---|---|
| merge-base | `f38af77d`（迁移分支起点，dev 的 10 个新提交之前） |
| 迁移分支 | 4 个提交（阶段 A×2 + 阶段 B + 阶段 C），全部组件形态 |
| dev | `0664e213`，merge-base 之后 10 个新提交（移动端 UI、auto-task hooks、**v7 SESSION_CLOSE** 等） |
| 冲突文件 | **4 个**（2 内容冲突 + 2 修改/删除）；`build.js` 等 3 个文件自动合并无冲突 |
| 冲突根因 | dev 的 `20603d6e`（插件 SDK 会话注入与 WASM host functions 扩展，v7 会话关闭）与迁移分支改动了同一批文件 |

模拟合并命令：`git merge-tree --write-tree HEAD dev`（结果：4 处 CONFLICT）。

## 2. 冲突文件清单与逐一对策

### 2.1 `packages/plugin-sdk-desktop/rust/src/abi.rs` — 内容冲突

**双方改动**：
- 迁移分支（阶段 C）：瘦身为仅 `ABI_VERSION` + `FORM_COMPONENT`，删除全部 core ABI 常量/签名表
- dev（`20603d6e`）：`ABI_VERSION` 6→7（新增 SESSION_CLOSE）、`import::SESSION_CLOSE` 常量、签名表 +1 行

**解决**：**采用迁移分支版本（瘦身形态）**，保留 dev 的版本演进语义：
- `ABI_VERSION: u32 = 7`（跟随 dev 的 v7 bump；组件 `abi.version()` 返回 7，宿主校验 `≤ 7`，旧组件 6 仍兼容）
- 版本演进注释补 v7 行（"新增会话关闭 SESSION_CLOSE"）
- dev 的 `import::SESSION_CLOSE` 常量与签名表**不保留**（core ABI 已删，WIT 编译期保证）

### 2.2 `packages/plugin-sdk-desktop/rust/src/wasm_host.rs` — 内容冲突

**双方改动**：
- 迁移分支（阶段 B）：整文件重写为 wit-bindgen import 调用（`host_session::create(...)` 模式），删除 extern C
- dev：`HostSession::session_close` + extern C 声明 + native stub（core ABI 形态）

**解决**：**采用迁移分支版本**，在 `impl crate::host::HostSession for WasmHost` 内补 `session_close`（沿用 `session_create` 的 WIT 调用模式）：

```rust
fn session_close(&self, session_id: &str) -> Result<(), HostError> {
    host_session::close(session_id).map_err(|e| host_err("session_close", e))
}
```

前提是 WIT 补 `close`（见 §3.1）。dev 的 extern C / native stub 不保留。

### 2.3 `src-tauri/src/plugin/wasm_runtime/host_functions/mod.rs` — 修改/删除冲突

**双方改动**：迁移分支删除（阶段 C 重组为 `host_impl/`）；dev 增加 `register!(SESSION_CLOSE, ...)` 注册行。

**解决**：**接受删除**（git 合并时 `git rm` 该文件）。dev 的注册行无需移植——core Linker 注册机制已整体删除。

### 2.4 `src-tauri/src/plugin/wasm_runtime/host_functions/session.rs` — 修改/删除冲突

**双方改动**：迁移分支删除（逻辑层迁至 `host_impl/session.rs`）；dev 追加 `host_session_close` 胶水函数（权限 + `kill_session_with_source` 异步执行）。

**解决**：**接受删除**，并把 `host_session_close` 的业务逻辑**移植为 `host_impl/session.rs` 的逻辑层函数**（去掉 `(ptr,len)` 搬运与 Caller，签名与返回值对齐组件形态）：

```rust
/// 关闭（终止）会话（v7，需要 `session:write` 权限）
///
/// 包一层核心已有的 `SessionManager::kill_session_with_source`，供插件
/// （如 auto-task 定时自动任务）在执行完毕后关闭自己创建的会话。
/// 停止 PTY 并置 Stopped，会话记录保留（与用户手动关闭一致）。
///
/// **异步执行**：`kill_session_with_source` 会同步分发 Stopping/Stopped
/// 生命周期事件，事件回灌同一插件实例需要重新获取 `wasm_plugins` 写锁
/// （tokio RwLock 不可重入）——与 `session_create` 同理，spawn 异步执行，
/// wasm 调用立即返回。
pub(crate) fn session_close(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session_id: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_SESSION_WRITE, "host_session_close") {
        return Err("permission denied".to_string());
    }
    if session_id.is_empty() {
        return Err("session error: empty session_id".to_string());
    }
    let sm = host_ctx.session_manager.clone();
    let sid = session_id.to_string();
    let pid = plugin_id.to_string();
    spawn_with_error_boundary("host_session_close", async move {
        match sm.kill_session_with_source(&sid, None).await {
            Ok(_) => {
                tracing::info!(
                    plugin_id = %pid, session_id = %sid,
                    "host_session_close: session closed (async)"
                );
            }
            Err(e) => {
                tracing::error!(
                    plugin_id = %pid, session_id = %sid, error = %e,
                    "host_session_close: kill_session failed (async)"
                );
            }
        }
    });
    Ok(())
}
```

## 3. 能力移植：v7 会话关闭（合并后必做，否则编译失败）

dev 的 auto-task 插件 `plugins/auto-task/rust/src/queue.rs` 已调用 `host.session_close(session_id)`（`maybe_close_scheduled_session`），合并后组件形态下该能力必须完整接线：

### 3.1 WIT 契约（`packages/plugin-sdk-desktop/rust/wit/bedcode.wit`）

`interface host-session` 增加：

```wit
close: func(session-id: string) -> result<_, string>;
```

（`host-session` 是 13 组 import 接口之一，bindgen 重新生成后 Host trait 增加 `close` 方法）

### 3.2 宿主接线（`src-tauri/src/plugin/wasm_runtime/component.rs`）

`impl bedcode::plugin::host_session::Host for WasmPluginState` 增加：

```rust
fn close(&mut self, session_id: String) -> Result<(), String> {
    session::session_close(&self.host_ctx, &self.plugin_id, &session_id)
}
```

### 3.3 SDK（§2.2 已覆盖）+ 测试组件版本号

`packages/plugin-component-test/src/lib.rs` 的 `abi::Guest::version()` 从 6 改 7（与 `ABI_VERSION` 一致；不改也能通过宿主校验 6 ≤ 7，但语义应跟随）。`plugin-sdk-test` 经宏引用 `$crate::abi::ABI_VERSION` 自动跟随，无需改。

### 3.4 可选增强：close 往返测试

`plugin-sdk-test` 增加 `test_session_close` 命令（调用 `WasmHost::session_close` 并回传结果），宿主 `test_sdk_plugin_component_roundtrip` 断言：无头环境下会话不存在 → 错误串透传（证明 import 接线通）。非必须，但建议。

## 4. 自动合并文件（已确认安全，无需处理）

| 文件 | 说明 |
|---|---|
| `plugins/auto-task/scripts/build.js` | dev 加 codex/opencode hook 复制逻辑（hooks 区），迁移分支加 componentize（cargo build 区），区域不重叠，merge-tree 确认 auto-merge 成功 |
| `packages/plugin-sdk-desktop/rust/src/host/session.rs` | dev 的 trait 加 `session_close`（迁移分支未动此文件），直接继承 dev 版本 |
| `packages/plugin-sdk-desktop/rust/src/constants.rs` | dev 加 opencode/codex 常量（迁移分支未动），直接继承 |
| `src-tauri/.../host_impl/transfer.rs` 等 | dev 无改动，rename 跟踪自动完成 |

## 5. 合并执行步骤

在迁移 worktree 中执行（推荐，冲突解决后提交到迁移分支再合 dev）：

```bash
cd D:/tauriProject/BedCode-wasmtime-component
git merge dev
# 冲突处理：
#  abi.rs          → 手动合并（§2.1：瘦身版 + ABI_VERSION=7）
#  wasm_host.rs    → 手动合并（§2.2：WIT 版 + session_close）
#  host_functions/mod.rs、session.rs → git rm（§2.3/2.4），session_close 逻辑层移植到 host_impl/session.rs
# 配套：wit/bedcode.wit 加 close、component.rs 接线、plugin-component-test version→7
```

合并提交 message 建议：

```
merge: 合入 dev（v7 会话关闭能力组件化移植）

- 冲突解决：abi.rs/wasm_host.rs 采用组件形态 + 保留 ABI_VERSION=7；
  host_functions/* 接受删除（阶段 C）
- 移植：WIT host-session 增加 close；host_impl/session.rs 增加 session_close
  （权限 + kill_session_with_source 异步执行）；component.rs 接线；
  plugin-component-test version 6→7
```

## 6. 验证清单（合并后依次执行）

```bash
# 1. 宿主测试（198 个应全绿）
cd bedcode-desktop/src-tauri && cargo test --lib

# 2. SDK wasm32 编译（WIT 变更触发 bindgen 重新生成）
cd bedcode-desktop/packages/plugin-sdk-desktop/rust && cargo check --target wasm32-unknown-unknown --features wasm

# 3. auto-task 插件构建（关键：验证 host.session_close 的 WIT 绑定可用）
cd bedcode-desktop && cargo build --release --target wasm32-unknown-unknown \
  --no-default-features --features wasm --manifest-path plugins/auto-task/rust/Cargo.toml
# 4. componentize + 产物魔法字节验证（0d 00 01 00）
# 5. 前端测试（若 dev 合并带入前端改动）：cd bedcode-desktop && npm run test:run
```

## 7. 注意事项

- **docs/ 与 .scratch/ 不入库**：doc-tracking 钩子在非 dev 分支提交时自动剔除，合并产生的 docs 冲突不适用（本分支无 docs 改动）；合入 dev 后若需更新迁移知识文档，直接在 dev 上改
- **移动端不受影响**：dev 的移动端改动（Toast/UI/构建）与迁移分支零交集；移动端 SDK 无 session_close（v7 目前仅桌面端）
- **合入 dev 的顺序**：迁移分支内完成 merge dev → 验证 → 提交 → 切 dev 合并迁移分支（或 PR）。dev 若在此期间继续前进，按本预案的同一模式处理新冲突（组件形态为主，新 ABI 能力按 §3 流程移植）
- **遗留**：合并后迁移分支即与 dev 对齐，移动端 Component Model 迁移仍为独立后续任务（ABI 差异 + SDK 独立）

---

## 8. 执行结果（2026-08-08）

**已按本预案完成合并**，提交 `96d4bd50`（merge dev → feat/wasmtime-component-migration）：

- 4 个冲突按 §2 解决；session_close（v7）能力组件化移植完整落地（WIT `close` + host_impl 逻辑层 + component.rs 接线 + SDK `host_session::close`）
- **附带修复 dev 存量错误**：`auto-task/state.rs:349` 的 `http_response::ok(json)` 应为 `ok_with_data(json)`（dev 的 `f69a1a29` 引入，dev 上 auto-task wasm 构建本就失败）
- 验证：cargo test **212 passed**（含 dev 带入的文件传输 host 单测）；SDK wasm32 check 通过；auto-task/file-transfer/ai-chatbox 构建 + componentize 产物均组件形态（`0d 00 01 00`）
- dev 新增的 codex/opencode hooks（`scripts/*.py/.ts`）与 build.js 合并逻辑均完整保留

### ⚠️ 合入 dev 时的 docs 冲突（新增风险）

doc-tracking 钩子在 merge 提交时剔除了 index 中的 docs（受保护路径），
故本分支相对 dev **删除了 docs/（modify/delete）**。在 dev 上合并本分支时：

```bash
git checkout dev && git merge feat/wasmtime-component-migration
# docs/ 出现 modify/delete 冲突时保留 dev 版本：
git checkout --ours docs/ && git add docs/
# 迁移知识文档的"已完成"更新需要时，合并后在 dev 上直接修改 docs/knowledge/wasmtime-component-migration.md
```

不要用 `git rm docs/` 或接受删除——dev 的 docs 是受保护文件，必须保留。
