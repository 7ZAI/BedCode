# mDNS 服务插件（完全插件化试点）实施规格

Status: ready-for-agent
Date: 2026-09-10
决策来源: 架构探索会话（同日）。裁剪线依据 `docs/adr/0022-plugin-host-interface-primitive-boundary.md`，插件互调 `docs/adr/0017-plugin-inter-plugin-call-gate.md`，wasmtime 锁版 `docs/adr/0019-wasmtime-version-locked-across-ends.md`。术语以 `CONTEXT.md` 为准。

---

## Problem Statement

BedCode 的插件系统已经验证了「引擎留内核、能力层插件化」这条裁剪线：HTTP 入站路由（动态代理）、HTTP 出站（宿主代发）、数据库（主库前缀隔离 + 插件独立库）均已插件化，它们的**引擎**（Actix 服务器、SQLite）正确留在内核。但这条线在两个地方还没走完，暴露了架构的不彻底：

1. **对等网络的节点发现（mDNS）仍是「半插件化」的并存态**：浏览（browse）已有 `host-mdns` 原语可供插件调用，但自我广播（advertise）仍锁在宿主启动流程里；「设备列表」这个产品概念（去重、TTL 过期、展示名、能力位解读）每个消费插件各自实现一份。ADR 0022 Phase 3–4 规划的「宿主侧 DiscoveryCache 退役」尚未完成，导致同一套发现能力同时存在「宿主 daemon + 桥接」与「插件自建 daemon」两条浏览路径。

2. **没有一个「服务插件」的先例**：现有插件（ai-chatbox / auto-task / file-transfer）都是面向用户的**功能插件**，各自独占能力。缺少一个「向其他插件提供服务、被其他插件调用」的**服务插件**形态，来验证 ADR 0017 插件互调机制作为基础设施复用的正解。

3. **WebSocket 业务层仍是内核独占**：WS 引擎（连接注册表、帧编解码、TLS/加密）必须留内核，但其上的「消息处理器」目前只能经事件总线间接参与，没有「插件声明 WS 消息处理器」的正式扩展点。

本规格选**对等网络的节点发现（mDNS）**作为「完全插件化」的第一个试点：把 mDNS 从「宿主半管 + 插件各自实现」收敛为「内核只留纯原语 + 一个 mDNS 服务插件对外提供服务」。同时把 HTTP / 数据库 / WebSocket 的「引擎 vs 能力」边界固化为文档基线，作为后续所有基础设施能力插件化的裁剪线判据。

---

## Solution

把对等网络的节点发现收敛成三层，其中「设备列表」这类业务编排只存在一份：

1. **内核原语补全**：`host-mdns` WIT 契约在既有 browse 原语旁，新增 advertise / stop-advertise / is-advertising 三个纯原语（投影 `bedcode-peer-net` 的自我广播能力）。原语无业务语义——只做「广播某服务类型 + 实例名 + TXT 记录」「停播」「查询广播状态」，不携带设备名、能力通告等产品概念。

2. **新增内置服务插件 `com.bedcode.mdns`**（内置 + 默认启用）：activate 时用原语广播本机节点身份并浏览对等网络服务类型，自建**唯一一份**设备列表派生视图；经 ADR 0017 互调 gate 对外声明 `mdns.*` API，供其他插件调用与订阅。

3. **消费插件迁移**：file-transfer 等插件不再各自调 `host-mdns` 浏览、不再自建设备缓存，改为互调 mDNS 服务插件的设备列表 API + 订阅设备变更事件。

4. **宿主侧退役**：宿主内建的发现 daemon、DiscoveryCache 与事件桥接随迁移完成而退役（ADR 0022 Phase 3–4 的收尾）。

5. **边界基线文档化**：HTTP / 数据库 / WebSocket 的「引擎留内核、能力层插件化」现状与 WebSocket 业务层的二期路径，作为裁剪线判据写入本规格的 Implementation Decisions。

对「无插件激活时主机照样可被发现」这一旧兜底，本期以「内置插件默认启用」等价替换——兜底语义从「内核启动流程」转移为「内置插件默认启用」，用户主动停用 mDNS 插件即主动选择不被发现。

---

## User Stories

