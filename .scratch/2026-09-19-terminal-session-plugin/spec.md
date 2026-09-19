# 终端会话中心插件（com.bedcode.session）实施规格 —— 配对 + 会话语义下沉并与 auto-task 合并

Status: ready-for-agent
Date: 2026-09-19
决策依据: `.scratch/2026-09-10-plugin-kernel-roadmap/spec.md`（阶段 2 / 阶段 3）；`docs/adr/0022`（裁剪线与「双端偏离」节）；`docs/adr/0017`（互调门）；`docs/adr/0019`（wasmtime 双端版本）；AGENTS.md §5（无业务内核红线）、§7（插件检查清单）、§8（认证链路红线）
取代关系: 本规格**取代** `.scratch/2026-09-18-devices-plugin-scope/auth-center-spec.md` §1/§8 中「headless `com.bedcode.devices` 作为独立插件长期存在」的形态判定，以及 roadmap 中「devices / session / terminal 三插件分立」的阶段划分；该规格的 A0（wasi3 + wasmtime 48）与 A（host-auth 原语 + JWT 密钥治理）成果**全部沿用**，不在本规格范围内重做
用户决策（2026-09-19）: ①测试接缝采纳「宿主真实 wasm 闭环 + 既有跨端协议集成测试」为唯一主接缝，前端接缝不动；②插件**以终端会话为主命名**；③本次**只下沉语义编排**，PTY 引擎与输出分发管道留内核；④配对 UI **迁成插件贡献的 toolbox 视图**

---

## Problem Statement

1. **内核带着产品语义**：会话管理器的会话记录里挂着四个任务字段（`task_status` / `task_reason` / `task_updated_at` / `task_questions`），事件广播与移动端会话 DTO 直接消费它们——「会话」这个内核概念实际上认识「Agent 任务」这件事。
2. **一个产品概念被拆成三份、且三份互不认识**：阶段 2 已做出 headless 认证中心（配对码 / QR / 信任 / consent，代码已落地但**未提交**，其 id 与 api 面尚未被外部发布）；roadmap 阶段 3 计划再开 `com.bedcode.session` 与 `com.bedcode.terminal`；而 auto-task（rust 7911 行 + TS UI 约 3000 行）事实上已经是会话最大的消费者——它建会话、投输入、监听生命周期与提交行、维护「Agent 会话 ↔ 床码会话」映射、把模式与任务状态广播给移动端。三者共享同一批事实（设备指纹、会话 id、任务状态），却各自记账。
3. **每开一个插件都要付一次全链条成本**：ABI bump、双端契约投影、WIT 契约、能力清单、权限域拆分五同步点、打包产物目录、构建脚本白名单、i18n 命名空间、迁移窗口——三次开窗口的代价与三次回归风险，而用户看到的产品形态仍然是「一台被远程设备连接的终端会话主机」。
4. **改一条规则要改内核发版**：配对码 TTL、QR 有效期、会话命名唯一化策略、config→launch 映射（含 WSL 分支）、resize 正统端裁决、自动/手动模式语义——这些纯产品规则今天全部要靠改桌面宿主并重新发布。
5. **派生视图口径漂移已经发生**：设备命令面的「已连接设备」里会话数是硬编码 0，而任务与队列状态其实是插件侧的权威数据——同一个「这台设备上有几个会话、在跑什么」的问题，宿主与插件各答一半。

## Solution

**一个内置插件 `com.bedcode.session`（终端会话中心）**承载完整产品闭环：*谁能连*（配对 / QR / 信任 / consent）→ *连上之后有什么*（会话配置语义、生命周期编排、设备与会话派生视图）→ *会话里跑什么*（任务队列 / 预设 / 定时任务 / Agent hooks / 自动授权模式）。rust-ts 形态、带 UI 贡献。

内核退回到引擎位置，**明确不动**的东西：PTY spawn / write / resize / 信号与终止汇聚、输出环与订阅分发管道（含 ack 与背压）、WS 终端通道与控制帧协议（TB v3 帧头、首消息 JWT、resync 语义）、连接注册表与端点表、TLS 私钥与 peer-net 传输引擎、JWT 签发与验签执行点、链路加密与流量过滤链、主库 schema 真源与迁移执行点、wasmtime 运行时。

