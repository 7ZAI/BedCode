# 04: WIT host-auth + SDK 投影 + 桌面 secret-store 实现（A2）

**What to build:** 桌面 WIT 新增 `host-auth` 接口（secret-store 原语：secret 的 set/get/delete + 属主隔离），SDK 桌面 guest 绑定与宿主实现：属主隔离、越权拒绝、delete、重启持久化、明文不落日志、权限门 deny；ABI 桌面 14→15（**v15 归本票**；host-pty 线已让位为 v16，见 `.scratch/2026-09-19-pty-base-service/spec.md` D7，两线不撞号）。**移动端不动**：AGENTS.md §7「改 WIT 双端同步」走文档化偏离（同 ws-base-service 先例，偏离记录留票 14）。

**Blocked by:** 02（接线一次到位，避免 async 化后返工）

**Status:** done（2026-09-19）

- [x] secret 属主隔离：插件 A 不能读/改/删插件 B 的 secret；越权返回明确错误
- [x] set / get / delete / 覆盖写 / 重启持久化 行为正确
- [x] 日志与存储不落明文（只记 `token.length()` 模式）；密钥明文不出宿主
- [x] 权限门：未声明 auth 权限的插件调用被 deny（Rust 端最终仲裁）
- [x] 宿主单测 + SDK wasm32 check 全绿；ABI bump 14→15 后旧插件（≤14）仍可加载（`version > 当前 → 拒绝` 语义兼容）

## 验证证据（2026-09-19）

- `cargo test --lib` **954 passed / 0 failed**（全量，含 host_impl::auth 5 单测 + 宿主全部）；auth 单测单独复跑 5/5 绿
- SDK `cargo check --features wasm` 过（guest 绑定三件套：host/auth.rs trait + wasm_host.rs impl）
- WIT 变更后 fixture 并发重建竞态复跑通过（aot_cache ×2 / fuel_exhaustion / backtrace，非代码问题）
- 前端 `pnpm run test:run` 直跑 70 files / 672 tests 绿（首跑 tinypool worker 崩溃为基础设施抖动，复跑即过）；根 `pnpm exec eslint .` 0 error

**产物**：WIT `interface host-auth`（secret-get/set/delete/keys）+ world import；ABI v15（abi.rs changelog + 测试更名）；`permission.rs` PERMISSION_AUTH="auth"；`host_impl/auth.rs`（权限门 + 主库 + read-through 缓存 + 明文只记长度）5 单测；`component.rs` Host impl + add_to_linker（import 同步）；`WasmHostContext.secrets_cache`；schema.sql `plugin_secrets` 表（CREATE TABLE IF NOT EXISTS，老库免迁移）；前端 permission.ts 'auth'。

**移动端**：未动（文档化偏离留票 14）。

**遗留风险**：SDK api_call.rs 请求 id 用 thread_local——wasip3 跨线程发起/回调可能 id 错位（当前单线程流内测试绿，未改，与移动端共享 SDK 故瞻前；票 03 已记录）。
