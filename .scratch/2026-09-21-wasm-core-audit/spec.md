# 桌面 WASM 内核全面审查（安全授权 / 扩展性 / 模块称职度）

Status: 进行中（审查已完成并取证，本文件是结论与证据真源。修复票 `issues/01..12` + 测试清理票 `issues/13`
+ 新立票 `issues/14`（`wasm_hash` 生产者，由票 03 登记项单独立票）与 `issues/15`（构建链权限映射漂移）；
**已落地 01 / 02 / 03 / 04 / 05 / 06 / 07 / 08 / 09 / 11 / 13 / 14 / 15，P0 全部完成**，
剩 P1 结构票 `10`（monitor 出口 + config 二选一）与 `12`（内核业务清零），以及票 07 延后的 preopen 只读档。
逐票状态看 `issues/*` 的 Status 行，接手顺序与门禁跑法看 `handoff-2026-09-22.md`）
Date: 2026-09-21
范围: **仅桌面端** WASM 内核 `bedcode-desktop/src-tauri/src/plugin/**`（含 `wasm_runtime`/`host_impl`/`security`/`bus`/`config`/`monitor`/`manager/**`）+ 前端插件通道 `bedcode-desktop/src/plugin/**` + SDK `packages/plugin-sdk-desktop/**` + 入站面 `server/**` 中与插件相关的路径。移动端零改动，受损/对齐项见 §7。
决策依据: `docs/adr/0017`（互调门）、`0019`（wasmtime 锁版）、`0020`（身份校验与权限审批）、`0022`（裁剪线/双端偏离）；`.scratch/2026-09-10-platform-kernel/spec.md`（无业务内核清单，权威真源）；`.scratch/2026-09-10-plugin-kernel-roadmap/spec.md`（阶段 0-4）；AGENTS.md §5/§7/§8/§9。
关联: 上一轮同类审查 `.scratch/wasm-core/audit-2026-09-15.md`（当时聚焦五模块骨架与票据 01-08 达标性；本轮聚焦**授权是否真的成立**与**扩展路径是否真的锁住**，两条线不重叠，见 §8）。
微内核差距分析: 见同目录 `microkernel-gap.md`（「作为微内核还缺哪些功能要素」的正式回答，含阶段 4 冻结前置清单）。

---

## 1. 结论速览

| 判定 | 内容 |
| --- | --- |
| 沙箱边界 | ✅ **成立**。无网络旁路、无文件系统旁路、资源上限生效、身份不可伪造、trap 自愈、实例串行符合 A0-3 红线（§3） |
| 安全授权 | 🔴 **不成立**。四条 P0 缺陷使「Rust 端最终仲裁」与「插件间禁止耦合」在关键路径上被绕过（§4） |
| 扩展性 | ⚠️ 站点齐、锁缺失。权限词汇实测三副本漂移（28/22/24），漂移锁只覆盖 2 个权限（§5） |
| 模块称职度 | manager/bus/config/security 主体称职；approval 桌面侧为**覆盖良好的死码**；monitor 有产无消；capability 零生产用户（§2） |
| 修复组织 | 1 张 prefactor + 6 张 P0/P1 纵向票 + 5 张结构/收尾票 = 12 张，另加 1 张测试断链清理票（`issues/13`），共 13 张（`issues/`） |

---

## 2. 逐模块判定（是否起到对应的作用）

