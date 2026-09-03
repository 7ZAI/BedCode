# 移动端插件信任加固 Spec（防冒名顶替 + 权限审批）

> 状态：**待审核**（审核通过后按 §4 步骤实施）
> 关联：桌面端已实施 `docs/adr/0020-plugin-identity-validation-approval.md`（本 spec 为其移动端落地，机制同源、接线差异按 §2 现状盘点）

## 1. 背景与目标

### 背景

移动端插件系统与桌面端同构：插件身份 = `plugin.json` 自报 `id` 字符串，权限授予 = 「manifest 声明即信任」。存在与桌面端相同的冒名顶替攻击面，且**移动端有真实的用户安装通道（文件/URL 安装 zip），风险更高**：

| 漏洞 | 现状 | 后果 |
|------|------|------|
| 目录名 ≠ manifest id 无校验 | loader 以 `manifest.id` 为 key 加载 | 伪造目录 `com.bedcode.evil/` 内放 `com.bedcode.trusted` 的 manifest 即以受信身份加载 |
| 重复 id 静默覆盖 | `HashMap.insert` 后到覆盖；`downloader::install_zip` **remove_dir_all 后 rename 静默替换同 id 插件** | 冒名包顶替已装插件，旧权限/数据延续到新代码 |
| 权限无人工确认 | load 时 granted = manifest 全量声明（未过滤非法权限） | `process:run` 等任意代码执行权限声明即得 |
| 无内容钉扎 | 无批准概念 | 审批（若引入）不与代码绑定，批准后换文件即绕过 |
| id 无格式约束 | 仅非空校验 | 大写/下划线/超长 id 均可，路径与日志注入面 |

### 目标

1. 身份校验：id 反向域名格式 + 目录名=id 绑定 + 重复 id 拒绝（与桌面 `validation.rs` 同源）
2. 权限审批门禁：非内置插件激活前必须人工批准，生效权限 = 批准 ∩ 请求
3. 内容哈希钉扎：批准时钉扎目录 SHA-256，激活时校验，文件被替换 → 撤销批准
4. 安装校验：非法 id 安装包拒绝；同 id 已存在拒绝（不再静默替换）
5. 前端审批 UI：NeedsApproval 状态展示 + 权限清单确认弹层

### 非目标

- 签名链 / 发布者隔离（依赖 PKI 基础设施，后置，见 ADR 0020「后续」）
- 桌面端同套 UI（桌面无用户安装通道，机制已在 ADR 0020 落地）
- `wasm_hash` 传输校验改造（现有机制保留：防传输损坏，非防冒名）

## 2. 现状盘点（2026-08 核对，以实际代码为准）

### 2.1 宿主插件系统 `bedcode-mobile/src-tauri/src/plugin/`

- **`loader.rs`**（组件迁移版，可编译）：
  - `load_all(plugins_dir, wasm_runtime, wasm_host_ctx) -> (HashMap<String, LoadedPlugin>, HashMap<String, LoadedComponentPlugin>)`
  - 子目录遍历，跳过 `_`/`.` 开头目录；key = `manifest.id`；`HashMap.insert` 后到覆盖（**无重复检测**）
  - `detect_source`：读 `.bedcode-source` 标记 → `ApkAsset`（前缀匹配）/ `FileInstall` / `RemoteDownload`；**无标记默认 ApkAsset**（历史产物视为内置）
  - WASM 插件 load 时 granted = **raw manifest.permissions** + storage（未过滤非法权限名），经 `instantiate_component(..., granted)` 内嵌进 WASM 实例（宿主侧 `has_permission` 另有 LoadedPlugin.granted_permissions，见 2.2）
  - `load_manifest`：id/name/version 非空；TS-only 必须有 main；Wasm 必须有 rustLibrary
