# wasm 插件分类体系（角色维度重构）

> 立项：2026-09-29 · 用户裁定输入
> 上游依赖：本分类是 `.scratch/2026-09-29-auth-center-registration/`（ADR 0031）的**前提**——
> 认证中心归入哪一类，直接决定 ADR 0031 的方案选型（见 §5 张力 4）
> 边界单一事实源：`docs/adr/0022-*.md`；本体系落地后另记 ADR 0032

## 1. 用户裁定的分类（原始形态）

```
wasm 分类
├── 1. 基础服务（basic service）—— 类似 http / mdns / pty 等基础服务
│      **现在没有，预留给将来实现**（例如 ffmpeg 服务等）
├── 2. 应用（application）
│    ├── 2.1 内部统一业务的应用 —— 类似认证中心
│    └── 2.2 wasm-app 真正的业务应用
│          ├── 业务插件 wasm      ← 预留
│          └── 业务 worker        ← 预留
```

**「预留」的口径**（AGENTS §0「文档字面 ≠ 事实」的教训）：
预留 = **登记类别 + 写清将来启用时必须补什么**，**不是**写一句注释了事。
本体系里每个预留类别都带一张「启用时需补」清单（§6），缺项即视为文档失真。

## 2. 现状：桌面端 wasm 核心**已经**有分类，是四个正交维度

回答「有没有对 wasm 做分类、分哪几类」——有，但**用户要的是其中维度 1 的重构**：

| # | 维度 | 类型（真源） | 取值 | 载体 | 语义 |
| --- | --- | --- | --- | --- | --- |
| 1 | **装配角色** | `PluginKind`（SDK `types.rs:167`） | `Application`（缺省）\| `System` | manifest `type` | `system` = 内置、**先于应用插件激活**、启停不持久化、向能力注册表提供 host-* 同形接口能力 |
| 2 | 产物形态 | `PluginType`（`types.rs:145`） | `Rust` \| `RustTs` \| `TsOnly` | manifest `pluginType` | 有无前端（与角色**正交**） |
| 3 | 来源 | `PluginSource`（`manager/types.rs:23`） | `StaticRegistry`→`builtin` \| `FileScan`→`scanned` \| `Wasm`→`wasm` \| `UserInstalled` | 非 manifest，宿主推导 | 可卸载性等 |
| 4 | 并发模型 | `CallModel`（`wasm_core/config.rs:121`） | `Mutex` \| `EventLoop` | 配置（非 manifest 声明） | 实例级串行 vs 常驻事件循环属主（ADR 0029） |

四个 wasm 应用的落点（`wasm-apps/*/plugin.json`）：

| 插件 | 维度1 角色 | 维度2 形态 | 维度3 来源 | 维度4 并发 |
| --- | --- | --- | --- | --- |
| `com.bedcode.terminal-session` | `application` | `rust-ts` | `wasm` | mutex（默认） |
| `com.bedcode.agent-hub` | `application` | `rust-ts` | `wasm` | mutex |
| `com.bedcode.ai-chatbox` | `application` | `rust-ts` | `wasm` | mutex |
| `com.bedcode.file-transfer` | `application` | `rust-ts` | `wasm` | mutex |

**`System` 角色生产零使用者**——唯一使用点是测试
`manager/host/tests/system_component_test.rs:90`。即用户说的「基础服务现在没有」。

## 3. `System` 角色 = 用户所说「基础服务」的机制骨架（已实现，且已核过边界）

| 语义 | 实现位置 | 状态 |
| --- | --- | --- |
| 先于应用插件激活 | `boot.rs:93 activate_system_components()`，在 `host.rs:515` 调用、严格早于 `:519` 的 `auto_activate_from_persisted_state()` | ✅ 已实现；按 id 排序保证确定性；**单个失败不阻断其余** |
| 启停不持久化 | `activation.rs:861` 跳过持久化写入 | ✅ 已实现 |
| 向能力注册表提供 host-* 同形能力 | `activation.rs:571` / `host/wasm.rs:197` `register_system_capabilities()` | ✅ 已实现，但 `capability.rs:96 ROUTABLE_CAPABILITIES` **当前只有 `host-storage` 一项** |
| 只停不删 | `install.rs:153 uninstall_plugin` 只判「未启用」，**无按 kind 拒绝的分支** | ⚠️ **名实不符**（仅存在于文档注释） |

