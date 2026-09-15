# BedCode WASM 插件 Component Model 迁移 — 交接文档

> 生成时间：2026-08-06 · 交接目的：让新会话/新 agent 无缝接续迁移工作
> 最近更新：2026-08-08（阶段 C 宿主清理完成，三阶段全部落地）

## 任务背景

`docs/knowledge/wasmtime-component-migration.md`（dev 分支上，受 doc-tracking 保护，
工作区副本存在于两个 worktree）定义了从自研 WASM ABI（`__bedcode_*` 导出 +
`bedcode` 命名空间 host functions）迁移到 Component Model（WIT + wit-bindgen）的
三阶段方案：**A 协议共存 → B SDK 切换 → C 宿主清理**。文档 §5 原结论是"暂不迁移"，
用户已决定启动（在隔离 worktree 中实施，可随时回滚）。

## 当前状态（三阶段全部完成）

**Worktree**：`D:\tauriProject\BedCode-wasmtime-component`，分支
`feat/wasmtime-component-migration`（基于 dev `f38af77d`）。
**提交**：
- `1e84b765 feat(plugin): WASM 插件 Component Model 迁移阶段 A——宿主协议共存`
- `6fee642d feat(plugin): 阶段 A 收尾——补全宿主 13 组接口接线并删除 plugin-phase-a 世界`
- `6d420d5e feat(plugin): 迁移阶段 B——SDK 切换为 Component Model 组件形态`
- `7b668a6b feat(plugin): 迁移阶段 C——宿主清理，仅保留 Component 形态`
- `96d4bd50 merge: 合入 dev（v7 会话关闭能力组件化移植 + 修复 dev 存量构建错误）`
**验证**：`cd bedcode-desktop/src-tauri && cargo test --lib` 全绿（**212 passed**，
含 dev 带入的文件传输 host 单测）。三个内置插件 wasm32 构建 + componentize
验证通过（产物 `0d 00 01 00`）。

### 阶段 A（宿主共存）已交付

| 文件 | 内容 |
|---|---|
| `packages/plugin-sdk-desktop/rust/wit/bedcode.wit` | WIT 契约（单一事实来源）：13 组宿主能力 import + 7 组插件导出；`plugin` 为唯一 world |
| `src-tauri/src/plugin/wasm_runtime/component.rs` | `bindgen!` 绑定、13 组 Host trait 全部接线、`LoadedWasmPlugin`（原 ComponentWasmPlugin） |
| `src-tauri/src/plugin/wasm_runtime.rs` | `WasmRuntime`、AOT 缓存、epoch 中断、ResourceLimiter |

### 阶段 B（SDK 切换）已交付

| 文件 | 内容 |
|---|---|
| `SDK rust/src/wasm.rs` | `wit_bindgen::generate!`（`pub_export_macro: true` + `default_bindings_module: "$crate::wasm"`）；`wasm_entry!` 宏重写为 7 组 Guest trait impl + `$crate::wasm::export!` 组件导出 |
| `SDK rust/src/wasm_host.rs` | `WasmHost` 13 组 host trait 改调 wit-bindgen import；错误经 WIT `result<string>` 透传宿主可读消息；删除 extern C/(ptr,len)/native_link_stubs |
| `SDK rust/src/lib.rs` | `pub use wasm::bedcode`（export! 宏类型路径） |
| `packages/plugin-sdk-test/` | SDK 组件测试插件（真实宏 + WasmHost），宿主测试 `test_sdk_plugin_component_roundtrip` 覆盖 storage/db/config/session/events/bus/notify |
| `SDK rust/tools/componentize/` | 组件编码工具（wit-component 0.255，等价 wasm-tools component new，幂等） |
| `plugins/*/scripts/build.js`（×3） | cargo build 后调用 componentize，产物从 core module 变组件 |

### 阶段 C（宿主清理）已交付