用户视角的变化：

- 设置页的「配对」分组消失，配对、已配对设备、连接历史与会话列表、任务历史收敛为插件贡献的 toolbox 页面（同一入口、同一份状态）。
- 规则改动（TTL、命名、模式、队列策略）落在插件工程，不必改内核发版。
- 禁用该插件后，底座仍可作为 PTY + 网络 + 存储 + 认证引擎跑其他插件（终端会话这一产品形态整体消失，内核不留残骸）。
- 移动端：HTTP 端点换基址但保留一版旧路径兼容；线协议（会话 DTO、同步事件、控制帧）零破坏性变更。

## User Stories

1. 作为桌面主机用户，我希望「设备能不能连我」和「连上之后有哪些会话」在同一处管理，以便不必在设置页、设备页、会话页之间来回跳转才能理解系统状态。
2. 作为桌面主机用户，我希望配对码有效期、QR 有效期、会话命名规则、自动授权模式这些产品规则可在插件设置里改，以便不必等桌面应用发版。
3. 作为桌面主机用户，我希望升级后原有配对记录、连接历史、会话配置、任务历史一条不丢，以便升级无感。
4. 作为桌面主机用户，我希望已连接设备列表里能看到该设备真正拥有的会话数与任务状态，以便判断哪台设备在干什么。
5. 作为移动端用户，我希望扫码或输码配对完成后，看到的会话列表与桌面端口径一致（含任务状态），以便远程决定要不要新建会话。
6. 作为移动端用户，我希望桌面端升级后旧版本 App 仍能沿用原 HTTP 路径连上（一版兼容窗口），以便不必强制双端同时升级。
7. 作为移动端用户，我希望本次桌面重构不改变我依赖的任何线协议形状，以便移动端可以按自己的节奏演进。
8. 作为 auto-task 现有维护者，我希望任务队列与会话状态来自同一份真源，以便不必再自己维护第二套「会话 ↔ Agent」映射账本。
9. 作为 auto-task 现有维护者，我希望「会话创建前注入 Agent 集成（写 hooks）」是我声明的编排步骤，而不是宿主反向回调我，以便流程可读、可单测。
10. 作为 auto-task 现有维护者，我希望 hook 脚本（Claude / Codex / pi / opencode）继续随我的插件打包、落点仍取自身资源目录，以便 Agent 集成与插件版本同进退。
11. 作为 file-transfer 插件作者，我希望 consent 决策与可信对端列表仍来自单一权威 api，以便不因内核认证语义改名而反复改调用点。
12. 作为第三方应用作者（如新的 AI 编码工具插件），我希望通过声明式 api 复用「配对状态 / 会话列表 / 任务模式」，以便不触碰内核即可做出远程终端产品。
13. 作为插件作者，我希望合并插件的 api 面按域分组命名（pairing / trust / consent / session / task），以便 ADR 0017「未声明不可调」的门禁粒度足够细。
14. 作为内核维护者，我希望会话记录里不再有任务语义字段，以便会话结构只描述进程与输出。
15. 作为内核维护者，我希望高频逐帧输出永不进 WASM，以便终端渲染手感不被燃料与 JSON 编解码拖垮（roadmap 阶段 3 性能红线）。
16. 作为内核维护者，我希望会话与设备域的宿主原语在一次 ABI bump 内收敛（而不是 devices 一次、session 一次、terminal 再一次），以便 ABI 版本线可读。
17. 作为内核维护者，我希望新增原语照旧「先权限门再属主」，以便 `session:config` 与 `session:write` 可以分别授予与审计。
18. 作为内核维护者，我希望信任记录的存储真源仍在内核（插件经只读/撤销原语访问），以便设备撤销不需要插件配合也不失效。
19. 作为安全审计者，我希望配对码 / QR token / JWT 的签发与校验执行点仍在宿主进程内，以便 WASM 崩溃或被攻破不构成认证绕过。
20. 作为安全审计者，我希望合并插件的权限清单是一份按域分组的显式列表（auth / peer / session:* / terminal:* / fs:write / timer / ui:* / storage / broadcast），以便「一个插件既能写项目 settings.json、又能投 PTY 输入、又能授权配对」是评审时的显式取舍而非合并副作用。
21. 作为安全审计者，我希望凭据在日志与存储中仍只记长度不落明文，以便下沉不降低 AGENTS.md §8 的红线等级。
22. 作为运维/打包者，我希望内置产物目录、构建脚本的插件白名单、dev-shell watch 只维护一条「session」线，以便不必再为将退役的 devices id 维护构建分支。
23. 作为测试者，我希望「配对 → JWT → WS → 真 PTY 输出」这条既有跨端链路在重构前后跑同一份断言，以便行为等价是门禁而不是愿望。
24. 作为测试者，我希望合并插件的三域语义在宿主真实 wasm 闭环里断言外部可见结果（DB 行、事件 payload、JSON-RPC 响应、HTTP 响应），以便不测内部函数。
25. 作为测试者，我希望前端配对与终端流程测试的接缝完全不变（仍打在宿主薄转发命令门面上），以便语义下沉不牵连 UI 断言。
26. 作为测试者，我希望双轨并存期有「同一输入 → 搬迁前后同输出」的对照测试，以便迁移正确性可证伪。
27. 作为桌面前端用户，我希望会话列表的排序、重名处理、状态文案由插件贡献并走 i18n，以便换插件即换产品形态。
28. 作为桌面前端用户，我希望终端预览、尺寸覆盖确认、滚动、写入管线仍留在宿主以保持手感与性能（本规格不下沉渲染管线），以便终端不出现 WASM 卡顿。
29. 作为设置页用户，我希望配对迁入插件视图后语言选项与动画开关仍与设置页一致，以便不出现视觉漂移。
30. 作为插件作者，我希望合并插件的三域在激活/停用上是同生共死的（一个实例），并且这一代价被显式记录与补偿，以便线上故障半径是可评估的。
31. 作为发布经理，我希望阶段 2 剩余工作（命令面桥接 / 退役 / 等价回归 / 文档）在合并形态下一次性收尾，以便不必先把 devices 独立形态做完再拆一遍。
32. 作为 roadmap 维护者，我希望这次「阶段 2+3 合并执行」被写成明确决策与其理由，以便后续阶段 4 内核冻结时边界可追溯。