**结论：L1「基础服务」= 现有 `System` 角色的正式化与扩展，不是新造机制。**
用户列举的 http / mdns / pty 正是 `host-*` 同形接口——那三者在 L1 形态下将由 wasm 组件
提供并经 Linker 路由，而**今天它们是宿主原生实现**（宿主是默认提供者，组件是可选覆盖者）。
`ffmpeg` 属于「将来新增的基础服务」，即**宿主今天根本没有对应 `host-*` 原语**的那一类。

## 4. 目标分类（角色维度重构后）

| 层 | 角色值（建议） | 归属 | 谁依赖谁 | 加载顺序 | 现状 |
| --- | --- | --- | --- | --- | --- |
| **L1** | `basic-service` | 宿主引擎域 | 应用插件的 import 经 Linker 路由到它 | ① 最先 | **预留**（机制 = 现有 `System`，零使用者） |
| **L2** | `internal-business` | 宿主与业务应用之间 | **宿主内核主动调用它**（反向依赖） | ② 次于 L1 | **本专项新建**（认证中心） |
| **L3** | `business-app` | 业务面 | 消费 L1 与宿主 `host-*` 原语 | ③ 最后 | 现状（四个应用） |
| L3.a | └ 业务插件 wasm | 业务面 | 同上 | ③ 同批 | **预留** |
| L3.b | └ 业务 worker | 业务面 | 同上，无前端 | ③ 同批 | **预留** |

### L2 是唯一「反向依赖」类别 —— 必须防扩散（本体系的核心红线）

- **L1**：组件提供能力，**应用的 import 被路由过去**（组件被调用）
- **L3**：宿主提供 `host-*` 原语，**应用调宿主**（应用被调用）
- **L2**：**宿主内核主动调用该 wasm 组件**做裁决——**方向与前两者相反**

这是本体系里**唯一一处「宿主业务依赖 wasm 组件」**。没有刹车机制的话，
它会自然蔓延成「宿主依赖一堆业务 wasm」，直接违反 §5.1 无业务内核红线
（今天的 `host-platform.wsl-distros` 已是宿主读产品事实的既存争议点）。

**写死的三条约束**（建议列入 ADR 0032）：

1. **白名单式登记**：L2 的实例由宿主显式登记（认证中心注册表），不自动发现；
2. **只允许安全闸门用途**：L2 只做「allow/deny 类裁决」，不承载业务编排与产品事实
   （配对码生成、设备列表、信任记录真源**都在 L2 自己的私有库里**，宿主不读——与今日
   「宿主主库 `pairings` 表已退役」同向）；
3. **宿主只转发不解释**：调用 L2 的返回值**零解析透传**（同 `utils/session_gateway.rs`
   口径），宿主不得基于返回值做产品判断。加防回接锁。

## 5. 张力清单与裁定状态（**全部裁定完毕**）

| 张力 | 主题 | 状态 |
| --- | --- | --- |
| 1 | worker 是角色还是形态 | ✅ **已裁定**（2026-09-29）：形态 → 新增 `lifecycle: persistent \| ephemeral` |
| 2 | L1 基础服务的能力声明方式 | ✅ **已裁定**（2026-09-29）：manifest 声明驱动（2a） |
| 3 | 加载优先级序列 | ✅ **已裁定**（2026-09-29）：`L1 → L2 → L3（插件 wasm + worker 同批）` |
| 4 | 认证中心是 L2，谁来承担 | ✅ **已裁定**（2026-09-29）：**4b** 暂留 terminal-session 兼任 L2；拆分为独立后续专项 |


