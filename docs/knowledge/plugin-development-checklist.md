# 插件开发检查清单

> 自 AGENTS.md §7 外移（2026-09-24）。**开发 / 修改插件前通读并逐项核对**；
> 硬约束与 AGENTS.md 同级——冲突时按 AGENTS.md §0 优先级裁决。
> 相关：ADR 0017（互调）/ 0019（wasmtime 双端）/ 0022（裁剪线与双端偏离）、`docs/knowledge/logging.md`。

插件位于 `wasm-apps/<plugin-id>/`（桌面端；移动端仍为 `plugins/`）（独立 package：`plugin.json` + `rust/` WASM 后端 + `src/` TS 前端 + `vite.config.ts`）。

## 检查清单

- [ ] manifest 声明 `permissions`（前端快速失败 + Rust 端最终仲裁；文件系统走 fs_auth **三层**校验：
  第一方具名集成目录预授权 → 已授权路径前缀（持久化「记住」）→ 弹窗授权。票 07 起旧的两条特权已退役：
  「`.claude/` 路径子串白名单」（对任何插件都免弹窗、任意位置的同名段都算）与「内置插件白名单 = 任意路径放行」；
  第一方清单在 `plugin/security/fs_auth.rs::FIRST_PARTY_TRUSTED_DIRS`，**逐条注释归属**，新增条目要说得出
  消费它的函数；任务单元（core-task 池线程）只走 `is_granted` 无弹窗判据，未授权即 fail-visible 拒绝，
  绝不从池线程触发弹窗）
- [ ] **系统文件选择器（`host-platform.pick-files` / `pick-folder` / `pick-folders`）需单独声明 `fs:pick`**（2026-09-27）：
  系统原生对话框浏览面不设限（用户可在任意位置浏览），但 ① **manifest 不声明 `fs:pick` 即在弹框前显性报错**
  （错误点名缺失权限位）；② **选择结果在交给插件前过 fs_auth 授权校验**——命中已授权目录前缀则静默放行，未授权的
  弹一次框且**按所在目录落账**（文件 → 父目录），拒绝 / 超时 → `Err` 且不回传任何路径（不得降级成空数组）。
  该位**不隐含** `fs:read` / `fs:write`（拿到路径 ≠ 能读内容）；`reveal-in-dir` 仍无门（不交付新路径）。
  manifest-gen 扫到 `platform_pick_*` 调用会自动补该位（`RUST_PERMISSION_RULES`）