## Implementation Decisions

### D1 插件身份与命名

- 新内置插件 **`com.bedcode.session`**（终端会话中心），`pluginType: rust-ts`，版本从 `1.0.0-beta` 起，`sandbox: inline`，`kind` 保持 **Application**（非 System）——它带 UI 贡献且必须可停可删；系统组件形态否决（会强制先于应用插件激活并失去独立启停）。
- `com.bedcode.devices` **退役**：其 rust 模块（pairing / trust / consent / keys）连同 60 个单测整体搬入合并插件，产物目录、`resources/plugins/desktop/` 条目、加载它做闭环测试的用例一并改指新 id。该插件从未提交入库、也未被 `plugins:build` 白名单收编，因此退役代价仅为改引用。
- `com.bedcode.auto-task` **退役**：rust 模块（hooks / state / queue / scheduled / agent / preset）与 TS 面（侧边栏任务历史、终端工具栏项、任务弹窗、hook 脚本、构建脚本）搬入合并插件，按域重组为三个同级模块目录，不做逐文件平移。
- 对外契约的 id 迁移：HTTP 基址由 `/api/plugin/com.bedcode.auto-task/...` 改为新 id 前缀，**path 段一个不改**；宿主插件路由在兼容窗口内**双前缀同投**（新 id 与 `com.bedcode.auto-task`），窗口长度 = 一个 minor 版本（实现期确认）。互调 api 名全部改为新命名空间，file-transfer 的两处消费点（consent 决策、可信对端列表）与认证中心桥接常量同批改指。
- 双端内置受信任插件白名单（fs_auth plugin whitelist）与移动端 HTTP 基址同步更新；老移动端在兼容窗口内仍可命中旧前缀。

### D2 边界：下沉什么、留什么

下沉进插件的语义（原宿主实现位置只作迁移线索，见附录）：

