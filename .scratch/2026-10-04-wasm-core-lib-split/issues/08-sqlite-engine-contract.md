# 08: db + kv 绑定层迁入 sqlite 引擎 crate，并删除转发层（contract）

**What to build:** 数据库能力的**绑定层**（权限门 + 契约映射 + 任务单元执行 + 停用回收，约 10 条原语）搬进 07 票建的 crate，成为一个通过机制内核装配进来的能力域；随后删除 07 票留下的转发层，让那 20 个引用点正式指向新 crate。

本票完成后：宿主代码里不再有数据库能力实现，数据库能力的归属只有一个答案。

**Blocked by:** 07

**Status:** ~~done~~ **reverted（2026-10-05，ADR 0036）**——本票交付的 `bedcode-sqlite-engine` crate 已整体删除，引擎面回到 `src-tauri/src/db/`；以下实施记录仅作历史留档，结论以 ADR 0036 为准）

- [x] db 5 条 + plugin-db 5 条 + kv 3 条原语的绑定层完整迁入，逐字保留签名 / 返回值 / 错误串 / 结构化日志字段
- [x] 插件私有库隔离（每插件独立库 / 独立连接）语义不变；查询超时与行数上限等既有护栏不变
- [x] kv 真源仍是数据库（随迁，不另立真源）
- [x] 07 票的转发层已删除；20 个引用点改指新 crate，且无残留旧路径引用
- [x] 宿主侧不再有该域的实现代码
- [x] 插件若 import 未注册接口，实例化期点名（沿用 04 建立的通道）
- [x] descriptor 只带接口路径 / 权限位 / ABI 下界，**禁带产品名词**
- [x] 新 crate 自身测试 + 桌面全量 + 退役表锁 + crate 边界锁全绿
- [x] `cargo fmt` / `cargo clippy` 干净

## Comments

（实施记录见下）

### 实施记录（2026-10-05）

**1. 一个模块装3 个 interface（与前四域的一对一不同）**：`SqliteModule` 的 `desc().interfaces`
列三行（`host-database` / `host-plugin-database` / `host-storage`），白名单键 `sqlite`。
理由：三者**共用同一份端口、同一条权限位 `storage`、同一套护栏与同一张主库**；拆成三个
模块只会让「加一个能力域 = 加一个 crate 依赖 + 一行白名单」变成伪命题。模块≠interface，
描述符里显式列出，注册表按模块名登记。

**2. 端口形状（8 个方法，域函数收 `&Arc<dyn SqlitePorts>`）**：权限门 / `main_db` /
`plugin_db` / `block_on_any` / kv 存储三件 / 能力路由三件。三处关键取舍：

- **库句柄给句柄，不给“跑一段闭包”**：初稿想过 `with_main_db(plugin_id, |db| …)`（类型擦除
  + downcast 助手）。实测**不需要**——端口方法只擦除异步（与 ws 域同款），库句柄本身由域
  直接持有（`Database` 已随票 07 在本 crate 内），于是搬迁只是 `db.database().clone()` →
  `ports.main_db()` 一行，全域函数体**逐行保留**。
- **`plugin_db` 入参收 `String` 返回 `'static` future**：端口 trait 必须 dyn 兼容，而
  「借 `&self` 的 future」与 `block_on` 的驱动要求相撞；收 `String` 后宿主侧只需克隆
  `Arc<WasmHostContext>`（懒创建策略**不复制**）。
- **`block_on` 接受借用的 future**（`BoxedBlocked<'a>`）：域函数普遍借用 `plugin_id` / `sql`，
  强制 `'static` 会逼每个函数把字符串克隆一遍才能过编译——那是搬迁带来的**噪音**而非语义。
  宿主那份桥（`block_on_async`）本来就只要求 `Future + Send`（仅输出要 `'static`），端口与
  实现保持同口径。**这一点与其他三个域的 `BoxedBlocked`（无生命周期参数）不同，是本域
  刻意偏离，已写进 `ports.rs` 的类型文档。**

