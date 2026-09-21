# 终端会话中心插件（com.bedcode.session）实施规格 —— 配对 + 会话语义下沉并与 auto-task 合并

Status: **landed（已完成，票 01-18 全绿；后续延续规格见 `.scratch/2026-09-21-terminal-into-session-plugin/spec.md`，其票线含终端窗口域下沉 / 插件改名 terminal-session / 私有库迁移，2026-09-22 全部完成）**
Date: 2026-09-19（同日二次修订：UI 策略改判 + 基础服务/wasip3 硬约束；三次修订：范围收敛为桌面端，见「用户决策」）
范围: **仅桌面端**（`bedcode-desktop/` 及其 `plugins/`、`packages/plugin-sdk-desktop/`、`src-tauri/`）；`bedcode-mobile/` 零改动、零验证责任，运行时互通破损按后置专项处理
决策依据: `.scratch/2026-09-10-plugin-kernel-roadmap/spec.md`（阶段 2 / 阶段 3）；`docs/adr/0022`（裁剪线与「双端偏离」节）；`docs/adr/0017`（互调门）；`docs/adr/0019`（wasmtime 双端版本）；AGENTS.md §5（无业务内核红线）、§7（插件检查清单）、§8（认证链路红线）
取代关系: 本规格**取代** `.scratch/2026-09-18-devices-plugin-scope/auth-center-spec.md` §1/§8 中「headless `com.bedcode.devices` 作为独立插件长期存在」的形态判定，以及 roadmap 中「devices / session / terminal 三插件分立」的阶段划分；该规格的 A0（wasi3 + wasmtime 48）与 A（host-auth 原语 + JWT 密钥治理）成果**全部沿用**，不在本规格范围内重做
用户决策（2026-09-19）:
①测试接缝采纳「宿主真实 wasm 闭环 + 既有跨端协议集成测试」为唯一主接缝，前端接缝不动；
②插件**以终端会话为主命名**；
③本次**只下沉语义编排**，PTY 引擎与输出分发管道留内核；
④**界面维持现状**——侧边栏由插件贡献多个目录项承载各域页面，宿主设置页新增**设置分组扩展点**，把与本域相关的配置项拆进插件（推翻同日早先「迁成 toolbox 视图」的判定）；
⑤合并插件**只用宿主基础服务接口（host-\*）实现**，并走 **wasm32-wasip3** 构建与运行时；
⑥**改动范围严格限于桌面端**——移动端仓库一个文件都不改；即使因此出现「移动端调用桌面端」的运行时互通破损，也**先完成桌面端改造**，移动端适配另立后置专项（见「移动端受影响清单」）

---

## Problem Statement

1. **内核带着产品语义**：会话管理器的会话记录里挂着四个任务字段（`task_status` / `task_reason` / `task_updated_at` / `task_questions`），事件广播与移动端会话 DTO 直接消费它们——「会话」这个内核概念实际上认识「Agent 任务」这件事。
2. **一个产品概念被拆成三份、且三份互不认识**：阶段 2 已做出 headless 认证中心（配对码 / QR / 信任 / consent，代码已落地但**未提交**，其 id 与 api 面尚未对外发布）；roadmap 阶段 3 计划再开 `com.bedcode.session` 与 `com.bedcode.terminal`；而 auto-task（rust 7911 行 + TS UI 约 3000 行）事实上已是会话最大的消费者——它建会话、投输入、监听生命周期与提交行、维护「Agent 会话 ↔ 床码会话」映射、把模式与任务状态广播给移动端。三者共享同一批事实（设备指纹、会话 id、任务状态），却各自记账。
3. **每开一个插件都要付一次全链条成本**：ABI bump、双端契约投影、WIT 契约、能力清单、权限域拆分五同步点、打包产物目录、构建脚本白名单、i18n 命名空间、迁移窗口——三次开窗口的代价与三次回归风险，而用户看到的产品形态仍然是「一台被远程设备连接的终端会话主机」。
4. **改一条规则要改内核发版**：配对码 TTL、QR 有效期、会话命名唯一化策略、config→launch 映射（含 WSL 分支）、resize 正统端裁决、自动/手动模式语义——这些纯产品规则今天全部要靠改桌面宿主并重新发布；而设置页今天只有 7 个**写死的**分组组件，插件根本没有把配置搬走的落点。
5. **派生视图口径漂移已经发生**：设备命令面的「已连接设备」里会话数是硬编码 0，而任务与队列状态其实是插件侧的权威数据——同一个「这台设备上有几个会话、在跑什么」的问题，宿主与插件各答一半。
6. **下沉容易但形状失控**：若三域搬迁时各自发明取数通道（前端直调宿主领域命令、给某域单开 WASM 接口），内核就会长出「只服务一个插件的私有门面」，与 ADR 0022 的裁剪线正相反。

## Solution

**一个内置插件 `com.bedcode.session`（终端会话中心）**承载完整产品闭环：*谁能连*（配对 / QR / 信任 / consent）→ *连上之后有什么*（会话配置语义、生命周期编排、设备与会话派生视图）→ *会话里跑什么*（任务队列 / 预设 / 定时任务 / Agent hooks / 自动授权模式）。rust-ts 形态，wasm32-wasip3 后端 + 贡献式前端，**全部能力只经宿主基础服务（host-\* 原语）获取**。

内核退回到引擎位置，**明确不动**的东西：PTY spawn / write / resize / 信号与终止汇聚、输出环与订阅分发管道（含 ack 与背压）、WS 终端通道与控制帧协议（TB v3 帧头、首消息 JWT、resync 语义）、连接注册表与端点表、TLS 私钥与 peer-net 传输引擎、JWT 签发与验签执行点、链路加密与流量过滤链、主库 schema 真源与迁移执行点、wasmtime 运行时。

用户视角的变化：

- **看到的东西不变**：侧边栏目录顺序、页面外观、设置页位置、路由深链、终端窗口手感全部维持今天的形态。变的是**这些界面的渲染方**——各域页面改由插件贡献的侧边栏目录承载，设置页里与配对/会话/任务相关的分组改由插件通过新增的**设置分组扩展点**注入。
- 规则改动（TTL、命名、模式、队列策略、hook 管理）落在插件工程与其贡献的设置分组，不必改内核发版。
- 禁用该插件后：贡献的目录与设置分组自动摘除，宿主兜底壳接管认证与基本会话操作，底座仍可作为 PTY + 网络 + 存储 + 认证引擎跑其他插件。
- 移动端：**本次不动一行**。桌面端改造后移动端的远程调用面可能阶段性不可用（清单见「移动端受影响清单」），但线协议形状保持不变，未来适配的成本被压到只改常量。

## User Stories

