# 移动端权限授权机制桌面同构（单一真源：内核 PermissionManager）

> 用户指令（2026-10-14）：**移动端权限授权机制应该照搬桌面端；`grant_permissions` 应该退役或者删除。**
> （同日审计发现：内建插件在移动端 `host_storage_get` 被拒——日志 588/945/1014/1024 实证。）
>
> 附带项（用户消息中的「2」）：激活失败时 `NeedsApproval` 被 `markError` 覆盖成 `Error`，
> 待审批入口消失。
>
> 状态：**已实施**（2026-10-14）。实现落点与验证证据见文末「实施记录」。

---

## 0. 事实基线（2026-10-14 代码走读 + 日志实测）

| # | 事实 | 证据 |
| --- | --- | --- |
| F1 | **桌面**权限唯一真源 = 内核 `PermissionManager`；激活时按「生效权限」授予全部插件（内置 = manifest 全量；用户安装 = 批准 ∩ 请求），停用时 `revoke_all` | `packages/bedcode-wasm-core/src/manager/host/activation.rs:308-334`（授予）、`:817`（撤销）；`manager/host.rs:523-525`（静态插件装载期授予） |
| F2 | **桌面**权限判定 = `host_api::check_permission(scope=WasmHostContext, …)` → `PermissionScope::permission()` → PermissionManager | `host_api.rs:162-174`；`host_api/context.rs:1136-1140` |
| F3 | **移动**激活只在「已批准的非内置插件」分支授予 PermissionManager；内置插件**从不授予** → 走内核统一门的域（storage / plugin-database）全拒 | `bedcode-mobile/src-tauri/src/plugin/manager.rs:639-710`（`if source != ApkAsset && != FrontendOnly` 内才 `ctx.grant_permissions`）；日志 `bedcode_wasm_core::host_api: permission denied … permission=storage api=host_storage_get` |
| F4 | **移动**另有一套平行权限面：实例化时 `for_plugin(manifest ∪ storage)` 写 `WasmHostContext.granted_permissions` 字段 + `LoadedPlugin.granted_permissions` 镜像，供移动自报域 adapter（bindings / mdns / ws / http）与 `PluginManager::has_permission` 读 | `plugin/loader.rs:196-204`；`host_api/context.rs:347/617`；`plugin/bindings.rs:38-51`；`host_domains/{mdns,ws,http}.rs`；`plugin/types.rs:29`；`manager.rs:946-951` |
| F5 | 该平行面**未经审批过滤**（实例化即 manifest 全量）且**内置与用户安装同形**——与 F1 的「生效权限」口径不一致（安全面：未获批准的位在 adapter 门仍放行） | `loader.rs:198-201`；`approval.rs::effective_permissions` 注释「没有任何恒授予的特例」 |
| F6 | 移动端**没有**停用撤销（桌面 `revoke_all` 无对应）——停用后 PermissionManager 里旧授权仍在 | `manager.rs:861-905`（deactivate_inner 无 revoke） |
| F7 | 移动 SDK（guest 侧，仅测试/API 面消费）`grant_permissions` 无条件塞 `storage`，桌面 SDK 同方法已按票 02 取消默认位 | `bedcode-mobile/packages/plugin-sdk-mobile/rust/src/permission.rs:180-186`（`granted.insert(PERMISSION_STORAGE)`）对照 `bedcode-desktop/packages/plugin-sdk-desktop/rust/src/permission.rs:381-394`（断言不得默认授予） |
| F8 | 移动 `PluginLoader::loadFrontend` 失败路径 `pluginMarkError` 无条件置 `Error`，覆盖后端审批门禁写入的 `NeedsApproval` → 壳「待审批」入口（详情页 / 权限总览）不出现 | `src/plugin/loader.ts:157-167` + `src-tauri/src/plugin/manager.rs::mark_error`（无守卫）；壳端 `ShellAppDetailScreen.vue:42` 只在 `state === 'approval'` 渲染批准按钮 |

**结论**：移动端有 **3 套**权限面（内核 PermissionManager / 主机上下文镜像字段 / SDK 副本），
其中两套是移动独有且语义偏离桌面；真正被判定的那套（内核）在移动端只被半填充。按用户口径
收敛为**一套**：内核 `PermissionManager`，授予 / 撤销 / 判定全走桌面同一条路径。

---

## 1. 设计

### D1 唯一真源 = 内核 `PermissionManager`；移动对齐桌面

- 移动 `PluginManager::activate_inner`：按桌面同款计算**生效权限**并授予**全部插件**：
  - 受信来源（`ApkAsset` / `FrontendOnly`）→ `manifest.permissions` 全量
    （等价 `effective_permissions(requested, None, trusted = true)`）；
  - 用户安装 → `effective_permissions(requested, approval, false)` = 批准 ∩ 请求；
  - 授予点：审批门禁**之后**、guest `activate()` **之前**（guest 启动期即可用宿主能力）；
  - 词表外声明告警（与桌面 `dropped` 告警同款）。
