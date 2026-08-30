# host-peer WIT 接口原语化收缩（ADR 0022 v2 实施）

Status: in-progress（Phase 1–4 已实施并验证，2026-08-26；真机回归待执行，见下文各 Phase 记录）

> **2026-08-26 ADR 0022 v3 修订**：经裁决 `set-receive-policy` / `set-download-dir` 改判「引擎安全闸门/落盘配置」原语保留（非业务编排），终态 host-peer = **13 个函数**；下文 Goal 的「11 个」为 v2 时点表述，以 v3 为准。

## 实施进度

### Phase 1 — host-peer v2 契约落地 ✅（2026-08-26）

**实现方式偏离说明（重要）**：issue 原计划「新函数与旧函数并存」逐条新增；实际实施时发现终态清单中 5 个函数与旧函数 **WIT 形状完全一致**（参数/返回均为 string），故采用**多态寻址**而非平行重复函数：

| 终态函数 | Phase 1 载体 |
|----------|--------------|
| `dial-peer(endpoint)` | 新增 `dial-peer-endpoint(endpoint-json) -> session-handle`（旧 `dial-peer` 原样保留，Phase 4 删旧后改名） |
| `close(handle)` | 新增 `close`（旧 disconnect/cancel/cancel-receiving 保留至 Phase 4） |
| `respond-consent` / `respond-transfer` / `list-trusted` / `revoke-trusted` | 已存在，签名不变，零改动 |
| `set-shared-roots(dirs)` | 新增（桌面条目 `{id,name,path}` / 移动 `{id,name,safTreeUri}`） |
| 读对端四函数按句柄寻址 | 复用现有同名函数：首参接受 session 句柄（`sess-<uuid>`）或旧式 node-id，宿主路由表翻译；Phase 4 收紧为仅句柄（WIT 无需再改） |

传输句柄 = batch-id（send/pull 返回 DTO 内已含，peer:transfer/receive 事件同源），不另造句柄空间。resume 语义：接收端 .part 已写偏移自动续传（既有行为），未新增 query-written-offset（二选一裁决取前者）。

### Phase 2 — host-mdns + host-platform ✅（2026-08-26）

- `host-mdns`：browse/stop-browse 句柄生命周期；found/lost 经总线原样透传（instanceName/addresses/port/txtRecords）；虚拟网卡禁用复用引擎逻辑（`disable_virtual_interfaces` 转 pub）；插件停用即回收其全部 browser（两端 PluginHost/Manager deactivate 钩子）；移动端 MulticastLock 首浏览 best-effort 申请、主动释放随 Phase 4 守护常开浏览退役一并落地
- `host-platform`：pick-files/pick-folder 平移（桌面复用 peer_transfer 既有实现，Phase 4 迁移本体）
- 权限：新增 `mdns` permission kind（双端 VALID_PERMISSIONS/API_MAP 同步）；platform 选源不设门（用户授权动作）

### 验证（2026-08-26）

- 引擎：packages/peer-net 全套件 102 绿（含 replace_all 新增 2 测试：全量替换持久化 + 批次原子拒绝）
- 桌面：SDK native+wasm 编译绿 + 75 测试绿；src-tauri 整库编译零错误 + **cargo test --lib 550 全绿**
- 移动：SDK native+wasm 编译绿 + 63 测试绿；src-tauri 整库编译零错误 + **cargo test --lib 299 全绿**
- 句柄路由表单测 ×4（铸造/解析/取出/双态寻址透传）；plugin-component-test 版本字面量随 ABI bump 同步
- 真机清单：待 Phase 3 插件切换后统一执行（本阶段无 UI 可测）

### 附带收尾（前一会话中断遗留，非本票范围）

issue 09 抽共享 crate 的半成品导致两端的整库编译被阻断，为解除验证阻塞做了机械性补全（意图均已在代码注释中明示）：desktop link_crypto.rs 删除与 `pub use proto::{…}` 重复的本地定义 + lib.rs 补 WsTextEnvelope re-export；mobile router/registry.rs 补 AuthPayload 新增 crypto 字段的初始化器。相关文件仍属 issue 09 工作区改动，提交归属由用户裁决。

### Phase 3 — file-transfer 业务自持（双端）✅（2026-08-26 实施，待真机验证）

实施方式与关键裁决（详见会话记忆）：