- [ ] 对外可调 API 在 manifest `api` 字段声明，经 `#[plugin_api]` 宏 + JSON-RPC 2.0；**未声明不可调**（ADR 0017）
- [ ] 契约边界单点维护在 WIT（`packages/plugin-sdk-*/rust/wit/bedcode.wit`）；改 WIT 必须双端同步 + ABI bump（wasmtime 双端版本见 AGENTS.md §2、ADR 0019）。**双端偏离（ADR 0022「双端偏离」节）**：桌面独有接口不要求移动端跟演——`host-websocket`（v14）、`host-auth`（v15 密钥托管 / v18 认证记录面）、`host-pty`（v16）、`auth-policy` 导出（v17）、`host-session` 会话语义批次与 `host-platform.wsl-distros`（v19）、`host-task`（v20 + `events-task`）、`host-peer`（v25 节点生命周期）只在 desktop WIT/ABI/SDK 演进；当前 **desktop v34 / mobile 11**（v27 = `host-session` / `host-terminal` 整 interface 退役；v28 = websocket 业务下沉：新增 `host-websocket.connection-context` + 破坏性退役 `host-events.broadcast-sync` 与 `host-pty.spawn` 的 `hostBroadcastSessionId`；v29 = HTTP 路由代码注册下沉（`host-http.register-endpoint` / `unregister-endpoint`）；v30 = peer 节点面新增；v31 = `host-peer.resume-all-transfers` 退役；v32 = 认证中心显式注册（`host-auth` 四函数，ADR 0031）；v33 = `host-auth` **退役** `device-token-issue` / `device-token-verify`（入场签发密钥与验签归认证中心自持，ADR 0033）；v34 = `host-auth` **退役** `biometric-credential-bound` / `biometric-verify-signature` / `biometric-credential-bind`（生物公钥托管与验签执行下沉认证中心私有库，B-downsink，ADR 0033 修订）——旧产物须按对应版本 SDK 重建，详见 ADR 0022 修订记录）。移动端要接同类能力时再补该端 interface 并对齐计数。**同一批次内函数级追加不再 bump**，别拿批次号当函数号数。**不 bump 的行为变更**：`host-bus` topic 命名空间由 `<base>.<owner>` 改为 `<owner>::<base>`（签名零变化），跨属主订阅/伪发布宿主显式拒绝，旧产物 activate 期拿到点明错误、须按 v22 SDK 重建；移动端 `host-mdns` 仍旧形态、总线无门禁，该端跟演时需同批补 SDK 原语 + 总线 ACL + file-transfer 迁移，桌面结果不构成移动端正确性依据。v21–v27 各版删改明细见 ADR 0022 修订记录
- [ ] **同实例串行红线（依据 `docs/adr/0029-plugin-concurrency-owner-and-on-demand-async.md`，含实例级门实测）**：每插件实例同一时刻**仍只允许一个 guest 调用在执行**——async 化只改变「宿主线程在等待时让出」，不引入同实例并发进入 guest；`host.rs` 实例锁（std `Arc<Mutex<LoadedWasmPlugin>>`）async 化时改为 tokio `Mutex`（await 持锁、不因等待释放），串行语义与现在等价；**禁止**改成细粒度「await 点释放锁」（会导致同实例交错：插件静态状态竞态——配对码/QR/挑战注册表/config 缓存/私有库 + wasmtime Store 重入 panic）。
  **2026-09-27 强化（ADR 0029 决定 4 / spec §12）**：同实例串行由 wasmtime **实例级门**保证，**与 import 是否 async 无关**（async import 挂起期间，同实例第二条显式调用零进展、宿主实现到放行才被进入）；且**挂起期间属主闭包不被调度**（event-loop 属主循环一并停摆）⇒ 「把 host-* import 改成宿主实现侧 async（`func_wrap_async`）」**不产生**用户可见收益，**按需 async 化不立项**（票 07/08 已退役）；需要「不等待」的能力时走**非等待形态**（立即返回句柄 + 事件回调，参考 `host-process.run` 与流式 `fetch`），改动前先读 ADR 0029 与两条边界锁（`runtime/tests/p3_async_host_import.rs`）
