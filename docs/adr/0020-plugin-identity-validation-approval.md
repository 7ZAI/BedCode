# 插件身份校验与权限审批（防冒名顶替）

## 状态

已实施（桌面端 2026-08；移动端同月落地，见 `docs/implementation-plans/mobile-plugin-trust-hardening.md`）

## 背景

插件系统以 `manifest.id`（plugin.json 自报字符串）作为插件唯一身份，权限授予为「manifest 声明即信任」（`grant_permissions` 按声明自动全量授予）。该模型存在冒名顶替攻击面：

1. **目录名 ≠ manifest id 无校验**：目录 `com.bedcode.evil/` 内放 id 为 `com.bedcode.trusted` 的 manifest，即以受信身份加载（watcher 热重载、卸载、文件服务路径按目录名定位，而注册表/权限按 manifest.id 定位，两者脱钩）
2. **重复 id 静默覆盖**：扫描加载用 `HashMap.insert`，后加载的插件直接覆盖先加载的（含静态注册内置插件）；`PluginRegistry` 的 command 注册同样后到覆盖
3. **权限无人工确认**：`process:run` 等任意代码执行权限，manifest 声明即授予
4. **无内容钉扎**：批准（若存在）与代码内容不绑定，批准后替换插件文件即绕过审批

## 决策

### 1. 身份校验（validation.rs）

- `validate_plugin_id`：id 必须为反向域名格式（`^[a-z0-9]+(\.[a-z0-9][a-z0-9-]*[a-z0-9]?)+$`，≤100 字符，至少两段）
- `validate_dir_binding`：插件目录名必须与 manifest id 完全一致
- 扫描时（`PluginLoader::load_all`）：非法 id / 目录绑定不一致 / 重复 id 一律拒绝加载并记录 error 日志（重复 id 先到先得）

### 2. 权限审批与内容钉扎（approval.rs）

- `PluginApprovalStore`：批准记录持久化于 `plugin_storage` 表 `__system__` 空间的 `plugin_approvals` key（与激活状态持久化同模式）
- `PluginApproval`：`{ approved_permissions, content_hash, version, approved_at }`
- `effective_permissions(requested, approval, trusted)`：生效权限 = 用户批准 ∩ manifest 请求（`storage` 恒授予）；`trusted=true`（内置插件）直接全量
- `compute_dir_hash`：批准时对插件目录全部文件计算 SHA-256（相对路径排序，顺序稳定）；激活时 `verify_approval` 重算校验，不匹配 → 撤销批准（防「批准 A 后换入 B 代码」的在位攻击）

### 3. 信任边界

| 来源 | 信任域 | 行为 |
|------|--------|------|
| 桌面 resources 随包插件（FileScan/Wasm/StaticRegistry） | 应用构建产物 | 全量授权，无需审批 |
| 移动端 APK assets（ApkAsset）/ FrontendOnly | 应用构建产物 | 全量授权，无需审批 |
| 移动端用户安装（file-install / remote-download） | 不可信 | 必须人工审批 + 哈希钉扎，激活门禁（`manager.activate` 前置检查） |

移动端落地内容：`validation.rs` / `approval.rs`（宿主侧）+ `manager.rs` 激活门禁 + `approve()` + `downloader.rs` 安装校验（id 格式 + 拒绝同 id 静默替换）+ `plugin_approve` 命令 + PluginView 审批 UI（权限清单弹层）+ 存量兼容（首启对已启用无审批记录的非内置插件自动批准一次）。

### 4. 状态模型

`PluginState` 新增 `NeedsApproval` variant（两端 SDK + 宿主 + 前端 types 同步），用于表达「请求的权限未经批准，激活被拒」。

## 影响

- 加载器：非法/伪造身份插件不再进入注册表与权限表
- 权限授予：审批门禁下生效权限严格为「批准 ∩ 请求」子集
- 兼容性：内置插件行为不变（可信域直通）；仅拒绝此前会被静默接受/覆盖的恶意形态

## 后续（未实施）

- 签名链（宿主内置受信公钥 + 插件包签名）替代「哈希钉扎 + 人工审批」——哈希钉扎防本地替换，签名链解决「批准的是谁发布的包」；届时 `approval.content_hash` 可作为签名验证的哈希输入
- 发布者隔离（id 前缀与签名密钥绑定）依赖签名链，一并后置
- 重复安装拒绝导致「重装更新」受限：后续引入受信更新通道（签名链）后放开