1. 作为一个插件开发者，我想要一份统一的设备列表 API，而不是自己实现 mDNS 浏览、去重与 TTL 过期，从而专注于自己的业务。
2. 作为 file-transfer 插件，我想复用 mDNS 服务插件的设备列表，从而删掉自己那份 browse 逻辑与设备缓存。
3. 作为一个未来的「投送」插件，我想像 file-transfer 一样复用同一份节点发现能力，从而无需重复造发现轮子。
4. 作为桌面端用户，我想主机开机后默认可被移动端发现，从而无需任何手动操作就能远程连接。
5. 作为高级用户，我想停用 mDNS 插件来让本机在局域网内不可被发现，从而控制自己的可见性。
6. 作为一个插件开发者，我想用 host-mdns 原语广播一个自定义服务类型，从而在发现链路上承载自定义能力通告。
7. 作为移动端节点，我想发现同一网络内的桌面节点，从而发起远程终端控制或文件传输。
8. 作为一个节点，我想让「设备列表」只展示当前在线的对端，从而避免对已离线节点的无效交互。
9. 作为一个节点，我想在发现记录里看到对端的能力通告位，从而知道对方是否具备与我相同的能力。
10. 作为一个插件开发者，我想通过声明的互调 API 调用 mDNS 服务，而不是通过自由 topic 广播约定，从而获得宿主门禁与接口校验。
11. 作为一个插件的运行时使用者，我想在 mDNS 服务插件未启用时得到明确的降级提示，而不是静默拿到空设备列表。
12. 作为一个插件开发者，我想在 mDNS 服务插件升级时不必修改我的消费代码，从而享受稳定的设备列表契约。
13. 作为一个架构维护者，我想让「设备列表派生视图」在代码库里只存在一份，从而避免多处实现漂移。
14. 作为一个插件开发者，我想让广告广播随我的插件停用自动回收，从而不留下僵尸广播句柄。
15. 作为一个移动端用户，我想在被发现时看到的是设备名而非裸节点 ID，从而识别「这是哪台机器」。
16. 作为一个宿主维护者，我想在退役宿主侧 DiscoveryCache 后仍保持发现行为不变，从而验证「完全插件化」没有功能回退。
17. 作为一个插件开发者，我想通过「注册 WS 消息处理器」而非裸事件桥接参与 WebSocket 链路（二期），从而获得类型化的消息处理扩展点。

---

## Implementation Decisions

### 裁剪线（本文档的架构地基，修订 ADR 0022 一处）

- **引擎原语留内核，业务编排插件化**：mDNS 的 UDP 组播 / 网卡遍历 / Android MulticastLock / 虚拟网卡禁用等「离宿主无法实现」的脏活，作为原语留在宿主；「广播什么 / 何时广播 / 设备列表怎么维护 / 能力通告怎么解读」全部进插件。HTTP、数据库遵循同一裁剪线（见下）。
- **修订 ADR 0022 v2 的「自我广播不进插件 ABI」**：advertise 以**纯原语**形式进 `host-mdns` ABI（与 browse 同构），但广播内容与生命周期的编排在服务插件。理由：不把 advertise 交给插件，「完全插件化」无法成立——插件无法在 activate 时广播自己。原决策的顾虑（「无插件激活时主机也可被发现」）改由「内置插件默认启用」承担。
- **两套 mDNS 严格区分（实施红线）**：本期只动**对等网络的节点发现**（`bedcode-peer-net` 的服务发现，与 peer-net 的 serve 通道同生共死）。**终端链路 mDNS**（`_bedcode._tcp.local.` 广播，供移动端发现桌面主机做远程终端）是另一个能力域，服务「远程终端配对」而非「对等网络」，本期明确不动。

### host-mdns 原语补全（WIT 契约）

- 保留既有 `browse` / `stop-browse` 纯能力，语义不变。
- 新增 `advertise(config)` → 返回 advertise 句柄；`stop-advertise(handle)`；`is-advertising(handle)`。
- `advertise` 入参为纯引擎参数：服务类型、实例名、端口、TXT 记录键值对。**不得**包含设备名展示、能力通告位解读、节点 ID 语义化等业务形状——那些由服务插件拼装后传入。
- 生命周期：advertise 句柄与 browse 句柄同口径——插件停用/卸载时由宿主统一回收（对齐既有 browse 的按属主回收机制），fail-safe 由引擎保证（daemon 关停即停止广播）。