- 移动 `deactivate_inner`：guest `deactivate` 之后 `revoke_permissions`（桌面 `revoke_all` 同款）。
- 判定：所有宿主侧权限判定统一 `WasmHostContext::has_permission`（内部 `PermissionManager::check`）。

### D2 移动独有的平行权限面退役（删除）

| 删除项 | 处理 |
| --- | --- |
| `WasmHostContext.granted_permissions` 字段 | 删除（含两个构造器初始化、`for_plugin` 参数） |
| `WasmHostContext::for_plugin(…)` | 删除（其余字段全是 Arc 克隆；调用方改用共享 `Arc<WasmHostContext>`） |
| `LoadedPlugin.granted_permissions`（移动镜像） | 删除（桌面 `LoadedPlugin` 早已不镜像——桌面注释「票 11 第 4 项：镜像字段只写不读」） |
| `loader.rs` 的 `granted = manifest ∪ storage` | 删除（不再有默认位；插件必须在 manifest 声明 `storage`——三个内置插件均已声明） |
| `PluginManager::has_permission` 读镜像 | 改读 `WasmHostContext::has_permission` |
| 移动自报域 adapter 的 `granted_permissions` 快照 | 改从内核管理器取（`has_permission` / `granted_permissions(plugin_id)` 快照） |

保留（**不是**移动独有）：guest 侧 SDK `PermissionManager` 与 `RustPluginContext.granted_permissions`
（桌面 SDK 同形）。仅删除移动 SDK 的 `storage` 默认位（D3）。

### D3 移动 SDK `grant_permissions` 去默认位（桌面同构）

- `bedcode-mobile/packages/plugin-sdk-mobile/rust/src/permission.rs`：删除
  `granted.insert(PERMISSION_STORAGE)`；补一条与桌面 SDK 同款的「storage 不得默认授予」测试。
- 影响面：`bedcode_plugin_api_mobile::PermissionManager` 现无生产消费方（`RustPluginContext::new`
  仅测试构造）；插件运行时权限判定本就在宿主 Rust 侧（AGENTS §8），此处只是把 guest 侧语义摆正。

### D4 新增的 `WasmHostContext` 公开窄接口（宿主 adapter 用）

```rust
pub fn has_permission(&self, plugin_id: &str, permission: &str) -> bool;   // = PermissionManager::check
pub fn granted_permissions(&self, plugin_id: &str) -> HashSet<String>;     // 端口构造快照
pub fn revoke_permissions(&self, plugin_id: &str);                          // = PermissionManager::revoke_all
// 既有 pub fn grant_permissions(...) 保留（管理器入口）
```
不暴露 `PermissionManager` 本体：安全闸门仍只经窄方法进出（沿用上下文既有纪律）。

### D5 附带修复：`NeedsApproval` 不被通用错误态覆盖

- `PluginManager::mark_error`：状态为 `NeedsApproval` 时**不覆盖**（待审批是需用户动作的事实，
  被 `Error` 覆盖会让审批入口消失）；其余状态照旧。
- 前端 loader 不改（它无法区分失败原因，后端守卫是唯一正确落点）。

---

## 2. 落点

| 文件 | 改动 |
| --- | --- |
| `packages/bedcode-wasm-core/src/host_api/context.rs` | 删字段 / `for_plugin`；加 `has_permission` / `granted_permissions` / `revoke_permissions`；注释同步 |
| `packages/bedcode-wasm-core/src/host_api.rs` | 文件头「移动形态 granted_permissions 仲裁」注释改为双端同源 |
| `packages/bedcode-wasm-core/src/test_support/bare.rs` | `build_test_component(granted)` 改经 `grant_permissions` 授权 |
| `packages/bedcode-wasm-core/src/permission.rs` | 头注释「移动宿主不经本表仲裁」更正 |
| `bedcode-mobile/src-tauri/src/plugin/{loader,types,manager,bindings}.rs` + `host_domains/{mdns,ws,http}.rs` | D1/D2/D5 |
| `bedcode-mobile/packages/plugin-sdk-mobile/rust/src/permission.rs` | D3（+ 测试） |
| 根 `CHANGELOG.md` / `CHANGELOG_zh.md` | 双语条目 |
| `bedcode-mobile/docs/code-map.md` + 桌面 `docs/knowledge/plugin-system.md` | 权限真源一句话同步 |

---

## 3. 行为契约

