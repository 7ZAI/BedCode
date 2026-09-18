# 阶段 2 摸底：设备连接编排下沉（com.bedcode.devices）——桌面端范围

Status: draft（摸底与范围建议，待确认后细化 spec）
Date: 2026-09-18
决策依据: `.scratch/2026-09-10-plugin-kernel-roadmap/spec.md`（阶段 2）、`docs/adr/0022-plugin-host-interface-primitive-boundary.md`（裁剪线）、`docs/adr/0017-plugin-inter-plugin-call-gate.md`（互调）、`docs/adr/0019-wasmtime-version-locked-across-ends.md`（双端锁版）
推进策略: **仅桌面端先行，移动端暂停推进**（2026-09-18 决策，已同步 roadmap / platform-kernel spec）

---

## 1. 摸底结论（先读这里）

桌面端存在**两套"设备"语境**，阶段 2 的下沉对象必须分开看：

| 语境 | 现状 | 阶段 2 判定 |
|---|---|---|
| **对等网络 peer**（桌面↔桌面 / 文件传输） | **已插件化 ✅**——引擎在 `packages/peer-net`（节点身份/自签证书/TLS1.3/信任存储/mDNS 发现/传输引擎）；宿主只留 `host_impl/peer.rs` 原语面（v2 句柄路由）；file-transfer 插件经 HostPeer trait 消费；设备列表派生视图已下沉插件侧（`deviceState.ts`） | **无剩余下沉工作**，com.bedcode.devices 无需覆盖 |
| **远程终端配对**（移动端↔桌面端认证） | **全在宿主侧**——commands 命令面 6 组 + utils/auth 安全原语 + server 连接注册表 + 前端 SettingsPairingSection / DevicesView / ConnectionHistoryView | **这是阶段 2 桌面端的实际下沉对象** |

**因此 com.bedcode.devices 的桌面端范围 = 远程终端配对的「编排 + 设备视图」下沉，安全原语留内核。**

---

## 2. 现状盘点（桌面端远程终端配对链路）

### 2.1 宿主命令面（Tauri commands，前端 invoke 直调）

| 命令 | 位置 | 语义 | 边界判断 |
|---|---|---|---|
| `generate_pairing_code` / `get_pairing_code_ttl` / `set_pairing_code_ttl` / `get_current_pairing_code` / `verify_pairing_code` / `clear_pairing_code` | `commands/system.rs`（PairingService） | 配对码生命周期 | 生成/校验=安全原语（留内核）；TTL 编排=业务（可下沉） |
| `list_paired_devices` / `remove_paired_device` | `commands/system.rs`（DB pairings 表） | 已配对设备管理 | 列表组织=业务（可下沉）；pairings 表=内核存储 |
| `list_connection_history` / `delete_connection_history` | `commands/system.rs`（DB） | 连接历史 | 业务（可下沉） |
| `generate_qr_code` / `clear_qr_code` / `get_qr_connection_info` / `get_qr_token_ttl` / `set_qr_token_ttl` | `commands/qr.rs`（QrTokenManager） | QR token 生命周期 | token 生成=安全原语（留内核）；展示编排=业务 |
| `get_connected_devices` | `commands/devices.rs` | 从 WS 连接注册表派生设备列表 | **业务投影**（注册表=内核；投影=业务，可下沉） |
| `list_quick_actions` / `create_quick_action` / `update_quick_action` | 宿主命令面（deviceCommands.ts 消费） | 快捷操作 | 业务（可下沉） |

### 2.2 安全原语（utils/auth —— 一律留内核，禁止旁路）

- `jwt.rs`：JWT 签发/校验、`generate_device_token` / `verify_device_token`
- `qr_token.rs`：`QrTokenManager`（128-bit 一次性 QR token + TTL，内存态）
- `pairing.rs`：配对码数据结构（`PairingCode`、TTL、Digit 常量）
- `biometric.rs`：生物凭证

### 2.3 引擎（server —— 一律留内核）

