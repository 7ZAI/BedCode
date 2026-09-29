# 认证中心显式注册与组合式认证原语（auth center as a registered host role）

## 状态

**已实施**（2026-09-29，desktop ABI v32；实施记录见文末「实施记录」节）。
spec：`.scratch/2026-09-29-auth-center-registration/spec.md`。
用户裁定 2026-09-29：**D1 无中心 = 拒绝** · **D2 单中心** · **D3 凭据与密码学不动** · **D4 架构调整**。
本 ADR 修订 ADR 0022 的「授权策略 = 安全闸门」节与「双端偏离」节。

**配套 ADR 0032**（wasm 插件分类体系：基础服务 / 内部统一业务 / 业务应用 + worker 预留）——
本 ADR 的 K9（认证中心优先加载）依赖它。K9 曾一度建议给 `terminal-session` 加
`"type": "system"`，因分类裁定**作废**（`system`/L1 是「提供 host-\* 同形能力的基础服务」，
而 terminal-session 是消费者，语义错误）。

## 背景

2026-09-29 移动端连接事故：桌面端把每个持 JWT 的请求与 WS 连接都拒掉
（`auth-policy not provided by this plugin` → HTTP 401 / WS close 4001），
移动端自愈重连无退避，形成 616 次 / 98 秒的日志与 toast 洪水。

根因不在「宿主桥接认证中心」这一形态（那是 ADR 0017 互调 + ADR 0022 裁剪线的必然结果），
而在**桥接的接线没有注册语义**：

1. SDK 让 `wasm_entry!` **无条件导出** `auth-policy`（默认拒绝实现），任何用 SDK
   构建的应用都被能力探测列为候选；
2. 宿主取「按 id 升序的第一个候选」当中心——无锚点、无唯一性仲裁，
   `com.bedcode.agent-hub` 排序第一被选中，而只有 `com.bedcode.terminal-session`
   实现了真实策略，于是全局拒绝；
3. 同一个「认证中心」概念存在**两套发现机制**：桥接路径走 api_registry 锚点
   （`SESSION_MARKER_API`），裁决路径走能力探测 + 排序取首个；
4. 闭环测试只激活单个候选，「取第一个」恰好正确 → 多候选场景零覆盖。

## 决定

**认证中心 ≡ 微服务架构的 auth center：唯一裁决者、注册进网关、可提供多种认证方式（grant）。**
与微服务唯一的结构差异是**没有也不需要服务发现**——注册表就是宿主进程内的单表，
同进程同 ABI，不存在网络拓扑、健康检查或多副本选主问题。宿主 server 网关是那个 API Gateway：
**所有需要认证的请求都由认证中心判断**，宿主只做验签、路由与仲裁。

| # | 决定 |
| --- | --- |
| K1 | 认证中心角色经 `host-auth.auth-center-register` **显式注册**进宿主单中心注册表；宿主只做唯一性仲裁 + 句柄登记 + 停用回收 |
| K2 | 所有需认证面（HTTP `/api/*` + WS 插件端点首消息）统一裁决：`验签(引擎) → 问注册中心` |
| K3 | **无中心 = 拒绝；中心调用失败 = 拒绝**。删除现有两条 fail-open 降级 |
| K4 | **单中心**：第二个注册者被显式拒绝并点名在册属主 |
| K5 | **凭据与密码学不动**：secret-store / HS256 签发验签 / P-256 验签 / 链路身份留在宿主 |
| K6 | **组合式认证原语**：`auth-methods-list` / `auth-method-invoke`，其他插件复用中心已实现的认证方式，宿主零解析窄转发 |
| K7 | 桥接门一并切注册表，退役 api_registry 锚点（**不留第二套发现机制**） |
| K8 | 注册权限位复用已有 `auth`（`PERMISSION_AUTH`），不新增词汇位 |
| K9 | 认证中心属 **L2「内部统一业务应用」**（`ADR 0032` 分类体系），加载顺序 `L1 → L2 → L3` | 用户裁定；L2 是宿主**唯一反向依赖**的类别（宿主主动调它），三条红线见 ADR 0032 |

**K3 的安全姿态变更需要显式记录**：现状是「认证中心未激活 → 宿主策略放行」「能力调用
传输失败 → 宿主策略放行」两条 fail-open（双轨并存期的无单点设计）。本 ADR 改为 fail-closed，
与 AGENTS §8 安全闸门「fail-safe 默认如无应答/超时即拒」一致。代价：认证中心插件未激活时
本机所有需认证面不可用——这是有意的（认证面失效时放行等于无认证裸奔）。

**K4 单中心而非多中心按 method 分派**：多中心会把 method 冲突裁决推给宿主，
那是业务编排（B2），越 ADR 0022 裁剪线。认证中心语义上本就唯一。