| 模块 | 判定 | 证据 |
| --- | --- | --- |
| `manager/wasm_runtime.rs` + `wasm_runtime/component.rs` | ✅ 内核最强面 | 见 §3 全节；`component.rs:948-974` 每次导出调用续费燃料；`component.rs:885` ABI 上界拒载 |
| `manager/host.rs` + `host/**` | ⚠️ 主干扎实，两处状态机缺陷 + 业务残留 | `activation.rs:139` `Activating` 落 `_ => proceed` → 并发双激活跑两次 guest `activate()`；`activation.rs:26-33` `deactivate_all` 只过 `Activated` → Degraded 插件退出收不到 `on_shutdown`；`host.rs:27` + `activation.rs:69,448` 硬编码 `FILE_TRANSFER_PLUGIN_ID` 驱动 peer-net 启停 |
| `manager/capability.rs` | ⚠️ 机制真实、生产零用户 | ROUTABLE 仅 `host-storage`（`capability.rs:87-90`）；全仓 `plugins/*/plugin.json` 无 `"type":"system"`（我核过桌面 4 插件 + 9 个 fixture 包）；转发只传 `key`、不传调用方 plugin_id（`capability.rs:291-297`）→ 系统组件代持 storage 时丢调用方命名空间 |
| `manager/loader.rs` / `registry.rs` / `validation.rs` | ⚠️ 职责与 code-map 不符 | code-map:127 称 loader 做「WASM 组件加载」，实为仅扫描 + manifest 校验（`loader.rs:96-132`），实例化在 `host.rs:219-253` 且 `host/install.rs:43-88` 另抄一份；contributes 注册三份复制（`host/register.rs:13-27`、`:83-99`、`host/wasm.rs:106-121`）；`granted_permissions` 只写不读（写于 `activation.rs:151`，真源在 `PermissionManager`） |
| `security/framework.rs` | ⚠️ 管线正确，覆盖面 2/6 | `ResourceKind` 六类中只有 Fs、ApiCall 注册了仲裁器（注册点 `wasm_runtime.rs:930-937`）；Network/Process/Storage/Bus 为枚举占位，其资源仍各处直调 `check_permission`；未注册资源 `Deny`（`framework.rs:141`）fail-closed 正确；`resolve_store_limits`（`framework.rs:277-288`）双级钳制正确且真被实例化路径消费 |
| `security/approval.rs` | 🔴 桌面未接线 | `approve`/`verify_approval`/`effective_permissions`/`compute_dir_hash` 生产零调用（全仓唯一命中 `host/install.rs:191` 的 `revoke`）；激活路径 `activation.rs:149-151` 直接全量 `grant_permissions`。移动端已接线（`bedcode-mobile/src-tauri/src/plugin/manager.rs:633,652,659`）。**与 ADR 0020「已实施（桌面端 2026-08）」直接冲突** |
| `security/fs_auth.rs` | ⚠️ 层级顺序真实、第一层名不副实 | 顺序 canonicalize→路径白名单→插件白名单→持久化→弹窗正确（`fs_auth.rs:90-135`，canonicalize 在 `:91` 早于 `:104`）；但 `path_whitelist` 恒空（`:66`），第一层实为 `/.claude/` 子串匹配（`match_path_whitelist`，`:278-296`）；第二层插件白名单（`:75-76,114-121`）让 session/file-transfer 对任意路径完全免弹窗；「第三层」注释重复标注两次（`:123`、`:133`），文档宣称的「三层」实为四层 |
| `bus.rs` + `host_impl/bus.rs` | 🔴 背压/解环真实，topic 无命名空间 | 每订阅者 64 有界队列 + `try_send` 丢弃计数（`bus.rs:263-285`）、消费任务独立 spawn（`:338`）、仅 trait 注入无 PluginHost 反引用（`bus.rs:54` + `host/services.rs:370`）；但订阅侧零校验（`host_impl/bus.rs:73-101`），见 §4-P0-4 |
| `config.rs` | ✅ 真被消费（⚠️一层未接线） | Engine 四参数进 `wasm_runtime.rs:644-679`，Store 八上限经 `ResourceLimiter` 与燃料注入生效；`validate` 拒 0 值/拒预留<上限；`set_config`（`wasm_runtime.rs:730`）生产零调用 → 「运行时覆盖」层尚未接线 |
| `monitor.rs` | ⚠️ 有产无消 | 埋点全接线（`component.rs:956` 燃料、`wasm_runtime.rs:326` 内存、`framework.rs:156` 授权计数、`bus.rs:251,268` 丢弃），全原子零日志；`MetricsRegistry::snapshot` 无任何生产消费者（无 Tauri command，前端用无关的 `get_server_metrics`）→ 票据 03 承诺的「诊断页统一数据源」未兑现 |
| `manager/task.rs`（core-task） | ⚠️ 配额基本兑现，一处承诺未落 | 池 8 / 每插件 4 / 256 单元 / 1MiB / 任务 3600s / 回调队列 64 / 保留 64 均有比较式落点；**`PLUGIN_TASK_UNIT_TIMEOUT_MS` 全仓零引用** → 2026-09-22 票 09 裁决 B 处置：常量退役（不可兑现），口径改为「v20 无单元级抢占，任务墙钟是唯一兜底」，文档与 SDK 注释同步；两条重入红线经核成立 |
| `permission.rs` | ✅ 纯 re-export，无本地漂移列表 | 6 行 `pub use bedcode_plugin_api::permission::*` |

