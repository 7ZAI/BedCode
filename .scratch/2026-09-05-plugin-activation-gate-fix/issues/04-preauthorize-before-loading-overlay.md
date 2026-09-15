# 04 — 授权弹窗与 loading 遮罩同现：预授权未前置 + manifest 声明目录未纳入收集

Type: task
Status: resolved
Effort: plugin-activation-gate-fix
Parent spec: `spec.md`
Related: 03-enable-first-flow-deadlock.md

## 问题

用户实测（2026-09-05）：启用 ai-chatbox 时授权弹窗与 loading 遮罩**同时出现**，与「授权弹窗先执行
→ 后启动；拒绝则启动失败」的既定时序不符。两个断裂点：

1. **前端时序未实现**：注释声称「loading 遮罩推迟到 preauthorize 之后才显示」，但
   `usePluginManager.togglePlugin` / `PluginDetailView.handleToggle` 实际在调用 activate 前就置
   `togglingId`，遮罩立即出现；且宿主并无独立 preauthorize 命令可供前端先行调用——
   注释描述的流程从未落地。
2. **后端收集缺口**：ai-chatbox 的授权目录（manifest `wasiPreopenDirs` 声明的
   `${home}/.bedcode/ai-chatbox`）不在 preauthorize 收集范围（只收集 provider/`preauth_paths`），
   弹窗由插件 WASM activate 内部 `fs_request_auth` 触发——此时遮罩已盖上。

## 修复

| 层 | 文件 | 改动 |
|----|------|------|
| Rust | `wasm_runtime/component.rs` | 从 `resolve_preopen_dirs` 抽出纯展开函数 `expand_preopen_declarations`（`${home}` 展开/滤空，**不过滤授权**），原函数改为调用它 + 授权过滤；+1 单测 |
| Rust | `plugin/wasm_runtime.rs` | re-export `expand_preopen_declarations`（顺带修复文件内注释中的字面 NUL 字节导致整文件被识别为二进制的问题） |
| Rust | `plugin/host.rs` | `preauthorize_plugin` 并入 manifest `wasiPreopenDirs` 展开目录（短读锁克隆，不跨 await 持锁）；+2 单测（未授权无头拒绝 / 已授权放行） |
| Rust | `plugin/api_bridge.rs` + `lib.rs` | 新增 Tauri 命令 `plugin_preauthorize`（独立于 activate 供前端先行调用） |
| 前端 | `plugin/commands.ts` | `pluginPreauthorize` 封装 |
| 前端 | `composables/usePluginManager.ts` | togglePlugin 启用方向先 `await pluginPreauthorize`（无遮罩）→ 通过后置遮罩 → activate |
| 前端 | `views/PluginDetailView.vue` | handleToggle 同上 |

`plugin_activate` 内部 preauthorize 保留为兜底（启动 auto-activate 无头场景；已授权路径短路无二次弹窗）。

## 移动端（2026-09-05 补）

检查确认移动端存在**同款时序问题**：`PluginView.vue` toggle 在 `pluginLoader.activate` 前就置
`toggleLoading=true`，而 preauthorize 在后端 `manager.activate` 内部弹窗（`plugin:fs-auth-request`
事件 → 前端弹窗）。移动端 file-transfer 的 `preauth_paths`（mount_local 写入）启用时同样会触发。
修复（与桌面端同构，无 manifest 收集部分——移动端 `PluginManifest` 无 `wasi_preopen_dirs` 字段，
当前也无插件声明；若未来声明需另行移植）：

| 层 | 文件 | 改动 |
|----|------|------|
| Rust | `plugin/commands.rs` + `lib.rs` | 新增并注册 `plugin_preauthorize`（app_handle.state 取 PluginManager） |
| 前端 | `plugin/commands.ts` | `pluginPreauthorize` 封装 |
| 前端 | `views/PluginView.vue` | toggle 启用方向先 `await pluginPreauthorize`（无 LoadingDialog；授权阶段不启动 30s 超时计时）→ 通过后 `pluginSetEnabled` + loading → activate；拒绝则 catch 回退开关不遮罩 |

## 时序契约（修复后）

```
启用 → plugin_preauthorize（无遮罩,合并授权弹窗）→ 通过 → 显示遮罩 → plugin_activate
                                              ↳ 拒绝 → toast 失败,无遮罩,不进入激活
```

## Answer

已修复（2026-09-05）。验证：桌面 `cargo test`、`pnpm run test:run` 全绿；dev 日志确认时序。
移动端不受影响（ai-chatbox 数据目录 `{AppDownloadsDir}/ai-chatbox` 已存在且无 wasiPreopenDirs 弹窗场景）。
