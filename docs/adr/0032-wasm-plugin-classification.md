# wasm 插件分类体系：基础服务 / 内部统一业务 / 业务应用

## 状态

**已实施（机制层，2026-09-29）**——角色三层 + 加载分批 + `lifecycle` 预留 + L2 红线锁均已落地。
**L2 的第一个真实组件（认证中心 `com.bedcode.terminal-session`）由 ADR 0031 专项挂上
`"type": "internal-business"`**，与它的动态就绪原语 `auth-center-register` 同批上线——
分类机制先行、两段判据成对落地（只挂静态声明会让「谁该先加载」与「是否已就绪」互相矛盾）。
spec：`.scratch/2026-09-29-wasm-classification/spec.md`，票：`.../issues/01-04`。
四项张力**全部裁定完毕**（用户 2026-09-29）：分类三层 + 加载顺序 `L1 → L2 → L3` +
worker 为 `lifecycle: ephemeral` 形态（本期只预留类型）+ L1 能力 manifest 声明驱动 +
认证中心**暂留 `terminal-session` 兼任 L2**。

本 ADR 修订 ADR 0022 的「双端偏离」节（新增 L2 类别）与分类相关表述。

## 背景

桌面端 wasm 核心**已有分类，但是四个正交维度**（不是没有分类）：

| 维度 | 枚举 | 取值 | 载体 |
| --- | --- | --- | --- |
| 装配角色 | `PluginKind` | `Application` \| `System` | manifest `type` |
| 产物形态 | `PluginType` | `Rust` \| `RustTs` \| `TsOnly` | manifest `pluginType` |
| 来源 | `PluginSource` | builtin / scanned / wasm / user-installed | 宿主推导 |
| 并发模型 | `CallModel` | `Mutex` \| `EventLoop` | 宿主配置 |

问题不是「没有分类」，而是**角色那一维不足以表达真实形态**：

1. `System` 的定义是「向能力注册表提供 **host-\* 同形接口**能力」——它是**基础服务的
   代理身份**（http / mdns / pty 将来 wasm 化后由它提供），但**今天零插件使用**
   （`ROUTABLE_CAPABILITIES` 只有 `host-storage` 一项），类别与现实脱节；
2. **装不下一个关键角色**：认证中心是宿主网关的裁决依赖方（宿主**主动调它**），
   它既不是基础服务（不提供 host-* 同形能力）、也不是业务应用（不是产品功能），
   只能硬塞进 `terminal-session` 这个会话应用里；
3. 缺「即用即弃」形态的表达位：`CallModel::EventLoop` 是**常驻**属主任务，
   与 worker 的即用即弃语义**相反**，无法复用。

**裁定补充（2026-09-29）**：L1 能力改为 **manifest 声明驱动**（取代硬编码
`ROUTABLE_CAPABILITIES`）；认证中心**暂留 `terminal-session` 兼任 L2**，拆分为
独立应用留作后续专项。

## 决定

### 分类（三层 + 预留子形态）

```
L1 基础服务 basic-service          预留（今天 http/mdns/pty 是宿主原生实现）
L2 内部统一业务 internal-business  认证中心（宿主网关的裁决依赖方）
L3 业务应用 business-app           现状四个应用
   ├ 业务插件 wasm                 wasm-app 的扩展，一般有页面，也可以没有
   └ 业务 worker                   即用即弃，**本期只预留类型**
```

**取值域只三个**（L1 / L2 / L3），历史拼写 `system` / `application` 作为 serde
**反序列化别名**保留（旧产物零迁移，序列化统一写新拼写）——不设第四个 `application`
值与 L3 并存：两个拼写指同一层就会变成「两处各拼一套必然漂移」的第二处。
`lifecycle`（`persistent` / `ephemeral`）是与角色**正交**的第四个字段（见下）。