### 桌面主机用户

1. 作为桌面主机用户，我希望「设备能不能连我」和「连上之后有哪些会话」在同一处管理，以便不必在设置页、设备页、会话页之间来回跳转才能理解系统状态。
2. 作为桌面主机用户，我希望配对码有效期、QR 有效期、会话命名规则、自动授权模式这些产品规则可在插件贡献的设置分组里改，以便不必等桌面应用发版。
3. 作为桌面主机用户，我希望升级后侧边栏目录、页面位置、外观和快捷键与今天完全一致，以便感受不到这次重构。
4. 作为桌面主机用户，我希望升级后原有配对记录、连接历史、会话配置、任务历史一条不丢，以便升级无感。
5. 作为桌面主机用户，我希望已连接设备列表里能看到该设备真正拥有的会话数与任务状态，以便判断哪台设备在干什么。
6. 作为桌面主机用户，我希望在插件贡献的会话目录里点开后仍能像今天一样进入终端窗口，以便高频终端操作路径不变。
7. 作为桌面主机用户，我希望禁用插件后应用不白屏、认证入口仍在，以便试验插件开关没有心理负担。

### 移动端用户

8. 作为移动端用户，我希望扫码或输码配对完成后，看到的会话列表与桌面端口径一致（含任务状态），以便远程决定要不要新建会话。
9. 作为移动端用户，我希望桌面端改动**不改变线协议形状**（会话 DTO 字段、同步事件、WS 控制帧、握手报文），以便未来适配只是改个基址而不是重做协议。
10. 作为发布经理，我希望「移动端暂不适配」落成逐条可追踪的受损清单，而不是一个被遗忘的默认假设，以便下次移动端立项时有输入而不是从零排查。

### 插件作者 / 第三方应用

11. 作为 auto-task 现有维护者，我希望任务队列与会话状态来自同一份真源，以便不必再自己维护第二套「会话 ↔ Agent」映射账本。
12. 作为 auto-task 现有维护者，我希望「会话创建前注入 Agent 集成（写 hooks）」是我声明的编排步骤，而不是宿主反向回调我，以便流程可读、可单测。
13. 作为 auto-task 现有维护者，我希望 hook 脚本（Claude / Codex / pi / opencode）继续随我的插件打包、落点仍取自身资源目录，以便 Agent 集成与插件版本同进退。
14. 作为 file-transfer 插件作者，我希望 consent 决策与可信对端列表仍来自单一权威 api，以便不因内核认证语义改名而反复改调用点。
15. 作为第三方应用作者（如新的 AI 编码工具插件），我希望通过声明式 api 复用「配对状态 / 会话列表 / 任务模式」，以便不触碰内核即可做出远程终端产品。
16. 作为插件作者，我希望合并插件的 api 面按域分组命名（pairing / trust / consent / session / task），以便 ADR 0017「未声明不可调」的门禁粒度足够细。
17. 作为插件作者，我希望只靠宿主已有的基础服务就能做出完整产品面，以便不需要给内核递「请给我单开一个通道」的需求。
18. 作为插件作者，我希望能在侧边栏贡献多个目录项并与宿主内置菜单共用同一排序空间，以便把不同域摊成独立入口而不打架。
19. 作为插件作者，我希望有正式的设置分组扩展点，以便把配置从宿主设置页搬进我的插件而不必劫持路由。

### 内核维护者 / 安全审计者

20. 作为内核维护者，我希望会话记录里不再有任务语义字段，以便会话结构只描述进程与输出。
21. 作为内核维护者，我希望高频逐帧输出永不进 WASM，以便终端渲染手感不被燃料与 JSON 编解码拖垮（roadmap 阶段 3 性能红线）。
22. 作为内核维护者，我希望会话与设备域的宿主原语在一次 ABI bump 内收敛（而不是 devices 一次、session 一次、terminal 再一次），以便 ABI 版本线可读。
23. 作为内核维护者，我希望新增原语照旧「先权限门再属主」，以便 `session:config` 与 `session:write` 可分别授予与审计。
24. 作为内核维护者，我希望信任记录的存储真源仍在内核（插件经只读/撤销原语访问），以便设备撤销不依赖插件配合也照样生效。
25. 作为内核维护者，我希望宿主设置页只剩「内置引擎分组 + 扩展点」，业务配置一律由插件贡献，以便内核界面不再随产品规则变化而改。
26. 作为内核维护者，我希望合并插件的前端只经 `PluginContext` 与插件命令通道取数，以便宿主领域命令门面不被插件 UI 绕过式依赖。
27. 作为安全审计者，我希望配对码 / QR token / JWT 的签发与校验执行点仍在宿主进程内，以便 WASM 崩溃或被攻破不构成认证绕过。
28. 作为安全审计者，我希望合并插件的权限清单是一份按域分组的显式列表（auth / peer / session:\* / terminal:\* / fs:write / timer / ui:\* / storage / broadcast），以便「一个插件既能写项目 settings.json、又能投 PTY 输入、又能授权配对」是评审时的显式取舍而非合并副作用。
29. 作为安全审计者，我希望凭据在日志与存储中仍只记长度不落明文，以便下沉不降低 AGENTS.md §8 的红线等级。
30. 作为安全审计者，我希望贡献的设置分组仍受同样的权限门约束，以便「UI 由插件渲染」不等于「校验由插件说了算」。

### 运维 / 测试 / 发布

31. 作为运维/打包者，我希望内置产物目录、构建脚本的插件白名单、dev-shell watch 只维护一条「session」线，以便不必再为将退役的 devices id 维护构建分支。
32. 作为测试者，我希望「配对 → JWT → WS → 真 PTY 输出」这条既有跨端链路在重构前后跑同一份断言，以便行为等价是门禁而不是愿望。
33. 作为测试者，我希望合并插件的三域语义在宿主真实 wasm 闭环里断言外部可见结果（DB 行、事件 payload、JSON-RPC 响应、HTTP 响应），以便不测内部函数。
34. 作为测试者，我希望前端配对与终端流程测试的接缝完全不变（仍打在宿主薄转发命令门面上），以便语义下沉不牵连 UI 断言。
35. 作为测试者，我希望双轨并存期有「同一输入 → 搬迁前后同输出」的对照测试，以便迁移正确性可证伪。
36. 作为测试者，我希望设置扩展点与侧边栏贡献有行为测试（注册、排序、缺权限拒绝、停用摘除、兜底壳渲染），以便 UI 下沉的可回退性可验证。
37. 作为桌面前端用户，我希望会话列表的排序、重名处理、状态文案由插件贡献并走 i18n，以便换插件即换产品形态。
38. 作为桌面前端用户，我希望终端预览、尺寸覆盖确认、滚动、写入管线仍留在宿主以保持手感与性能，以便终端不出现 WASM 卡顿。
39. 作为发布经理，我希望阶段 2 剩余工作（命令面桥接 / 退役 / 等价回归 / 文档）在合并形态下一次性收尾，以便不必先把 devices 独立形态做完再拆一遍。
40. 作为 roadmap 维护者，我希望这次「阶段 2+3 合并执行」连同 UI 贡献策略的改判被写成明确决策与理由，以便阶段 4 内核冻结时边界可追溯。