- **`manager.rs`**：
  - `PluginManager` 字段：plugins / wasm_runtime(OnceLock) / wasm_plugins / wasm_host_ctx(OnceLock) / storage(Arc<PluginStorage>) / settings(Arc<SettingsManager>) / plugins_dir / plugin_db / app_handle / fs_auth / message_bus
  - `scan_and_load`：load_all 结果 insert 进 map（覆盖语义）
  - `load_all(app_handle)`（启动）：settings 中 `PLUGIN_ENABLED_KEY_PREFIX{id}`=="true" → `activate`
  - `activate(plugin_id, app_handle)`：① 短锁查状态/类型（Activated 直接返回）② TS-only 仅置 Activated ③ WASM 取实例执行 activate 导出 ④ 更新状态。**无权限门禁**
  - `deactivate`：卸总线订阅 + 摘文件服务挂载（`unmount_plugin` + `after_unmount`）
  - `has_permission(plugin_id, perm)`：读 `LoadedPlugin.granted_permissions` —— **宿主侧权限裁决点**
  - `uninstall`：内置（ApkAsset）拒绝；deactivate → 移除实例/记录 → 清 enabled key + storage → 删目录
  - `report_ready`：Error/Loaded → Activated（自愈）
- **`downloader.rs`**：`install_from_file` / `download_and_install` → `install_zip(zip, plugins_dir, source)`：
  - 读 plugin.json（必须存在）→ 校验 id/name/version 非空 → `is_safe_zip_name` 防路径穿越 → 解压到 `plugins_dir/.download/{id}` → `wasm_hash` SHA-256 校验（声明时）→ 写 `.bedcode-source` 标记 → **rename 到 `plugins_dir/{id}`（已存在则先 remove_dir_all 静默覆盖）**
- **`commands.rs`**：plugin_list_loaded / plugin_get_info / plugin_activate / plugin_deactivate / plugin_is_enabled / plugin_set_enabled / plugin_mark_error / plugin_report_ready / plugin_storage_* / plugin_download / plugin_install_from_file / plugin_uninstall / reload_wasm_plugin / fs_auth 系列
- **`storage.rs`**：`PluginStorage::new(app_data_dir)`；get/set/delete/flush/clear_plugin（无 `__system__` 空间概念，key 约定自定义）

### 2.2 权限模型

- load 时 granted 进入两处：WASM 实例内嵌（instantiate_component 参数，SDK 侧插件自省）+ `LoadedPlugin.granted_permissions`（宿主裁决）
- 宿主裁决点：`manager.has_permission`（commands.rs 的 `require_fileservice` / `require_system_open` 调用）；fs_auth/transfer 的具体查询链实施时核对（原则：一切以 `LoadedPlugin.granted_permissions` 为准）

### 2.3 类型与状态

- SDK Rust `types.rs`：`PluginState { Loaded, Activated, Deactivated, Error{error} }`，serde tag="state" + camelCase；`PluginManifest` 含 `wasm_hash`/`rust_library`；SDK `permission.rs` 有 `VALID_PERMISSIONS` + `PERMISSION_STORAGE`
- 宿主 `types.rs`：`PluginSource { ApkAsset, RemoteDownload, FileInstall, FrontendOnly }`（无 serde derive）；`MobilePluginInfo.source` 序列化为字符串 `"apk-asset"` / `"remote-download"` / `"file-install"` / `"frontend-only"`
- 前端 `PluginState` 从 `@binblink/plugin-sdk-mobile`（`packages/plugin-sdk-mobile/src/types.ts`）导入，当前无 NeedsApproval

### 2.4 前端

- `src/plugin/commands.ts`：invoke 封装（pluginListLoaded / pluginActivate / pluginSetEnabled / pluginInstallFromFile / pluginDownload / pluginUninstall …）
- `src/plugin/loader.ts`：`activate(pluginId)` → pluginGetInfo → pluginActivate → loadFrontend（失败抛错，PluginView toast 展示）
- `src/views/PluginView.vue`：列表（已启用/未启用分区 + Toggle + 状态徽章）+ 详情（enable/disable/uninstall 按钮 + 权限 CollapseSection 已用 `getPermissionMeta` 展示清单）+ 安装弹层（文件/URL）+ 卸载确认。`isBuiltin(source === 'apk-asset')`；`getStateKey` 映射 stateActivated/stateError/stateLoaded/stateDeactivated
- i18n：`src/locales/{zh-CN,en}/mobile.ts`（plugin 状态 key：stateActivated/stateError/stateLoaded/stateDeactivated）

