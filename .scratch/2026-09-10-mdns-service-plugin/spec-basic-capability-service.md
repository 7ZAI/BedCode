# mDNS 基础能力服务（Basic Capability Service）设计规格 v2

Status: implemented（2026-09-15 双端落地并验收；D5 ADR 0022 v4 已补记）
Date: 2026-09-15
取代: 同目录 `spec.md`（v1「完全插件化」方案——mDNS 不再作为独立 WASM 插件，方案作废，本规格为现行设计）

决策来源: 架构探索会话（2026-09-15）。裁剪线依据 `docs/adr/0022-plugin-host-interface-primitive-boundary.md`，wasmtime 锁版 `docs/adr/0019-wasmtime-version-locked-across-ends.md`，插件互调 `docs/adr/0017-plugin-inter-plugin-call-gate.md`。术语以 `CONTEXT.md` 为准。

---

## 1. 需求变更（相对 v1）

v1（2026-09-10 spec.md）主张「mDNS 完全插件化」：新增内置服务插件 `com.bedcode.mdns`，由插件维护设备列表、经互调对外提供 `mdns.*` API。**现需求否决该形态**：

> mDNS 不再单独作为一个 wasm 插件，而是像 HTTP / 数据库服务一样，通过封装成为一个**基础能力服务**，对插件提供接口调用。要求：① 隔离不同业务之间的调用；② 保留一定的扩展性。

即 mDNS 走 **host-http / host-database 同款形态**——引擎与能力封装留在内核（WIT 原语接口 + 宿主实现），业务编排留插件层。区别于 v1 的核心：设备列表等派生视图仍由消费插件自建，宿主只提供**隔离的、可扩展的**原语通道。

---

## 2. 现状盘点（wasm-core 分支 2026-09-15 实测）

### 2.1 对等网络发现 `_bedcode-peer._tcp.local.`（本期改造对象）

| 能力 | 位置 | 现状 |
| --- | --- | --- |
| 节点广播 | `peer_net.rs:1123` → `peer_net::spawn_peer_mdns_daemon`（packages/peer-net/src/discovery.rs:628 起） | 宿主启动自动广播，TXT 固定拼装 `id/name/ver/cap`，cap 写死 `CAP_FILE_TRANSFER`；带周期 re-announce 续期 |
| 引擎浏览 + DiscoveryCache | 同上 daemon 内 | 浏览回灌 cache（拨号前校验 `not in discovery cache`），事件经 400ms 轮询桥接 `forward_discovery_event` → 插件总线全局 `mdns:found` / `mdns:lost` |
| 插件浏览原语 | `host_impl/mdns.rs`（desktop 205 行 / mobile 327 行） | `host-mdns` WIT 仅 `browse` / `stop-browse`；**per-browser 各建独立 daemon**；事件全局广播，payload 无属主/类型字段；目前**无任何插件实际调用**（file-transfer 依赖引擎守护浏览） |
| 消费插件 | file-transfer（device_bridge.rs + lib.rs:59-93） | 订阅全局 `mdns:found`/`mdns:lost` → 前端 deviceState.ts 自建设备缓存（去重/TTL/展示名/能力位） |

### 2.2 已知病灶（驱动本设计的动因）

1. **双 daemon 并存**：peer-net daemon + host-mdns 每浏览器独立 daemon，同绑 5353 互抢多播包（真机实证「只发现自己、发现不了对端」，2026 修复过一回，属结构性风险）。
2. **事件无业务隔离**：`mdns:found`/`mdns:lost` 全局广播，A 插件 browse 的结果 B 插件全量收到；payload 无 browser-id / service-type / owner，消费方无法区分。
3. **advertise 锁死**：服务类型固定、TXT 固定、能力位写死，插件无自定义广播通道（无 advertise 原语）；能力通告与插件启停脱节。
4. **扩展性差**：WIT 接口只有 2 个函数，无 advertise / 状态查询 / 事件结构化演进空间。

### 2.3 非改造范围（红线，沿用 v1）