## Implementation Decisions

### D1 插件身份与命名

- 新内置插件 **`com.bedcode.session`**（终端会话中心），`pluginType: rust-ts`，版本从 `1.0.0-beta` 起，`sandbox: inline`，`kind` 保持 **Application**（非 System）——它带 UI 贡献且必须可停可删；系统组件形态否决（会强制先于应用插件激活并失去独立启停）。
- `com.bedcode.devices` **退役**：其 rust 模块（pairing / trust / consent / keys）连同 60 个单测整体搬入合并插件，产物目录、内置资源条目、加载它做闭环测试的用例一并改指新 id。该插件从未提交入库、也未被构建脚本白名单收编，因此退役代价仅为改引用。
- `com.bedcode.auto-task` **退役**：rust 模块（hooks / state / queue / scheduled / agent / preset）与 TS 面（侧边栏任务历史、终端工具栏项、任务弹窗、hook 脚本、构建脚本）搬入合并插件，按域重组为三个同级模块目录，不做逐文件平移。
- 对外契约的 id 迁移：HTTP 基址由 `/api/plugin/com.bedcode.auto-task/...` 改为新 id 前缀，**path 段一个不改**。互调 api 名全部改到新命名空间，file-transfer 的两处消费点（consent 决策、可信对端列表）与认证中心桥接常量同批改指（均在桌面端仓库内）。
- **旧前缀兼容是桌面端单方的免费兜底，不是对移动端的承诺**：宿主插件路由可双前缀同投（约十几行匹配代码，移动端零改动即继续可用）。若实现期判定它会拖慢退役或引入审计歧义，**允许直接切断**——切断后果由「移动端受影响清单」承接，不构成本规格失败。P0 内不排优先级，P4 收尾时按实际成本定。
- 移动端相关的一切（自身 HTTP 基址常量、自身受信任插件白名单种子、移动端 SDK 的 WIT/ABI、移动端构建链）**本规格一律不动**。桌面端内部的 fs_auth 内置白名单种子改指新 id 属桌面侧改动，照做。
- **协议兼容口径（用户 2026-09-19 授权豁免）**：AGENTS.md §9「协议改动必须两端同步部署」在本规格内**豁免**——桌面端可先行破坏移动端可见行为。豁免的边界仍自守：线协议形状（会话 DTO 字段、同步事件、WS 控制帧、认证握手报文）**保持不变**，因为这是后置适配专项的成本基线，且保持它不需要移动端改一行代码。

### D2 实现方式硬约束：只用宿主基础服务 + wasip3

**能力来源收口（红线）**：

- 合并插件后端可达的能力**只能是宿主已有的 20 组 `host-*` 基础服务**；缺口一律按 D3「在既有 interface 上追加函数」补，**禁止**新开 WASM import 私有接口、新开宿主 Tauri 领域命令、或让宿主为这一插件特化。
- 三域的映射（全部落在既有接口内，无新通道）：配对与 QR / `host-auth`（secret-store + 记录面）与 `host-storage`；设备与连接事实 / `host-session.connections-list`（D3 新增函数）与 `host-peer`；会话实例与配置 / `host-session`；输入与输出观测 / `host-terminal` + `terminal-hooks` + `host-pty`（仅插件私有 PTY 场景，业务会话线不混用）；任务与模式广播 / `host-events`（`broadcast-sync` / `emit`）；队列与历史持久化 / `host-plugin-database`；定时 / `host-timer`；写 hooks / `host-fs`；跨插件 / `host-api-call` + `host-bus`；平台选择器 / `host-platform`；日志 / `host-log`；发现 / `host-mdns`。
- **前端侧同样收口**：插件贡献的 UI 只能经 `PluginContext`（`commands` / `session` / `terminal` / `ui` / `events` / `storage` / `http` / `i18n` / `system`）与插件命令通道（宿主以 `plugin_invoke` 路由到本插件 WASM 的 `command.invoke`）取数；**禁止**在插件前端直接 `invoke` 宿主领域命令（`list_sessions`、`generate_pairing_code` 等）——那层门面是留给宿主 UI 的兼容接缝（见 D6）。
- 输入校验与仲裁仍双层：Rust 端（宿主原语权限门 + 属主校验）为最终裁决，插件与前端校验只作 UX（AGENTS.md §7/§8）。

**wasip3 构建与运行时**：

- 构建目标 **`wasm32-wasip3`**，走仓库共享构建配置（`WASM_TARGET` + `wasip3CargoEnv()`，pinned nightly 提供 std；cdylib **直出 Component**，免 componentize 编码步骤）。桌面四个既有插件与 devices 均已切到该 target（阶段 2 前置票 01–03 已 done），因此本规格**不引入新构建链工作**，只要求合并插件沿用同一份配置。
- 运行时 = wasmtime **48** + `wasmtime-wasi` `p3` feature + async store（票 02 门禁已过）。搬迁含义：**auto-task 的同步风格代码在搬迁时一并 async 化**——hooks 扫描、队列 tick、历史统计里的整批文件遍历与长循环必须周期性让出，禁止在 async store 内做无让出的阻塞循环（否则拖垮同实例的配对回调，见 D7）。
- 熵与时钟直接用 `wasi:random` / `wasi:clocks`（0.3 async 形态）：配对码随机数、TTL 与超时判定不再依赖宿主注入；**但**凭据的持久托管仍走 `host-auth` secret-store（WASI 无成熟密钥托管面，阶段 2 已裁决）。
- 燃料与内存限额沿用 07 资源覆盖机制；async 语义下的燃料续费在本规格的批次落 ABI 时复验一次（该批次号见 D4 更正，现记 v19）（阶段 2 已在 A4 验过一轮，本轮只需覆盖新增函数面）。

**合并插件权限清单（manifest `permissions`，17 项，按域分组即审计视图）**：

| 域 | 权限位 | 来源 |
| --- | --- | --- |
| 认证与设备 | `auth` `peer` | devices 现状 |
| 会话 | `session:read` `session:write` **`session:config`（新增）** | 内核现状 + D4 |
| 终端 | `terminal:input` `terminal:observe` `terminal:output` | auto-task 现状 |
| 文件与存储 | `fs:read` `fs:write` `storage` | auto-task / devices 现状 |
| 事件与调度 | `broadcast` `timer:schedule` | auto-task 现状 |
| UI 贡献 | `ui:sidebar` **`ui:settings`（新增）** `ui:input` `ui:dialog` | D6 |