**加载顺序（已裁定）**：`L1 → L2 → L3`。对应现有启动序列只需一处改动——
`activate_role_driven_components()`（`boot.rs`，原 `activate_system_components`）扩为
**按角色两批**，层序真源取自 SDK 常量 `PluginKind::ROLE_DRIVEN_LOAD_ORDER`；
`auto_activate_from_persisted_state()` 就是 L3 批、不过滤以外的额外动作（它现在**只**
取 L3，角色驱动层不进该批）。
L3 内部（业务插件 wasm / 业务 worker）**同批**，不再分层。
**单个失败不阻断其余**沿用：失败组件落 Error 态并记 `error` 日志点名角色，其余照常激活
（L2 失败 → L3 照常激活、认证面 fail-closed，可见信号由 ADR 0031 负责）。

**宿主只按谓词判角色**：`is_role_driven()` / `provides_host_capabilities()` /
`is_business_app()`，加载顺序读 SDK 常量——宿主源码里**不出现** `PluginKind::X` 三个
角色值（防回接锁 `host_switches_on_role_predicates_not_role_values` + 可扩的
`ROLE_VALUE_ALLOWLIST`）。这样新增角色不必改宿主，分类学也不会退化成「宿主按角色名
做业务分支」——那正是红线要防的蔓延形态。

### L2 是唯一「反向依赖」类别 —— 三条红线

- **L1**：组件提供能力，应用的 import 经 Linker 路由过去（**组件被调用**，调用方是应用）
- **L3**：宿主提供 `host-*` 原语，应用调宿主（**应用被调用**，调用方是应用）
- **L2**：**宿主内核主动调用该 wasm 组件**（**组件被调用**，调用方是宿主）——方向与前两者相反

这是**唯一一处「宿主业务依赖 wasm 组件」**。无刹车机制则会蔓延成
「宿主依赖一堆业务 wasm」，直接违反 §5.1 无业务内核红线。故写死三条：

1. **白名单式登记**：L2 实例由宿主显式登记（认证中心注册表），**不自动发现**；
2. **只允许安全闸门用途**：L2 只做 allow/deny 类裁决，不承载业务编排与产品事实
   （配对码生成 / 设备列表 / 信任记录真源全在 L2 私有库，宿主不读——与
   「宿主主库 `pairings` / `connection_history` / `session_configs` 已退役」同向）；
3. **宿主只转发不解释**：调 L2 的返回值**零解析透传**（同 `utils/session_gateway.rs`
   口径），宿主不得基于返回值做产品判断。加防回接锁
   `internal_business_host_dependency_stays_gated`。

### L1 能力声明 = manifest 声明驱动（已裁定）

基础服务在 manifest 声明**自己提供哪些 host-\* 同形能力**，宿主按声明装配路由
（`capability.rs:96 ROUTABLE_CAPABILITIES` 从硬编码表改为注册表驱动）。
**推论**：`ffmpeg` 这类新能力天然可加，不需要「先加内核原语再想怎么 wasm 化」两步走。

**红线**：探测与路由两张表必须分开——`auth-policy` 必须**永远**留在探测侧
（`capability.rs:108`：注册为路由提供者会让任意插件接管认证策略，语义错误），
不得因「注册表驱动」而误入路由侧。

**实施口径（2026-09-29 定案）**：`ROUTABLE_CAPABILITIES` 改造**随第一个真实 L1 组件一起做**——
今天它是单条目（`host-storage`）、零 L1 使用者，先把硬编码表换成注册表只会得到一张空表
加一套无人调用的机制。改造时必须保持 `auth-policy` 在探测侧（ADR 0031 已把策略发现
改为显式注册表，`PROBE_CAPABILITIES` 中该项届时随之下线）。

### 认证中心：暂留 `terminal-session` 兼任 L2（已裁定）

**L2 允许「一个插件兼任 L3 + L2」**——L2 的三条约束都是关于「**宿主怎么用 L2**」，
没有一条要求 L2 独占一个插件。拆分为独立 `com.bedcode.auth-center` 留作后续专项，
避免把 ADR 0031 的核心（注册 + fail-closed）绑架在六域拆分风险上。

**L2 身份须「静态声明 + 动态就绪」两段，不合并**：