- `server/ws`：`WebSocketManager` 连接注册表（`list_clients`）
- `server/connection_types.rs`：`DeviceConnectionInfo`（addr/device_id/fingerprint/session_count）
- `server/filter.rs`（TrafficFilterChain）、`middleware/`、`link_crypto.rs`（链路加密）、`DeviceIdentity`

### 2.4 前端消费面（宿主 src/）

- `composables/commands/deviceCommands.ts`：6 组命令的封装（当前单一消费者）
- `components/settings/SettingsPairingSection.vue`：配对设置 UI
- `views/DevicesView.vue`：设备视图（`PluginPageToolbar target="devices"`）
- `views/ConnectionHistoryView.vue`：连接历史
- 集成测试：`src/__tests__/integration/pairing-flow.test.ts`

---

## 3. 边界分析（按 ADR 0022 尺度）

**留内核（无业务语义的引擎原语/安全边界）**：

- JWT 签发/校验（`generate_device_token` / `verify_device_token`）
- QR token 生成/校验/清除（`QrTokenManager` 内存态一次性 token）
- 配对码生成/校验（`PairingService` 的生成与校验内核，`PairingCode` 数据结构）
- WS 连接注册表（`WebSocketManager::list_clients` 原始记录）+ `DeviceConnectionInfo` 原始字段
- TrafficFilterChain / 链路加密 / DeviceIdentity / pairings 与 connection_history 存储表

**可下沉（业务编排/派生视图/UI）**：

- 配对流程编排：QR 何时生成/展示、TTL 设置与校验边界（`1..=86400` 是规则可下沉，但最终输入校验仍在 Rust 端——宿主原语层校验保留）
- 设备列表**派生视图**：注册表原始记录 → 展示字段组织（连接数、状态、排序）
- 已配对设备 / 连接历史 / 快捷操作的**列表组织与生命周期编排**
- 上述全部 UI（SettingsPairingSection / DevicesView / ConnectionHistoryView 迁至插件 `src/`）

---

## 4. com.bedcode.devices 桌面端范围建议

### 4.1 分步（每步独立可验证）

| 子步骤 | 内容 | 动 | 不动 |
|---|---|---|---|
| **2a** | WIT 新增 `host-auth` 原语接口（qr-token-generate/clear/status、pairing-code-generate/verify/clear、device-token-mint/verify、connections-list）——纯引擎原语，无业务语义；host_impl 接线 + component 注册 | WIT 契约 + ABI bump + 双端 host_impl 投影（ADR 0019）+ 权限门 | 现有 Tauri 命令面（并行期，插件未激活时宿主 UI 原样工作） |
| **2b** | 新建内置插件 `com.bedcode.devices`（rust-ts）：配对流程编排状态机 + 设备/历史/快捷列表组织，经 `host-auth` 原语访问安全能力 | 新插件工程 + 06 装配框架（依赖声明、权限声明） | 宿主命令面（并存） |
| **2c** | 前端 UI 迁移：SettingsPairingSection / DevicesView / ConnectionHistoryView 迁至插件 `src/`，deviceCommands.ts 消费改经插件互调/插件自有 composable；配对流程集成测试（pairing-flow.test.ts）迁移到插件 | 宿主前端 + 插件前端 | 其他设置分组 UI |
| **2d** | 退役宿主命令面（system.rs 配对/历史/快捷命令、qr.rs、devices.rs）与 pairings 相关宿主视图，验收行为等价 | 宿主命令面 | 安全原语（JWT/QR/配对码内核、连接注册表、filter 链） |

### 4.2 与 peer 侧的关系

- **不重复**：peer 设备列表已在 file-transfer 插件侧，com.bedcode.devices 只管远程终端配对链路，两者互不消费。
- **不做服务插件**：按 mDNS 决策确立的「引擎性能力留内核作基础服务」先例，若未来多个插件需要复用「已配对设备列表」，应评估 `host-auth` 原语扩展或基础服务形态，不建 WASM 服务插件。