- 新增位共两个：`session:config`（配置 CRUD 与 launch spec 构造，与 `session:read` / `session:write` 并列，走 host_impl 权限门）与 `ui:settings`（纯前端贡献面）。
- `ui:toolbox` **不声明**（本规格不贡献 toolbox 页），`pty:spawn` / `pty:io` / `ws:*` / `network:http` / `process:run` / `mdns` / `system:open` 也不声明——合并插件不碰插件私有 PTY、不自建 WS、不外呼；仅当贡献页面确实需要打开本地目录（历史/日志跳转）时，再按 file-transfer 先例追加 `system:open`。权限清单必须与 D2 的能力映射一一对应，多一项就是审计噪音。
- 能力路由现状不变：`ROUTABLE_CAPABILITIES` 仍只有 `host-storage`；合并插件是原语的**消费方**，不提供可路由能力（若实现期需要宿主中间件向插件取认证策略，走既有 `host-api-call` 门面而非扩表）。

### D3 边界：下沉什么、留什么

下沉进插件的语义：

- 会话配置语义：配置 CRUD 与校验、`environment` / `wsl_distro` 分支、config→launch 映射与默认命令推导、排序与展示组织。
- 生命周期编排：命名唯一化策略、两阶段启动的编排决策、restart / remove 的流程编排、状态推进顺序与对外事件形状。
- 尺寸裁决：正统端（canonical renderer）仲裁规则——裁决逻辑归插件，「谁是当前渲染端」的登记事实留内核。
- 设备域：配对码与 QR 的生命周期编排与 TTL 策略、已配对设备与连接历史的列表组织、派生视图（在线判定 + 会话数 + 任务状态合并）、consent 决策编排。
- 任务域：任务队列与状态机、预设任务、定时任务与恢复、模式（自动执行 / 自动应答）持久化与广播、Agent hooks 安装与清理、会话↔Agent 映射。
- UI 渲染方：会话列表/详情业务列、配对页、已配对设备、连接历史、任务历史与任务弹窗、终端工具栏项，以及贡献的设置分组（见 D6）。

**明确留内核**（红线，越线需重新裁决）：

- PTY 引擎（进程状态、slave fd 策略、终止汇聚、裸命令参数 exec、特殊键写入）。
- 输出分发管道（统一输出队列、订阅句柄窗口与 ack、全局输出管理器、历史快照与 resync 偏移）——**逐帧输出不进 WASM**。
- WS 连接骨架（心跳、首消息认证策略、帧过滤链、注册表与端点表、优雅停机）与终端控制帧协议。
- JWT 签发与验签执行点、密钥托管（`host-auth` secret-store 现状）、TLS 私钥、链路加密、流量过滤链、设备身份与节点身份生成与持久化。
- `pairings` / `connection_history` 表（真源在内核）与主库 schema 与幂等迁移执行点。
- 会话状态检测（WaitingInput 判定、ANSI 解析）作为「会话状态机引擎」的一部分留内核（roadmap 阶段 3「不动」列）；插件侧只做状态推进与广播。
- 终端渲染管线（预览组件、写入管线、renderer、resize debounce、滚动、IME 守卫）、终端窗口与宿主布局骨架（侧边栏容器、标题栏、页面工具栏、设置页外壳）。

### D4 WIT / ABI 契约增量（一次 bump：desktop 17 → 19）

> **2026-09-19 实施期更正（原表述「16 → 17」已失效）**：并发线在本规格开工前已把 desktop ABI 用掉两号——
> **v17 = `auth-policy` 导出**（认证中心票 12：宿主 server 中间件验签后向插件取策略）、
> **v18 = `host-notification`**（贡献式通知面板）。二者与本规格的会话语义追加批次互不相干，
> 故本规格的「一次 bump」窗口顺延为 **17 → 19**（票 07 起落 `host-session` / `host-auth` 函数级追加）。
> 双端偏离口径不变：v19 仍属桌面独有，移动端 SDK 不跟演（mobile 11）。
> 教训沿用 AGENTS.md §7：**ABI 号是共享序列，任何票开工前须先读 `abi.rs` 现状而非规格正文**。

全部为**既有 interface 的函数级追加**，不新增 interface：

- `host-session` 追加：
  - `config-upsert` / `config-get` / `config-delete`（配置 CRUD，权限 `session:config`）
  - `create-with-spec`（接收插件算好的 launch spec-json `{command, args, cwd, cols, rows, env, name}`；宿主只做 shell 包装 / WSL 转换 / 尺寸缺省 / ID 预生成，权限 `session:write`）——把「映射决策」交给插件、把「执行」留内核的关键切口
  - `restart` / `remove` / `rename`（补齐插件今天做不到的三个动作，权限 `session:write`）
  - `resize`（带请求端标识；裁决规则在插件、事实登记在内核，权限 `session:write`）
  - `connections-list`（连接注册表原始记录 addr / device_id / fingerprint，无排序无解读；替代今天硬编码 0 的会话数来源，权限 `session:read`）
  - `annotate`（会话注解槽写入，见 D5，权限 `session:write`）
- `host-auth` 追加只读/撤销面：`trusted-devices-list` / `trusted-device-revoke` / `connection-history-list` / `auth-setting-set`（认证域设置项写入，读取沿用 `host-config`）。定位仍是无业务语义的凭据与记录存取，符合 ADR 0022 裁剪线。
- **不新增** `host-session-output`（订阅/ack 原语）——输出订阅插件化被显式否决（性能红线）。
- 复用现状：`host-pty`（v16，插件私有 PTY，与会话线零交叉）、`host-terminal.send`、`host-peer`、`host-mdns`、`host-websocket`、`host-bus`、`host-api-call`、`host-plugin-database`、`host-database`、`host-storage`、`host-config`、`host-events`、`host-fs`、`host-timer`、`host-log`、`host-platform`。
- 双端偏离：本规格的批次（v19，见 D4 更正）属**桌面独有**会话语义下沉（与 host-websocket v14 / host-auth v15 / host-pty v16 同列，ADR 0022「双端偏离」节）——移动端 SDK **不跟演也不投影**，WIT / ABI / 构建链全部保持现状（mobile 11），移动端仓库零改动；未来移动端要接同类能力时再补该端 interface 并对齐计数。
- 扩展点方向：`terminal-hooks` / `on-session-lifecycle` / `on-input-submitted` 三处反向回调**保留为兼容面**，但合并插件改为「自己作为会话编排方主动推进」，不再依赖宿主回调注入 hooks——迁移期两套并存，P4 之后按实际使用裁决是否退役（默认保留，成本为零）。