- [ ] 宿主能力经 `host-*` 原语访问（清单见 `plugin/manager/capability.rs::HOST_PRIMITIVE_CAPABILITIES`，现 **21 组**：进程 = `host-pty`（交互式）/ `host-process`（非交互）/ `host-task`（并发任务域 v20，WASM 插件调度宿主 OS 线程池），网络 = `host-http` / `host-websocket` / `host-mdns` / `host-peer` / `host-connection`（在册连接清单，权限位 `connection:read`），存储 = `host-database` / `host-plugin-database` / `host-storage` / `host-fs`，宿主面 = `host-events` / `host-config` / `host-log` / `host-timer` / `host-app` / `host-platform` / `host-crypto`，互调与总线 = `host-bus` / `host-api-call`），能力**不得携带业务语义**（ADR 0022）；**授权无默认位**（旧形态在 `grant_permissions` 里无条件塞 `storage` 使权限门恒过，现只授予 manifest 声明且在本表内的权限，被过滤项由激活路径 `warn` 留痕），且主库 SQL 面与私有库面分域：主库（`host-database` 的 `db_*`）挂独立高危位 `database:main`——**当前生产插件零消费者，改判为仅第一方按需申请**（逐位人工确认），访问还受 SQLite 引擎层表名白名单仲裁（正则 `validate_sql_table_prefix` 只是早失败文案，不是边界）；私有库（`host-plugin-database`）与 KV（`host-storage`）仍走 `storage`；权限按风险域拆分（如 `pty:spawn` / `pty:io`、`ws:client` / `ws:server`、`task:run` + 每单元 kind 既有域权限门双门），拆分后同步点必须同步落：**权限词汇唯一真源是桌面 SDK `packages/plugin-sdk-desktop/rust/src/permission.rs`**（打包 CLI 与前端合法集读的都是它的生成物 `bin/permission-vocabulary.json` / `src/plugin/permission-vocabulary.ts`，加/拆位后跑 SDK `pnpm run gen:permissions` 重出，禁止再手抄清单），随后落宿主能力清单与 host_impl 权限门——漏任一处即词汇漂移锁翻红（锁在 `plugin/permission.rs`，断言集合相等而非包含）；**插件构建链的映射表**（`packages/plugin-sdk-desktop/bin/manifest-gen.js` 的 `RUST_PERMISSION_RULES` / `FRONTEND_PERMISSION_RULES`）同为消费方——退役/改名权限位必须同步改表，表含词汇表外权限时 manifest-gen **加载即抛错**（词汇自检护栏；权限词汇唯一真源是 `permission.rs`）
- [ ] **会话真源在插件**：会话登记 / 状态机 / 生命周期分发 / 输入输出编排归 `com.bedcode.terminal-session` 私有登记域（`sessions` / `session_annotations` 两表为落盘真源）；宿主读会话事实**一律经** `utils/session_gateway.rs`（互调 api），插件未激活**显性报错**；**禁止**新增任何「宿主内核持有会话」代码（防回接锁 `retired_kernel_session_domain_is_not_reintroduced`；架构现状与退役清单见 AGENTS.md §5，WS 终端通道与变更明细见 ADR 0022）。连接清单读取挂独立位 **`connection:read`**（只授 `session:read` 不再能读）。插件用 PTY：manifest 声明 `pty:spawn`（创建/终止）+ `pty:io`（数据面）+ `dependencies: ["host-pty"]`，并发条数用 **`ptyQuota`** 自我声明（构建期只校形态=正整数，加载期 0 或 > `PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN`(64) 直接拒 manifest **不夹取**，未声明回落默认档 8；会话插件声明 8，与退役前内核上限同档）
- [ ] **插件事件面（websocket 业务下沉 v28 起）**：宿主 `host-events.broadcast-sync` 已退役
  （`SyncEvent` / `SyncPayload` 同步广播面删除，旧 `/ws/event` 与 `Message::SyncData` 不再存在）。
  插件事件一律 **`host-bus.publish`（属主私有 topic，跨插件用 public topic）+ `host-events.emit`**
  （桌面前端 Tauri 事件），载荷由插件自定义 JSON。宿主不再按事件类型/载荷决定广播目标或生成
  刷新通知——会话/设备/任务事件真源与派生视图都在 `com.bedcode.terminal-session`。
- [ ] **插件分类（manifest `type` / `lifecycle`，ADR 0032）**：两个字段**正交**，都缺省即旧行为
  （`type` 不写 = L3 业务应用，`lifecycle` 不写 = 常驻），既有工程零迁移。取值域真源是 SDK
  `packages/plugin-sdk-desktop/rust/src/types.rs`（`PluginKind` / `InstanceLifecycle`），
  构建链 `manifest-validate.js` 只抄拼写（改枚举必须同改它），非法取值**构建期 + 宿主加载期双侧拒**。
  - `type`：**L1 `basic-service`**（引擎域组件，最先激活，其导出的 host-* 同形接口注册为能力提供者）
    → **L2 `internal-business`**（宿主网关的裁决依赖方，宿主**主动调它**；本仓唯一的反向依赖类别）
    → **L3 `business-app`**（业务应用面，缺省）。加载顺序固定 L1 → L2 → L3，批内按 id 排序；
    L1/L2 是**角色驱动**（启停不持久化，持久化真源是「角色」）。历史拼写 `system` / `application`
    仍被宿主按别名接受（只告警不拦）。L2 的三条红线（白名单式登记 / 只做安全闸门 / 宿主只转发不解释）
    由防回接锁 `internal_business_host_dependency_stays_gated` 守着（`host/tests/l2_gating_test.rs`）：
    新增 L2 消费点必须登记进锁内白名单并写明理由
  - `lifecycle: ephemeral`（业务 worker：只对 store 操作、无页面、即用即弃）**本期只预留类型**——
    宿主一次性实例机制与调度框架未落地，声明即**双侧显性拒绝**，不做「静默当常驻处理」；
    启用时需补齐的清单见 ADR 0032 §6（调度方 / 传参协议 / 权限模型 / per-app 配额 / 内存动机回归锁）。
    存在理由是**内存生命周期**（wasm 线性内存只增不减，长驻实例撞单实例限额且不自愈），不是业务分层
  - 宿主侧**只按谓词**判角色（`is_role_driven` / `provides_host_capabilities` / `is_business_app`），
    加载顺序取自 SDK 常量 `PluginKind::ROLE_DRIVEN_LOAD_ORDER`——新增角色不必改宿主
