# 02: 主库 SQL 隔离闭环（P0-1，全插件密钥泄露）

**What to build:** 一条真实的纵深：插件对**主库**的 SQL 访问只能命中自己的 `plugin_<id>_` 前缀表，任何其他表（`plugin_secrets` / `pairings` / `connection_history` / `settings` / 他插件表）在 SQLite 引擎层就不可见；主库访问不再是人人自动持有的 `storage` 权限。先红测、后修、再锁。

**Blocked by:** 01（权限位拆分依赖词汇单源与锁）

**Status:** done（2026-09-22 实施完成，见「实施记录」；既有触发器误拒与集成测试断链已另立登记）

## 现状与攻击链（已复核）

1. `packages/plugin-sdk-desktop/rust/src/permission.rs:233-236` 对每个插件无条件 `granted.insert(PERMISSION_STORAGE)` → `host_impl/database.rs:86,105` 的权限门恒过；
2. 主库唯一隔离是 `validate_sql_table_prefix`（`database.rs:564-580`）+ `extract_table_names`（`:585-613`），八个正则模式**都不识别逗号多表**；
3. `SELECT b.value FROM plugin_com_bedcode_demo_x a, plugin_secrets b` 通过校验并执行 → `db/schema.sql:76` 的 `plugin_secrets`（明文，宿主托管密钥同表不同域：`utils/auth/host_secrets.rs:4`）与全部配对记录泄露；
4. 连接上无 `sqlite3_set_authorizer`（全仓无）；现有 `database.rs:687` 的「多表」用例只覆盖 `JOIN` 形态。

## 验收

- [x] **先落红测**（当前实现必须失败）：逗号多表读 `plugin_secrets`、逗号多表写他插件表、`ATTACH`/`PRAGMA` 形态、以及正则可预见的其它逃逸写法——每种一条，断言被拒
- [x] 修复选型落地并在 Comments 记录取舍：(A) `rusqlite` 的 `set_authorizer` 回调按表名白名单仲裁（引擎层，推荐）；(B) 真 SQL 解析器提取全部表引用；(C) 主库访问改走受限视图/影子库。**禁止**只在正则表上打补丁
- [x] `storage` 不再自动授予；主库访问用独立权限位（`database:main`，与 `host-plugin-database` 的私有库权限分离），并同步进 01 的词汇真源与漂移锁
- [x] 现有 `host-database`（主库）消费者清单：全仓 grep 确认改动不破坏既有插件（若第三方无处可用，明确「主库面改判为仅内置插件可申请」并写进 AGENTS §7 能力清单）
- [x] 凭据红线复核：`plugin_secrets` 的 `value` 在错误/日志路径只记长度（AGENTS §8）
- [x] 回归：`cd bedcode-desktop/src-tauri && cargo test` 全绿；`host_impl/tests/*` + `wasm_runtime/tests/*` 相关闭环用例全绿；跑完清理测试残留进程（**全 target 仍受既有编译断链阻塞，见末条**）

## Comments

- 2026-09-21 立项：来源 spec §4-P0-1。这是本轮唯一「无需任何权限即可跨插件取密」的缺陷，优先级最高。

### 实施记录（2026-09-22，票 02 done）

**红测先行已做**：五条逃逸用例先落在旧实现上跑红（`main_db_comma_multitable_read_of_secrets_is_denied`、
`..._exfil_write_is_denied`、`..._quoted_identifier_cross_read_is_denied`、`main_db_attach_is_denied`、
`main_db_pragma_is_denied` 全部 FAILED），再上修复转绿；断言面是宿主函数 × 真实主库
（`build_host_ctx` 跑过 `init_schema`，里面就有真的 `plugin_secrets`，播种一条明文密钥后断言读不到），
不断言 `extract_table_names` 内部返回。