### D5 内核去业务化的落地形状

- 会话记录的四个任务字段从内核结构体摘除，替换为**不透明注解槽**：内核只按 `session-id → map<string,string>` 搬运与透传，绝不解释键名。
- 线协议不变：移动端会话 DTO 与同步事件里的 `taskStatus` 等字段继续原样输出，值由插件经 `annotate` 写入后由内核透传——**老端零改动**，字段演进遵循「老端忽略未知字段」。
- 配置存储归属：会话配置从主库表迁入插件私有库（`host-plugin-database`），主库旧表保留一版并只读退役；搬迁走**一次性幂等迁移**（可对旧库重跑，先例是 peer 数据向插件存储键的迁移模块），配套补迁移幂等测试。
- 任务与队列数据继续留在插件私有库现有六张表，但表名随域重组（`session_*` / `task_*` 前缀统一），需要一次表级重命名迁移窗口。

### D6 UI 策略：界面维持，贡献方换人

**总原则**：像素级不变、代码归属变。所有 UI 改动按 AGENTS.md §6 强制先加载 `frontend-styles` skill，并遵守其 token 与组件规范。

- **侧边栏多目录贡献**：合并插件在 manifest `contributes.views` 声明多个 `type: sidebar` 目录项，运行期以 `ui.registerSidebarPanel` 注册（权限 `ui:sidebar`，已存在）。描述符字段沿用现状：`id` / `title` / `icon`（Heroicons outline、`stroke-width=2`、`viewBox=0 0 24 24`，与宿主内置菜单同一图标体系）/ `order` / `component`。
  - `order` 必须落在今天的槽位上以保持菜单顺序。实际排序空间（内置菜单常量为准，间隔 100 供插入）：设备配对 **100**、终端会话 **200**、服务器 300（保留不复用，防撞位）、插件管理 9998、设置 9999——后两者恒在最末。因此合并插件贡献项取：设备与配对 100 段、连接历史紧随其后（101+）、会话列表 200 段、任务历史保持其今天的相对位置。
    > 漂移待修：`SidebarPanelDescriptor.order` 的注释仍写着「终端会话 100 / 服务器 200 / 设备 300 / 插件 400 / 设置 700」，与内置常量不一致——P0 一并改正，避免实现者照注释选槽。
  - 贡献目录的宿主路由由注册表统一派生（`/plugin/sidebar/<pluginId>/<viewId>`），插件页 id 与内置菜单 id **同一命名空间去重**：宿主对应的内置入口（设备配对、终端会话）在插件激活时摘除，退为深链跳转壳，避免同域出现两个入口；未激活 / error / 停用时宿主原页面照常渲染（兜底）。
  - **由此产生的第二个内核小改动**：内置菜单项需能按「该域贡献插件是否处于 Activated」条件显隐——读插件注册表现有状态即可，不新增 API、不新增权限；实现要与 D7 的摘除/恢复路径共用同一判据（避免菜单与页面状态各判一次）。
- **页面搬迁**：现有会话列表/详情、设备与配对、连接历史、任务历史四块视图组件整体搬进插件 `src/`，宿主只保留容器与外壳；插件前端经共享模块运行时（`__BEDCODE_SHARED__`）复用宿主的 Vue / Pinia / vue-i18n，**禁止自带第二份运行时**；取数只走 D2 的前端收口路径。
- **终端窗口不进插件**：从插件贡献的会话页进入终端 = 触发宿主既有路由深链（入口行为不变）；终端窗口、预览组件、渲染与输出管线、resize 覆盖确认全部留宿主（性能红线 + 手感）。
- **设置页扩展点（与上面的内置入口条件显隐，合成本规格仅有的两处内核 UI 改动）**：
  - 前端 API：`ui.registerSettingsSection({ id, titleKey, icon?, order, component })` → 新权限位 **`ui:settings`**。它是**纯前端贡献面权限**（无 WASM 宿主函数对应），因此同步点是「SDK 权限常量 + 前端合法集合与 `ui.*` API 映射 + 打包 CLI 的 manifest 权限校验」三处，而非 host_impl 权限门那一路（别漏改，否则 manifest 声明即被拒）。
  - 宿主改造：设置页从「7 个写死的分组组件」改为「宿主内置分组 + 注册表分组按 `order` 合并渲染」；共享状态（语言选项、动画开关、`lang-fade` 容器交互）由设置页父级持有并下传，分组子组件不得自行推导（沿用 2026-09-17 拆分教训）。
  - 分组退役：宿主设置页的**配对分组**整体删除，改由合并插件贡献「终端会话与设备」分组，承载配对码 TTL / QR 有效期 / 会话默认值 / 任务模式默认 / hooks 管理与全局 hook 清理入口。
  - **两层配置形态，声明式优先**：能用 JSON Schema 表达的标量项走既有插件配置面（宿主插件配置页按 `configSchema.properties` 渲染，值落 `host-storage`）；只有需要交互控件的场景（配对码展示与倒计时、QR 生成、hook 列表管理、设备撤销确认）才用组件式 settings section。这条既压内核扩展点的复杂度，也让审计口径清晰。
  - 校验不降级：贡献的分组内任何写入仍经插件命令通道到 WASM，再由宿主原语权限门仲裁；「UI 由插件渲染」不等于「校验由插件说了算」。
- **i18n**：宿主 `desktop.device.*`、`settings.pairing.*` 分组随迁移进入插件文案；插件侧扁平 key（auto-task 现状）**必须先加命名空间前缀**（`session.*` / `task.*` / `pairing.*` / `settings.session.*`），否则与既有 `hub.*` 风格在同一注册表后写覆盖。宿主与插件两侧 key 均须 zh-CN / en 双文件同步。

### D7 合并代价与补偿（错误隔离是最大项）

三域同实例后，配对侧 trap 会连带终端回调与调度 tick 一起停。补偿四条，属本规格硬性验收：

- 认证路径**保留宿主兜底**：合并插件未激活 / 探活超时（现桥接为 5s）时，宿主命令面走降级路径并 `warn` 留痕——双轨并存期不允许出现认证单点。
- WASM 侧按域收口：三域各自 `Result` 边界，禁止跨域持锁（配对不得阻塞任务 tick），`timer_register` 回调按命令名分域并各自计数失败，失败只降级本域；async store 下的长循环必须让出（D2）。
- 生命周期分段：activate 顺序为「建表 → 订阅 → 恢复（定时任务 / 注解重放）」，任一步失败落 `Degraded` 而非 Activated，保持 v8 起「导出返回值如实上抛」的约定。
- **UI 故障半径补偿**：贡献的侧边栏目录与设置分组在插件进入 error 态时由注册表统一摘除，宿主兜底壳接管（不白屏、不残留空目录）；摘除与恢复路径须有行为测试（见 Testing Decisions）。