- 会话配置语义：配置 CRUD 与校验、`environment` / `wsl_distro` 分支、config→launch 映射与默认命令推导、排序与展示组织。
- 生命周期编排：命名唯一化策略、两阶段启动的编排决策（配置存在 → 建实例 → 回填归属）、restart / remove 的流程编排、状态推进顺序与对外事件形状。
- 尺寸裁决：正统端（canonical renderer）仲裁规则——裁决逻辑归插件，「谁是当前渲染端」的登记事实留内核。
- 设备域：配对码与 QR 的生命周期编排与 TTL 策略、已配对设备与连接历史的列表组织、派生视图（在线判定 + 会话数 + 任务状态合并）、consent 决策编排。
- 任务域：任务队列与状态机、预设任务、定时任务与恢复、模式（自动执行 / 自动应答）持久化与广播、Agent hooks 安装与清理、会话↔Agent 映射。
- UI：会话列表/详情业务列、配对页、已配对设备、连接历史、任务历史与任务弹窗、终端工具栏项。

**明确留内核**（红线，越线需重新裁决）：

- PTY 引擎（进程状态、slave fd 策略、终止汇聚、裸命令参数 exec、特殊键写入）。
- 输出分发管道（统一输出队列、订阅句柄窗口与 ack、全局输出管理器、历史快照与 resync 偏移）——**逐帧输出不进 WASM**。
- WS 连接骨架（心跳、首消息认证策略、帧过滤链、注册表与端点表、优雅停机）与终端控制帧协议。
- JWT 签发与验签执行点、密钥托管（`host-auth` secret-store 现状）、TLS 私钥、链路加密、流量过滤链、设备身份与节点身份生成与持久化。
- `pairings` / `connection_history` 表（真源在内核）与主库 schema 与幂等迁移执行点。
- 会话状态检测（WaitingInput 判定、ANSI 解析）作为「会话状态机引擎」的一部分留内核（roadmap 阶段 3「不动」列）；插件侧只做状态推进与广播。
- 终端渲染管线（预览组件、写入管线、renderer、resize debounce、滚动、IME 守卫）与三块宿主视图的**路由与外壳**。

### D3 WIT / ABI 契约增量（一次 bump：desktop 16 → 17）

- `host-session` 追加（函数级追加按仓库 ABI 惯例可不上抛，但本轮同时新增权限域与注解槽，故统一记为 v17）：
  - `config-upsert` / `config-get` / `config-delete`（配置 CRUD，权限 `session:config`）
  - `create-with-spec`（接收插件算好的 launch spec-json `{command, args, cwd, cols, rows, env, name}`，宿主只做 shell 包装 / WSL 转换 / 尺寸缺省 / ID 预生成，权限 `session:write`）——这是把「映射决策」交给插件、把「执行」留内核的关键切口
  - `restart` / `remove` / `rename`（补齐插件今天做不到的三个动作，权限 `session:write`）
  - `resize`（带请求端标识，裁决规则在插件、事实登记在内核，权限 `session:write`）
  - `connections-list`（连接注册表原始记录：addr / device_id / fingerprint，无排序无解读，替代今天硬编码 0 的会话数来源，权限 `session:read`）
  - `annotate`（会话注解槽写入，见 D4，权限 `session:write`）
- `host-auth` 追加只读/撤销面原语：`trusted-devices-list` / `trusted-device-revoke` / `connection-history-list` / `auth-setting-set`（TTL 等认证域设置项写入；读取沿用 `host-config`）。定位仍是无业务语义的凭据与记录存取，符合 ADR 0022 裁剪线。
- **不新增** `host-session-output`（订阅/ack 原语）——输出订阅插件化被显式否决（性能红线）。
- 复用现状：`host-pty`（v16，插件私有 PTY，与会话线零交叉）、`host-terminal.send`、`host-peer`、`host-mdns`、`host-websocket`、`host-bus`、`host-api-call`、`host-plugin-database`、`host-database`。
- 双端偏离：v17 属**桌面独有**会话语义下沉（与 host-websocket v14 / host-auth v15 / host-pty v16 同列），移动端 SDK 不跟演、ABI 保持 11；移动端要接同类能力时再补该端 interface 并对齐计数。
- 扩展点方向反转（本次最大的一处契约变化）：`terminal-hooks` / `on-session-lifecycle` / `on-input-submitted` 三处反向回调**保留为兼容面**，但合并插件改为「自己作为会话编排方主动推进」，不再依赖宿主回调注入 hooks——迁移期两套并存，收敛在 P4 之后按实际使用情况裁决是否退役（默认保留，成本为零）。