| 文件 | 内容 |
|---|---|
| `src-tauri/src/plugin/wasm_runtime/host_impl/`（16 文件） | 由 `host_functions/` 迁移：逻辑层函数保留（权限校验 + 服务调用），全部 core 胶水删除 |
| `src-tauri/src/plugin/wasm_runtime.rs` | 删 `LoadedCorePlugin`、`LoadedWasmPlugin` 枚举（改 `pub use component::LoadedWasmPlugin`）、`compile_module*`/`instantiate`/core linker；`load_plugin_from_file` 只走组件路径 |
| `src-tauri/src/plugin/wasm_runtime/component.rs` | `ComponentWasmPlugin` → `LoadedWasmPlugin`；删 `ArtifactKind`/`detect_artifact_kind` |
| `SDK rust/src/abi.rs` | 瘦身为仅 `ABI_VERSION` + `FORM_COMPONENT`（wasm.rs 宏用）；删 NAMESPACE/MEMORY/export/import/签名表 |
| `packages/plugin-test/` | **已删除**（core 形态测试载体） |
| 测试 | 删 15 个 core 路径测试（签名表/内存搬运/host fn 注册/AOT core/连通性）；补组件 AOT stale 回退测试；`test_load_plugin_from_file` 简化 |

### 关键实现细节（新会话必读）

- **SDK 宏展开要点**：
  - `wasm_entry!` 参数必须 `ident` 而非 `ty`：Rust 宏片段卫生限制——`ty` 片段传给
    需要 `ident` 的 `export!` 宏报 "no rules expected ty metavariable"
  - 宏体内插件类型方法调用必须显式限定：`<$plugin_type as $crate::wasm::WasmPlugin>::xxx()`
    （插件类型同时实现 WasmPlugin 与 Guest traits，同名方法歧义 E0034）
  - export trait 在 `exports::bedcode::plugin::<iface>::Guest`（wit-bindgen 0.41
    的 export 命名空间），import 函数在 `bedcode::plugin::<iface>`
- **WasmHost 改造**：trait 签名不变（插件业务零改动）；native（非 wasm32）构建由
  wit-bindgen stub（unreachable!）兜底，替代原 native_link_stubs
- **ABI 差异**：组件 log 无 file/line 调用点（WIT host-log 暂无通道）；错误从
  core 的 -1 状态码升级为 WIT 错误串透传（宿主可读消息）
- **host 测试环境限制**：无头（无 app_handle）→ plugin-db/notify 按设计返回错误
  （断言错误透传）；fs 依赖 fs_auth 白名单（测试未覆盖）
- **componentize 工具**：幂等（产物已是组件直接复制）；插件 build.js 中
  `cargo run --manifest-path .../componentize/Cargo.toml`
- 宿主测试 `build_sdk_test_component` 的源码变更检测覆盖 SDK 的 wasm.rs/wasm_host.rs/wit
- doc-tracking 钩子：本分支（非 dev）提交自动剔除 docs/AGENTS.md/.pi/.scratch，属预期

## 下一步（未完成）

**✅ 已合入 dev**（`0b95ce17`，2026-08-08）：
- 在 dev 上 `git merge feat/wasmtime-component-migration`；docs 被合并剔除后
  按规则恢复（`git checkout 0664e213 -- docs/`，dev 为跟踪分支正常入库）
- dev 上验证全绿：cargo test **212 passed**、SDK wasm32 check ✓、
  三插件构建 + componentize 组件形态 ✓、前端 vitest **240 passed**（22 文件）
- 待办：删除 worktree（`git worktree remove`）、push 两个分支

**移动端**：`bedcode-mobile` 同步迁移（ABI 有差异，如 log 参数；移动端 SDK 独立）。
可复用本分支的三阶段经验与 WIT 契约结构（移动端 world 需按自身能力面裁剪）。

**发布流程**：正式版本构建走 `npm run tauri:build` → 各插件 `plugin-build.js` →
内置插件自动组件化。

## suggested skills

- `frontend-styles` — 若涉及插件前端 UI 改动
- `ctx7` — 查询 wasmtime/wit-bindgen/wit-component 最新 API 文档（版本演进快）
- `code-review` — 阶段 B/C 提交合入前的审查
- `tdd` — 移动端迁移前先确认测试矩阵

## 测试矩阵（198 个 lib 测试）

| 路径 | 载体 | 覆盖 |
|------|------|------|
| 组件（手写 wit-bindgen） | `plugin-component-test` | 绑定层、13 组 import 接线、多接口往返 |
| 组件（SDK 宏 + WasmHost） | `plugin-sdk-test` | 宏产物、WasmHost 全 trait、错误透传 |
| 宿主运行时 | — | AOT 缓存命中/stale 回退、load_plugin_from_file 组件路径、epoch/ResourceLimiter |
| 宿主能力层 | `host_impl/*`（database 内嵌测试） | SQL 表名前缀校验、参数绑定辅助 |