---

## 5. 设计取舍（需拍板）

**Q1：安全原语暴露形态** —— 配对码/QR token/JWT 是安全敏感能力，下沉编排必须经原语而非旁路：

- 方案 A（推荐，与 host-peer 同构）：新增 `host-auth` WIT 原语接口，编排全部走原语；宿主命令面在 2d 退役。代价：ABI bump + 双端契约同步（移动端仅投影不消费，前例 v2 mDNS）。
- 方案 B（轻量）：配对链路 Tauri command 保留为「宿主安全命令面」，com.bedcode.devices 只下沉 UI + 编排 composable（前端 invoke 宿主命令）。无 ABI 变更、风险最低，但「编排下沉」打折（编排逻辑仍在宿主命令实现里）。
- 方案 C（折中）：先 2b/2c 下沉 UI + 编排层（调现有 Tauri command），2a/2d 的 `host-auth` 原语化作为二期（命令面退役前必须完成）。

**Q2：`get_connected_devices` 下沉方式** —— 派生视图下沉插件侧，需新原语 `connections-list`（返回注册表原始记录）还是复用现有事件（`peer-devices-changed` 是 peer 语境，不适用）？倾向 2a 顺带加 `connections-list`。

**Q3：quick_actions（快捷操作）是否随迁** —— 语义上属设备连接编排（设备维度快捷动作），建议随迁；若归属其他业务可留宿主。

**Q4：移动端契约同步范围** —— 若走方案 A，移动端仅 WIT/ABI 投影同步（不消费不实现 host_impl 业务），与 v2 mDNS 移动端处理一致；确认接受。

---

## 6. 风险与依赖

- **认证链路红线（AGENTS.md §8）**：JWT/QR token/配对码的**签发与校验内核绝不进 WASM**；插件只调原语。输入校验（如 TTL 边界）在宿主原语层保留，插件侧校验仅是 UX。
- **WIT/ABI（ADR 0019）**：方案 A 触发 ABI bump，双端契约必须同步更新（SDK 绑定 + host_impl 投影），即便移动端不消费。
- **依赖 06 系统组件装配框架**：内置插件 `com.bedcode.devices` 默认启用、只停不删；依赖检查（manifest `dependencies`）按 06 框架落地。
- **前端迁移风险**：SettingsPairingSection 是设置页分组（lang-fade 容器交互，参考 2026-09-17 拆分教训——父级持有 languageOptions/animationsEnabled 状态）；DevicesView 依赖 PluginPageToolbar。迁移需过 `frontend-styles` skill。
- **测试**：pairing-flow.test.ts 迁移后行为等价；新增原语单测（TTL 边界、一次性 token、注册表列表）+ 插件闭环测试。

---

## 7. 待确认

1. 方案 A / B / C 选哪个（推荐 A，二期化 C 为兜底）
2. quick_actions 是否随迁（Q3）
3. 移动端契约同步范围确认（Q4）
4. 本摸底确认后，细化为正式实施 spec（含 2a–2d 验收门槛）

---

## 附：参考位置

- 桌面宿主命令：`src-tauri/src/commands/system.rs`（配对/历史/快捷）、`commands/qr.rs`、`commands/devices.rs`
- 安全原语：`src-tauri/src/utils/auth/{jwt,qr_token,pairing,biometric}.rs`
- 引擎：`src-tauri/src/server/ws`、`server/connection_types.rs`、`server/filter.rs`、`server/link_crypto.rs`
- 前端消费：`src/composables/commands/deviceCommands.ts`、`src/components/settings/SettingsPairingSection.vue`、`src/views/{DevicesView,ConnectionHistoryView}.vue`
- 测试：`src/__tests__/integration/pairing-flow.test.ts`
- 先例：`packages/peer-net`（引擎留内核）、`host_impl/peer.rs`（原语面）、file-transfer 插件 `deviceState.ts`（派生视图下沉）、`host_impl/mdns.rs`（基础能力服务形态）