- **终端链路 mDNS** `_bedcode._tcp.local.`（`mdns/advertiser.rs`、移动端 `mdns/discovery.rs` + `commands/mdns.rs`）：服务「远程终端配对」，供移动端发现桌面主机，另一能力域，**本期不动**。
- HTTP / 数据库 / WebSocket 边界（v1 §Implementation Decisions 已文档化）不变。

---

## 3. 设计目标

1. **内核形态**：mDNS 封装为内核内建基础能力服务 `MdnsService`（单例），经 WIT `host-mdns` 原语接口对插件提供调用——与 `host-http`（内核 reqwest 连接池 + fetch 原语）、`host-database`（内核 SQLite + execute/query 原语）同构。
2. **业务隔离**：不同插件（业务）的 browse / advertise 生命周期、事件投递、句柄操作完全互不干扰；权限与属主仲裁在宿主。
3. **可扩展**：服务类型任意化、TXT/能力位由插件自写、事件 payload 结构化增量演进、原语函数可增量追加（ABI bump 一次后保持稳定）。
4. **单守护收敛**：消灭双 daemon 病灶——全局唯一 `ServiceDaemon`，peer-net 与插件浏览共享同一实例。
5. **零业务代码红线（D3 定案）**：`MdnsService` 只做引擎原语（共享 daemon、句柄登记、re-announce 续期、按属主回收、事件定向投递），**不拼装、不解读任何业务字段**——服务类型、实例名、端口、TXT 键值一律由调用方（peer-net 引擎 / 插件）构造后传入，宿主零业务语义。这与 AGENTS.md §5「无业务内核」红线一致。

---

## 4. 总体设计

### 4.1 组件：`MdnsService`（内核内建，非插件）

新模块（desktop `src-tauri/src/mdns/service.rs`；mobile 对应 `src/mdns/service.rs` 或并入 host_impl）：

```
                     ┌──────────────────────────────────────────┐
                     │           MdnsService（内核单例）           │
                     │                                          │
  插件 A  ──browse──▶ │  BROWSERS 表（browser_id → {daemon-clone,  │
  插件 B  ──browse──▶ │    service_type, task, owner}）            │
  插件 A  ─advertise─▶│  ADVERTISERS 表（adv_id → {daemon-clone,   │
                     │    service_type, instance, reannounce,     │
                     │    owner}）                                 │
                     │          │                                 │
                     │  全局唯一 ServiceDaemon（LazyLock 单例）     │
                     │  （init 时 disable_virtual_interfaces）     │
                     │          │                                 │
 peer-net 节点身份 ──▶│  共享同一 daemon（register + browse + cache）│
   （owner=host）     └──────────────────────────────────────────┘
```

要点：
- **单 daemon**：`static DAEMON: LazyLock<ServiceDaemon>`（mdns-sd 设计即为单守护多服务共享，browse/register 各自独立订阅，互不干扰）；init 时执行 `disable_virtual_interfaces`（桌面；移动端无此逻辑，Android 多播锁随守护常驻获取——落地 v1 注释「主动释放暂不做，随守护常开退役一并落地」）。
- **双句柄表**：BROWSERS（沿用现有结构）+ 新增 ADVERTISERS，条目均带 `owner: plugin_id`。
- **事件定向投递**：browse 事件不再广播全局 topic，按属主发 `mdns:found.<owner>` / `mdns:lost.<owner>`（详见 §5.2）。
- **生命周期回收**：`purge_for_plugin(plugin_id)` 同时回收双表（沿用现有 `purge_browsers_for_plugin` 模式，补 advertise 回收）。

### 4.2 WIT `host-mdns` v2（双端同步，ABI bump 一次）