- [ ] **认证中心面（`host-auth` v32 四函数，ADR 0031）**：**只有中心插件需要**，其余插件用组合式
  原语即可，不要自己注册。① 中心在 `activate()` 内调 `auth_center_register(methods)` 自注册
  （注册表**单中心**：第二个注册者被拒并点名在册属主），`deactivate()` 调
  `auth_center_unregister()`；**宿主还会按插件停用/卸载兜底 `purge`**，但插件自己注销才是
  及时路径。② 宿主裁决一律 **fail-closed**：无中心 / 中心调用失败 / 中心拒绝 → 拒绝
  （HTTP 401 / WS close 4001，`deny_kind` 分 `no_center` / `unavailable` / `policy`）——
  **中心插件漏注册 = 全机认证面不可用**，这是有意的，配套的 fail-visible 手段是宿主在 L2
  激活后打点名 `error` 日志（「按当前 SDK 重建」），所以**中心产物必须与宿主同批发布**。
  ③ 组合式：其他插件 `auth_method_invoke(method, params)` 复用中心已实现的认证方式
  （先 `auth_methods_list()` 看在册清单），宿主只校验「method 在注册表内」后**零解析窄转发**
  到中心 `auth-grant` 互调 api，中心业务错误原样透传。④ `methods` 是**声明式**能力清单
  （宿主不解释语义，缺席一个 method 由中心分派侧显性失败）
- [ ] **入场签发密钥归中心自持（v33 / ADR 0033）**：设备入场 JWT 的**生成 / 签发 / 验签**
  全部在中心插件内（密钥环 = 属主为本插件的 secret-store 值 `jwt.keyring`，最多两代；
  宿主 `utils/auth/jwt.rs` 与 `host_secrets.rs` 已整模块删除，`host-auth` 的
  `device-token-issue` / `device-token-verify` 两原语**已退役**）。
  ① **中心必须自验签**：`policy::evaluate` 的**第 0 步**就是密码学验签——
  对称密码学下验签方必须持密钥，签发与验签拆到两处就必然把密钥推回宿主。
  ② **`kid` 是可选 claim 且声明在 claims 末尾**：不带 `kid` 的 token 与迁移前
  **逐字节相同**；`kid` 只做**诊断标签不是授权门**（真正的闸门是「签名能否用环内
  某把密钥验过」），拿它当安全判据会造出「kid 参与了安全决策」的错觉。
  ③ **密钥环损坏一律显性失败**，绝不「顺手重写」——静默换新密钥会让全部已配对设备
  静默失效而系统看起来一切正常；同理由 `activate()` 读密钥环失败即**阻断激活**
  （不得降级为进程随机密钥）。
  ④ **轮换**（D4）：最多两代（当前 + 上一代），上一代在宽限期（= 最长 token TTL = 7 天）
  内继续可验签；触发面 = 插件命令 `session.auth.rotate-key` 与组合式出口
  `auth-grant` / `jwt` / `rotate-key`；**轮换不撤销既有 token**（撤销归撤销域）。
  ⑤ 迁移代价：存量已配对设备需**全量重新配对**。
- [ ] **生物凭证面归中心自持（v34 / B-downsink，ADR 0033 修订）**：生物凭证 P-256
  公钥的**托管与验签执行**从宿主移入中心——宿主 `host-auth` 的
  `biometric-credential-bound` / `biometric-verify-signature` /
  `biometric-credential-bind` 三原语**已退役**（desktop ABI 34），宿主
  `utils/auth/biometric.rs` 整模块删除，生物公钥真源 = 中心私有库
  `auth_biometric_keys`（`auth_records::biometric_key_*` 端口），验签在 WASM 内
  （p256 crate，`auth_http/biometric.rs::verify_biometric_signature`）。
  ① 挑战闸门 = **配对活跃 + 私有库公钥存在**（不再查宿主原语）；
  ② 绑定/解绑写私有库（空串 = 解绑删行），与配对记录 `auth_pairings` 解耦（不碰
     `connect_count` / `last_seen`）；
  ③ 迁移代价：宿主旧 `plugin_secrets` 的 `biometric:*` 行被幂等清扫，存量已绑定
     生物认证的设备需**重新绑定**（配对记录与其它认证方式不受影响）。