| 契约ID | 来源 | 规则 | 前置 | 预期 |
| --- | --- | --- | --- | --- |
| C-01 | F1/D1 | 内置插件激活后持有 manifest 全量权限 | ApkAsset 插件激活 | `has_permission(pid, "storage") == true`（file-transfer 日志缺陷回归） |
| C-02 | F1/D1 | 用户安装插件激活后只持有「批准 ∩ 请求」 | 已批准插件 | 未批准的声明位 `false`，批准的 `true` |
| C-03 | F1/D1 | 未批准插件不授予任何权限 | NeedsApproval | 激活被拒 + `has_permission == false` |
| C-04 | F6/D1 | 停用撤销全部权限 | Activated → deactivate | `has_permission` 全 false |
| C-05 | D2 | 权限判定与 `LoadedPlugin` 无关（镜像已删） | — | 无该字段可读（编译期） |
| C-06 | D2 | adapter 门与内核门同源 | 插件激活 / 停用 | bindings/mdns/ws/http 的判定结果与 `has_permission` 一致 |
| C-07 | D3 | SDK 不再默认授予 storage | 未声明 storage 的 manifest | `check("storage") == false`；声明了才 true |
| C-08 | D5 | NeedsApproval 不被 mark_error 覆盖 | mark_error(NeedsApproval 插件) | 状态仍 NeedsApproval |
| C-09 | D5 | 其余状态照旧可转 Error | Activated + mark_error | 状态 = Error{msg} |

---

## 4. 非目标

- 不改审批机制本身（哈希钉扎 / 整单批准语义不动）。
- 不做「停用即卸载实例」（上一次运行期 store 作用域的票已定：实例复用 + 插件 deactivate 自清）。
- 不动桌面既有授权流程（只删一个未被生产读取的上下文字段）。

## 5. 验证计划

- 移动宿主 `cargo test`（`bedcode-mobile/src-tauri`）：manager 契约（C-01..C-04、C-08/C-09）+ 既有全量。
- 内核 `cargo test`（`packages/bedcode-wasm-core`）：`check_permission` / 生命周期既有用例 + 编译面。
- 桌面宿主 `cargo test`（`bedcode-desktop/src-tauri`）：确认 `for_plugin` / 字段删除的编译与行为回归。
- 移动 SDK `cargo test`（`bedcode-mobile/packages/plugin-sdk-mobile/rust`）：C-07。
- 前端移动 `pnpm run test:run`（不受影响，跑一遍防回归）。
- 真机验证：file-transfer 打开后 `host_storage_get` 不再 denied（无设备时注明未跑）。

---

## 6. 实施记录（2026-10-14）

**落点（与 §2 一致）**

- wasm-core：删 `WasmHostContext.granted_permissions` 字段与 `for_plugin(权限集)`（无调用方，整方法删除）；
  新增 `has_permission` / `granted_permissions(plugin_id)` / `revoke_permissions` 窄接口；
  `test_support::build_component_state` 改经 `grant_permissions` 授权；`permission.rs` / `host_api.rs`
  / `context.rs` 头注释同步（移动不再有平行权限面）。
- 移动宿主：`loader` 传共享上下文（不再注入 manifest∪storage 副本）；`types.rs` 删镜像字段；
  `manager` 激活期按生效权限授予全部插件（受信 = manifest 全量；用户安装 = 批准 ∩ 请求；
  授予在审批门禁之后、guest activate 之前）+ 词表外声明告警；停用期 `revoke_permissions`
  （未运行时同样收权）；`bindings` / `host_domains/{mdns,ws,http}` / `peer_ports` / `has_permission`
  全部改读内核真源；`mark_error` 不再覆盖 `NeedsApproval`。
- 移动 SDK：`grant_permissions` 去 `storage` 默认位 + 3 条测试更新（无默认位口径）；
  README 中英 / 模板 README 同步。
- 测试改线：`terminal_session_e2e` / `ws_client_e2e` / `component_e2e` / `shared_artifact_probe`
  的 `for_plugin(权限集)` → 向共享上下文授权后 `Arc::clone`。
- 文档：移动 code-map 加「权限真源」条；桌面 `plugin-system.md` §5.3 修正（原文称「激活不重新授权」
  与代码相反）；CHANGELOG 双语条目。

**验证**

- 移动宿主 `cargo test` 全量绿（lib 239 + 全部集成目标 0 failed）；内核 `cargo test` 578 全绿；
  移动 SDK `cargo test --features wasm` 全绿；桌面 `cargo check --all-targets` 干净。
- 桌面 `cargo test --no-fail-fast`：除**既有无关失败** `capability_crates_unit_tests_only`
  （报 `bedcode-host-api-core/tests/boundary_lock.rs` 存在，该文件在 HEAD 已有、本票未动）外全绿。
- 新增 5 条契约用例：受信插件激活授予 manifest 权限（回归日志缺陷）/ 批准∩请求 /
  停用撤销 / mark_error 保留 NeedsApproval / mark_error 其它态置 Error。
- 未跑（手工项）：真机复验 file-transfer `host_storage_get` 不再 denied——本机无 Android 设备；
  行为由宿主侧 manager 契约用例 + 内核 578 用例覆盖。
- 环境注记：并行会话在同时重命名 `wasm-apps/terminal-session` → `wasm-apps/bedcode`，
  本票未触碰其文件；文档/清单里的路径引用以并行会话为准。
- 既有无关失败（未修，另行登记）：桌面 SDK `plugin-sdk-desktop` 的 `wire::key` 遗留格式 4 条用例。
