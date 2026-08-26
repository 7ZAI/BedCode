# host-peer WIT 接口原语化收缩（ADR 0022 v2 实施）

Status: in-progress（Phase 1–2 已实施并验证，2026-08-26；Phase 3–4 规划已定稿见 `.scratch/peer-network/spec-plugin-self-hosting.md`，ready-for-agent）

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

### Phase 3 — file-transfer 业务自持（双端）⬜ 未开始 —— 规划见 `.scratch/peer-network/spec-plugin-self-hosting.md`
### Phase 4 — 旧接口退役 ⬜ 未开始 —— 规划见 `.scratch/peer-network/spec-plugin-self-hosting.md`

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