| 段 | 载体 | 回答什么 |
| --- | --- | --- |
| 静态声明 | manifest（`terminal-session` 声明 L2） | 「谁该先加载」→ 步骤 4 第二批 |
| 动态就绪 | `auth-center-register`（ADR 0031 K1） | 「我已就绪 + 唯一性仲裁」 |

合并成「只有静态声明」则无唯一性仲裁（第二个声明者无人拒绝）；
合并成「只有动态注册」则加载顺序退化成时序巧合。

### worker（业务 worker）：`lifecycle` 是第三个正交维度

| 维度 | 取值 | 对 worker 的适用性 |
| --- | --- | --- |
| `role` | L1 / L2 / L3 | L3 |
| **`lifecycle`（新增）** | `persistent`（缺省）\| `ephemeral` | `ephemeral` |
| `pluginType` | rust / rust-ts / ts-only | 仅 `rust` |
| `CallModel` | Mutex / EventLoop | **不适用**（无常驻属主任务这回事） |

**动机是内存生命周期，不是业务分层**（代码里的硬证据）：

1. wasm 线性内存**只能 grow 不能 shrink**；
2. 宿主限额是**单实例口径**（`runtime.rs:204 memory_growing` 按 `max_memory_bytes`
   拒绝增长 → guest trap，`record_memory_growth` 只记账不释放）；
3. 长驻实例处理大输入 → 内存单调增长 → 撞限额 trap，**不重启进程好不了**；
4. 即用即弃 → drop 实例 → 内存归还 OS → 限额从低水位重算。

**推论**：某任务若既不吃内存也不吃 CPU，就**不该**为它开 worker。
**本期只预留类型，不实现调度框架**（用户裁定 2026-09-29）：`lifecycle: ephemeral`
的声明在构建链（`manifest-validate.js`）与宿主加载期（`validation.rs::validate_lifecycle`）
**双侧显性拒绝**——静默当常驻处理会让作者以为 worker 生效，而常驻恰是它存在理由的反面。
Rust 侧类型名带 `Instance` 前缀（`InstanceLifecycle`），与 `contributes.lifecycle`
（应用生命周期钩子 `LifecycleContribution`）区分：两者毫无关系，同名会误导。

## 预留类别的「启用时需补」清单（防文档失真）

> 自 spec §6 搬入（spec 是过程文档，本 ADR 是持久记录；**多处代码注释与票面按
> 「ADR 0032 §6」引用本节**）。「预留」的口径：登记类别 + 写清启用时必须补什么；
> 下列未勾选项即「文档承诺」，谁想启用该类别，先把对应项做完。

### L1 基础服务（`basic-service`）

- [ ] `ROUTABLE_CAPABILITIES` 改注册表驱动（张力 2a）——**与第一个真实 L1 组件同批**，
      提前改只会得到空表 + 无人调用的装配
- [ ] 能力覆盖：当前 `host-storage` 一项；将来 http / mdns / pty wasm 化需各自补
- [ ] 补 `uninstall_plugin` 的 kind 拒绝分支（兑现「只停不删」——**L1 零使用者期间
      文档只承诺「角色驱动 / 启停不持久化」**，不承诺不可卸载）
- [ ] 第一个真实 L1 组件（候选：ffmpeg 转码服务——**宿主今天没有 `host-ffmpeg` 原语**，
      属新增引擎域，需走 ADR 0022 同款裁剪流程）

### L2 内部统一业务应用（`internal-business`）

- [x] 宿主只转发不解释的防回接锁（`host/tests/l2_gating_test.rs`：L2 消费点白名单 +
      裁决门只回裁决 + 宿主只按谓词判角色）
- [x] 「L2 只能做安全闸门」的白名单式约束落代码（同上锁的白名单即该约束的载体）
- [ ] 注册表 + 唯一性仲裁（认证中心专项票 01-03）——**ADR 0031 实施中**
- [ ] L2 未就绪的可见信号：已落 `error` 日志 + `deny_kind=no_center`（`boot.rs` 激活
      就位点）；**未落** core-monitor 计数与前端一次性提示——没有后者，fail-closed
      在用户侧表现为「所有东西都连不上」而无处下手

### L3.a 业务插件 wasm（wasm-app 的扩展，一般有页面，也可以没有）

