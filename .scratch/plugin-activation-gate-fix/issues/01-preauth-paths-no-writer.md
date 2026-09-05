# 01 — file-transfer 预授权门禁断裂：preauth_paths 无写入方

Type: task
Status: resolved
Effort: plugin-activation-gate-fix
Parent spec: `spec.md`

## 问题

宿主 `preauthorize_plugin`（桌面 `host.rs` / 移动 `manager.rs`）收集预授权路径时：
1. 优先查 `PREAUTH_PROVIDERS` 静态注册表（全仓库无任何 `register_preauth_provider` 调用）
2. 回退查插件 storage key `preauth_paths`（file-transfer 插件 rust 中无任何 `storage_set` 写入）

两者均无写入方 → 对 file-transfer **恒得空路径** → 命中「共享目录未配置」特判 → 恒返回
`"Please configure shared directories in plugin settings first"`，插件永远无法启用（即便已配置共享目录）。

引入 commit `73e87088`（2026-09-04）声称「mount-local 时追加写入 preauth_paths」，代码从未落地。

## 根因

`plugins/{desktop,mobile}/file-transfer/rust/src/peer.rs::mount_local` 与 `update_roots`
只维护共享目录注册表（桌面 plugin-database 表 / 移动 storage 键），未同步宿主启用门禁读取的 `preauth_paths` 数组。

## 修复

两端 `peer.rs`：
- `mount_local`：`apply_and_push` 成功后把新目录路径追加进 storage key `preauth_paths`（去重，幂等）
- `update_roots`：移除共享目录时同步剔除对应预授权路径（未知 id 幂等 no-op）
- storage 写失败如实上抛（不静默吞掉，重试幂等）

## Answer

已修复（2026-09-05）：两端 peer.rs 增加 `PREAUTH_PATHS_KEY` + `load/push/remove_preauth_path`
辅助函数，`mount_local`/`update_roots` 接入；测试：桌面 3 个（追加去重、剔除、未知 id）、移动 2 个。
宿主侧无需改动（读取逻辑已就绪，storage key 字符串两端与宿主常量 `"preauth_paths"` 一致）。
验证：桌面 `cargo test` 23 passed、移动 22 passed；wasm32-unknown-unknown 产物可编译。