### 张力 1：~~`业务插件 wasm` / `业务 worker` 是角色还是形态？~~ → **已裁定：形态，需新增第三个正交维度 `lifecycle`**

**用户裁定的语义（2026-09-29）**：

- **业务插件 wasm** = 对 wasm-app 的**扩展**，一般具有页面（UI），**也可以没有**
- **业务 worker** = 单纯是对 store 操作、**即用即弃**的 wasm
- **wasm-app 通过调度启动 worker**，来完成一些**消耗线性内存**的操作，**用完即弃**

#### 1.1 修正：worker 不能用 `CallModel` 表达（本专项曾给出错误建议）

**本专项先前建议「worker ≈ `pluginType: rust` + `CallModel::EventLoop`」是错的**：
`CallModel::EventLoop`（`wasm_core/config.rs:121`）是**常驻**事件循环属主任务
（ADR 0029 的并发模型），而 worker 是**即用即弃**——两者语义**正好相反**。

正确表达：worker 需要**第三个正交维度 `lifecycle`（实例生命周期策略）**：

| 维度 | 取值 | 含义 | 对 ephemeral 的适用性 |
| --- | --- | --- | --- |
| `role` | L1 `basic-service` / L2 `internal-business` / L3 `business-app` | 装配角色 | 均适用 |
| `lifecycle`（**新增**） | `persistent`（缺省）\| `ephemeral` | 实例是否常驻 | —— |
| `pluginType` | `rust` / `rust-ts` / `ts-only` | 有无前端产物 | `ephemeral` **仅允许** `rust` |
| `CallModel` | `Mutex` / `EventLoop` | 实例**内**并发模型 | **`ephemeral` 无意义**（无「常驻属主任务」这回事；单次任务无并发问题） |

若把 worker 写成 `role` 的第四个值，会同时表达「角色」与「生命周期」两件事，
并与 `CallModel` 语义重叠——**两处各拼一套必然漂移**。

#### 1.2 worker 的技术动机：代码里的硬证据（登记为知识，本期不实现）

不是「为了跑得快」，而是**为了避开线性内存单调增长撞限额**：

1. WebAssembly 线性内存**只能 grow 不能 shrink**（规范硬事实）——实例内存一旦涨上去永不回落；
2. 宿主限额是**单实例口径**：`runtime.rs:204 memory_growing` 按
   `limits.max_memory_bytes` 拒绝增长（`Ok(false)` → guest 分配 trap），
   且不随时间回落（`record_memory_growth` 只记账，不释放）；
3. 因此**长驻实例**处理大输入（大文件哈希 / base64 / 大 JSON 解析 / 索引扫描）
   → 内存单调增长 → 撞限额 trap，**且不重启进程好不了**；
4. worker **用用即弃** → 实例 drop，内存归还 OS，限额从低水位重新起算。

**推论（写死为约束）**：worker 存在的理由是**内存生命周期**，不是业务分层。
将来若某个任务既不吃内存又不吃 CPU，就**不该**为它开 worker（启动成本白付）。

#### 1.3 worker 的契约（本期只登记，不实现）

| 项 | 契约 | 理由 |
| --- | --- | --- |
| 产物形态 | `pluginType: rust` | 无页面 |
| 生命周期 | `ephemeral`（用用即弃） | §1.2 |
| 职责边界 | **只对 store 操作**，无业务编排、无产品状态、无页面 | 用户裁定；保证它可被任意 L3 应用复用 |
| 调度方 | **wasm-app**（非宿主、非用户） | 用户裁定；宿主不解释何时该跑 worker |
| `CallModel` | 不适用 | §1.1 |
| 权限 / 配额 / 传参协议 | **未定**（启用前必须定，见 §6 L3 清单） | 安全边界，不可默认 |

### 张力 2：L1 基础服务的能力声明方式 —— **已裁定 2a（用户 2026-09-29）**