### D8 分步（纵向 tracer-bullet，P0 为前置 prefactor）

- **P0 prefactor**（零行为变化）：权限域拆分与同步点、i18n 命名空间前缀、HTTP 双前缀路由（**可选桌面侧兜底**，见 D1）、构建脚本与产物目录收编新插件、**设置分组扩展点落地（`ui:settings` + registry + 设置页合并渲染，此时仍由宿主分组占位）**。
- **P1 第一条贯穿线**：新插件骨架（wasip3 产物 + async 编排）+ devices 三模块搬迁 + 认证桥接改指新 id + devices 退役；判据 = 「生成配对码 → **以客户端形态发 HTTP 请求**校验 → 已配对设备列表 → 撤销」在宿主真实 wasm 闭环 + 桌面端协议集成测试双绿（客户端由测试模拟，不需要真机移动端参与）。
- **P2 会话语义**：D4 的 `host-session` / `host-auth` 函数追加（v19，见 D4 更正）+ 配置 CRUD / 命名 / launch spec / restart / remove / resize 裁决 / connections-list 下沉 + 注解槽替换任务字段 + 配置表迁私有库（含幂等迁移测试）。
- **P3 UI 贡献切换**：设置页配对分组退役 → 插件贡献分组接管；四块视图搬入插件并以侧边栏多目录注册（按实际排序常量取槽）；宿主内置入口按「贡献插件是否 Activated」条件显隐并退为跳转壳；共享状态与 i18n 命名空间收口；`frontend-styles` 自查通过。
- **P4 任务域并入**：auto-task 六模块搬迁与 **async 化**、私有库表前缀重组与重命名迁移、hook 脚本资源目录接线、`com.bedcode.auto-task` 旧 api/权限清单退役。
- **P5 回归与文档**：三域端到端等价回归、roadmap 阶段 2/3 标记与合并决策补记（**含「移动端受影响清单」就地挂档 + §9 双端同步豁免留痕**）、ADR 0022 补记（会话语义下沉 + 注解槽 + 设置分组扩展点 + 双端偏离表加本规格批次号）、AGENTS.md §7 ABI 计数与 §8 认证语义措辞、**桌面端** code-map（移动端 code-map 因其代码未变，不动）、CHANGELOG 记条目但移动端版本号不动。

## Testing Decisions

**什么算好测试**：只断言外部可见行为——DB 行、事件 payload、JSON-RPC 响应体、HTTP 响应体、协议帧字段、注册表可见项；不测内部函数、不以 mock 替代真实原语、不用快照替代行为断言、不为覆盖率补测（`unit-test-discipline` 门禁 G1–G6 全适用）。

**接缝（采纳用户确认：不新增接缝）**：

- **S1 宿主真实 wasm 闭环（主接缝）**：加载合并插件真实产物（wasip3 组件），经真实 `host_impl` 原语与临时目录数据库，断言外部结果。先例即仓库既有矩阵——host-pty 闭环矩阵、host-ws 闭环矩阵、devices 产物生命周期用例、配对桥接闭环用例；本规格新增 `test_session_*` 矩阵（配置 CRUD 成功闭环、`create-with-spec` 参数传递、`annotate` 透传、`connections-list` 属主隔离、resize 裁决）。**已知缺口必须补**：现有 `host-session` 侧 12 个单测全是权限/参数门，没有一条成功闭环。
- **S2 桌面端跨模块协议集成测试（最高层，已存在）**：桌面 `src-tauri/tests/` 下的 PTY 会话全链路用例（配对 → JWT → WS → 真 PTY 输出）、WS 会话路由、WS 认证规则、HTTP 生物认证契约。**这些是本规格的门禁**。移动端自身的 `http_auth_flow` / `ws_protocol_integration` 等用例**不作为门禁、不修改、不为其调整桌面端设计**——它们测的是移动端自身，桌面端改动不该让它们变红；一旦变红，判定标准是「是否误触了 `packages/peer-net` / `packages/link-crypto` 这类共享 crate」，那属范围违规，须先停下确认。线协议形状保持不变是 D1 的自守边界：如果 S2 里某条断言必须改才过，视为破坏协议，回到用户处重议而非改测试。
- **S3 前端 vitest（接缝不动 + 新增贡献面测试）**：配对流程与终端流程集成测试仍以宿主命令名为接缝（薄转发门面），预期零改判；新增（a）侧边栏目录贡献测试——注册项、`order` 合序、与内置项去重、缺 `ui:sidebar` 权限拒绝、停用后摘除；（b）设置分组扩展点测试——注册/排序/权限门/插件 error 态兜底壳；（c）插件视图测试写在插件工程内（沿用 agent-hub 的 devMock 先例），并把插件目录纳入 vitest include（当前缺 auto-task，是必须补的覆盖缺口）。
- **S4 插件 crate 单测**：纯策略与状态机——配对码 TTL 边界与一次性消费、QR token 语义、claims 组织、consent 决策正反例、命名唯一化冲突、config→launch 映射分支、任务队列 `pending→waiting→executing→done|cancelled` 全迁移（含尝试上限与静默超时）、定时任务 `creating` 恢复、resize 裁决矩阵、JWT HS256 官方向量（沿用现有对照向量测试）。
- **双轨对照**（P1–P4 并存期强制）：同一输入下「搬迁前宿主实现」与「插件实现」输出逐字段相等——这是把「行为等价」从形容词变成断言的唯一手段。

**门禁**（AGENTS.md §10 的桌面端子集）：改 Rust → **桌面端** `cargo test` 为门禁（移动端零改动，其测试不计入本规格门禁）；改前端 → 桌面端 `pnpm run test:run` + 根 `pnpm exec eslint .` 0 error；i18n key zh-CN / en 双文件同步（指桌面端的两份 locale）；UI 改动过 `frontend-styles` 自查；每轮测试后清理测试 spawn 的后台进程与端口。CI（`test.yml` 合并到 master/uat 时跑双端）仍会跑移动端——它是**范围守卫**而非待适配项：变红即说明本次改动越界碰了共享 crate，按 D1 的范围违规处置。

## Out of Scope

