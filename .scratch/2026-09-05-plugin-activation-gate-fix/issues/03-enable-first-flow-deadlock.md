# 03 — file-transfer 启用死锁：预授权门禁与「配置需先激活」互为前置

Type: task
Status: resolved
Effort: plugin-activation-gate-fix
Parent spec: `spec.md`
Supersedes: spec.md 验收项「移除全部目录 → 启用恢复『请先配置共享目录』拒绝」与「注意」节遗留的流程断点

## 问题

宿主 `preauthorize_plugin`（桌面 `host.rs` / 移动 `manager.rs`）对 file-transfer 空共享目录硬拒绝激活，
但共享目录的唯一配置入口在插件自身 UI（sidebar view 内 SettingsPanel），而插件前端模块加载依赖
激活成功（激活失败 → frontend module load FAILED）：

```
启用 → 门禁拒绝(需先配置共享目录) → 插件 UI 无法加载 → 无法配置 → 再启用仍拒绝
```

首次启用永远失败；且用户移除全部已配置目录后会再次落入同一死锁（issue 01 的
`update_roots` 剔除 preauth_paths 使门禁重新变空）。

## 决策（2026-09-05，用户拍板）

**启用先行（enable-first）**：移除 file-transfer 空共享目录硬拒绝，空 `preauth_paths` 一律放行。
- 已配置目录时门禁行为不变：仍走 `fs_auth::check_batch` 单次合并弹窗。
- 「避免启用空功能插件」改由插件设置面板的空态引导承担，不在宿主门禁层拦截。

## 改动

| 端 | 文件 | 改动 |
|----|------|------|
| 桌面宿主 | `src-tauri/src/plugin/host.rs` | `preauthorize_plugin` 删除 file-transfer 空目录拒绝分支；测试改为断言放行 |
| 移动宿主 | `src-tauri/src/plugin/manager.rs` | 同上；删除随之失去使用方的 `FILE_TRANSFER_PLUGIN_ID` 常量 |
| 桌面前端 | `src/composables/usePluginManager.ts` | 删除对已不存在错误信息的特判分支 |
| 移动前端 | `src/views/PluginView.vue` | 同上 |
| i18n | 两端 zh-CN/en 各删 `enableAuthRequired` key（错误源已消失，避免死文案） |

## Answer

已修复（2026-09-05）。验证：桌面 `cargo test --lib`、移动 `cargo test`、桌面 `pnpm run test:run` 通过。
人工路径：dev 下启用 file-transfer → 直接激活成功 → 插件设置面板挂载共享目录（mount_local 写
preauth_paths）→ 停用再启用 → 单次合并授权弹窗。
