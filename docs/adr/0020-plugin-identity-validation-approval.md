# 插件身份校验与权限审批（防冒名顶替）

## 状态

已实施（移动端 2026-08 落地，见 `docs/implementation-plans/mobile-plugin-trust-hardening.md`；**桌面端 2026-09-22 补齐**，此前桌面侧只有实现与单测、无生产调用点，详见「修订记录」）

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
- `effective_permissions(requested, approval, trusted)`：生效权限 = 用户批准 ∩ manifest 请求；`trusted=true`（内置插件）直接全量。**无任何恒授予位**（原表述里的「`storage` 恒授予」已废止：该特例既是 `grant_permissions` 的默认位——票 02 移除——也是本函数的无条件插入，见「修订记录」）
- 批准集只记录词汇表内的声明位（`known_permissions`）：把词汇表外的装饰声明写进批准清单会让「用户看到的批准集」与「实际生效集」不一致
- `compute_dir_hash`：批准时对插件目录全部文件计算 SHA-256（相对路径排序，顺序稳定）；激活时 `verify_approval` 重算校验，不匹配 → 撤销批准（防「批准 A 后换入 B 代码」的在位攻击）。**运行时数据文件不入哈希**：插件私有 SQLite 库（`app_data/plugins/<id>/plugin.db`）及其 `-wal`/`-shm`/`-journal` 边车文件在启用后会变化，计入会让「批准 → 启用 → 再启用」被误判为内容替换
- 生效集**必须落到权限管理器**才算门禁：宿主激活时只 `grant_permissions(批准 ∩ 请求)`，宿主机能门查的是这份集合（不是 manifest 全量）

### 3. 信任边界

| 来源 | 信任域 | 行为 |
|------|--------|------|
| 桌面 resources 随包插件（FileScan/Wasm/StaticRegistry） | 应用构建产物 | 全量授权，无需审批 |
| 移动端 APK assets（ApkAsset）/ FrontendOnly | 应用构建产物 | 全量授权，无需审批 |
| 移动端用户安装（file-install / remote-download） | 不可信 | 必须人工审批 + 哈希钉扎，激活门禁（`manager.activate` 前置检查） |
| 桌面用户安装（zip file-install，`PluginSource::UserInstalled`） | 不可信 | 必须人工审批 + 哈希钉扎，激活门禁（`PluginHost::activate_plugin` 前置检查，2026-09-22） |

移动端落地内容：`validation.rs` / `approval.rs`（宿主侧）+ `manager.rs` 激活门禁 + `approve()` + `downloader.rs` 安装校验（id 格式 + 拒绝同 id 静默替换）+ `plugin_approve` 命令 + PluginView 审批 UI（权限清单弹层）+ 存量兼容（首启对已启用无审批记录的非内置插件自动批准一次）。

桌面落地内容（2026-09-22，审计票 03）：激活前置门禁 `approval_gate`（`NeedsApproval` + 显性拒绝 + 失配即撤销批准）+ `effective_permissions` 落权限管理器 + `PluginHost::approve_plugin` / `plugin_approve` 命令 + `PluginApprovalDialog` 审批弹层（列表页与详情页共用，高危位红标 + 后果文案）+ `downloader.rs` 的 `wasm_hash` 校验与解压体积/条目上限 + 存量兼容（首启对持久化已启用且无批准记录的用户插件自动批准一次并 `warn` 留痕）。

### 4. 状态模型

`PluginState` 新增 `NeedsApproval` variant（两端 SDK + 宿主 + 前端 types 同步），用于表达「请求的权限未经批准，激活被拒」。

## 影响

- 加载器：非法/伪造身份插件不再进入注册表与权限表
- 权限授予：审批门禁下生效权限严格为「批准 ∩ 请求」子集
- 兼容性：内置插件行为不变（可信域直通）；仅拒绝此前会被静默接受/覆盖的恶意形态

## 修订记录

- **2026-09-22 — 桌面端补齐（审计票 03）**：本 ADR 的状态自 2026-08 起与桌面代码相反。
  `approval.rs` 的 `approve` / `verify_approval` / `effective_permissions` / `compute_dir_hash`
  在桌面侧全仓唯一生产调用点是 `install.rs` 的 `revoke`（卸载清理），激活路径直接按 manifest 全量
  `grant_permissions` —— 即本 ADR 自陈要解决的「`process:run` 声明即授予」（风险 3）与「无内容钉扎」
  （风险 4）在桌面端**仍然成立**。本次补齐：

  1. 激活前置审批门禁：无批准记录或目录哈希失配 → 落 `NeedsApproval` + 拒绝激活（禁止静默降级为
     「部分权限」），失配同时撤销批准并要求重新审批；
  2. 生效权限 = 批准 ∩ 请求，并**写入权限管理器**（宿主机能门查的就是这份集合）；
  3. 哈希钉扎排除运行期数据文件（私有库 `plugin.db` 与 SQLite 边车文件），否则「批准 → 启用 →
     再启用」会被误判为内容替换；
  4. `plugin_approve` 宿主命令 + `PluginApprovalDialog` 审批弹层（列表页/详情页共用），高危位
     （`process:run` / `pty:spawn` / `terminal:input` / `database:main`）红标 + 后果文案，权限展示
     文案补齐到词汇表全量（此前 13/31，其余渲染为「未知权限」）；
  5. 安装期校验：manifest 新增可选 `wasm_hash`（与移动端同形，空串跳过）并在 `downloader.rs` 比对
     WASM 摘要；zip 解压补条目数 / 总量 / 单文件上限，失败路径清理临时目录；
  6. 存量兼容：持久化状态为已启用且无批准记录的用户插件，首启自动批准一次并 `warn` 留痕
     （照移动端先例，不打死用户现有功能）；「权限清单变更时重弹」本轮不加。

  **双端偏离（如实登记）**：`effective_permissions` 的 `storage` 无条件插入与
  `grant_permissions` 的 `storage` 默认位在桌面端已移除（票 02 / 票 03），移动端两份实现仍然保留
  ——该端要跟演需另行评估其私有库/主库权限门；本 ADR 的「生效权限 = 批准 ∩ 请求」在桌面端是
  **字面成立**的，在移动端仍是「批准 ∩ 请求 ∪ {storage}」。桌面 ABI 不动：`plugin_approve` 是宿主
  自有管理面命令，不进 WIT。签名链仍为后续，见下节。

- 签名链（宿主内置受信公钥 + 插件包签名）替代「哈希钉扎 + 人工审批」——哈希钉扎防本地替换，签名链解决「批准的是谁发布的包」；届时 `approval.content_hash` 可作为签名验证的哈希输入
- 发布者隔离（id 前缀与签名密钥绑定）依赖签名链，一并后置
- 重复安装拒绝导致「重装更新」受限：后续引入受信更新通道（签名链）后放开