```wit
/// mDNS 浏览纯能力（v2）：既有 browse/stop-browse 签名不变；
/// 事件投递语义升级为「按属主定向」（见宿主实现），payload 增结构化字段。
interface host-mdns {
    /// 浏览某服务类型，返回 browser 句柄；重复浏览同一类型允许（各自独立生命周期）。
    /// 事件定向投递到 `mdns:found.<plugin-id>` / `mdns:lost.<plugin-id>`。
    browse: func(service-type: string) -> result<string, string>;
    /// 停止浏览并回收句柄（返回是否存在该句柄）；仅属主可停（跨插件拒绝）；
    /// 插件停用时宿主自动回收其全部句柄。
    stop-browse: func(browser-id: string) -> result<bool, string>;

    /// 广播某服务类型（v2 新增）：返回 advertise 句柄。
    /// config-json 为纯引擎参数（服务类型 / 实例名 / 端口 / TXT 键值），
    /// 宿主零业务拼装；实例名省略时宿主按 `{plugin}-{短指纹}` 默认。
    advertise: func(config-json: string) -> result<string, string>;
    /// 停止广播并回收句柄（返回是否存在该句柄）；仅属主可停；
    /// 插件停用时宿主自动回收其全部广播句柄。
    stop-advertise: func(advertise-id: string) -> result<bool, string>;
    /// 查询广播状态（返回是否存在该句柄）；仅属主可查。
    is-advertising: func(advertise-id: string) -> result<bool, string>;
}
```

advertise config-json 形状（宿主只校验「服务类型非空」，其余原样透传）：

```json
{
  "serviceType": "_bedcode-peer._tcp.local.",
  "instanceName": "bedcode-3f2a1b",        // 可选；缺省宿主默认
  "port": 19000,
  "txtRecords": { "id": "...", "name": "...", "ver": "1", "cap": "3" }
}
```

ABI bump：desktop `ABI_VERSION` 12→13，mobile 10→11（ADR 0019 双端同步；wasmtime 47 不动，仅 wit-bindgen 重新生成）。

### 4.3 peer-net 收敛（单守护）

- `spawn_peer_mdns_daemon`（peer-net crate）**不再自 new daemon**：函数签名增加 daemon 参数（`ServiceDaemon` 为 Arc 共享，Clone 传入），内部 register / browse / cache 逻辑不动（engine 内部 `handle_browse_event` 自持 DiscoveryCache：`cache.observe` / `remove_by_fingerprint` 不依赖任何外部通道，实测确认）。
- 宿主侧接线：peer-net 启动时从 `MdnsService` 取全局 daemon 传入；节点身份广播在内部语义上视为 **owner=host** 的 advertise——TXT/ServiceInfo 由 peer-net 引擎构造（节点身份/证书/能力位是引擎语义，D3 定案），`MdnsService` 只负责注册到共享 daemon + 句柄登记，零业务拼装。
- **面向插件的全局发现桥接整体退役（D1/D2 定案）**：退役 `events_tx → forward_discovery_event → publish_mdns_bus`（全局 `mdns:found`/`mdns:lost` 桥接）与 `spawn_discovery_refresh_subscriber`（cache 重发通道）两条链路——file-transfer 为唯一消费方且一期同迁（D2），无遗留订阅者，**无需迁移期垫片**；DiscoveryCache 仍由引擎内部维护（拨号寻址校验用，行为不变）。
- 兜底语义不变：宿主节点身份广播随 peer-net 启动自动注册，无插件激活时主机照样可被发现（等价 v1「内置插件默认启用」的兜底）。

---

## 5. 业务隔离设计（需求①）

隔离目标：**A 插件的 mDNS 调用对 B 插件零可见、零影响；任何插件只能操作自己的句柄、只能收到自己 browse 的事件。**

### 5.1 句柄属主隔离（操作面）

- BROWSERS / ADVERTISERS 双表条目带 `owner: plugin_id`（BROWSERS 已有）。
- `stop-browse` / `stop-advertise` / `is-advertising` 先权限门（`PERMISSION_MDNS`），再**属主校验**：`plugin_id != entry.owner` → `Err("not owner of mdns handle")`，杜绝跨插件停他人句柄。
- 插件停用/卸载：`purge_for_plugin` 回收该插件全部 browse + advertise 句柄（**仅本人**），其余插件句柄不受影响。
- 实例名冲突（同服务类型下两插件同实例名）：mdns-sd 引擎返回冲突事件，宿主**定向**通知属主插件（`mdns:conflict.<owner>`），属主自行决定改名/放弃——隔离 + 扩展兼得。

### 5.2 事件定向投递（数据面）