---

## 3. 守住的边界（本轮确认，勿回退）

- **无裸网络旁路**：`p2::add_to_linker_async` + `p3::add_to_linker`（`component.rs:730-737`）虽全量接线 WASI，但 wasmtime-wasi 48 的 `WasiCtxBuilder::new()` 默认「TCP/UDP 允许但所有地址默认拒绝、ip-name-lookup 拒绝」（该 crate `src/ctx.rs:47-66`，`sockets/mod.rs:160-165` `SocketAddrCheck::default` 恒 false），代码亦未调 `inherit_network`；`wasmtime-wasi-http` 不在依赖树 → 插件无法绕开 `host-http`。SSRF 侧公网→私网跳转 Stop（`host_impl/http.rs:49-73`）。
- **无文件系统旁路**：WASI preopen 逐个经 `fs_auth.is_granted`（无弹窗版）过滤（`component.rs:1477-1490`），无 tokio 上下文时返回空 = fail-closed；`FsPerms::ReadWrite` 是唯一的放宽点（无只读档，见票 07）。
- **资源与逃逸**：`memory_reservation` + `memory_may_move(false)` + `max_wasm_stack`（`wasm_runtime.rs:665-677`）+ `ResourceLimiter`（`:314-356`）；AOT 产物只写宿主 cache 目录、明确不放插件目录（`wasm_runtime.rs:213-219,696-710`，理由：`Component::deserialize` 是 unsafe、假定数据可信）。
- **身份不可伪造（guest 侧）**：`plugin_id` 是 Store state 字段，由宿主在实例化时写入（`component.rs:836-838`），Host trait 转发时一律取 `&self.plugin_id`（如 `component.rs:163-166`），非 guest 传参；`host_api_call`/`bus` 的 caller 身份同源。
- **故障半径**：`catch_unwind` + trap 限频自动重载（`host/commands.rs:72-157`）；实例串行靠 `Arc<Mutex<LoadedWasmPlugin>>` 且**持锁跨 `.await`**（`commands.rs:149`、`host.rs:366`），符合 AGENTS §7 A0-3「禁止 await 点释放锁」。
- **fail-closed 三处**：`PermissionManager::check` 未知插件 false、框架未注册资源 Deny、互调注册表空 Deny。
- **配额 fail-visible**：pty/ws/task/db 的 `PLUGIN_*_MAX_*` 全部有实际比较且返回 Err（非静默截断）。

---

## 4. P0：安全授权不成立的四条

### P0-1 主库 SQL 前缀隔离可绕过 → 全插件密钥与配对记录泄露

- 根因链：`grant_permissions` 对**每个插件无条件插入 `storage`**（`packages/plugin-sdk-desktop/rust/src/permission.rs:233-236`「storage 权限默认授予」）→ `host_impl/database.rs:86,105` 的 `PERMISSION_STORAGE` 门恒过；主库唯一隔离是表名前缀正则。
- 绕过：`extract_table_names`（`database.rs:585-613`）八个模式都不识别**逗号多表**——`SELECT b.value FROM plugin_com_bedcode_demo_x a, plugin_secrets b` 只提取到第一个表名，校验通过。连接上无 `sqlite3_set_authorizer` 兜底（全仓 grep 无）。
- 后果：任意已激活插件读穿 `plugin_secrets`（明文真源，`db/schema.sql:76`，宿主托管密钥同表不同域见 `utils/auth/host_secrets.rs:4`）、`pairings`、`connection_history`、`settings`，以及他插件的全部 `plugin_*` 表。
- 测试面缺口：`database.rs:687` 的 `test_validate_sql_table_prefix_multiple_tables` 只覆盖 `JOIN` 形态——意图正确、实现漏。

### P0-2 ADR 0020 的审批 + 内容钉扎在桌面从未接线

