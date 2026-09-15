# 08: fs 链路接入统一授权框架

**What to build:** `host_impl/fs.rs` 的单路径 fs 宿主能力（read / write / copy / delete / exists）改经 `core-security` 三段决策管线（`SecurityFramework::authorize` + `ResourceKind::Fs`）授权，替代现状手工内联链（`check_permission` + `fs_auth.check`）。对外错误文案保持 `"permission denied"` 不变（插件契约与既有 18 个用例锁定）；改造后 fs 授权决策进入 `authz` 埋点，安全审计最活跃的资源路径首次可观测（此前 `authorize()` 的唯一生产调用点是 `host_impl/bus.rs` 互调门）。

批量预授权 `fs_request_auth`（一次弹窗覆盖多路径，`check_batch` 语义）**不在本票据范围**，保持现状并在代码注释说明原因。

**Blocked by:** 04（core-security 统一授权框架）

**Status:** resolved

- [x] `host_impl/fs.rs` 新增内部授权函数（构造 `AuthRequest` 走框架），5 个单路径函数改为调用它
- [x] 行为等价：权限不足 / 三层校验拒绝均返回 `"permission denied"`；`fs_copy` 保持源读 + 目标写双授权
- [x] 埋点生效：决策进 `authz.allow` / `authz.deny` 计数（`SecurityFramework::set_monitor` 已在 `PluginHost` 生产注入）
- [x] 测试：既有 18 个用例零回归；新增埋点断言（拒绝 → deny+1、放行 → allow+1）、声明段优先于路径白名单、`fs_copy` 双路径双权限
- [x] `cargo test` 全绿（桌面）

## Comments

- 2026-09-15 立项：来源为同日代码审核（`.scratch/wasm-core/audit-2026-09-15.md` 发现 S-1）——票据 04 已声明「统一路由后续渐进」，本次承接。
- 2026-09-15 完成：`host_impl/fs.rs` 新增 `authorize_fs()`（构造 `AuthRequest{resource: Fs, operation: "read"|"write", target: path}` 走 `SecurityFramework::authorize`），`fs_read`/`fs_write`/`fs_copy`/`fs_delete`/`fs_exists` 五个单路径函数全部改经它；`fs_copy` 保持源读 + 目标写双授权（两次授权）。对外错误文案仍为 `"permission denied"`，既有 18 个用例零回归。
  行为等价性核对：`FsAuthorizer` 的 declared 段权限映射（read→fs:read / write→fs:write）与 enforce 段 `fs_auth.check(plugin_id, path, FsOp)` 与原手工链逐项一致；新增的 `check_approved`（持久授权快速放行）不改变结果集。
  日志级别变化（有意为之）：授权拒绝统一为 `warn!` + 结构化字段（plugin_id/path/operation），替代原「权限不足 error! / 校验拒绝 warn!」的分裂写法——符合 AGENTS.md §8 日志红线（授权拒绝属可恢复异常/过滤拒绝）。
  `fs_request_auth`（批量预授权，`check_batch` 一次弹窗覆盖多路径）保持手工链，已在代码注释说明原因。
  测试新增 5 个（埋点 deny/allow、write 权限映射、copy 双权限、未授权路径强制段拒绝）；桌面 `cargo test --lib` 665（+5）全绿，集成测试全绿。