### D4 内核去业务化的落地形状

- 会话记录的四个任务字段（`task_status` / `task_reason` / `task_updated_at` / `task_questions`）从内核结构体摘除，替换为**不透明注解槽**：内核只按 `session-id → map<string,string>` 搬运与透传，绝不解释键名。
- 线协议不变：移动端会话 DTO 与同步事件里的 `taskStatus` 等字段继续原样输出，值由插件经 `annotate` 写入后由内核透传——**老端零改动**，字段演进遵循「老端忽略未知字段」。
- 配置存储归属：会话配置从主库表迁入插件私有库（`host-plugin-database`），主库旧表保留一版并只读退役；搬迁走**一次性幂等迁移**（可对旧库重跑，先例是 peer 数据向插件存储键的迁移模块），配套补迁移幂等测试。
- 任务与队列数据继续留在插件私有库现有六张表，但表名随域重组（`session_*` / `task_*` 前缀统一），需要一次表级重命名迁移窗口。

### D5 权限与能力清单（五同步点必须同落）

- 新增权限位 `session:config`（配置 CRUD 与 launch spec 构造），与 `session:read` / `session:write` 并列；`session:write` 语义扩展为「会话实例生命周期与注解写入」。
- 合并插件 permissions ≈ 18 项：`auth` `peer` `session:read` `session:write` `session:config` `terminal:input` `terminal:observe` `terminal:output` `fs:read` `fs:write` `storage` `broadcast` `timer:schedule` `ui:sidebar` `ui:toolbox` `ui:input` `ui:dialog` `system:open`。这是把三个信任域合于一身的**显式取舍**，须在评审记录中留痕（AGENTS.md §7）。
- 拆分后五同步点一并落：SDK 权限常量与 api 映射、打包 CLI / manifest 派生、前端合法权限集合、宿主能力清单、host_impl 权限门。
- 能力路由现状不变：`ROUTABLE_CAPABILITIES` 仍只有 `host-storage`；合并插件是原语的**消费方**，不提供可路由能力（若实现期需要宿主中间件向插件取认证策略，走既有 `host-api-call` 门面而非扩表）。

### D6 UI 与前端

- 配对 / 已配对设备 / 连接历史 / 会话列表四块以 **toolbox 页面**贡献（`registerToolboxPage`，权限 `ui:toolbox`）；宿主设置页的配对分组退役；宿主既有 `/devices`、`/connection-history`、`/sessions` 路由在过渡期保留为跳转壳（深链不破），一版后删除。
- 宿主前端命令封装层（设备命令、会话命令）继续作为**前端唯一接缝**：命令签名不变，实现改为「薄转发到合并插件 api」——这正是阶段 2 票 11 已落地的桥接方向，本次把转发目标从 devices 改指 session，并扩到会话与任务命令组。
- 输入校验双层不变：Rust 端为最终仲裁（TTL 边界、一次性 token、路径白名单），插件与前端校验只作 UX。
- i18n：宿主 `desktop.device.*`、`settings.pairing.*` 分组随退役迁移到插件文案；插件侧扁平 key（auto-task 现状）**必须先加命名空间前缀**（`session.*` / `task.*` / `pairing.*`），否则与既有 `hub.*` 风格混用会在同一注册表后写覆盖。宿主与插件两侧 key 均须 zh-CN / en 双文件同步。
- UI 改动全程受 `frontend-styles` skill 约束；toolbox 页需从宿主继承语言选项与动画开关（沿用设置页分组的状态传递教训：父级持有共享状态，不在子视图各自推导）。

### D7 合并代价与补偿（错误隔离是最大项）

三域同实例后，配对侧 trap 会连带终端回调与调度 tick 一起停。补偿三条，属本规格硬性验收：

