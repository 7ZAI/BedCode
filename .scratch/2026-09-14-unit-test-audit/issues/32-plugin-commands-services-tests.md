# 32 — commands.rs + services.rs 补测试（711 行零测试）

**What to build:** 为 `plugin/host/commands.rs`（359 行）和 `plugin/host/services.rs`（352 行）补测试，当前零覆盖。

**Blocked by:** 无

**Status:** done（2026-09-15 修复）

- [ ] `commands.rs`：插件命令路由/分发逻辑
- [ ] `commands.rs`：命令参数校验
- [ ] `commands.rs`：错误处理路径
- [ ] `services.rs`：插件服务注册/查找
- [ ] `services.rs`：服务生命周期管理
- [ ] `services.rs`：服务间调用
- [ ] `cargo test --lib plugin::host::` 通过

## 证据

2 个文件共 711 行零测试。plugin host 层的命令路由和服务管理是插件系统的核心接线——路由错误会导致插件命令无法到达或被错误分发。

## 修复方向

提取纯逻辑（路由匹配、参数校验、服务查找）为可单测的函数，handler 层保留 Tauri/actix 接线。

## 影响面

仅新增测试 + 小范围重构。

## Comments

- 2026-09-14 审计发现，见 `../plugin-impl-spec.md` §3