**选型 = (A) 引擎层 authorizer**。`rusqlite 0.32` 的 `hooks` feature 仓库早已开启，
`Connection::authorizer` 收 `FnMut(AuthContext) -> Authorization`（可捕获 owned 前缀串，
不需要 thread-local）。新增 `with_main_db_guards(plugin_id, conn, timeout, sql, f)`：
安装表名白名单 → 复用既有 `with_statement_timeout` → Drop 守卫卸载（与
`ProgressHandlerGuard` 同形态，panic 展开也卸）。主库五面（`db_execute` / `db_query` /
`db_execute_params` / `db_query_params` / `db_execute_batch`）全部改走该组合，
**私有库五面保持不装守卫**（本就一库一插件，票面只要求收紧主库）。

正则层的定位改写清楚了：**不再是边界**，只保留早失败 + 可读文案（`validate_sql_table_prefix`
文档块 + AGENTS §7 都记了这句）。另补一条 `RENAME TO` 模式——引擎的 `AlterTable` 动作只上报
原表名，`ALTER TABLE 自己的表 RENAME TO 别人的名字` 是引擎看不见的越界，只能在文本层拦
（用例 `main_db_rename_to_foreign_prefix_is_denied`，同时断言前缀内改名仍可用）。

**策略表（实测校准，非推测）**：先用一次性探针跑完 13 类语句，把 SQLite 实际上报的动作抄下来，
据此定档——
- 按前缀放行：`Read` / `Update` / `Insert` / `Delete` / `AlterTable` / `Create(Drop)Table` /
  `Create(Drop)TempTable` / `Create(Drop)Vtable` / `Analyze` / `Create(Drop)Index` / 其 Temp 变体 /
  `CreateTrigger`（含 Temp）/ `Create(Drop)View`
- 一律放行（不携带跨表访问，拒了就误伤自身能力）：`Select`、`Transaction`、`Savepoint`、
  `Function`（`ALTER TABLE` 内部用 `printf`/`substr`）、`Recursive`、
  `Reindex`（**`CREATE INDEX` 必然附带一条隐式 Reindex**，拒它等于禁建索引）
- 一律拒绝（fail-closed）：`Pragma`（`database_list` 会把宿主库文件路径交给插件）、
  `Attach`/`Detach`（挂载任意库文件）、`Unknown` 码、以及 `DropTrigger`
  （rusqlite 0.32 该变体只上报触发器名、判不出归属表 → 宁拒不错放）
- **`sqlite_master` / `sqlite_temp_master` / `sqlite_sequence` 特例**：任何 DDL 都会隐式记账到
  这几张目录表（实测 `CREATE TABLE` 上报 `Insert(sqlite_master)` + 五条 `Update(sqlite_master.*)`
  + `Read(sqlite_master.ROWID)`；`AUTOINCREMENT` 触 `sqlite_sequence`），全拒等于禁掉全部建表。
  裁决改成**看语句自己有没有点名目录表**：`names_schema_catalog(sql)` 先剥 `--` 行注释、
  `/* */` 块注释与单引号字符串字面量再按词元匹配，没点名 → 放行隐式记账；点了名 → 整条拒。
  双引号/反引号/方括号是**标识符**，保留在扫描范围内（那正是 `"sqlite_master"` 的规避形态）；
  单引号里的 `sqlite_master` 是数据，不算点名（用例 `main_db_catalog_name_inside_literal_is_allowed`
  锁住这条不误伤）。目录表按三条枚举而非 `sqlite_` 前缀通配：`sqlite_stat1` 之类不给放行。

**纵深不过拦的正面证明**（与红测成对，缺了就等于「恒拒绝假绿」）：
`main_db_own_prefix_tables_still_work_end_to_end`（建表/插/查/改/删/自连接逗号多表/带引号标识符/
ALTER ADD COLUMN/DROP）与 `main_db_own_prefix_ddl_family_still_works`（AUTOINCREMENT、
CREATE/DROP INDEX、CREATE/查/DROP VIEW）全绿；
`main_db_authorizer_error_names_the_boundary` 钉住两层分工：正则认得出的仍是
`does not match required prefix`，正则漏掉的由引擎报 `prohibited` 且**点不出内容**（断言不含密钥明文）。