**形态已就位**：`type: business-app`（缺省）+ `dependencies` 声明式扩展，与既有
业务应用无差别装配（无新增机制）。「预留」指的是**无任何生产使用者**与下列待定项：

- [ ] 谁声明「我是某应用的扩展」——今天只有 id / 命名空间隐含归属，无显式声明面
- [ ] 宿主对扩展的**加载顺序**是否需要在 L3 批内再分层（当前同批、批内按 id 排序；
      真有依赖再拆，避免提前引入层级）

### L3.b 业务 worker（即用即弃）

**本期只预留类型，不实现 worker 的统一调度框架**（用户裁定 2026-09-29）。
已落地：`lifecycle` manifest 字段（缺省 `persistent`）+ 构建链取值域校验
（`ephemeral` + 非 `rust` 形态 → 构建期拒；`ephemeral` 本身亦拒）+ 宿主加载期拒。
以下为启用时必须补的清单（防「预留」变成永久悬空的谎言）：

- [x] **`lifecycle` manifest 字段**（缺省 `persistent` 保持旧行为、零迁移）+
      `manifest-validate.js` 校验（`ephemeral` + 非 `rust` 形态 → 构建期拒）
- [ ] 宿主侧一次性实例机制（**现状核查：全仓不存在**，实例化只在加载期
      `LoadedWasmPlugin` 路径；`host-task` v20 方向相反——它让宿主 OS 线程池跑
      **宿主原语**，`manager/task.rs:1-8` 明写「不接触 WASM / Store」）
- [ ] 统一调度框架（**本期不做**）：谁触发、并发多少、实例池与否、超时与取消
- [ ] 内存动机的回归锁（启用时）：长驻实例涨到限额后处理小输入 → trap；
      同样工作在 ephemeral worker 里**重复多次不撞限额**（§1.2 的实证）
- [ ] store 传参协议：传句柄 + 小指令，**禁止**大块数据编进参数——
      否则 worker 省下的内存会在参数拷贝阶段还回去，**抵消全部意义**
- [ ] 权限模型定案（继承调度方 vs worker 独立声明）+ 权限位词汇同步
      （`permission.rs` 真源 + `pnpm run gen:permissions` + `manifest-gen.js` 映射表）
- [ ] per-app worker 配额（参照 `ptyQuota` 模式：自声明 + 加载期区间仲裁）。
      无配额 = 循环调度能把 CPU 打爆且**内存并没省**
- [ ] 先不做实例复用（复用即回到长驻，与 worker 的存在理由矛盾）
- [ ] 停用语义：ephemeral 组件本身**不可停用**（无常驻实例可停）——
      与角色驱动的「启停不持久化」是两回事，manifest 校验要区分

## 归属裁决（ADR 0022 §5.1.2 三问）

1. **离宿主能实现吗？** 加载顺序、角色仲裁、生命周期托管——只有宿主能做 → 放宿主
2. **携带产品语义吗？** 分类本身是**机制学**（谁先加载、谁提供什么、活多久），
   不含产品概念；`methods` / `methods` 列表是声明式，宿主不解释 → 不命中 B1–B6
3. 都不命中 → 放宿主，落点：注册表 + 安全闸门 + 零解析转发（§5.1.3 允许的四类薄壳）

## Considered Options

| 方案 | 为什么不选 |
| --- | --- |
| **worker 进 `role` 枚举** | 同时表达「角色」与「生命周期」，并与 `CallModel` 语义重叠——两处各拼一套必然漂移 |
| **用 `CallModel::EventLoop` 表达 worker** | 语义相反：`EventLoop` 是**常驻**属主任务（ADR 0029），worker 是即用即弃 |
| **给 `terminal-session` 加 `"type": "system"`** | `system`/L1 定义是「提供 host-* 同形能力」；terminal-session 是**消费者**（`dependencies: ["host-pty"]`），打该标签等于把消费者标成提供者 |
| **立即拆分独立 `com.bedcode.auth-center`** | 概念最纯，但六域（配对/QR/生物/JWT/策略/信任/记录）拆分成本高，且会把 ADR 0031 的核心（注册 + fail-closed）绑架在拆分风险上。**裁定：暂不拆（4b），留作后续专项** |
| **L1 能力声明沿用硬编码白名单**（2b） | 每加一个基础服务（如 ffmpeg）要改宿主代码再发版。**裁定：否决，改 manifest 声明驱动（2a）** |