### 为什么没有也不需要服务发现

用户裁定：**认证中心与微服务中的认证中心概念一致；本仓的差别只是没有也不需要服务发现。**

服务发现（etcd / consul）要解决的是「中心跨进程跨主机、地址会漂移、多副本要选主、
健康与可达性不确定」。这五条在本仓**全部不成立**：中心是同进程 wasm 组件，宿主经
插件 id + 实例表触达（地址是进程内指针，不是网络地址）；实例表天然单例；生命周期走
插件闸门（审批 + 停用回收）而非副本数；同进程无网络分区，「不可达」等价于实例缺失/trap。

**因此注册表本身就是服务发现**——发现协议只是「激活时调 `auth-center-register`
+ 宿主唯一性仲裁」这一个函数。中心换人（terminal-session → 独立 auth-center app）
= 换一个插件 id，不需要任何发现基础设施；这也是 D2 单中心敢这么定的前提。

代价同步登记：**没有服务发现 = 没有「中心多副本 failover」**。K3 的 fail-closed 正是
这条代价的定价——中心不可用就拒绝，而不是降级到「无认证放行」。

### 拒绝原因的分类是可观测性契约

`no_center`（中心没注册）/ `unavailable`（注册了但实例缺失、trap、超时）/ `policy`
（中心明确拒绝，如设备被撤销）三类必须以结构化字段 `deny_kind` 区分，不得合并成一句
文案：前两类是部署/故障问题，第三类是产品语义问题，排障路径完全不同。宿主域用
`frontend.*` 错误码，中心业务拒绝用中心自己的 `com.bedcode.<id>.*` 码（ADR 0030）。

## 归属裁决（ADR 0022 §5.1.2 三问）

1. **离宿主能实现吗？** 注册表、唯一性仲裁、停用回收、窄转发——只有宿主能做（网关在宿主）→ 放宿主
2. **携带产品语义吗？** `methods` 是**声明式列表**，宿主不解释每个 method 的语义；
   认证记录真源在中心私有库；宿主不做编排 → 不命中 B1–B6
3. 都不命中 → 放宿主，且满足：WIT `host-auth` 纯增量、权限位 `auth` 有门禁落点、停用可回收

合法薄壳归类：**通用注册表与寻址**（注册表）+ **安全闸门**（fail-closed、唯一性仲裁）+
**零解析窄转发**（`auth-method-invoke`）——均在 ADR 0022 §5.1.3 允许的四类之内。

## Considered Options

| 方案 | 为什么不选 |
| --- | --- |
| **A 只修排序启发式**（把 `agent-hub` 从候选里排除 / 硬编码 terminal-session） | 只换一种猜法：硬编码 id 是 v29 特意去掉的；排序启发式换个插件集合就再翻车。不解决「两套发现机制并存」 |
| **B 哨兵错误 + 逐个试**（SDK 默认实现返回可识别哨兵，宿主跳过该候选） | 启发式仍然是启发式；且需要「中心不可用」与「策略拒绝」两种语义在错误通道上可分，脆弱。本方案**作为 fail-visible ③ 的补充保留**：注册表未命中时仍可保留能力探测的 warn 留痕，但不再影响裁决 |
| **C 插件侧按需导出 `auth-policy`**（非中心插件不导出该能力） | WIT world 是静态的，SDK 宏无法按插件开关；旧产物兼容面爆炸 |
| **D 多中心 + 按 method 分派** | 冲突裁决落宿主 = 业务编排（B2） |
| **E 认证中心下沉为独立 wasm-app（从 terminal-session 拆出）** | 概念上更纯（微服务里 auth server 确实独立），但**与本仓形态无冲突**：单中心注册表天然允许未来换人（换插件 id 即可）。**裁定：暂不拆**，认证中心暂留 terminal-session 兼任 L2（ADR 0032 4b），拆分作为后续独立专项——避免一次改动同时动边界与拓扑 |

## Consequences

**正面**

- 「谁是认证中心」从**猜测**变成**注册事实**，与微服务 auth center 的心智模型一致；
- 认证面失效变成**显性失败**（403/4001 带点名原因），不再出现「静默放行」；
- 其他插件可组合复用中心的认证方式，配对码/QR/生物**不再需要复制第二份**；
- 双套发现机制合并为一套，根除同型病灶。

**代价 / 风险**

- **破坏性行为变更**：不带注册原语的中心插件激活后，全部认证面拒绝（fail-closed）→
  须走 fail-visible 形态 ②（`stale_artifact_rebuild_hint` 点名「按 v32 SDK 重建」）；
- **可用性耦合**：认证中心插件停用 = 本机认证面全断（有意为之，但需要 UI 侧给明确提示）；
- **host-auth 追加 4 函数**：desktop ABI 31 → 32；移动端不跟演（`host-auth` 已是桌面独有接口），
  mobile ABI 保持 11；