- 认证路径**保留宿主兜底**：合并插件未激活 / 探活超时（现桥接为 5s）时，宿主命令面走降级路径并 `warn` 留痕——双轨并存期不允许出现认证单点。
- WASM 侧按域收口：三域各自 `Result` 边界，禁止跨域持锁（配对不得阻塞任务 tick），每插件的 `timer_register` 回调按命令名分域并各自计数失败，失败只降级本域。
- 生命周期分段：activate 顺序为「建表 → 订阅 → 恢复（定时任务 / 注解重放）」，任一步失败落 `Degraded` 而非 Activated，保持 v8 起「导出返回值如实上抛」的约定。

### D8 分步（纵向 tracer-bullet，P0 为前置 prefactor）

- **P0 prefactor**（零行为变化）：权限域拆分与五同步点、i18n 命名空间前缀、HTTP 双前缀路由、构建脚本与产物目录收编新插件、toolbox 权限声明。
- **P1 第一条贯穿线**：新插件骨架 + devices 三模块搬迁 + 认证桥接改指新 id + devices 退役；交付判据 = 「生成配对码 → 移动端校验 → 已配对设备列表 → 撤销」在宿主真实 wasm 闭环 + 跨端协议测试双绿。
- **P2 会话语义**：D3 的 `host-session` 增量（v17）+ 配置 CRUD / 命名 / launch spec / restart / remove / resize 裁决 / connections-list 下沉 + 注解槽替换任务字段 + 配置表迁私有库（含幂等迁移测试）。
- **P3 UI 迁移**：四块 toolbox 页 + 设置页配对分组退役 + 路由壳 + 宿主前端命令封装改薄转发。
- **P4 任务域并入**：auto-task 五模块搬迁、私有库表前缀重组与重命名迁移、hook 脚本资源目录接线、`com.bedcode.auto-task` 旧 api/权限清单退役。
- **P5 回归与文档**：三域端到端等价回归、roadmap 阶段 2/3 标记与合并决策补记、ADR 0022 补记（会话语义下沉 + 注解槽 + 双端偏离表加 v17）、AGENTS.md §7 ABI 计数与 §8 认证语义措辞、两端 code-map、CHANGELOG 与版本号同步。

## Testing Decisions

**什么算好测试**：只断言外部可见行为——DB 行、事件 payload、JSON-RPC 响应体、HTTP 响应体、协议帧字段；不测内部函数、不以 mock 替代真实原语、不用快照替代行为断言、不为覆盖率补测（`unit-test-discipline` 门禁 G1–G6 全适用）。

**接缝（采纳用户确认：不新增接缝）**：

- **S1 宿主真实 wasm 闭环（主接缝）**：加载合并插件的真实产物，经真实 `host_impl` 原语与临时目录数据库，断言外部结果。先例即仓库既有矩阵——host-pty 闭环矩阵、host-ws 闭环矩阵、devices 产物生命周期用例、配对桥接闭环用例；本规格新增 `test_session_*` 矩阵（配置 CRUD 成功闭环、create-with-spec 参数传递、annotate 透传、connections-list 属主隔离、resize 裁决）。**已知缺口必须补**：现有 `host-session` 侧 12 个单测全是权限/参数门，没有一条成功闭环。
- **S2 跨端协议集成测试（最高层，已存在）**：PTY 会话全链路用例（配对 → JWT → WS → 真 PTY 输出）、WS 会话路由、WS 认证规则、HTTP 生物认证契约，加移动端 HTTP 认证流与 WS 协议集成用例作为协议回归门。本规格**不得**改动这些用例的断言；改动即视为破坏线协议，回到 D1 兼容窗口重议。
- **S3 前端 vitest（接缝不动）**：配对流程集成测试与终端流程测试仍以宿主命令名为接缝（D6 薄转发门面），预期零改判；新增插件视图测试写在插件工程内（沿用 agent-hub 的 devMock 先例），并把插件目录纳入 vitest include（当前缺 auto-task，是必须补的覆盖缺口）。
- **S4 插件 crate 单测**：纯策略与状态机——配对码 TTL 边界与一次性消费、QR token 语义、claims 组织、consent 决策正反例、命名唯一化冲突、config→launch 映射分支、任务队列 `pending→waiting→executing→done|cancelled` 全迁移（含尝试上限与静默超时）、定时任务 `creating` 恢复、resize 裁决矩阵、JWT HS256 官方向量（沿用现有对照向量测试）。
- **双轨对照**（P1–P4 并存期强制）：同一输入下「搬迁前宿主实现」与「插件实现」输出必须逐字段相等；这是把「行为等价」从形容词变成断言的唯一手段。