## Consequences

**正面**

- 三个概念各有其位：基础服务（引擎域）、内部统一业务（宿主依赖）、业务应用（产品面）；
- 加载顺序从「隐式」变「显式两批」，认证中心不再需要靠插件 id 排序抢第一；
- worker 的存在理由有据可查（内存生命周期），不是玄学分层。

**代价 / 风险**

- `PluginKind` 取值域扩大是**破坏性 manifest 变更**（缺省值不变 → 旧产物零迁移）；
- **L2 是全新边界**（宿主反向依赖组件），红线必须落成**代码与锁**，不能只写文档；
- L1「现在没有」意味着该类别在实现前**零真实使用者**——`ROUTABLE_CAPABILITIES`
  仍只有 `host-storage` 一项，`ffmpeg` 之类属将来新增引擎域，需另走 ADR 0022 同款裁剪流程；
- **既存名实不符（已处理，2026-09-29）**：`PluginKind::System` 文档曾称「只停不删」，但
  `install.rs::uninstall_plugin` 只判「未启用」，无按 kind 拒绝的分支。本轮**不补该分支**
  （L1 尚无真实使用者，补了是无人调用的守卫），改为**把文档口径改准**：角色语义只承诺
  「角色驱动 / 启停不持久化」，「只停不删」进 §6 启用清单，与第一个 L1 组件同批补。

**双端偏离**：分类是双端共有的机制学，但落地**桌面独有**（移动端 SDK 无 `PluginKind` /
无角色驱动的加载分批，是自持业务 App 的客户端）。移动端何时跟进、跟进到哪一层，
由其首个需要分层的场景决定（ADR 0022「双端偏离」节已登记该条款）。

## 修订记录

- **2026-09-29**：立项。用户裁定分类三层 + 加载顺序 `L1 → L2 → L3` + worker 为
  即用即弃形态；修正「worker = `CallModel::EventLoop`」的错误表达（改为 `lifecycle`
  第三维度）；登记「只停不拆」名实不符待修。与 ADR 0031（认证中心注册）互为前提。
- **2026-09-29（同日补裁）**：四项张力全部裁定——L1 能力改 **manifest 声明驱动**（2a）；
  认证中心**暂留 `terminal-session` 兼任 L2**（4b），并确立 L2 身份
  「静态声明（加载顺序）+ 动态就绪（唯一性仲裁）」两段式、不合并。
- **2026-09-29（实施，票 01–03）**：角色枚举落为**三值 + 旧拼写反序列化别名**
  （`system`→L1、`application`→L3；不新增与 L3 并存的第四值）；`activate_system_components`
  更名 `activate_role_driven_components` 并扩为 L1/L2 两批；L3 批与持久化表只收 L3，
  L1/L2 启停不持久化；新增第四个正交字段 `lifecycle`（`persistent` 缺省 / `ephemeral`）
  并在**构建期与加载期双侧显性拒绝** `ephemeral`（fail-visible，不静默当常驻）；
  新增防回接锁文件 `host/tests/l2_gating_test.rs`（L2 消费点白名单 + 裁决门只回裁决 +
  宿主只按谓词判角色）。`ROUTABLE_CAPABILITIES` 注册表化推迟到第一个真实 L1 组件同批
  （否则只是空表搬家）。**副作用（与 ADR 0031 同批生效）**：`com.bedcode.terminal-session`
  挂 L2 后它的启停转为**角色驱动**——不再进持久化激活表、也不再由 L3 批激活，
  用户在插件管理页停用它只对当前会话生效（下次启动按角色恢复）。这是 L2「内核依赖、
  不可持久停用」的方向（与 fail-closed 一致），但它是一处**用户可见的语义变化**，
  UI 口径需与 ADR 0031 的一处提示一并确认