- **设备缓存状态机放前端 TS**（deviceState.ts 纯函数：parseFoundPayload/applyLost/sweepStaleDevices + cap 位 bit0 解读 + TTL=120s 惰性清扫）：解决 wasm32-unknown-unknown 无时钟问题（两端插件 target 均为 unknown-unknown，禁 std::time/uuid/getrandom，TTL 判定需 Date.now）；Rust 只做 mdns browse 生命周期 + 快照持久化命令（get/save-device-snapshot，storage 键 `device_snapshot`）+ endpoint memo + nodeId→session 句柄映射。快照恢复条目标注「最近可见」（recent），手动 refresh 触发清算；found 刷新即摘标记。
- **传输任务/历史 store 放 Rust WASM**（transfer_store.rs 纯函数 cargo 直测 ×10：merge 快照按 batchId upsert、终态不被旧快照复活、interrupted 允许被引擎快照覆盖回真实态、200 封顶最旧先出、retryMeta 回放）；桌面落 plugin_db 表 transfer_entries，移动落 storage 单键数组。时间戳全部取自引擎事件载荷 *Ms 字段。
- **拉取 retryMeta 挂载**：pull-files 入队时压入 PENDING_PULLS 队列（封顶 8），新接收条目首现且文件集匹配时消费挂载（引擎逐文件铸造 batchId，调用点拿不到 id）。
- **接收策略 auto 分支在 Rust on_message 自动 respond-transfer**（storage 读设置同步可用）；ask 弹窗编排留前端不动；设置真源 = storage 键 `transfer_settings`（SettingsPanel wire 形状），set-settings 写 storage + 推 set-receive-policy/set-download-dir；get-settings 空 storage 时惰性迁移自引擎读接口。
- **共享根注册表 id = 路径/URI 的 FNV-1a hex**（免随机源、同根天然去重）；变更后全量推 set-shared-roots 失败回滚；移动端 builtin local-downloads 不进注册表，get-settings 以只读形状（builtin:true + treeUri）合并展示。
- **引擎改动（步骤 5 零 ABI）**：host_impl/peer.rs send 载荷双形态解析（string | {path, encrypt}，任一 true → 批量强制加密 override）→ peer_transfer 新签名 send_files_to_peer_with_policy；SDK HostPeer::peer_send_files 改 &[Value] 直通。
- **翻译层收敛**：两端 peer.rs 旧 DTO 搬运函数（transfer_to_task/terminal_history/list_* 族等约 60% 体量）删除；新 wire = 引擎 PeerTransferDto camelCase + 插件扩展字段（retryMeta/interrupted/recent）；devMock 重造为 wire 形状种子（设备 = mdns:found 载荷形状 deviceSeeds）；双端 dev-shell mock 同步新契约（get/save-device-snapshot / dial-peer{endpoint} / mdns-found 推送）。
- manifest：双端 permissions 补 `mdns`；commands 清单移除 query-peer/list-peers、新增 get/save-device-snapshot。
- **偏离记录**：①pull destRelPath 缓行——需要动引擎接收管线落盘计算且当前零消费方（落点覆盖已由 A1 保留的 set-download-dir 承载），待首个真实场景立票；②重试回放中 send 条目原地换 batchId（一条历史），pull 条目重新入队产生新批、原失败记录保留为历史（引擎 pull 批无 retry-transfer 语义，与旧行为一致）；③TTL 清算对快照恢复条目在手动 refresh 时执行（activate 首屏先渲染，spec 故事 2 与步骤 1 清扫语义的折衷）。

验证：插件 Rust 双端 cargo test 各 20 绿（纯函数直测）；前端 vitest 桌面 515+/移动 297 全绿（含重写的 usePeerDevices/useTasks/useReceiving 编排测试）；vue-tsc 双端干净；宿主 cargo test --lib 桌面 550 / 移动 299 全绿；packages/peer-net 85 绿；双端插件产物（dist + wasm 组件）已重建并同步 resources。

遗留：真机清单（发现→连接→互发→断点续传→历史/设置核对 + 插件停用后本机仍可被发现）待 Phase 4 收口后统一执行。

### Phase 4 — 旧接口退役 ✅（2026-08-26 实施，待真机验证）

**步骤 7：WIT 旧函数删除 + 句柄表升级**

