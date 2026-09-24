# 插件开发检查清单

> 自 AGENTS.md §7 外移（2026-09-24）。**开发 / 修改插件前通读并逐项核对**；
> 硬约束与 AGENTS.md 同级——冲突时按 AGENTS.md §0 优先级裁决。
> 相关：ADR 0017（互调）/ 0019（wasmtime 双端）/ 0022（裁剪线与双端偏离）、`docs/knowledge/logging.md`。

插件位于 `plugins/<plugin-id>/`（独立 package：`plugin.json` + `rust/` WASM 后端 + `src/` TS 前端 + `vite.config.ts`）。

## 检查清单

- [ ] manifest 声明 `permissions`（前端快速失败 + Rust 端最终仲裁；文件系统走 fs_auth **三层**校验：
  第一方具名集成目录预授权 → 已授权路径前缀（持久化「记住」）→ 弹窗授权。票 07 起旧的两条特权已退役：
  「`.claude/` 路径子串白名单」（对任何插件都免弹窗、任意位置的同名段都算）与「内置插件白名单 = 任意路径放行」；
  第一方清单在 `plugin/security/fs_auth.rs::FIRST_PARTY_TRUSTED_DIRS`，**逐条注释归属**，新增条目要说得出
  消费它的函数；任务单元（core-task 池线程）只走 `is_granted` 无弹窗判据，未授权即 fail-visible 拒绝，
  绝不从池线程触发弹窗）