- browse 事件改为按属主定向 topic：`mdns:found.<owner>` / `mdns:lost.<owner>`（owner = 发起 browse 的 plugin_id）。
- 消息总线按精确 topic 分发（bus.rs 现有机制），非属主插件**订阅不到** → 物理隔离（A 的发现结果不进入 B 的队列，不占用 B 的背压额度）。
- payload 结构化增量：`{ instanceName, addresses, port, txtRecords, serviceType, browserId }`（前 4 字段与现状一致；后 2 为新增，老订阅者按「忽略未知字段」增量原则兼容）。
- 宿主身份 browse（owner=host，peer-net cache 回灌用）：事件走宿主内部消费，不进插件总线（或进 `mdns:found.host` 供宿主静态订阅，实现时二选一）。

### 5.3 权限与背压

- 权限门：`PERMISSION_MDNS` 保留（前端 manifest 声明 + Rust 端仲裁，现状不变）。
- 背压隔离：总线每订阅者有界队列独立（bus.rs 现成机制），一插件慢消费只阻塞自己，不阻塞发布方与其他插件——继承现状。

---

## 6. 扩展性设计（需求②）

1. **服务类型任意化**：browse / advertise 的 service-type 为字符串参数，插件可广播/订阅任意类型（`_bedcode-peer._tcp.local.`、自定义能力类型等）——原语层无白名单。
2. **TXT / 能力位插件自写**：advertise config 的 `txtRecords` 由插件传入，宿主零拼装。能力通告随上层插件启停变化（如 file-transfer 停用 → 其 cap 位不再写入），编排权完全在插件——宿主只提供「广播通道」，不做业务解读。
3. **事件结构化演进**：payload 字段增量追加（serde 默认值 + 老端忽略未知字段），后续可加 `ttl`、`instance-conflict` 事件等。
4. **原语函数增量**：WIT 一次 bump 后保持稳定；未来「设备列表快照/增量（devices-changed）」「按类型通配订阅」「TXT 自动汇总」等均为**增量追加**，不破坏既有函数。
5. **多句柄并存**：同一插件可并发多个 browse（多服务类型）+ 多个 advertise（多能力），各自独立生命周期（现状 browse 已允许）。

---

## 7. 消费插件迁移（file-transfer，D2 定案：一期同步迁移、无垫片）

- **改造前**：file-transfer（**双端同构**，desktop/mobile lib.rs 同 59-60/86-87/215-218 模式）订阅全局 `mdns:found`/`mdns:lost`，依赖宿主引擎守护浏览（宿主替它发现）。
- **改造后**：双端 file-transfer 同步迁移——`activate` 时自调 `browse("_bedcode-peer._tcp.local.")` 拿到 browser 句柄，订阅 `mdns:found.<file-transfer>` / `mdns:lost.<file-transfer>`；设备列表派生视图（去重/TTL/展示名/能力位）继续留在前端 deviceState.ts（宿主零业务语义，隔离在基础设施层）；`deactivate` 时 stop-browse → 宿主 purge 兜底。
- 权限不变（manifest 保留 `mdns`），无需互调。
- **无迁移期垫片**：file-transfer 是全局 `mdns:found`/`mdns:lost` 的唯一订阅者（双端全仓 grep 确认，其余命中均为宿主桥接/注释/devMock），一期同迁后全局桥接与 refresh 重发通道直接退役（§4.3），不存在遗留订阅者需要垫片。迁移期间全局 topic 不再发布新事件，仅保留旧代码的停用回滚路径（git 可逆）。

---

## 8. 双端契约同步

- WIT 变更按 ADR 0022「五处同步税」同步：WIT → 双端 SDK 绑定 → 双端 host_impl → 插件翻译层；ABI bump（desktop 12→13 / mobile 10→11）。
- 移动端本期实现完整 host_impl（advertise / stop-advertise / is-advertising + 定向投递 + 属主校验 + purge 双表），与桌面同构（移动端 peer-net 同样收敛到共享 daemon，`peer_net.rs:1039`）。
- Android 多播锁：随 MdnsService 单守护常驻获取（fire-and-forget 幂等，沿用现状模式），不再随浏览句柄增删。

---

## 9. 测试策略