- WIT host-peer 收缩为终态原语：**桌面 13 个**（dial-peer/close/respond-consent/list-trusted/revoke-trusted/send-files/respond-transfer/set-receive-policy/set-shared-roots/list-shared-roots/browse-directory/pull-files/set-download-dir）、**移动 12 个**（移动端无 set-download-dir，落点固定 MediaStore.Downloads——双端原语面差异由 A1 落点配置语义决定，文档注明）
- 删除：list-devices / dial-peer(node-id) / disconnect-peer / cancel-transfer / retry-transfer / clear-transfer-history / list-receiving / cancel-receiving / clear-receiving-history / get-receive-settings / list-shared-directories / add-shared-directory / remove-shared-directory / pick-files / pick-folder / set-transfer-encryption
- `dial-peer-endpoint` 更名 `dial-peer`（endpoint 语义转正）；四个数据面函数（send/list-shared-roots/browse/pull）收紧为仅 session 句柄寻址
- 句柄表升级：`handle → {node_id, addr, port}`（拨号时记忆 endpoint）；新增 `with_auto_redial` 包装——数据面失败且命中「发现缓存缺失」字样时以记忆 endpoint 重走引擎握手后重试一次（信任检查照走引擎握手），是退役 DiscoveryCache 的前置
- `send-files` 返回值收窄为传输句柄字符串（batch-id）；插件 enqueue/retry 先入店最小条目占位 + retryMeta，引擎快照事件到达后按 batchId 合并补全明细
- ABI bump：desktop 9→10 / mobile 7→8；双端 WIT 副本同步；SDK trait 收窄；component-test 版本字面量同步（10/8）

**步骤 8：宿主侧链路退役**

- `peer:devices` topic 与 `peer-devices-changed` 事件映射从 `bus_topic_for` 删除
- `drive_discovery_push` 守护（spawn_peer_mdns_daemon 内的 browse+快照比对推送）退役：删除 spawn + 函数体 + DISCOVERY_PUSH_INTERVAL/FORCE_REPUSH 常量
- 引擎侧新增 `spawn_peer_mdns_advertiser`（advertise-only：注册/注销广播与 TLS listener 同生命周期，不做浏览）；双端 start_locked 切换调用、runtime 字段类型换 `DiscoveryAdvertiser`；`DiscoveryCache` 本体保留为引擎内部簿记（dial_peer_endpoint 的 observe 桥接仍在，数据面读缓存用——spec 允许「保留为引擎内部结构」）
- Tauri 命令面去留：`start/stop_peer_node` + `respond_peer_consent` + `list/revoke_trusted` + `set_receive_policy`/`set_transfer_encryption`（双端）保留（生命周期 + 首连确认 + 信任管理宿主级兜底）；其余查询/管理命令从 invoke_handler 注销（函数体暂留一版：部分仍为 host_impl 内部簿记调用如 cancel_peer_transfer/cancel_peer_receiving，下版本删除）
- 引擎历史持久化停写（裁决 B）：`persist_history` 改为 no-op 桩，`ensure_history_loaded` 保留一版只读兼容回滚；插件历史成为唯一产品历史
- 移动端 MulticastLock：浏览随 advertiser 退役后不再常开（Phase 2 申请随 Phase 4 释放）

**步骤 9：数据迁移与兼容**

- 双端新增 `peer_migration` 模块（setup 阶段一次性、幂等）：引擎 `transfer_settings.json` → 插件 storage 键 `transfer_settings`（policy 词表 always_*→UI 词表映射、超时钳制）、`shared_dirs.json` → 插件 storage 键 `shared_roots`（id 以 FNV-1a 内容哈希重算与插件算法对齐）；键已存在即跳过（版本戳防重复导入）；插件历史不迁移
- 桌面迁移用 PluginStorage（SQLite 句柄）、移动用文件型 PluginStorage（app_data_dir）

验证：插件 cargo test 双端各 20 绿；前端 vitest 桌面 515+/移动 297 绿（worker OOM 偶发属环境 flakiness）；vue-tsc 双端干净；宿主 cargo --lib 桌面 550/移动 299 绿；peer-net 90 绿；双端插件产物（dist + wasm 组件）已重建并同步 resources；component-test wasm 重建后 ABI 协商测试绿。

遗留：真机清单（双端交叉：桌面↔桌面、桌面↔手机、手机↔手机 各一轮全链：发现→连接→互发→断点续传→历史/设置核对 + 旧版升级数据迁移验证 + 插件停用后本机仍可被发现）待执行；ABI 跨版本兼容（旧插件 wasm 遇 v10/v8 宿主被拒——需双端同版发布，发布线遵守 Git Rules）。