**授权面拆分**：`storage` 的无条件默认授予从 `grant_permissions` 删除（该函数此前是
「主库权限门恒过」的根因），新增 `database:main`（SDK 反射表一行 → 两份生成物重出，词汇 30 → 31，
票 01 的漂移锁与集合相等断言自动覆盖）。SDK 三条新用例：不声明就没有、`storage` 与 `database:main`
互不代持、`check_api` 随之关闭。宿主侧原「未授予 storage」两条用例合并成**三态**用例
`main_db_and_private_db_faces_require_separate_bits`（未授予 / 只 storage / 只 database:main）。
另把「声明了却被词汇表过滤」从静默变成激活路径 `warn`（`host/activation.rs`，
票 01 的 SDK 文档注释承诺了这一点，本票落实）。

**主库消费者清单（全仓 grep 实测）**：四个桌面生产插件的 WASM 侧只调 `plugin_db_*`（私有库），
`db_execute` / `db_query` 系**零生产消费者**；仅有的真实使用者是 SDK 自检 fixture
`packages/plugin-sdk-test`（`src/lib.rs`）与 crate 内 `test-plugin` 组件——两处已补位
（fixture manifest 加 `database:main`；无头 e2e 共享脚手架 `wasm_runtime.rs` 的 `all_permissions` 加同位）。
据票面兜底口径定档并写进 **AGENTS §7**：主库面改判为**仅第一方按需申请**的高危位，
票 03 起进逐位人工确认清单。

**凭据红线复核**：`utils/auth/host_secrets.rs` 与 `host_impl/auth.rs` 对 `plugin_secrets` 的读写
全走参数化 SQL，错误/日志路径无 `value` 拼接（grep 实测零命中）；宿主侧访问不经插件守卫
（守卫只在插件调用的作用域内挂载、Drop 即卸，主库连接由 `Arc<Mutex<Database>>` 串行）；
本票新增的拒绝文案只出表名/列名，用例断言不回带明文。

**门禁实测（2026-09-22）**：
`cd bedcode-desktop/src-tauri && cargo test --lib` → **1083 passed / 0 failed**（含主库隔离矩阵 30+ 条）；
SDK `cargo test --lib` → **93 passed / 0 failed**；
`pnpm exec vitest run --pool=forks --maxWorkers=2` → **75 files / 734 tests 全绿**
（生成物 31 条后前端词汇锁仍绿）；根 `pnpm exec eslint .` → **0 error**（125 warning）。
`rustfmt --check`：改动的 LF 文件（`database.rs` / `activation.rs`）零 diff，且复核 38 行删除全是本票自身改写；
CRLF 文件（SDK `permission.rs` / `context.rs`、`plugin-sdk-test/plugin.json`）行尾字节数逐文件核对无漂移。
测试后检查无残留进程与监听端口。
`cargo test` 全 target 仍被**本票之前既有**的 `src-tauri/tests/{ws_session_route,pty_session_chain}.rs`
编译断链挡住（配对/QR 退役 + host-session v21 遗留，已登记为独立清理项）。

**过程中发现、未在本票修的既有缺陷（登记）**：
1. `reject_bare_transaction_control` 的「末 token ∈ {commit, rollback, end, release}」启发式会把
   `CREATE TRIGGER ... BEGIN ... END` 误判成裸事务控制 → 主库面触发器完全不可用（本票的正面用例
   因此跳过触发器场景，只留注释指向本条）。修法应是首 token 为 `CREATE` 时不套末 token 规则，
   属独立小修，不在本票顺手改。
2. `query_to_json` / 批次失败会把插件的 SQL 原文回带给**该插件自己**（`execute '{sql}'`）——
   不构成跨插件泄露，但主库面既然改判为高危位，票 03 做逐位确认时应顺手评估是否收敛该文案。
3. 正则层仍会把字符串字面量里的表名当表引用而误拒（既有行为，本票未动）。

**移动端分叉（必须显式，禁止静默）**：`bedcode-mobile/packages/plugin-sdk-mobile/rust/src/permission.rs`
的 `grant_permissions` **仍是 storage 无条件默认授予**（该文件 `:168` 注释与用例为证），且移动端词汇表
没有 `database:main`。→ 两端授权语义自本票起分叉：桌面「不声明就没有」，移动端「不声明也有 storage」。
移动端代码与版本号本票一律不动（spec §10 出范围）；移动端若接同类能力，需自行为其端加位并取消默认授予。