- 事实：见 §2 `security/approval.rs` 行。ADR 0020 状态段写「已实施（桌面端 2026-08）」，并把「`process:run` 等任意代码执行权限，manifest 声明即授予」列为该 ADR 要解决的风险 3、把「无内容钉扎」列为风险 4。
- 叠加面：桌面 `downloader` 无 `wasm_hash`（`downloader.rs:112` 注释自陈；移动端有）、无签名链（ADR 0020「后续」段已登记）、zip 解压无体积/条目上限（`downloader.rs:87-109` 无界 `io::copy`）。已具备的防御：id 格式 + 目录绑定 + 重复 id 先到先得（`validation.rs`）、zip 绝对路径/`..`/`.` 段拒绝（`downloader.rs:170-183`，不落 symlink）、ABI 上界拒载。
- 后果：用户安装一个 zip = 该插件自报 `process:run`（宿主 OS 级任意执行）即生效，零人工确认；批准（若将来存在）与内容不绑定。

### P0-3 会话/终端域缺属主校验 + 生命周期监听无门

- 事实：`host_impl/terminal.rs:16` 只查 `terminal:input`；`host_impl/session.rs:453,506`（`session_close`/`session_remove`）只查 `session:write`，均不比对会话归属——`session_close` 文档注释自己写「关闭**自己创建**的会话」，代码未实现；`host_impl/lifecycle.rs:11` `session_lifecycle_register` **无任何权限门**（对照 `:26` 的 `session_input_register` 需 `terminal:observe`）。
- 攻击链：任意插件注册生命周期监听（无需权限）→ 取得全部 session id/名称 → 声明 `terminal:input`（P0-2 下零确认）→ 向**用户正在使用的交互终端注入命令**。
- 佐证「是遗漏非取舍」：pty / ws / mdns 三域早有统一的属主判定与文案（`host_impl/pty.rs:49` `not owner of pty handle`、`ws.rs:51,54` handle/endpoint 两形、`mdns.rs:91`），core-task 的任务记录也带 `owner` 字段并以其身份复用宿主门禁（`manager/task.rs:111,145,295-315`）——机制成熟，只有会话/终端域未接。
- 同类：`host_impl/peer.rs:61,141` 句柄表无 owner 列；`host_impl/session.rs:645` 注解槽为全局扁平 map，跨插件同键互相覆盖（两项待复核，见票 04）。

### P0-4 消息总线无 topic 命名空间（内核承诺的通道本身无访问控制）

- 事实：订阅侧零校验（`host_impl/bus.rs:73-101` 把任意 topic 直投 `subscribe_wasm`）；发布侧只有 `bedcode.api.*` 目标门（`:9-34`），普通 topic 显式放行；宿主事件以 `sender="host"` 发布（`ws.rs:968`、`pty.rs:317`），派发仅跳过 sender 自身（`bus.rs:246`）。
- 后果：B 插件订阅字面量 `pty:exit.<A>` / `ws:client-connect.<A>` / `mdns:found.<A>` 即窃听 A 的连接地址、设备名、发现事件；也可向 `pty:exit.<victim>` **伪发布**，让受害插件按假事件行动。
- 文档冲突（三处，均已核对原句）：`bedcode.wit:262`（host-websocket 接口注释「topic **内嵌 owner**（非属主物理上订阅不到）」）、code-map:163、`.scratch/2026-09-19-pty-base-service/spec.md:129` ——均不成立。
- 结构背景：AGENTS §5 规定插件间**只**经互调 API 与消息总线通信，故总线是强制信任边界，无 ACL 等于该边界不存在。

---

## 5. P0-5 / P1：其余授权与信任缺口