- **移动端必须同批修**（close code 丢弃 + 自愈无退避）：否则桌面端任何快速拒绝都会在移动端
  变成 6 Hz 日志/toast 风暴。**这是本事故的直接放大器，不能只修桌面**。
- **发布原子性**：裁决切 fail-closed 与中心插件升级到 v32 SDK **必须同批发布**；
  中间态（宿主已切、中心未注册）会让远程终端完全不可用。缓解手段：分三步走，
  第一步先上注册表代码但**保留旧裁决路径**，为第二步留回归窗口与回滚点
  （spec §14）。

**双端偏离登记**（ADR 0022「双端偏离」节追加）：`host-auth` v32 的
`auth-center-register` / `auth-center-unregister` / `auth-methods-list` / `auth-method-invoke`
为桌面独有，移动端 WIT/ABI/SDK 不跟演不投影（移动端是客户端，不承载服务端网关与认证中心角色）。
恢复条件：移动端需要本地认证中心时再补该端 interface。

## 修订记录

- **2026-09-29**：立项。由 4001 日志风暴事故驱动；用户裁定 D1–D4；
  取代 `enforce_connection_policy` 的「能力探测 + 排序取首个 + 两条 fail-open」实现。
- **2026-09-29**：**实施完毕**（见下节）。中间态风险按 spec §15 的同批发布纪律处理。

## 实施记录（2026-09-29）

**一句话**：K1–K9 全部落地，裁决切 fail-closed 走注册表，中心侧（terminal-session）随
v32 SDK 产物同批重建；移动端配套 M1/M2 同批修。

| 决定 | 落点 |
| --- | --- |
| K1 / K4 单中心注册表 | 宿主 `wasm_core/host_api/auth_center.rs`（`AuthCenterEntry { center_id, owner, methods }`；状态机是**纯函数** `register_inner` / `unregister_inner` / `purge_inner`，全局表只做锁外包装；第二个注册者被拒并点名在册属主） |
| K2 / K3 fail-closed | `utils/auth/auth_center.rs::enforce_connection_policy` 重写：查注册表（O(1)）→ 无中心 `no auth center registered`（`deny_kind=no_center`）/ 调用失败 `auth center unavailable: …`（`deny_kind=unavailable`）/ 中心拒绝原样透出（`deny_kind=policy`）。两条 fail-open 降级与 `SESSION_MARKER_API` / `api_registered` / `log_fallback` / `auth_center_candidates` 同步删除 |
| K5 | 未动（宿主仍自持 JWT / P-256 / 密钥托管） |
| K6 组合式认证 | `host-auth.auth-methods-list` / `auth-method-invoke` → `invoke_auth_method` 零解析窄转发到中心 `<owner>.auth-grant`；**只校验 method 在注册表内**（安全闸门判据），中心业务错误原样透传 |
| K7 桥接门切注册表 | `session_active` 改查 `is_registered()`；两套发现机制合一 |
| K8 | 复用 `PERMISSION_AUTH`，未新增权限词汇位 |
| K9 L2 | `terminal-session/plugin.json` `"type": "internal-business"`（静态）+ `activate()` 内 `auth_center_register(["pairing_code","qr","biometric","jwt"])`（动态） |

**fail-visible 三形态**（AGENTS §8）：① 无中心 → 显性拒绝并点名原因（不静默放行）；
② 旧产物 → `boot.rs::activate_role_driven_components` 在 L2 激活后就位点 `error!` 点名
「按当前 SDK 重建（activate 内调 `auth-center-register`）」，另有
`stale_artifact_rebuild_hint` 的**反向**指引（v32+ 产物在旧宿主上 → 「升级 BedCode」）；
③ 退役符号防回接锁 `retired_auth_center_discovery_is_not_reintroduced`。

**spec §15 三步迁移压缩为一批**：步骤 1「保留旧裁决路径」未做（双轨并存期会同时留着病灶
与两条 fail-open），改为**同批切 fail-closed + 重建中心产物**
（`wasm-apps/terminal-session` `pnpm run build` 已执行，产物含 34 个 api 与
`auth-center-register`）。其余三个 wasm 应用不是认证中心，v31 旧产物在 ABI 32 宿主上
仍可加载（`verify_abi` 只拒 `version > 32`），不阻塞发布。

**移动端配套（M1/M2）**：`ServerClosed { code, reason }` 保留关闭码 →
`WS_AUTH_FATAL_CLOSE_CODES = [4001, 4003]` 判**致命**（不自愈，只发一次
「需重新配对」toast）；自愈加**退避下限 1s** + **同因熔断 5 次**（纵深防御，
杜绝任何桌面端快速拒绝引发的 6 Hz 风暴）。