- **单测（宿主层）**：
  - 属主校验：B 停 A 的 browser/advertise 句柄 → Err；B 查 A 的 is-advertising → Err。
  - `purge_for_plugin` 双表回收且不误伤他人句柄。
  - 未知句柄幂等（`stop_browser` 已测，补 advertise 同款）。
  - advertise 状态翻转（advertise → is-advertising true → stop → false）。
  - config 解析：服务类型空拒绝；txtRecords 原样透传（含中文值）。
- **集成（行为等价回归）**：
  - 双插件 browse 隔离：A、B 各 browse 同类型，A 的事件 topic 不出现 B 的实例集合之外的消息（按 topic 断言）。
  - host advertise + 插件 advertise 共存（同 daemon）。
  - file-transfer 迁移链路：activate 后自 browse 拿设备列表，mDNS 引擎未就绪/停用时降级（空列表 + 提示）。
  - peer-net 集成测试全绿（发现 / 在线判定 / 能力通告行为等价，`packages/peer-net/tests/*`）。
- **不做**：mdns-sd 引擎自身行为（crate 保证）、终端链路 `_bedcode._tcp.local.` 任何改动。
- 完成验证（AGENTS.md Done When）：改 Rust → `cargo test` 双端通过；改前端 → `pnpm run test:run`；eslint 0 error；i18n key 双端同步（如有 UI 文案）。

---

## 10. 非目标与红线

- 终端链路 `_bedcode._tcp.local.`（远程终端配对）**不动**（v1 红线延续）。
- 设备列表派生视图仍归消费插件，宿主**不**代做业务（隔离在基础设施层，非宿主业务化）。
- 不做独立 WASM 服务插件（v1 形态作废）；无互调 API（ADR 0017 不涉及本设计）。
- 设备发现 UI 不改数据源之外的视觉。
- wasmtime 47 不动，仅 wit-bindgen 重生成。

---

## 11. 决策记录（2026-09-15 架构会话全部定案）

| # | 决策点 | 定案 | 理由 |
| --- | --- | --- | --- |
| D1 | peer-net 事件桥接退役路径 | 退役 `events_tx → forward_discovery_event → publish_mdns_bus` 桥接 + `spawn_discovery_refresh_subscriber` 重发通道；DiscoveryCache 由引擎内部自持（现状即如此，`handle_browse_event` 内 `cache.observe` 不依赖外部通道，实测确认） | cache 回灌是引擎内部行为，桥接只服务于插件（唯一消费者 file-transfer 一期同迁），整体退役最干净，peer-net crate 仅 daemon 来源变化 |
| D2 | file-transfer 迁移时机 | 双端一期同步迁移，**无迁移垫片**（唯一订阅者同迁，全局 topic 直接退役） | 消费方唯一（全仓 grep 实证），垫片无存在价值；迁移期全局 topic 停止发布，git 可逆 |
| D3 | host advertise 归属 | peer-net 引擎构造 ServiceInfo/TXT（节点身份/能力位），`MdnsService` 零业务代码只做注册 + 句柄登记（owner=host） | 用户原则：mdns 基础服务**不包含任何业务性代码**；节点身份/证书是对等连接引擎语义，留在 peer-net |
| D4 | advertise 默认实例名 | `{plugin}-{短指纹}`，显式优先 | 缺省友好，显式传参优先 |
| D5 | ADR 产出 | 实施验收后补记 ADR（修订 ADR 0022 相关表述），本期先以本 spec 为准 | 与用户确认 |

## 12. 实施顺序建议

1. WIT host-mdns v2 + 双端 SDK 重生成（ABI bump：desktop 12→13 / mobile 10→11）。
2. desktop `MdnsService`（单 daemon + 双表 + 定向投递 + advertise 原语 + 双表 purge）。
3. peer-net 共享 daemon 接线 + 全局桥接退役（D1）+ host advertise 走 MdnsService 登记（D3）。
4. mobile host_impl 同步（同构）+ mobile peer-net 接线。
5. 双端 file-transfer 迁移（自建 browse + 定向 topic，D2）。
6. 测试：宿主单测（属主校验/purge/状态翻转/透传）+ 双插件隔离集成 + peer-net 回归全绿 + 完成验证（cargo test 双端 / eslint / i18n）。
7. 验收后补记 ADR（D5）。