## 3. 目标形态

### 3.1 信任边界

| 来源（PluginSource） | 信任域 | 授权方式 |
|----------------------|--------|----------|
| `ApkAsset`（含无标记历史产物） | 应用构建产物 | 全量授权，无需审批 |
| `FrontendOnly` | 内置注册 | 全量授权，无需审批 |
| `FileInstall` / `RemoteDownload` | 不可信 | **必须人工审批 + 哈希钉扎**，激活门禁 |

### 3.2 模块设计

#### 3.2.1 `plugin/validation.rs`（新，与桌面同源）

```
pub const PLUGIN_ID_MAX_LEN: usize = 100;
pub fn validate_plugin_id(id: &str) -> bool      // ^[a-z0-9]+(\.[a-z0-9][a-z0-9-]*[a-z0-9]?)+$，≥2 段
pub fn validate_dir_binding(dir_name: &str, manifest_id: &str) -> bool  // 严格相等
```
接入：`loader.rs::load_all` 每目录校验（非法 id / 绑定不一致 / 重复 id → error 日志 + `continue`，先到先得）；`downloader.rs::install_zip` 校验包内 manifest id。

#### 3.2.2 `plugin/approval.rs`（新，与桌面同源，存储接口适配移动端）

```
pub const APPROVAL_STORAGE_KEY: &str = "plugin_approvals";   // plugin_id 空间 "__system__"
pub struct PluginApproval { approved_permissions: Vec<String>, content_hash: String, version: String, approved_at: String }
pub enum ApprovalStatus { Approved, Pending, HashMismatch }
pub struct PluginApprovalStore { storage: Arc<PluginStorage> }
    load_all() / save_all() / get(id) / approve(id, perms, hash, version) / revoke(id)
pub fn compute_dir_hash(dir: &Path) -> crate::Result<String>        // 全文件 SHA-256（相对路径排序）
pub fn effective_permissions(requested, approval, trusted) -> HashSet<String>  // 批准∩请求 + storage 恒授
pub fn verify_approval(approval, dir) -> crate::Result<(ApprovalStatus, String)>
```

#### 3.2.3 激活门禁（`manager.rs::activate` 开头）

```
非 trusted（非 ApkAsset/FrontendOnly）：
  approval = approvals.get(id)
  (status, hash) = verify_approval(approval, 插件目录)
  Approved  → granted_permissions = effective_permissions(requested, approval, false)（短锁写回，宿主裁决收紧）
  Pending   → state = NeedsApproval；Err("requires user approval")
  HashMismatch → revoke(approval)；state = NeedsApproval；Err("files changed since approval")
```
门禁覆盖 TS-only 与 WASM 分支（置于现有「1. 检查状态与插件类型」之前）。

#### 3.2.4 审批命令（`manager.rs` + `commands.rs`）

```
pub async fn approve(&self, plugin_id) -> Result<()>        // manager：内置拒绝；计算哈希 → 存审批 → NeedsApproval→Loaded
#[tauri::command] pub async fn plugin_approve(app_handle, plugin_id) -> Result<()>
```
卸载时 `approvals.revoke(id)`（`uninstall` 内）。

#### 3.2.5 存量兼容（迁移平滑策略）

首次启动扫描后：对 **已启用（settings enabled=true）且无审批记录的非内置插件** 自动批准（记录当前请求权限 + 当前哈希），避免升级后全部用户插件失活。此后任何文件变更 / 权限变更 → 哈希校验失败 → 需重新人工批准。

#### 3.2.6 安装校验（`downloader.rs::install_zip`）

- manifest id 非法格式 → 拒绝安装
- `plugins_dir/{id}` 已存在 → **拒绝**（错误信息：先卸载；内置同 id 亦拒绝——内置不可卸载）

#### 3.2.7 状态与 UI