**门禁**（AGENTS.md §10）：改 Rust → 双端 `cargo test`（移动端只需不回归，不跟演）；改前端 → 对应端 `pnpm run test:run` + 根 `pnpm exec eslint .` 0 error；i18n key zh-CN / en 双文件同步；每轮测试后清理测试 spawn 的后台进程与端口。

## Out of Scope

- **终端渲染与输出管线插件化**（预览组件、写入管线、renderer、滚动、IME、输出订阅与 ack）——roadmap 阶段 3 性能红线，逐帧输出不进 WASM。
- **WS 终端通道与控制帧搬进插件**——传输引擎与安全闸门红线。
- **移动端任何业务改造**：移动端 auto-task 插件仍是独立 TS 实现（其 rust 面基本为空壳），移动端 SDK 不跟演 v17，移动端不新建会话中心插件。
- **`com.bedcode.terminal` 独立插件**（roadmap 阶段 3 的另一半）：本规格不做，终端 UI 仍留宿主。
- **密码学引擎 / 密钥托管 / JWT 验签执行点 / TLS / 链路加密 / 流量过滤链 / 连接注册表 / 设备与节点身份**下沉。
- **保留 devices 为独立 headless 认证中心**的路线（用户已选合并）。
- **wasi3 运行时与 wasmtime 升级**（阶段 2 前置工程 A0 已完成，桌面已切 wasip3 构建链）。
- **快捷操作（quick_actions）**的归属裁决：阶段 2 摸底遗留问题，仍不在本规格随迁，另票处理。
- **旧 HTTP 前缀与旧路由壳的永久保留**：兼容窗口一版，之后单独票据清理。

## Further Notes

- **与 roadmap 的张力要显式承认**：roadmap 的渐进原则第 1 条（每阶段只做一件事）与第 2 条（每步可回退）被本规格主动打破——阶段 2 与阶段 3 的一部分合并执行。换来的收益是省掉两次 ABI 开窗、两次 UI 迁移、两次 i18n 冲突处理，并把「设备—会话—任务」这一事实三元组收敛到单一权威；代价是 P1–P4 的回归面比原计划宽。风险由 D8 的纵向切片与 S1/S2 既有接缝吸收，而非由流程假设吸收。
- **合并的真实痛点在 D7**，不在工作量：三域同实例后故障半径从「一个插件」扩到「会话产品面」。如果实现期发现认证兜底路径无法保持（例如桥接超时与宿主降级互相放大），回到用户处重新裁决「devices 留独立 headless、只并会话与任务」这一备选。
- **实现期需用户确认的 2 个开放点**：①注解槽 vs 宿主 DTO 组装时反向调用插件 api 取任务字段（推荐槽：内核零语义、无热路径额外调用）；②旧 HTTP 前缀与旧路由壳的兼容窗口长度（建议一个 minor 版本 + 一次 deprecation `warn`）。
- **不要先做票 11–14 的原始形态**：阶段 2 剩余四张票（命令面桥接、中间件、命令面退役、等价回归）在合并形态下需重定义——桥接目标改指新 id；「命令面退役」改判为「门面保留、实现改转发」，因为前端接缝就打在门面上（用户 D6 决策）。
- 规格发布即视为拆票就绪；建议按 P0–P5 六张纵向票拆，P0 为前置 prefactor。

---

## 附：参考位置（迁移线索，正文刻意不带路径）