- [ ] 插件导出：`activate`/`deactivate`、`command`、`_http_endpoint`（v27 起 `terminal-hooks` 与 `events` 的生命周期/输入观察两个回调已随会话观察面退役，不再属导出面）
- [ ] 插件 HTTP 面（`_http_endpoint`，审计票 08）：**只认声明**——`contributes.httpEndpoints` 未声明的路径宿主直接 404（「未声明清单 → 前缀内 ANY 放行」的零迁移过渡已退役，未声明清单等于没有 HTTP 面）。每条可写 `{path, auth}` 声明认证档位，档位词汇 `none | jwt`（真源桌面 SDK `rust/src/types.rs::EndpointAuth`，与 `host-websocket` 注册面共用同一枚举；缺省档各面自定：WS = `none`、HTTP = **`jwt` 最严**），非法取值构建期由 `manifest-validate.js` 拒绝、运行期不登记该条（端点不可达）；`auth: "none"` 是免凭证可达的唯一形态，写给「拿不到 JWT 的调用方」（本机 hook 脚本、配对 / QR 这类 token 之前的入口）。宿主转发的入参带 `caller` = `device | localhost | anonymous`（环回按 TCP 对端判），可信设备另带 `device` 对象——**JWT 本体与设备指纹不透传**（AGENTS.md §8 凭据红线）。网关别名条目的 `RouteAuth` 与插件声明档位**取较严者**，两方都不得单方面开门。信任模型详见 `docs/knowledge/plugin-http-endpoint-trust.md`
- [ ] 存储：插件独立库（私有 SQLite）/ 主库前缀隔离（表名强制 `plugin_id_` 前缀，且由 SQLite authorizer 在引擎层仲裁——逗号多表、引号标识符、`main.` 限定、ATTACH/PRAGMA 都绕不过去，见票 02）；**禁止在 dev-shell 写具体业务 mock**——mock 数据/演示种子归各自插件工程（插件入口导出 `devMock`）
- [ ] 产物摘要 `wasmHash`（审计票 14）：**由构建链注入产物目录的 plugin.json，源清单不写该键**
  （`packages/plugin-sdk-desktop/bin/wasm-hash.js` 按 `<rustLibrary>.wasm` 现算 SHA-256；四插件
  `scripts/build.js`、dev 复制 `scripts/plugin-watch.js`、SDK CLI `build --resources-dir` 三个装配点共用
  这一实现）。源里手写它不会被 `manifest-gen` 刷新 → `manifest-validate` 告警，且发布链
  `scripts/package-plugins.mjs` 出包前逐条复核（缺键/形态非法/字节失配即 exit 1）。
  「产物与源 manifest 逐字一致」的口径自本票起收窄为**除注入的 wasmHash 外一致**；移动端仍无生产者（桌面独有）
- [ ] 测试内夹具 / 合成 manifest **禁止把 SDK 类型的结构体字面量逐字段列全当契约用**：SDK 追加
  可选字段即批红，且只在跑到依赖该夹具的用例时才暴露（`pty_quota` 追加时六处字面量手改、漏一处
  → 宿主 `--lib` 红 6 项（会话下沉专项票据 14）。构造
  `PluginManifest` 一律只列本用例断言的字段 + `..Default::default()`；「`Default` 与 serde 缺省
  等价」这条真有意义的不变量由 SDK 锁 `types.rs::default_manifest_equals_minimal_json_manifest` 守住
- [ ] 日志：target=`bedcode_lib::plugin::plugin_log`，`[plugin:xxx]` 前缀，WASM trap backtrace 不得关闭（详情见 `docs/knowledge/logging.md`）
- [ ] **错误推送约定（ADR 0030）**：跨边界失败一律走错误信封 `{code, request_id, params?}`（宿主域 `host.*` / 插件域 `<plugin_id>.*` / 前端域 `frontend.*`，**code 即前端 i18n key**，用户提示文案在插件自己的 `src/locales/`）；**技术详情（错误原文 / 调用堆栈）不出产生方进程**，UI 只显示友好文案（固定模板 + 已消毒具名参数），不显示任何错误码；业务错误用 SDK `bail_with_code(code, params)`（标记信封，宿主只透传不解释语义，畸形/未标记错误宿主按普遍兜底处理）；`plugin:notify` 与错误推送不得携带技术详情；`params` 只收用户安全值（显示名 / 端口 / 秒数 / 文件名），**禁止技术文案 / 堆栈 / 凭据**进参数或文案