### mDNS 服务插件 `com.bedcode.mdns`

- **形态**：内置插件（随宿主分发，不可卸载只可停用）+ **默认启用**。Rust（WASM 后端）+ TS（前端）双层，与既有插件同构。
- **activate 行为**：
  - 调 `advertise` 原语广播本机节点身份：服务类型 = 对等网络发现类型，TXT 记录携带节点 ID（原始指纹，非语义化展示名）、协议版本、能力通告位。
  - 调 `browse` 原语订阅对等网络服务类型。
- **设备列表派生视图**：在插件内自建缓存，只此一份——去重、TTL 过期、展示名（设备名 + 短指纹）、能力位解读全部在此。订阅 `mdns:found` / `mdns:lost` 事件维护。
- **互调 API（manifest `api` 声明，ADR 0017 JSON-RPC 2.0）**：
  - `mdns.advertise(config)` / `mdns.stop-advertise()`
  - `mdns.browse(service-type)` / `mdns.stop-browse(service-type)`
  - `mdns.list-devices()` → 当前在线设备列表（含展示名、节点 ID、能力位）
  - `mdns.get-device(node-id)` → 单个设备
  - `mdns.on-devices-changed()` → 订阅设备变更（或经既有 bus topic 推送）
- **事件**：沿用既有 `mdns:found` / `mdns:lost` topic 语义；新增 `mdns:devices-changed` 承载「设备列表已更新」的增量/快照通知，供消费插件刷新。

### 消费插件迁移（file-transfer 首个）

- file-transfer 移除「直接调 `host-mdns` 浏览 + 自建设备缓存」，改为互调 `com.bedcode.mdns` 的 `mdns.list-devices` + 订阅 `mdns:devices-changed`。
- file-transfer 不再需要 `mdns` 权限，改为对 mDNS 服务插件的互调（互调走宿主注册表门禁，不涉及原语权限）。
- 拨号寻址不变：file-transfer 仍从设备列表解析 endpoint 后显式调用对等连接原语（对齐 ADR 0022「对端寻址显式句柄化」）。

### 宿主侧退役

- 退役宿主内建的发现 daemon、DiscoveryCache 与 `mdns:found`/`mdns:lost` 桥接链路（ADR 0022 Phase 3–4 已规划的收尾）。退役以「行为等价」为验收前提——迁移后设备发现、在线判定、能力通告展示均无回退。
- 宿主侧仅保留 `host-mdns` 纯原语 + 节点身份/证书基础设施（`bedcode-peer-net` 的节点身份与自签证书仍留在宿主，那是对等连接的引擎语义）。

### HTTP / 数据库 / WebSocket 边界基线（文档化，本期不新开发）

- **HTTP**：入站——宿主持有 Actix 服务器 + JWT 网关 + 链路加密，插件经动态路由 `/api/plugin/{plugin_id}/{path}` 声明端点处理函数（`_http_endpoint`），已是完整形态。出站——宿主代发（含私网直连/系统代理分流、SSE 流式），插件作为客户端使用。**引擎不可插件化**（TCP 监听、TLS、连接池、限流必须留内核）。
- **数据库**：宿主持有 SQLite 引擎，插件经两种模式访问——插件独立库（私有 SQLite，无表名限制）与主库前缀隔离（表名强制 `plugin_id_` 前缀 + 权限校验）。**SQLite 引擎不可插件化**（mmap/文件锁/页缓存/共享连接池必须留内核）。
- **WebSocket**：引擎（连接注册表、帧编解码、TLS/链路加密、心跳重连）**必须留内核**；其上的「消息处理器」可插件化，路径为「宿主持有连接 + 帧分发，插件按 session 或消息类型声明处理器，经 bus/events 收发」。**本期出圈，列入二期**。

### 双端契约同步

- `host-mdns` WIT 变更需**双端（desktop / mobile）同步 bump ABI 版本**（ADR 0019）。移动端即使本期不做完整服务插件，也必须同步 WIT 契约与 host_impl 投影（advertise 原语），保持双端契约同版。
- wasmtime 版本两端锁死（`47`），本次原语新增不升级 wasmtime 与 wit-bindgen 组合。

