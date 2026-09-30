# 票 03 — ADR 0034 + 注释/文档口径反转

**状态**：resolved · 2026-09-30

## ADR

- `docs/adr/0034-wasi-preopen-worker-only.md`（新）：裁定记录 + 双侧显性拒绝 + worker
  启用时需补（wasip3 preopen 装配未解决）+ 双端偏离（移动端无 WASI 面，不适用）。

## 注释口径反转（全部仅注释，零逻辑改动）

| 文件 | 内容 |
| --- | --- |
| `packages/plugin-sdk-desktop/rust/src/types.rs` | `wasi_preopen_dirs` doc：仅 worker 类别可用（ADR 0034），双侧拒绝 |
| `packages/plugin-sdk-desktop/src/types.ts` | `wasiPreopenDirs` doc：同上（原「仅 wasm32-wasip2 插件」陈旧口径修正） |
| `src-tauri/.../runtime/component.rs` | 预打开节标题 + `build_wasi_ctx` / `resolve_preopen_dirs` / `preopened_dirs()`：仅 worker 类别可达 |
| `src-tauri/.../host/preauth.rs` | 步骤 1.5 注释：仅 worker 可达（原「如 ai-chatbox 数据目录」陈旧示例删除） |
| `src-tauri/.../host/activation.rs` | 漂移检测注释：仅 worker 类别可达 |
| `src-tauri/.../manager/runtime.rs` | `build_wasi_test_component` 注释：夹具 = worker 预留能力守门测试 |
| `src-tauri/.../security/fs_auth.rs` | `is_granted` 注释：preopen 消费者仅 worker 可达 |
| `src-tauri/.../host/tests/runtime_preauth_test.rs` | 两例注释：机制测试在策略闸门之下 |
| `src/composables/usePluginManager.ts` | preauthorize 注释：preopen 仅 worker 可达 |

## 文档反转

- `wasm-apps/ai-chatbox/README.md`：激活流程（删「首次启用需停用再启用完成预打开挂载」）
  + 数据目录（改 host-fs 绝对路径口径，删 guest `/data`/`std::fs` 直读直写说法）；
- `bedcode-desktop/docs/code-map.md`：plugin-wasi-test 两处 + 测试插件行（worker 守门测试）；
- `docs/knowledge/wasip3-toolchain.md` §6：preopen fixture 行补 worker-only + 装配悬项；
- `packages/plugin-wasi-test/`：`lib.rs` / `Cargo.toml` / `plugin.json`（CRLF，python 保留行尾改）描述改 worker-only 口径。

## 验证

改动文件全部经 pi-lens（Rust/TS/Markdown/TOML）检查，仅 advisory；SDK crate
`cargo test wasi_preopen` → 9 例全绿。