- [ ] 对外可调 API 在 manifest `api` 字段声明，经 `#[plugin_api]` 宏 + JSON-RPC 2.0；**未声明不可调**（ADR 0017）
- [ ] 契约边界单点维护在 WIT（`packages/plugin-sdk-*/rust/wit/bedcode.wit`）；改 WIT 必须双端同步 + ABI bump（wasmtime 双端版本见 AGENTS.md §2、ADR 0019）。**双端偏离（ADR 0022「双端偏离」节）**：桌面独有接口不要求移动端跟演——`host-websocket`（v14）、`host-auth`（v15 密钥托管 / v18 认证记录面）、`host-pty`（v16）、`auth-policy` 导出（v17）、`host-session` 会话语义批次与 `host-platform.wsl-distros`（v19）、`host-task`（v20 + `events-task`）、`host-peer`（v25 节点生命周期）只在 desktop WIT/ABI/SDK 演进；当前 **desktop v27 / mobile 11**（v27 = `host-session` / `host-terminal` 整 interface 退役，破坏性变更，旧产物须按 v27 SDK 重建）。移动端要接同类能力时再补该端 interface 并对齐计数。**同一批次内函数级追加不再 bump**，别拿批次号当函数号数。**不 bump 的行为变更**：`host-bus` topic 命名空间由 `<base>.<owner>` 改为 `<owner>::<base>`（签名零变化），跨属主订阅/伪发布宿主显式拒绝，旧产物 activate 期拿到点明错误、须按 v22 SDK 重建；移动端 `host-mdns` 仍旧形态、总线无门禁，该端跟演时需同批补 SDK 原语 + 总线 ACL + file-transfer 迁移，桌面结果不构成移动端正确性依据。v21–v27 各版删改明细见 ADR 0022 修订记录
- [ ] **同实例串行红线（A0-3 宿主 async 化，P3；依据 `.scratch/2026-09-21-a0-3-host-async/spec.md`，探针已证兼容）**：每插件实例同一时刻**仍只允许一个 guest 调用在执行**——async 化只改变「宿主线程在等待时让出」，不引入同实例并发进入 guest；`host.rs` 实例锁（std `Arc<Mutex<LoadedWasmPlugin>>`）async 化时改为 tokio `Mutex`（await 持锁、不因等待释放），串行语义与现在等价；**禁止**改成细粒度「await 点释放锁」（会导致同实例交错：插件静态状态竞态——配对码/QR/挑战注册表/config 缓存/私有库 + wasmtime Store 重入 panic）
- [ ] 宿主能力经 `host-*` 原语访问（清单见 `plugin/manager/capability.rs::HOST_PRIMITIVE_CAPABILITIES`，现 **21 组**：进程 = `host-pty`（交互式）/ `host-process`（非交互）/ `host-task`（并发任务域 v20，WASM 插件调度宿主 OS 线程池），网络 = `host-http` / `host-websocket` / `host-mdns` / `host-peer` / `host-connection`（在册连接清单，权限位 `connection:read`），存储 = `host-database` / `host-plugin-database` / `host-storage` / `host-fs`，宿主面 = `host-events` / `host-config` / `host-log` / `host-timer` / `host-app` / `host-platform` / `host-crypto`，互调与总线 = `host-bus` / `host-api-call`），能力**不得携带业务语义**（ADR 0022）；**授权无默认位**（旧形态在 `grant_permissions` 里无条件塞 `storage` 使权限门恒过，现只授予 manifest 声明且在本表内的权限，被过滤项由激活路径 `warn` 留痕），且主库 SQL 面与私有库面分域：主库（`host-database` 的 `db_*`）挂独立高危位 `database:main`——**当前生产插件零消费者，改判为仅第一方按需申请**（逐位人工确认），访问还受 SQLite 引擎层表名白名单仲裁（正则 `validate_sql_table_prefix` 只是早失败文案，不是边界）；私有库（`host-plugin-database`）与 KV（`host-storage`）仍走 `storage`；权限按风险域拆分（如 `pty:spawn` / `pty:io`、`ws:client` / `ws:server`、`task:run` + 每单元 kind 既有域权限门双门），拆分后同步点必须同步落：**权限词汇唯一真源是桌面 SDK `packages/plugin-sdk-desktop/rust/src/permission.rs`**（打包 CLI 与前端合法集读的都是它的生成物 `bin/permission-vocabulary.json` / `src/plugin/permission-vocabulary.ts`，加/拆位后跑 SDK `pnpm run gen:permissions` 重出，禁止再手抄清单），随后落宿主能力清单与 host_impl 权限门——漏任一处即词汇漂移锁翻红（锁在 `plugin/permission.rs`，断言集合相等而非包含）；**插件构建链的映射表**（`packages/plugin-sdk-desktop/bin/manifest-gen.js` 的 `RUST_PERMISSION_RULES` / `FRONTEND_PERMISSION_RULES`）同为消费方——退役/改名权限位必须同步改表，表含词汇表外权限时 manifest-gen **加载即抛错**（护栏见 `.scratch/2026-09-21-wasm-core-audit/issues/15-manifest-gen-stale-permission-map.md`）
- [ ] **会话真源在插件**：会话登记 / 状态机 / 生命周期分发 / 输入输出编排归 `com.bedcode.terminal-session` 私有登记域（`sessions` / `session_annotations` 两表为落盘真源）；宿主读会话事实**一律经** `utils/session_gateway.rs`（互调 api），插件未激活**显性报错**；**禁止**新增任何「宿主内核持有会话」代码（防回接锁 `retired_kernel_session_domain_is_not_reintroduced`；架构现状与退役清单见 AGENTS.md §5，WS 终端通道与变更明细见 ADR 0022）。连接清单读取挂独立位 **`connection:read`**（只授 `session:read` 不再能读）。插件用 PTY：manifest 声明 `pty:spawn`（创建/终止）+ `pty:io`（数据面）+ `dependencies: ["host-pty"]`，并发条数用 **`ptyQuota`** 自我声明（构建期只校形态=正整数，加载期 0 或 > `PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN`(64) 直接拒 manifest **不夹取**，未声明回落默认档 8；会话插件声明 8，与退役前内核上限同档）
- [ ] **同步事件面（`host-events.broadcast_sync`）**：载荷类型与出站形状是**同一份 wire**——SDK
  `bedcode_plugin_api::events::SyncEvent` 的 serde 形态即 `{"type":"<snake_case 变体>","data":{…}}`，
  与 `bedcode_plugin_api::wire::SyncPayload`（宿主 → 移动端的出站形状）逐变体同构，唯一例外是
  `session_stopped` / `session_removed` 的 `source_device`（只给宿主做「排除发起设备」，不出站）。
  **禁止**在宿主 `enums/` 或 `events/` 里再定义会话事件镜像或按变体搬运字段：宿主只剩
  `HostSyncEvent` 薄适配 + 瘦处理器（防回接锁
  `retired_session_event_mirror_is_not_reintroduced` / `sync_handler_does_not_interpret_session_variants`，
  专项票 01–04 见 `.scratch/2026-09-24-session-events-app-event-poly/`）。会话概要字段用 SDK
  `wire::SessionSummary`（**snake_case 键名**，`status` 是展示字符串——状态机折算归生产者，
  放 `{"error":…}` 对象会让整条载荷解析失败）。WIT `broadcast-sync` **无返回值**（ABI 稳定），
  宿主侧的拒绝（畸形 / 旧格式产物 / 折不成载荷 / 事件源未注册）只落宿主 `error!` 日志，
  **插件收不到异常**——载荷必填自足必须在发布点之前自己保证（取不到就 warn + 不广播，
  不发半成品载荷）。格式换血后**未重建的旧产物在解析期即被点名拒绝**，改 `SyncEvent` 必须随包重建
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
  → 宿主 `--lib` 红 6 项，见 `.scratch/2026-09-23-session-engine-downsink/issues/14`）。构造
  `PluginManifest` 一律只列本用例断言的字段 + `..Default::default()`；「`Default` 与 serde 缺省
  等价」这条真有意义的不变量由 SDK 锁 `types.rs::default_manifest_equals_minimal_json_manifest` 守住
- [ ] 日志：target=`bedcode_lib::plugin::plugin_log`，`[plugin:xxx]` 前缀，WASM trap backtrace 不得关闭（详情见 `docs/knowledge/logging.md`）