### 测试接缝（Seam）

- **主 seam = 插件互调 gate（ADR 0017）**：以「调用 mDNS 服务插件的互调 API 观察其行为」作为测试主接缝——测 `mdns.list-devices` 返回内容、advertise/browse 状态、设备变更事件，不深入 mdns-sd 引擎内部。
- **次 seam = `host-mdns` 原语**：测 advertise/browse 原语的句柄生命周期与 fail-safe（停用回收、未知句柄幂等），沿用既有 host-mdns 单测模式。
- 不做、也不需要新增第三个接缝：引擎（mdns-sd crate）自身行为由 crate 保证，不在此规格测试范围。

---

## Testing Decisions

- **只测外部行为，不测实现细节**：断言「设备列表内容」「广告广播状态」「互调调用的请求/响应形状」，不断言 daemon 句柄数量、内部缓存结构、事件循环实现。
- **模块覆盖**：
  - `host-mdns` 原语（advertise 句柄生命周期、停用回收、未知句柄幂等、自播回显过滤）。
  - mDNS 服务插件的互调 API（list-devices 去重/TTL/展示名、advertise/stop-advertise 状态翻转、devices-changed 事件）。
  - 消费插件迁移后的互调链路（file-transfer 调 `mdns.list-devices` 拿设备列表、mDNS 插件未启用时的降级行为）。
- **先例**：host-mdns 既有单测（`stop_unknown_browser_is_idempotent_false`）、peer-net 集成测试、ADR 0017 的插件互调 scheduler 测试。
- **回归门槛**：退役宿主侧 DiscoveryCache 后，对等网络集成测试须保持全绿（发现、在线判定、能力通告行为等价）。
- **编译级验证**（按 `AGENTS.md` Done When）：修改的 Rust `cargo test` 通过；前端 `pnpm run test:run` 通过；i18n key 双端同步；前端 UI 改动经 `frontend-styles` 自查。

---

## Out of Scope

- **WebSocket 业务层插件化**（「注册 WS 消息处理器」扩展点）——二期独立立项，本期仅文档化路径。
- **HTTP / 数据库引擎插件化**——引擎永远留内核，任何尝试均违背裁剪线，明确不做。
- **终端链路 mDNS 广播**（`_bedcode._tcp.local.`）的服务插件化——服务远程终端配对，是另一能力域，本期不动。
- **mDNS 之外的服务插件化**（文件服务、同步服务等）——复用本期确立的「服务插件」模式后续再做，不在本期实现。
- **移动端完整服务插件实现**——移动端本期仅同步 WIT 契约与 host_impl 投影，完整服务插件逻辑二期或随移动端需要再上。
- **设备列表 UI 的大改**——设备发现 UI 归属消费插件的既有页面，本期只改数据来源（互调），不做视觉重构。
- **自定义服务类型的运行时注册 UI**——advertise/browse 支持任意服务类型是原语能力，但面向用户的自定义服务类型管理 UI 不在本期。

---

## Further Notes

- **ABI 五处同步税**（ADR 0022）：`host-mdns` 每次改动要同步 WIT → 双端 SDK 绑定 → 双端 host_impl → 插件翻译层。本规格一次性补 advertise 原语后，该契约应保持稳定，不再随业务迭代漂移。
- **服务插件是运行时依赖**：mDNS 服务插件成为 file-transfer 等插件的运行时依赖，需在消费插件侧显式处理「mDNS 未启用」的降级（空设备列表 + 提示启用）。互调调用失败（目标未声明 / 超时）的错误语义沿用 ADR 0017 JSON-RPC。
- **内置插件的启用偏好**：mDNS 插件默认启用，但用户可停用；停用即主机不可被发现（对等网络语境），与「终端链路 mDNS」的广播无关，两者独立。
- **能力通告与插件启停的关系**：能力通告位随上层功能插件（如 file-transfer）的启停变化，不随 mDNS 服务插件变化——mDNS 服务插件只负责「发现链路」，能力位是上层插件经 mDNS 服务插件写入广播的元数据。
- 本规格是「完全插件化」的第一个试点；若验证成功，后续「投送 / 同步」等能力均按「内核原语 + 服务插件 + 消费者互调」的同一模式挂载到对等网络上。