**决定：manifest 声明驱动。** 基础服务在 manifest 声明**自己提供哪些 host-\* 同形能力**，
宿主按声明装配路由——`capability.rs:96 ROUTABLE_CAPABILITIES` 从**硬编码表改为注册表驱动**。

| 选项 | 做法 | 裁定 |
| --- | --- | --- |
| **2a** | manifest 声明驱动，宿主按声明装配 | ✅ **采纳** |
| 2b | 沿用硬编码白名单 | ❌ 否决：每加一个基础服务（如 ffmpeg）要改宿主代码再发版，违背可扩展初衷 |

**推论**：`ffmpeg` 这类新能力**天然可加**（基础服务自己声明 → 宿主装配），
不需要「先加一个 `host-ffmpeg` 内核原语再想怎么 wasm 化」的两步走。

**现状与差距**：`ROUTABLE_CAPABILITIES` 当前**只有 `host-storage` 一项**；
`auth-policy` 是**仅探测不路由**（`capability.rs:108`：注册为路由提供者会让任意插件
接管认证策略，语义错误）。改造后**探测与路由两张表要分开**——`auth-policy` 必须
**永远**留在探测侧，不得因「注册表驱动」而误入路由侧（否则任意声明 `system` 的插件
都能接管认证策略裁决）。

### 张力 3：加载优先级序列 —— **已裁定（用户 2026-09-29）**

**裁定的加载顺序**：

```
L1 基础服务  →  L2 内部统一业务  →  L3 wasm-app（业务插件 wasm + 业务 worker）
```

与现有启动序列的对应关系（`host.rs:505-520`）：

| 启动序列步骤 | 现状 | 改造后 |
| --- | --- | --- |
| 步骤 4 | `activate_system_components()`（`boot.rs:93`）—— 单一System 批 | **拆两批**：先 L1 基础服务，再 L2 内部统一业务（批内按 id 排序，确定性） |
| 步骤 5 | `auto_activate_from_persisted_state()` | **不变**——它就是 L3 批（插件 wasm + 业务 worker 同批） |

**为什么复用而不是新造**：步骤 4 已经在 `auto_activate` 之前跑，机制与位置都对，
只需把「一批 System」扩为「按角色两批」。批内继续按 id 排序（不引入隐式优先级规则）。

**「单个失败不阻断其余」的沿用与代价**：现有语义是 system 组件失败落 Error 态、
其余照常。沿用到 L2 后——**认证中心激活失败时 L3 照常激活，但认证面全拒**（K3
fail-closed）。安全上正确，但必须有可见信号（§6 L2 清单第 3 项），
否则用户看到的是「所有东西都连不上」而无处下手。

**待确认的两个子点**：

1. **L3 内部（业务插件 wasm vs 业务 worker）是否还要再排序**？当前裁定把两者并列在
   L3 一批内（批内按 id 排序）。若将来 worker 需要先于插件就位（例如 worker 承载
   worker 侧任务调度、插件要向它派活），则需再拆一层。**建议现在不拆**——
   提前引入层级的成本高于收益，等真有依赖再拆（避免又一次「先凑合再返工」）。
2. **`StaticRegistry`（builtin / inventory）来源的组件**：现有
   `boot.rs:98` 的 filter 跳过 `source == StaticRegistry`（它们是原生 Rust 插件，
   经 inventory 静态注册，不走磁盘加载，因此无需激活）。L1/L2 若是 wasm 组件
   （`source == Wasm`）不受影响；但**若将来出现原生 builtin 形态的基础服务**，
   这个 filter 会把它跳过——启用前需确认该 filter 的原始意图是否仍成立。

### 张力 4：认证中心是 L2，谁来承担 —— **已裁定 4b（用户 2026-09-29）**

**决定 4b：认证中心暂留 `com.bedcode.terminal-session`，由它兼任 L2 角色。**
L2 允许「一个插件兼任 L3 + L2」。L2 拆分为独立应用（`com.bedcode.auth-center`）
**作为独立后续专项**——不与本批绑定，避免把 ADR 0031 的核心（注册 + fail-closed）
绑架在六域拆分风险上。