### Phase 4 后续清理（下版本）
- 引擎侧 retired 命令函数体删除（cancel_peer_transfer 等若已无内部调用）
- peer_migration 模块删除（迁移窗口过后）
- DiscoveryCache observe 桥接评估退役（需数据面全面改传 StaticPeerRecord，属更深引擎重构）

Phase 4 收紧要点备忘：① 删旧 dial-peer/disconnect-peer/cancel-*/add-remove-shared-directory/pick-*/settings 类函数；② 四个双态寻址函数删除 node-id 直呼分支 + dial_peer_endpoint 改名 dial-peer；③ 数据面缓存观察桥接（dial_peer_endpoint 内 cache.observe 回退记录）随数据面全面句柄化一并退役；④ DiscoveryCache 守护与 peer:devices 快照链路退役；⑤ 移动端 MulticastLock 引用计数释放。

依据：`docs/adr/0022-plugin-host-interface-primitive-boundary.md` **v2（2026-08-26 二次收紧版）**。边界裁决、最终函数清单与理由都在 ADR，本票只做可执行拆解，不重复论证。

## Problem Statement

issue 12 切换时把对等网络四个模块的 Tauri 命令面 1:1 全量投影成 `host-peer` 的 27 个 WIT 函数，业务编排（设备列表 / 任务 / 历史 / 共享目录注册表 / 接收策略 / 设置 / 重试）泄漏进 ABI 层：每次业务迭代同步五处（WIT → 双端 SDK → 双端 host_impl → 插件翻译层 → devMock/fixtures）、DTO 翻译两遍、第二个对等网络消费者无法进入。

## Goal

- `host-peer` 收缩为 **11 个无业务语义的引擎原语**，对端寻址全面句柄化：
  `dial-peer(endpoint)` / `close(handle)` / `respond-consent` / `respond-transfer` / `list-trusted` / `revoke-trusted` / `set-shared-roots(dirs)` / `list-shared-roots(session)` / `browse-directory(session, …)` / `send-files(session, …)` / `pull-files(session, …)`
- 新增 `host-mdns`（browse-only 纯能力）与 `host-platform`（收编 pick-files / pick-folder）
- file-transfer 插件自建：设备缓存、共享目录注册表、任务队列与历史、接收策略弹窗编排、设置面、重试编排
- 安全语义不下沉：首连闸门与接收闸门的 fail-safe 默认（无应答即拒）留在宿主

## 前置条件 / Blocked

- Blocked by：双端 `bedcode.wit` 副本在途改动合入
- ABI bump + 双端 SDK 绑定重生成 + 两端同版发布
- 口径依赖：`.scratch/peer-network/spec.md` 决策 7 后半（宿主发现缓存归属）与 `.scratch/peer-ui-plugin-migration/spec.md` 决策 1（WIT 不新增不修改）自本票起由 ADR 0022 v2 修正，两份 spec 已加注

## Implementation Steps（阶段化，每步独立可验证）

### Phase 0 — 过渡拆分（可选，默认跳过）

单一 host-peer 拆 discovery/connection/trust/data 四个 interface。v2 最终收敛回单 interface 11 函数，本阶段只剩权限声明粒度价值，直接进 Phase 1 时跳过。

### Phase 1 — host-peer v2 契约落地（新函数与旧函数并存）

1. WIT 增补终态签名（11 函数）：
   - `dial-peer(endpoint-json) -> session-handle`：endpoint = `{ nodeId, addr, port }`（camelCase JSON），插件从自身设备缓存解析后显式传入；握手期「证书指纹 ↔ nodeId 绑定 + 信任检查」语义不变，只换寻址来源
   - `close(handle) -> bool`：统一关闭会话句柄（= 断开连接）与传输句柄（= 取消传输）；关闭 pending 接收批即拒绝——闸门 fail-safe 的自然结果，非独立业务函数
   - `respond-transfer(batch-id, accept)`：接收闸门应答原语，保留在宿主（v1 曾误判下沉，ADR v2 已纠正）
   - 读对端 / 写对端四函数改按 session 句柄寻址；`pull-files` 增加 resume 语义或配套 `query-written-offset`（断点真源在接收端落盘侧）
   - `set-shared-roots(dirs-json)`：全量幂等替换引擎广播源；条目 id/name/path 由插件注册表生成持有
   - 信任存储三函数不变