- **终端渲染与输出管线插件化**（预览组件、写入管线、renderer、滚动、IME、输出订阅与 ack）、**终端窗口本体进插件**——roadmap 阶段 3 性能红线。
- **toolbox 页面形态**：同日早先判定已被推翻，本规格用侧边栏多目录 + 设置分组扩展点承载贡献 UI，不新增 toolbox 页。
- **WS 终端通道与控制帧搬进插件**——传输引擎与安全闸门红线。
- **移动端一切改动与适配**（用户决策 ⑥）：不改移动端任何文件、不做移动端的桌面适配、不为移动端保留设计余地。移动端 auto-task 插件仍是独立 TS 实现（其 rust 面基本为空壳），移动端 SDK 不跟演本规格的会话语义批次（v19）、不切 wasip3 构建链、不新建会话中心插件，移动端设置面与路由不动。延续 roadmap 阶段 2 已确立的「仅桌面端先行，移动端暂停推进」，本规格把该口径从「不迁移移动端」收紧为「不为移动端负责」。
- **`com.bedcode.terminal` 独立插件**（roadmap 阶段 3 的另一半）。
- **密码学引擎 / 密钥托管 / JWT 验签执行点 / TLS / 链路加密 / 流量过滤链 / 连接注册表 / 设备与节点身份**下沉。
- **保留 devices 为独立 headless 认证中心**的路线（用户已选合并）。
- **wasi3 之外的运行时演进**与 wasmtime 版本再升级（阶段 2 前置工程 A0 已落地，桌面已切 wasip3 构建链）。
- **快捷操作（quick_actions）**归属裁决：阶段 2 摸底遗留，仍不在本规格随迁，另票处理。
- **旧 HTTP 前缀与旧路由壳的长期维持**：二者都是可牺牲的桌面侧兜底（D1），切断时机随移动端适配专项一并决定，本规格不承诺窗口长度。

## 移动端受影响清单（本规格不修，作为后置适配专项的输入）

桌面端改造完成后，以下移动端可见行为会受损。逐条**已知且被授权**（用户决策 ⑥），P5 文档步骤须把本清单挂进 roadmap 阶段 2/3 的标记处，不许只留在规格里。

| # | 受损项 | 触发条件 | 未来适配动作（不在本规格） |
| --- | --- | --- | --- |
| M1 | 移动端任务面板全部 HTTP 调用（task-status / session-mode / session-settings / task-history / supported-agents / task-queue / scheduled-jobs）整体 404 —— **票 17 判定保留旧前缀兜底，本条暂未触发**（旧插件已退役，但 `LEGACY_HTTP_PLUGIN_ALIASES` 仍把旧前缀转给合并插件，移动端零改动继续可用） | 桌面端**将来**切断 `com.bedcode.auto-task` 旧前缀时（D1 允许；票 17 明确不切，时机随移动端适配专项） | 改移动端 api 基址常量一处 + 移动端插件重打包，**同批**删宿主别名表与 `resolve_http_owner` |
| M2 | 会话列表的 `taskStatus` 等字段值变空或延迟 | 桌面端合并插件未激活 / error 态，注解槽无人写 | 移动端对空注解做降级显示（不阻塞连接） |
| M3 | 配对码 / QR 的有效期展示与实际 TTL 不一致 | TTL 真源进了插件贡献设置分组，未激活时走宿主兜底默认 | 移动端读认证域设置项的取值路径复核一次 |
| M4 | 移动端插件无法调用 `host-session` 新函数（v19 批次） | 移动端 SDK 不跟演（WIT 无该接口，ABI 11） | 移动端真要接同类能力时补该端 interface + host_impl + 计数对齐 |
| M5 | 「撤销已配对设备」仍不断开在线连接 | 本规格刻意保持宿主现状语义（只置 `is_active=0` + 删历史） | 若要撤销即踢下线，是新协议工作，需双端立项 |

- **不属于受损项**（避免误判）：移动端自身的 fs_auth 内置受信任插件白名单（针对移动端自己打包的插件，与桌面端 id 无关）、移动端 `pairings` 语义、移动端与桌面端的 mDNS 服务类型与链路加密握手——本规格都不触碰。
- 版本号规则（AGENTS.md §2「双端同步维护」）在单端功能变更下仍需记 `CHANGELOG.md`，但**移动端 APK 不随本次重发**，其版本号保持不动。

## Further Notes

- **与 roadmap 的张力要显式承认**：roadmap 渐进原则第 1、2 条（每阶段只做一件事、每步可回退）被本规格主动打破——阶段 2 与阶段 3 的一部分合并执行。收益是省掉两次 ABI 开窗、两次 UI 迁移、两次 i18n 冲突处理，并把「设备—会话—任务」事实三元组收敛到单一权威；代价是 P1–P4 回归面变宽。风险由 D8 纵向切片与 S1/S2 既有接缝吸收，而非由流程假设吸收。
- **合并的真实痛点在 D7**，不在工作量：三域同实例后故障半径从「一个插件」扩到「整个会话产品面」。若实现期发现认证兜底与桥接超时互相放大，回到用户处重议「devices 留独立 headless、只并会话与任务」这一备选。
- **UI 改判的净效果**：内核多做一件事（设置分组扩展点），少做两件事（toolbox 页迁移、宿主页面重构）；「界面维持」把回归风险从视觉层挪到了归属层，因此 S3 的贡献面测试（注册/排序/去重/摘除）是这次改判的必要配套，不可省。
- **实现期需用户确认的 2 个开放点**：①注解槽 vs 宿主 DTO 组装时反向调用插件 api 取任务字段（推荐槽：内核零语义、无热路径额外调用）；②设置分组扩展点的粒度（一个合并分组 vs 按域三个分组，建议先一个，P3 验收后按需拆）。旧前缀兼容窗口的长度问题已随决策 ⑥消解：不再需要对移动端承诺，只按桌面侧成本择机切断。
- **票 17 的三处当场裁决（contract 步收口，实施记录见 `issues/17-*`）**：①旧 HTTP 前缀
  **保留**别名兜底，切断时机并入移动端适配专项（M1 因此未触发）；②`manifest-gen` 的
  命令面口径取**人工裁剪优先**——`invoke_command` 的匹配臂含宿主桥接与闭环调试入口，
  全量覆写等于把内部接缝 advertise 成产品命令，故已声明 commands 的插件生成器只报告
  差集不写回；③权限清单按**实际消费者**收口为 15 项，D2 表里的 `terminal:output` 与
  `ui:dialog` 前后端均查无调用点（旧 auto-task 那两份是死声明），按「多一项就是审计
  噪音」不声明，偏离 D2 表格一事记在票面。