- SDK Rust `types.rs`：`PluginState` 加 `NeedsApproval`
- SDK TS `types.ts`：`PluginState` union 加 `{ state: 'NeedsApproval' }`
- `PluginView.vue`：
  - 列表徽章：NeedsApproval 样式（琥珀/警示色）+ `getStateKey` 映射 `stateNeedsApproval`
  - 详情页：NeedsApproval 时操作区显示「批准权限」按钮（disabled 时无）→ 点击弹确认层（复用权限 CollapseSection 的 `getPermissionMeta` 展示完整权限清单 + 说明哈希钉扎）→ 确认调 `pluginApprove` → toast 成功 → 刷新列表
- i18n（zh/en）：`stateNeedsApproval` / `approve` / `approveTitle` / `approveDesc` / `approveSuccess` / `approveFailed` / `approveHint`（提示批准后文件变更需重新批准）

### 3.3 命令与类型变更清单

| 文件 | 变更 |
|------|------|
| `src-tauri/src/plugin/validation.rs` | 新增 |
| `src-tauri/src/plugin/approval.rs` | 新增 |
| `src-tauri/src/plugin.rs` | 注册 2 模块 |
| `src-tauri/src/plugin/loader.rs` | 身份校验块（格式/绑定/重复） |
| `src-tauri/src/plugin/downloader.rs` | id 格式校验 + 同 id 拒绝安装 |
| `src-tauri/src/plugin/manager.rs` | 门禁 + `approve()` + uninstall revoke + 存量自动批准 |
| `src-tauri/src/plugin/commands.rs` | `plugin_approve` |
| `packages/plugin-sdk-mobile/rust/src/types.rs` | PluginState + NeedsApproval |
| `packages/plugin-sdk-mobile/src/types.ts` | 同上（TS） |
| `src/plugin/commands.ts` | `pluginApprove` |
| `src/views/PluginView.vue` | 徽章 + 审批弹层 |
| `src/locales/{zh-CN,en}/mobile.ts` | 审批文案 |

## 4. 实施步骤

- **S1**：`validation.rs` + `loader.rs` 校验 + `downloader.rs` 安装校验（含单测）→ `cargo test`
- **S2**：`approval.rs`（store + hash + effective + verify，含单测）→ `cargo test`
- **S3**：`manager.rs` 门禁 + `approve()` + 存量自动批准 + `commands.rs` 命令 → `cargo test`
- **S4**：SDK Rust/TS PluginState + 前端 commands/types → 编译
- **S5**：`PluginView.vue` 审批 UI + i18n → `npm run test:run` + 手动验收
- **S6**：文档同步（ADR 0020 移动端状态更新）

## 5. 风险与缓解

| 风险 | 缓解 |
|------|------|
| 升级后存量用户插件全部失活 | §3.2.5 存量自动批准（仅首启一次性） |
| WASM 实例内嵌 granted（全量上限）与宿主收紧集不一致 | 宿主裁决为准（`has_permission`/fs_auth 均走 `LoadedPlugin.granted_permissions`）；实例内嵌仅插件 SDK 侧自省，文档注明 |
| dev 窗口（非 Android）内置复制标记 ApkAsset → 直通 | 符合信任边界（构建产物），无影响 |
| 无标记历史目录默认 ApkAsset 直通 | 保持现状（历史产物视为内置），文档注明 |
| 重复安装被拒影响「更新插件」流程 | 当前无更新通道；错误信息引导先卸载（无签名链时无法区分更新与冒名，安全优先） |

## 6. 验收标准

- 宿主 `cargo test` 通过（新增 validation/approval 单测 + 既有 293+ 全绿）
- SDK `cargo test --features wasm` 通过（PluginState 序列化）
- 前端 `npm run test:run` 通过
- 手动场景（模拟器/桌面 dev 窗口）：
  1. 安装 zip → 列表出现「待授权」→ 启用被拒（toast）→ 详情页批准（权限清单正确）→ 启用激活成功
  2. 批准后修改插件目录文件（如 wasm）→ 停用再启用 → 拒绝 + 重新待授权
  3. 重复安装同 id → 拒绝且旧插件不受影响
  4. 内置插件（auto-task/ai-chatbox/file-transfer）激活行为不变

## 7. 工作量粗估

S1-S2 各 ~1h，S3 ~1.5h，S4 ~0.5h，S5 ~1.5h，S6 ~0.5h；合计 ~6h（含测试与回归）。
