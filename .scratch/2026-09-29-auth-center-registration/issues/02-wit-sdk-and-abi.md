# 02: WIT `host-auth` 追加 4 函数 + SDK 绑定 + ABI 32

**What to build:** `host-auth` 追加 `auth-center-register` / `auth-center-unregister` / `auth-methods-list` / `auth-method-invoke`（spec §4.1），桌面 SDK 侧绑定 + guest 侧可调封装，desktop ABI 31 → 32。

**Blocked by:** 01

**Status:** todo

- [ ] WIT（`packages/plugin-sdk-desktop/rust/wit/bedcode.wit`）追加 4 函数 + 逐条文档注释（口径见 spec §4.1）
- [ ] 桌面 SDK：`abi.rs` `ABI_VERSION = 32` + `test_abi_version_is_v32` 同步
- [ ] 桌面 SDK：guest 侧 `wasm_auth_policy.rs`/host 封装加 4 个可调函数（`bedcode_plugin_api::host::auth::*` 同风格）
- [ ] 宿主 `host_api/auth.rs`：4 个 import 实现 + 权限门 `check_permission(perm, plugin_id, PERMISSION_AUTH, ..)`
- [ ] 错误信封（ADR 0030）：跨边界失败走 `{code, request_id}`，code 为前端 i18n key
- [ ] **移动端偏离登记**：`host-auth` 已是桌面独有 → mobile WIT **不加**、mobile ABI 保持 11；在 ADR 0031「双端偏离」与 checklist 记条款
- [ ] 单测：4 个 import 的权限门（无 `auth` 位一律 `err` 点名缺位）/ `auth-method-invoke` 未注册中心时 fail-closed 拒绝

## 关键实现事实

- **纯增量 ABI 变更**（函数级追加，不删不破）→ 旧产物仍可实例化；行为破坏由 fail-visible 形态 ② 覆盖（票 05）
- `auth-method-invoke` 是**零解析窄转发**：宿主不解析 `params`，与 `utils/session_gateway.rs` 同口径
- K8：复用 `PERMISSION_AUTH`（`"auth"`），**不新增权限词汇位** → 无需 `pnpm run gen:permissions`、不动 `manifest-gen.js` 映射表
- terminal-session manifest 已声明 `auth`（`plugin.json:11`）→ 无需改 manifest

## 验收

- `cd bedcode-desktop/src-tauri && cargo test` 全绿（§10 收尾跑）
- `cd bedcode-desktop/packages/plugin-sdk-desktop && cargo test` 全绿