- **P0-5 前端命令面 plugin_id 自报**：`manager/api_bridge.rs:130-147` 校验的是**参数里那个 plugin_id** 的激活态与权限 → 同 webview 内任一插件前端可 `invoke('plugin_storage_get', {pluginId:'受害者', key})` 读写他插件 storage；`api_bridge.rs:254` 注释「前端无法伪造 plugin_id」不成立。`sandbox:'isolated'` 只存在于类型定义（`src/plugin/types.ts:26`），`loader.ts:51` 把所有非 inline 跳过 → 所有插件前端代码与宿主同权共生。
- **P1-1 LAN 无凭证插件 HTTP 面**：`server/middleware/jwt_auth.rs:92-95` 对 `/api/plugin/**` 无 JWT 直接放行（`:9-11` 注释自陈「0.0.0.0 监听，局域网内任意设备均可无凭证调用已激活插件的 HTTP 端点（含写操作）」）；未声明 `contributes.httpEndpoints` 的插件整前缀放行（`plugin_controller.rs:71-80`）；此路径 `device: None`（`:135-138`）→ 插件无从判调用方。对照：WS 侧有声明式 `auth: none|jwt`（code-map:156-157），HTTP 侧无对等原语。
- **P1-2 fs_auth 白名单过宽**：见 §2 该行。
- **P1-3 task 单元超时未实现**：挂死单元永久占用进程级 8 线程池槽位，跨插件无隔离 → 2026-09-22 票 09 裁决 B：不实现单元级抢占，退役该常量并如实写文档；残余风险转入待立项 E5'（宿主调用侧统一超时闸门）。
- **P1-4 互调接收侧无门**：ADR 0017 层 1 只校验目标声明；`host_impl/api.rs:39-50` ReplyHandler 不校验 `msg.sender`，配合 P0-4 可订阅他人 api topic 抢答（**待复核**，票 05 范围内确认）。

### 扩展性：漂移实测（脚本比对三副本）

| 词汇源 | 条数 | 缺项 |
| --- | --- | --- |
| SDK Rust `permission.rs:81-110`（授予期真源） | 28 | 缺 `ui:pageToolbar`、`ui:fileHandler` → 声明了被静默过滤（`permission.rs:225-231`） |
| 打包 CLI `bin/cli.js:410-432`（校验真源？） | 22 | 缺 `fs:read` `fs:write` `mdns` `auth` `process:run` `timer:schedule` `app:cli` `ui:dialog` |
| 前端 `src/plugin/permission.ts:9-35` | 24 | 缺 `fs:read` `fs:write` `mdns` `auth` `process:run` `timer:schedule` `app:cli` |
| 生产 manifest 实测 | — | `bus` `fileservice` `transfer` 三处**都不认** = 纯装饰词汇 |

- 打包 CLI 的 `cmdValidate`（`cli.js:487-490`）对未知权限报 error，但**既不在任何插件 build 脚本里，也不在 CI workflow 里** → 校验锁从未运行，其列表也已落后 SDK 8 项。
- 「五同步点漂移锁」实为 2 个权限：`host_impl/tests/pty.rs:87`（pty:spawn/io）、`host_impl/session.rs:1077`（session:config）；ws/task/fs/network:http 无锁。
- `capability.rs:58 HOST_PRIMITIVE_CAPABILITIES` 手抄字符串表 vs `component.rs:696-717` Linker 接线清单：我做了集合比对，**当前无漂移**（21 声明 ⊇ 22 接线 − `host-auth` 刻意不入清单），但无编译期派生，纯靠自觉。
- 加一个 host 原语需改 ~8-10 处、跨 2 语言 2 端（WIT → `abi.rs` bump → SDK `host/*.rs` 薄层 + 权限常量 → TS API → 前端合法集 → capability 清单 → host_impl + `check_permission` → component.rs 接线 → 移动端偏离裁决）。WIT 是真单源、ABI 只拒更高版本（旧产物放行）这两点健康。

---

## 6. Businessless（ADR 0022 / platform-kernel §清单）符合性

`platform-kernel` 权威清单要内核只含「运行时 + 进程 + 网络 + 存储 + 通信 + 安全边界」。本轮实测残留：