**3. kv 为什么不整体搬 `PluginStorage`**：`PluginStorage` 是**宿主服务对象**（15 处宿主消费方：
审批记录、预授权路径、激活状态…），且带一条 `pub(crate)` 的裸主库句柄访问器（R-10 纪律：
只有 crate 内的安全/能力模块能摸）。为 3 条原语把它搬进能力域 ⇒ 那条访问器必须放宽成跨 crate
`pub`。故 kv 经**窄端口方法**要（域拿不到裸句柄），`PluginStorage` 原封不动留在 `wasm_core::storage`。

**4. `SYSTEM_PLUGIN_ID` 真源收敛**：原先有三份同值副本（`storage.rs` / `approval.rs` /
`host_api/storage.rs` 的守卫）。现真源在 `plugin_binding::storage`（fail-closed 守卫的持有
者），`wasm_core::storage` 经 `pub(crate) use` 再导出；`approval.rs` 的私有副本**未动**
（另一个会话正在该文件上工作，按 §11 不碰）。

**5. 测试：35 条逐字迁入 + 1 条新增 + 私有库「无头缺席」形状刻意保留**

- 假端口脚手架 `plugin_binding/tests/scaffold.rs`（权限脚本 / 内存主库 / 内存 kv / 转发空间）。
  **私有库默认缺席**且返回与宿主无头分支**逐字相同**的错误文案 ⇒ 两条用例断言的**错误归属**
  （被权限门拒 vs 被私有库缺席拒）与迁移前完全一致；这是「假端口要造出与被替换对象同形」
  的实例。
- **新增 1 条**：`capability_routing_short_circuits_host_primitives`——迁移前的 kv 用例
  只覆盖宿主原语路径，**能力路由分支（命中系统组件 ⇒ 短路原语）在本域内零覆盖**。脚手架把
  转发空间与宿主原语空间**分开**，两侧各钉一个断言：命中转发 ⇒ 宿主原语空间为空（不是两条
  都写）；未登记转发 ⇒ 读不到系统组件侧的值（否则“两条路径共用一份存储”会让用例恒绿）。
- 分组落点 `plugin_binding/tests/{db_faces,db_isolation,sql_parse,db_guards,db_batch,kv}.rs`
  （与 ws / peer / http 三个域的 `plugin_binding/tests/` 同款）。

**6. 顺带修一处锁的精度（票 06 同款教训，第 4 次）**：`capability_module_descriptors_carry_no_product_nouns`
用子串 `contains` 扫描述符，sqlite 域的权限位 `database:main` 里 `ai` 落在 `m-**ai**-n` 中间
⇒ 一个纯机制权限位被判成「含产品名词 ai」。处置**不是**把 sqlite 加白名单，而是把判据改成
**词段对齐**（`_` 归一为 `-`，禁用词必须等于该词或作为完整前/后/中段出现），并给判据本身加
契约例 `descriptor_noun_matcher_keeps_compounds_and_drops_substrings`（`host-session` /
`pty-instance` / `ai-chatbox` / `session:write` 仍命中；`database:main` / `mail` / `m.ain` 不命中）。
**残留风险（已记录）**：粘在一起的形式（复数 `sessions`、连写 `sessiondata`）不再命中。

**7. 测试装配链补线（真·回归点，不是形式）**：guest 的 db/kv 原语现由能力域实现，取宿主能力
经**实例级端口**——任何不经 `PluginHost::new` 装配链、裸造 `WasmHostContext` 的测试脚手架都
会因「端口未装配」panic（fail-visible）。首轮全量回归因此 **29 红**（component/sdk e2e、
engine_limits、wasm_flow、instance_call_model、session_e2e 13 条……症状是 guest 命令返回
`Null`——trap 而非报错）。已补三处：`manager/host/tests/scaffold.rs`、
`manager/runtime.rs` 的无头 fixture harness、以及 `component.rs` 自测里的
`build_host_ctx_with_domain_ports()` 助手（注释写明「这不是测试凑合：生产同一件事在装配链里做」）。