- **范围豁免要留痕**：本规格经用户 2026-09-19 指令豁免 AGENTS.md §9「协议改动两端同步部署」与 roadmap「移动端暂停推进」的两处既有口径（豁免仅限本规格）。P5 文档步骤须把豁免写进 roadmap 阶段 2/3 标记与 ADR 0022 双端偏离节，避免下一个读者以为还能假设双端齐步。
- **不要先做票 11–14 的原始形态**：阶段 2 剩余四张票（命令面桥接、中间件、命令面退役、等价回归）在合并形态下需重定义——桥接目标改指新 id；「命令面退役」改判为「门面保留、实现改转发」，因为前端接缝就打在门面上。
- 拆票已发布：`issues/01..18`（18 张纵向票，编号即依赖序）。与本节 P0–P5 的对应关系：P0 → 01/02/03，P1 → 04/05/06，P2 → 07/08/09/10/11/12，P3 → 13/14，P4 → 15/16/17，P5 → 18。三处宽改造按 expand–contract 排程：插件 id 改名（04/05 → 06、15/16 → 17）、任务字段→注解槽（11 → 12）、旧 HTTP 前缀（16 落双投 → 17 判定切断）。

---

## 附：参考位置（迁移线索，正文刻意少带路径）

| 域 | 现在在哪 |
| --- | --- |
| 配对码 / QR / JWT / 生物凭证 | `bedcode-desktop/src-tauri/src/utils/auth/`（`pairing.rs`、`qr_token.rs`、`jwt.rs`、`biometric.rs`、`host_secrets.rs`、`auth_center.rs`〔未跟踪，桥接层〕） |
| 配对编排与设备视图 | `src-tauri/src/commands/system.rs`（配对码 4 命令已转发、TTL 与 pairings/history 仍宿主）、`commands/qr.rs`、`commands/devices.rs`（`session_count` 硬编码 0）、`server/services/pairing_service.rs`（内存态码与 pending 设备）、`server/controllers/auth_controller.rs` |
| 会话语义 | `src-tauri/src/session/`（`session_manager.rs` 编排不变量、`session_components.rs` 命名/映射/状态检测、`session_output.rs` 输出环〔不动〕、`session_lifecycle.rs`、`input_line.rs`、`session_event.rs` 四个任务字段）、`commands/session*.rs`、`enums/{session,summary,sync}.rs`、`server/dtos/session_dto.rs`、`server/controllers/session_controller.rs`、`server/services/session_{control,sub}.rs` |
| 会话传输引擎（不动） | `server/ws/`（`conn.rs`、`subscription.rs`、`registry.rs`、`terminal_ws/`、`control_frame.rs`、`message.rs`）、`pty/` |
| 宿主原语实现 | `plugin/manager/wasm_runtime/host_impl/`（`session.rs`、`lifecycle.rs`、`terminal.rs`、`pty.rs`、`auth.rs`〔未跟踪〕、`peer.rs`、`ws.rs`）、`plugin/manager/capability.rs`（20 组清单 + `ROUTABLE_CAPABILITIES` 仅 host-storage） |
| WIT 与 SDK | `packages/plugin-sdk-desktop/rust/wit/bedcode.wit`（`host-session`:322、`host-auth`:369、`host-pty`:403、`terminal-hooks`:482、`events`:458）、`rust/src/abi.rs`（**票 05 实读**：`ABI_VERSION = 17`，v18 在本分支空闲（notification 线未落树）→ 本批次 host-auth 记录面落 **v18**，见票 05 Comments「ABI 编号」）、`rust/src/permission.rs`、`bin/cli.js` + `bin/manifest-gen.js` |
| wasip3 构建链 | `scripts/plugin-wasm-config.mjs`（`WASM_TARGET` / `wasip3CargoEnv()`）、各插件 `scripts/build.js`（票 03 注释：cdylib 直出 Component）、`src-tauri/Cargo.toml`（wasmtime 48 + `wasmtime-wasi` p3）、`wasm_runtime/component.rs`（async store 接线） |
| 待并入的两个插件 | `plugins/auth-center/`（**票 05 现状**：原 `plugins/devices/` 已改名，`rust/src/pairing/*` + `lib.rs` 共 32 测试，headless，api 9 项，permissions `auth`；trust / consent / policy 已迁出至 `plugins/session/`）、`plugins/auto-task/`（`rust/src/{lib,hooks,state,queue,scheduled,agent,preset}.rs` 7911 行、`src/` TS + i18n、`scripts/` 5 个 hook 脚本、28 commands / 12 permissions / 1 sidebar view） |
| UI 贡献面 | `src/plugin/{types,context,registry,permission}.ts`（`SidebarPanelDescriptor`、`registerSidebarPanel`、`ui:toolbox` 等合法集合）、`src/composables/useSidebarMenu.ts`（内置菜单 `BUILTIN_MENU_ORDERS` 与插件视图合流、`/plugin/sidebar/*` 路由）、`src/views/SettingsView.vue`（静态 7 分组，D6 改造点）、`src/components/settings/SettingsPairingSection.vue`（退役对象）、`src/views/PluginConfigView.vue`（`configSchema` 声明式配置面，两层配置的落点） |
| 前端消费面 | `src/composables/commands/{deviceCommands,sessionCommands}.ts`、`src/views/{SessionsConfigView,TerminalWindowView,DevicesView,ConnectionHistoryView}.vue`、`src/components/{SessionForm,TerminalPreview}.vue`、`src/stores/{session,device}.ts`、`src/composables/{usePairing,useSessionWindows,useConnectedDevices}.ts`、`src/router/`、`src/plugin/commands.ts:173`（`plugin_invoke` 通道） |
| 移动端耦合点 | `bedcode-mobile/plugins/auto-task/src/{api.ts,index.ts}`（HTTP 基址 `/api/plugin/com.bedcode.auto-task`）、`bedcode-mobile/src-tauri/src/lib.rs:181`（内置受信任插件白名单）、`bedcode-mobile/packages/plugin-sdk-mobile/rust/{src/abi.rs,wit/bedcode.wit}`（ABI 11，不跟演） |
| 测试接缝先例 | `plugin/manager/wasm_runtime.rs`（pty / ws 闭环矩阵、devices 产物生命周期、配对桥接闭环）、`src-tauri/tests/{pty_session_chain,ws_session_route,ws_auth_rules,http_auth_biometric,server_integration}.rs`、`bedcode-mobile/src-tauri/tests/{http_auth_flow,ws_protocol_integration}.rs`、`bedcode-desktop/src/__tests__/integration/{pairing-flow,terminal-flow}.test.ts`、`src/__tests__/fixtures/{pairing,session}.ts` |
| 上游文档 | `.scratch/2026-09-18-devices-plugin-scope/{auth-center-spec.md,mapping.md,issues/01..14}`、`.scratch/2026-09-10-plugin-kernel-roadmap/spec.md`、`.scratch/2026-09-19-pty-base-service/spec.md`、`docs/adr/0017`、`0019`、`0022` |