| 域 | 现在在哪 |
| --- | --- |
| 配对码 / QR / JWT / 生物凭证 | `bedcode-desktop/src-tauri/src/utils/auth/`（`pairing.rs`、`qr_token.rs`、`jwt.rs`、`biometric.rs`、`host_secrets.rs`、`auth_center.rs`〔未跟踪，桥接层〕） |
| 配对编排与设备视图 | `src-tauri/src/commands/system.rs`（配对码 4 命令已转发、TTL 与 pairings/history 仍宿主）、`commands/qr.rs`、`commands/devices.rs`（`session_count` 硬编码 0）、`server/services/pairing_service.rs`（内存态码与 pending 设备）、`server/controllers/auth_controller.rs` |
| 会话语义 | `src-tauri/src/session/`（`session_manager.rs` 编排不变量、`session_components.rs` 的命名/映射/状态检测、`session_output.rs` 输出环〔不动〕、`session_lifecycle.rs`、`input_line.rs`、`session_event.rs` 四个任务字段）、`commands/session*.rs`、`enums/session.rs`、`enums/summary.rs`、`server/dtos/session_dto.rs`、`server/controllers/session_controller.rs`、`server/services/session_{control,sub}.rs` |
| 会话传输引擎（不动） | `server/ws/`（`conn.rs`、`subscription.rs`、`registry.rs`、`terminal_ws/`、`control_frame.rs`、`message.rs`）、`pty/` |
| 宿主原语实现 | `plugin/manager/wasm_runtime/host_impl/`（`session.rs`、`lifecycle.rs`、`terminal.rs`、`pty.rs`、`auth.rs`〔未跟踪〕、`peer.rs`、`ws.rs`）、`plugin/manager/capability.rs`（20 组清单 + `ROUTABLE_CAPABILITIES`） |
| WIT 与 SDK | `bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit`（`host-session`、`host-auth`、`host-pty`、`terminal-hooks`、`events`）、`rust/src/abi.rs`（`ABI_VERSION = 16`）、`rust/src/permission.rs`、`bin/cli.js` + `bin/manifest-gen.js` |
| 待并入的两个插件 | `bedcode-desktop/plugins/devices/`（未跟踪：`rust/src/{pairing,trust,consent}/*`，3042 行 / 60 测试，headless，api 11 项）、`bedcode-desktop/plugins/auto-task/`（`rust/src/{lib,hooks,state,queue,scheduled,agent,preset}.rs` 7911 行、`src/` TS + i18n、`scripts/` 5 个 hook 脚本、28 commands / 12 permissions / 1 sidebar view） |
| 前端消费面 | `bedcode-desktop/src/composables/commands/{deviceCommands,sessionCommands}.ts`、`src/views/{SessionsConfigView,TerminalWindowView,DevicesView,ConnectionHistoryView}.vue`、`src/components/settings/SettingsPairingSection.vue`、`src/components/{SessionForm,TerminalPreview}.vue`、`src/stores/{session,device}.ts`、`src/composables/{usePairing,useSessionWindows,useConnectedDevices}.ts`、`src/plugin/{context,types,permission}.ts`（toolbox/sidebar 扩展点）、`src/router/` |
| 移动端耦合点 | `bedcode-mobile/plugins/auto-task/src/{api.ts,index.ts}`（HTTP 基址 `/api/plugin/com.bedcode.auto-task`）、`bedcode-mobile/src-tauri/src/lib.rs`（内置受信任插件白名单）、`bedcode-mobile/packages/plugin-sdk-mobile/rust/{src/abi.rs,wit/bedcode.wit}`（ABI 11，不跟演） |
| 测试接缝先例 | `plugin/manager/wasm_runtime.rs`（pty / ws 闭环矩阵、devices 产物生命周期、配对桥接闭环）、`src-tauri/tests/{pty_session_chain,ws_session_route,ws_auth_rules,http_auth_biometric,server_integration}.rs`、`bedcode-mobile/src-tauri/tests/{http_auth_flow,ws_protocol_integration}.rs`、`bedcode-desktop/src/__tests__/integration/{pairing-flow,terminal-flow}.test.ts`、`src/__tests__/fixtures/{pairing,session}.ts` |
| 上游文档 | `.scratch/2026-09-18-devices-plugin-scope/{auth-center-spec.md,mapping.md,issues/01..14}`、`.scratch/2026-09-10-plugin-kernel-roadmap/spec.md`、`.scratch/2026-09-19-pty-base-service/spec.md`、`docs/adr/0017`、`0019`、`0022` |