**已作废的中间方案**：给 `terminal-session` 加 `"type": "system"`——
`system`/L1 定义是「**提供 http/mdns/pty 同形能力**」，而 `terminal-session` 是
**消费者**（`dependencies: ["host-pty"]`，不提供任何 host-\* 同形能力），
打该标签等于把消费者标成提供者，语义错误；且 `register_system_capabilities` 对它是
no-op，标签只会带来「只停不删」等无关语义。

**为什么 4b 不违反任何红线**：L2 的三条约束（ADR 0032）是关于「**宿主怎么用 L2**」
——白名单式登记、只做安全闸门、只转发不解释；**没有一条要求 L2 必须独占一个插件**。

#### 4b 落地必须解决的一处实现细节：L2 身份要**静态声明 + 动态就绪**两段

`auth-center-register`（ADR 0031 K1）是**运行时**注册，而 K9 要求 L2 **先于 L3 加载**。
若 L2 身份纯靠 activate 里的注册动作确立，激活顺序就退化成「L2 与其他应用同批、
靠时序巧合抢先」——不可靠。故拆成两段，**两者不重复**：

| 段 | 载体 | 回答什么 | 时机 |
| --- | --- | --- | --- |
| **静态声明** | manifest（`terminal-session` 声明 L2 身份） | 「谁该先加载」→ 步骤 4 第二批 | 加载期可判定 |
| **动态就绪** | `auth-center-register` 调用 | 「我已就绪 + 唯一性仲裁」 | activate 内 |

**不要合并**：合并成「只有静态声明」则没有唯一性仲裁（第二个声明者无人拒绝）；
合并成「只有动态注册」则加载顺序无法保证。

## 6. 预留类别的「启用时需补」清单（防文档失真）

### L1 基础服务（`basic-service`）

- [ ] `ROUTABLE_CAPABILITIES` 改注册表驱动（张力 2a）
- [ ] 能力覆盖：当前 `host-storage` 一项；将来 http / mdns / pty wasm 化需各自补
- [ ] 补 `uninstall_plugin` 的 kind 拒绝分支（兑现「只停不删」，消除 §3 的名实不符）
- [ ] 第一个真实 L1 组件（候选：ffmpeg 转码服务——**宿主今天没有 `host-ffmpeg` 原语**，
      属新增引擎域，需走 ADR 0022 同款裁剪流程）

### L2 内部统一业务应用（`internal-business`）

- [ ] 注册表 + 唯一性仲裁（认证中心专项票 01-03）
- [ ] 宿主只转发不解释的防回接锁
- [ ] L2 未就绪的可见信号（core-monitor + 前端提示），否则 fail-closed 表现为「什么都连不上」
- [ ] 「L2 只能做安全闸门」的白名单式约束落代码（不是只写文档）

### L3.a 业务插件 wasm / L3.b 业务 worker

**本期只预留类型，不实现 worker 的统一调度框架**（用户裁定 2026-09-29）。
以下为启用时必须补的清单（防「预留」变成永久悬空的谎言）：

- [ ] **新增 `lifecycle` manifest 字段**（缺省 `persistent` 保持旧行为、零迁移）+
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
      与 `PluginKind::System` 的「启停不持久化」是两回事，manifest 校验要区分

## 7. 与既有文档的关系

- 本体系**不改** ADR 0022 的裁剪线（B1-B6 判据）——L2 的三条约束是**加法**不是替代
- `PluginKind` 的两值 → 四值是**破坏性 manifest 变更**（`type` 字段取值域扩大）：
  缺省值保持 `application` 向后兼容，**旧产物零迁移**（这是与 ABI 31→32 独立的一件事）
- 落地时需同步：`plugin-development-checklist.md`（分类说明）、
  `manifest-validate.js`（取值域校验）、ADR 0022 修订记录