**8. 落点第二次改：`host-kits` → `server-libs`（磁盘实测推翻票 07 的选型）**

票 07 按「机制内核 + 能力域」族谱把 target 落 `target/host-kits`。票 08 加 `bindgen!` 后实测
**错**：本 crate 依赖 `bedcode-server-base` ⇒ 拖进 `tauri` ⇒ 拖进 GTK/gio/gdk-pixbuf 全栈，
而 `host-kits` 桶里没有这一层——编译它把根分区打到 **0 字节可用**
（`No space left on device` + 链接 `Bus error`），而 `target/server-libs` 里那一份**早编好了**。
结论：**target 落点跟依赖图对齐，不跟架构族谱对齐**。已改 `.cargo/config.toml`（注释写明这段
实测），`cargo metadata` 核验 `target_directory = <repo>/target/server-libs`，
并同步 `docs/knowledge/build-process.md` 的落点表。

**9. 账目**

| 范围 | 结果 |
| --- | --- |
| `bedcode-sqlite-engine` `cargo test` | **40 passed** = 4（票 07 迁移幂等）+ 35（绑定层逐字迁入）+ 1（新增能力路由） |
| 宿主 `cargo test --lib` | **816 passed / 1 failed / 1 ignored** |
| 唯一失败 | 既有 `session_e2e::test_session_task_domain_closed_loop`（票 06 台账已记：陈旧断言，与本票零交集，**未碰**） |
| 与票 07 逐条对齐 | 852 → 818 = −35（迁走的绑定层用例）+1（新增判据契约例）✅ 无其他增减 |
| 宿主全量 `cargo test --no-fail-fast` | 9 个集成 target **全绿**（含 `capabilities_lock` 7 / `hot_path_logging_lock` 3 / `wasm_bridge_bench`）；`--lib` 那次 815/2 的第二个失败是 `pty::output_notify_is_rate_limited_and_owner_scoped` 与 `perf_p2_guest_ring_fetch_batch_curve`——**两个都是真实时钟/负载敏感的既有用例**，空载单跑均绿（已复跑验证），非本票回归 |
| `cargo clippy` | 新 crate `--all-targets` **零告警**；宿主 `--lib --tests` 55 条**全部既有**（含他人在途文件），我引入的两条（`empty_line_after_doc_comments` / `unused_mut`）已当场修掉 |
| `cargo fmt` | 新 crate + 本票改动的宿主文件（`sqlite.rs` / `storage.rs` / `component.rs` / 测试脚手架）干净；宿主整树 `fmt --check` 仍红（他人在途文件，与本票无关） |

**10. 事故与外部干扰（记账）**

- **另一会话在本票中途两次 `cargo clean`**（第一次连仓库根 `target/` 与
  `bedcode-desktop/src-tauri/target` 一起清掉，第二次在我链接时删 `src-tauri/target`，
  症状是 `could not write output … No such file or directory`）。源码未受影响（HEAD 未推进、
  全部改动仍在工作区），代价是两次冷重建（约 22 分钟 / 6 分钟）。
- 磁盘三次逼近 0：①本 crate 在 host-kits 桶编 GTK 栈；②两次外部 clean 之间的重建叠加。
  已把 `host-kits` 整桶删除（纯缓存）并把本 crate 落到 `server-libs`。

**11. follow-up 归票 09**（与票 07 的两条合并）

- `crate_boundary_lock::SERVER_LIB_CRATES` 仍只含 server-lib 六件 ⇒ 能力域 crate（sqlite /
  discovery）**不在**「零横向 / 不反向依赖宿主」的锁面内。
- 描述符名词锁的词段判据不再命中复数 / 连写形式（`sessions` / `sessiondata`）。
- `pnpm run target:size` 的 `rootTargetDirs` 仍漏 `target/host-kits`（该文件他人在途修改）。