2. 宿主 host_impl 建句柄路由表（session 表 + transfer 表）；Tauri 命令面本身不动
3. 旧函数中不在终态清单者并行保留至 Phase 4，插件切换期内不破坏现状；ABI bump、双端 WIT 副本同步、SDK 绑定重生成
4. 验证：peer-net 双节点 harness 扩展（endpoint 拨号、close 双语义、set-shared-roots 幂等替换、resume 偏移续传）、双端 cargo 编译通过、plugin-test 连通性覆盖新原语

### Phase 2 — 新增 host-mdns 与 host-platform

1. `host-mdns`：`browse(service-type) -> browser-id` / `stop-browse(browser-id)`；bus topic `mdns:found` / `mdns:lost` payload 原样透传（instance-name / addresses / port / txt-records），宿主不做任何加工
   - 双端 host_mdns_impl 各自包装本端 mDNS 浏览引擎，浏览从「守护常开」改为「按 browser 句柄生命周期」；Android MulticastLock 按 browser 引用计数、网卡变化重绑定、插件停用/崩溃时宿主回收句柄
   - **自我广播（advertise）不动**：留在 peer-net 启动流程自动完成，无插件激活时主机照样可被发现
   - `peer:devices` topic 与宿主全量快照指纹比对链路标记废弃（Phase 4 删除）
2. `host-platform`：迁入 `pick-files` / `pick-folder`（host_impl 实现平移换 interface 归属）；移动端 SAF 目录树选择器语义随迁
3. 验证：plugin-test 覆盖 browse 句柄生命周期、found/lost 事件透传、pick 双端行为

### Phase 3 — file-transfer 业务自持（双端）

1. 设备缓存自建：订阅 `mdns:found`/`mdns:lost` → 按 TXT node-id 去重 + TTL 过期 + 展示名/能力位解读；activate 即 browse；last-seen 持久化缓解首屏空窗（替代宿主常热列表）
2. 共享目录注册表迁 `host-plugin-database`；变更后调 `set-shared-roots` 全量推送；添加目录改走 `host-platform.pick-folder`
3. 任务队列与历史落 plugin-database，bus `peer:transfer`/`peer:receive` 驱动状态机；重试 = 批元数据持久化后重调 send/pull（带 resume 偏移）；取消与断开统一走 `close(handle)`
4. 接收策略自持：ask / always_accept / always_deny + timeout 存 storage；ask 弹窗编排在插件（复用现有常驻事件订阅单例模式），逐批应答走 `respond-transfer`；超时拒由宿主闸门兜底
5. 设置面（download-dir / encryption 开关）迁 storage，经 send/pull 参数传入引擎
6. 翻译层收敛：删除仅服务旧命令面的 DTO 搬运代码；fixtures / devMock 按 wire 形状重造
7. 验证：双端 vitest（设备缓存状态机、接收策略编排、重试元数据回放）、vue-tsc 干净

### Phase 4 — 旧接口退役

1. WIT 删除终态清单外的全部旧函数及 host_impl 对应实现；删除 `peer:devices` 快照链路与宿主 DiscoveryCache 守护（mDNS 广播/浏览引擎本体保留，供 host-mdns 复用）
2. Tauri 命令面去留单独评估（主前端若仍直调则保留）
3. ABI 再 bump；双端回归：发现 → 连接 → 首连确认 → 互发文件 → 断点续传 → 历史/设置全链真机验证，附旧 storage 键兼容检查

## Out of Scope

- peer-net 引擎本体零改动（Phase 1 句柄路由除外）；TLS / 信任语义不变
- 自我广播机制、终端控制链路、其他 host-* 接口
- 主前端（非插件）的对等网络 UI 去留

## Testing Decisions

沿用父 spec 三条既定接缝，不新增：

1. **双节点对打 harness**：扩展 endpoint 拨号、close 双语义（会话/传输）、set-shared-roots 幂等、pull resume 偏移四个新原语的引擎级行为
2. **无头纯函数单测**：设备去重/TTL 清扫、批元数据回放等纯逻辑独立直测
3. **插件 vitest**：设备缓存事件驱动状态机、接收策略弹窗编排、任务队列迁移后的编排逻辑

真机清单沿用 issue 05–07 验收项 + 历史/设置迁移数据兼容检查。

## Prior art

- ADR 0022（v2，本决策）
- `.scratch/peer-network/spec.md` 决策 11/12（peer-net 归属与迁移节奏——本票是 issue 12 的粒度修正而非方向变更）
- `.scratch/peer-network/issues/12-cutover-and-remove-legacy.md`（切换收口先例）