| 残留 | 位置 | 裁决 |
| --- | --- | --- |
| `FILE_TRANSFER_PLUGIN_ID` 常量 + 激活/停用硬编码 peer-net 节点启停 | `host.rs:27`、`host/activation.rs:69,448` | 🔴 真违规：内核携带产品身份与产品生命周期，应改为「插件声明依赖 → 内核按声明装配」 |
| `WasmHostContext` 持 `SessionManager` / `SessionConfigManager` | `wasm_runtime.rs:917-927` | ⚠️ 阶段 3 未完成的存量（roadmap 明标终端本体留内核），不算新违规，但属阶段 4 冻结前必清 |
| `plugin/quick_actions_migration.rs`、`plugin/task_data_migration.rs` 每次启动执行 | `plugin.rs:24-28`、`lib.rs:448,452` | ⚠️ 活的过渡态（幂等 marker + 退役条件已文档化），且 quick_actions 反向依赖 `utils/auth/auth_center` 并硬编码业务 api 名 → 建议登记「契约退役即删」 |
| `plugin/downloader.rs` 未归位 manager | `plugin.rs:17-20` 已自标注 | ⚠️ 归位债务 |
| 双实例化路径（`host.rs:219-253` vs `host/install.rs:43-88`）+ contributes 三份复制 | 见 §2 | ⚠️ 复制即漂移面 |

---

## 7. 移动端与双端影响

- 票 01-06 全部为桌面独有面（`host-session`/`host-terminal`/`host-database`/`host-bus` 语义与授权），按 ADR 0022「双端偏离」不要求移动端跟演；**但票 01（权限词汇单源）与票 03（SQL 隔离）会同时改到 `packages/plugin-sdk-desktop/rust/src/permission.rs` 与移动端 SDK 的同名文件**——移动端 SDK 是其独立契约（ADR 0018），必须双端各自落并在票内列清单（用户偏好：可单端先做完，另一端破损列清单延后）。
- 移动端**不得**沿用桌面「`storage` 自动授予」的修复中间态：若票 03 只改桌面而移动端 SDK 仍自动授予，两端词汇/行为分叉要显式登记，不得静默。
- 票 08（HTTP 声明式 auth）若改 `/api/plugin/*` 线协议形状（加 auth 声明），属跨端协议变更 → 按 AGENTS §9 两端同步评估，禁止破坏性替换。

---

## 8. 与上一轮审计（2026-09-15）的关系

上一轮聚焦 wasm-core 改造票据 01-08 的达标性（骨架/config/monitor/security/bus/manager），并补做了票 07（per-plugin 资源覆盖，即 `resolve_store_limits`）与票 08（fs 接入统一框架）；其「未覆盖风险」四条是自调用回落、注册路径单测、弹窗日志断言、总线吞吐指标维度——**均未覆盖本轮结论**。本轮新增结论：SQL 前缀绕过（P0-1）、approval 桌面未接线（P0-2）、会话/终端属主缺失（P0-3）、总线 topic ACL（P0-4）、前端 plugin_id 冒名（P0-5）、权限词汇三副本漂移、monitor 有产无消、系统组件零生产用户。

---

## 9. Testing Decisions（本轮修复的统一验收口径）

- **每条 P0 先落红测再修**（`unit-test-discipline` 强制）：票 02 的逗号多表用例、票 04 的跨属主注入用例、票 05 的跨 owner topic 窃听用例，都必须先在当前实现上失败，修复后转绿——禁止只加断言在修好之后的「同步测试」。
- **主 seam 不变**：以「guest 真实 wasm 闭环 → 宿主原语 → 拒绝/放行」为断言面（既有 `wasm_runtime/tests/*_e2e.rs` + `packages/plugin-*-test` fixture 先例），不断言内核内部结构。
- **权限词汇锁 = 集合相等断言**，不是包含断言；生成源与两份消费方产物必须逐字比对（票 01）。
- **门禁**：改 Rust → `cd bedcode-desktop/src-tauri && cargo test`；改前端 → `cd bedcode-desktop && pnpm run test:run` + `pnpm exec eslint .` 0 error；改 SDK → SDK 侧 `cargo test` + `pnpm run test:run`；删代码要 `cargo check --lib --tests`（项目记忆：否则假绿）；vitest 用 `--pool=forks`（否则 OOM）；跑完清理测试残留进程。
- 文档命令字眼一律用 AGENTS §3 黄金命令。

---

## 10. Out of Scope

- 不实施任何修复（本任务只交付结论与票）。
- 不动移动端代码。
- 不做签名链信任模型（ADR 0020「后续」段已定为独立立项，票 03 只做哈希钉扎 + 审批接线）。
- 不改 `platform-kernel` 的阶段 3 终端下沉本体（另见 roadmap）。
- 输出分发管道下沉（性能红线，禁止）。
