# wasm_core OCR 审核报告登记（统一修复待办）

**审核日期：** 2026-10-03
**审核工具：** alibaba/open-code-review v1.12.11（OCR 自管模式，sensenova / deepseek-v4-flash）
**审核范围：** `bedcode-desktop/src-tauri/src/wasm_core/` 全部 99 个 .rs 文件（56,279 行）
**审核方式：** 按模块分 4 批 `ocr scan`（audience=agent，输出落盘 /tmp/ocr_*.txt）
**登记人/来源：** pi 会话 OCR 集成后首次实战（session: 9789bbe6-29fb-45eb-9cf0-11345b77a4f3 等）

> 目的：把 OCR 审核发现的**全部评论**逐条登记（含原文位置、严重度、类别、修复建议），作为后续统一修复的单一真源。未作任何过滤——low 级别也保留，修复时可按优先级取舍。**所有条目初始 triage 状态 = `open`**，修复后改为 `done` 并附 commit。

> **修复进度（2026-10-03 起）**：security/ 模块 16 条**全部 done**（commit `612cb2f7d`）；wasm_core 根模块 20 条**全部 done**（commit `bc77476ed`，R-09 按 fail-visible 口径拒改）；host_api 模块 12 条**全部 done**（commit `0aa059e70`，H-03 文档口径）；manager 模块 21 条**全部 done**（commit 待定）。修复会话：pi（本会话）。

---

## 0. 汇总

| 模块 | 文件数 | 评论数 | high | medium | low | 耗时 | 输出 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `security/` | 8 | 16 | 3 | 8 | 5 | 6m50s | /tmp/ocr_security.txt |
| 根目录（顶层 10 文件） | 10 | 20 | 6 | 9 | 5 | ~7m | /tmp/ocr_root.txt |
| `manager/host_api/` | 29 | 12 | 3 | 7 | 2 | — | /tmp/ocr_host_api.txt |
| `manager/` | 52 | 20 | 6 | 9 | 5 | ~22m | /tmp/ocr_manager.txt |
| **合计** | **99** | **68** | **18** | **33** | **17** | ~40m | — |

**跨模块高频主题：**

1. **check-then-act 跨锁非原子**（security/api_registry、security/frontend_channel、security/approval、host_api/ws.rs、manager/watcher、manager/task ×2）——安全闸门处普遍存在
2. **panic/expect 作为宿主进程的错误处理**（manager/host/boot、manager/task、根 monitor/runtime_util/host_api/storage）——wasmtime Store 污染风险
3. **文档承诺 ≠ 代码行为**（approval load_all、storage 损坏处理、strategy AutoAllow 审计、boot notify_shutdown）——fail-visible 原则的反面教材
4. **错误信息在 WIT 边界被拍平**（crypto to_string、wsl_fs NotFound 合并、ws TrySendError::Full 混淆）
5. **测试可靠性**（pty_e2e 固定 sleep、terminal_output_perf 无 deadline、http_e2e 仅 happy-path 清理）

---

## 1. security/ 模块（8 文件，16 条）

### High

- **[S-01] `security/api_registry.rs:37-40` — API 名劫持（last-writer-wins）** `register()` 静默覆盖已注册的 api→owner 映射。后激活插件可抢占他人已声明的 API 名，`owner_of()`（reply-sender 验证源）读同一张表 → 劫持者回复可被当原属主接受。
  > 建议：他人已声明时拒绝/返回 error（fail-closed），仅允许同插件幂等重注册。
  > **已修（2026-10-03，commit TBD）**：`register()` 返回 `crate::Result`，跨插件冲突拒绝（先全量校验后一次性写入）；激活路径 map_err 后失败激活（fail-visible）；测试 `register_idempotent_and_overwrite` → `register_idempotent_and_rejects_cross_plugin_conflict`、`owner_of_follows_last_register` → `owner_of_keeps_first_declarer_when_conflict_rejected`；bench 夹具（wasm_bridge_bench support.rs）主实例不再声明同名 api（调用方角色不声明）。
- **[S-02] `security/frontend_channel.rs:127-132` — issue_token 跨锁 TOCTOU** loader 会话校验与 token 写入分处独立锁 → 并发 `reset()`/`revoke_plugin()` 可插缝（校验通过→reset 清域→重插过期凭据）。三个独立 `Mutex<HashMap>` 建模同一逻辑凭据域。
  > 建议：check+insert 单锁原子化，或合并为单 mutex 的 per-webview `HashMap<label, Domain>`。
  > **已修（2026-10-03）**：三把锁合并为单 `Mutex<HashMap<label, WebviewDomain>>`（Domain = loader_session + tokens + plugin_token 双向表），`issue_token` 校验+写入同临界区；11 个既有测试全过。
- **[S-03] `security/approval.rs:143-145` — 符号链接逃逸破坏哈希钉扎** `Path::is_dir()/is_file()` 跟随 symlink：插件目录含符号链接可哈希/读取插件根外文件；链接环 → 递归栈溢出 DoS。
  > 建议：用 `entry.file_type()`（lstat 语义）并跳过/拒绝 symlink 条目。
  > **已修（2026-10-03）**：改用 `entry.file_type()`（lstat），发现 symlink 即拒绝整个哈希计算（fail-closed，错误点名 rel 路径）；新增 `compute_dir_hash_rejects_symlinks` 测试。
- **[S-04] `security/approval.rs:95-97` — 审批 map 非原子读改写** `approve()/revoke()` 各自 `load_all()`→`save_all()` 两次独立 DB 操作。并发交错：先 load 同一份 map，后 save 覆盖对方 → revoke 丢失 = 过期授予存活。
  > 建议：`PluginApprovalStore` 加 Mutex 串行化，或 `PluginStorage` 加事务化 update 原语。
  > **已修（2026-10-03）**：`PluginStorage::update()` 原子读-改-写（同一 DB 锁内 get→f→set，跨实例共享同 Arc 存储即串行化）；approve/revoke 改走 update；新增 `concurrent_approve_revoke_do_not_lose_updates` 并发测试。

### Medium

- **[S-05] `security/api_registry.rs:50-53` — 门禁+属主解析分锁漂移** `contains()` 与 `owner_of()` 各自独立读锁；register/unregister 落在两次锁获取之间 → 门禁放行但属主解析不一致，违背模块文档「不存在漂移」承诺。
  > 建议：单读锁内同时返回存在性与属主（如 `gate(&self, api) -> Option<String>`）。
  > **已修（2026-10-03）**：新增 `ApiRegistry::gate()`（单读锁返回 Option<owner>）；framework `ApiCallAuthorizer::enforce` 与 `bus.rs::api_gate_target_owner` 改走 gate；新增 `gate_combines_existence_and_owner_in_one_read` 测试。
- **[S-06] `security/api_registry.rs:36` — 锁中毒静默恢复** 每次 `unwrap_or_else(|e| e.into_inner())` 绕过 poison 标志继续跑，破坏的 registry 不变量可能被当作有效。
  > 建议：fail-closed（传播错误/拒绝调用）或至少记录 poison 事件。
  > **已修（2026-10-03）**：收敛到 `recover_poison()` 助手，恢复前记录 warn（含错误载荷）；fail-closed 传播 poison 会让激活插件全部解析失败（更糟），故记录+恢复是此处正确取舍。
- **[S-07] `security/approval.rs:163-166` — 哈希帧歧义** 裸拼接 `rel+[0]+content+[0]` 无长度前缀/文件计数，内容含 NUL 时不同文件集可产出相同 SHA-256 → 在位替换攻击可保持哈希不变。
  > 建议：每字段 u64 LE 长度前缀 + 哈希文件数（需现有钉扎目录重新审批）。
  > **已修（2026-10-03）**：帧改为 文件计数 + rel 长度前缀 + content 长度前缀（均 u64 LE）；存量审批摘要失效 = 需重新审批（fail-visible，已注明）。
- **[S-08] `security/approval.rs:146-148` — 排除规则按裸文件名过宽** 任何深度的 `plugin.db*` 都被排除（应只排除根级私有库）→ `assets/plugin.db` 可在审批后改动而不失效哈希。
  > 建议：按根相对路径精确匹配 `rel == "plugin.db" || rel.starts_with("plugin.db-")`。
  > **已修（2026-10-03）**：改按根相对路径精确匹配；新增 `compute_dir_hash_excludes_only_root_level_runtime_db` 测试（根级豁免 / 嵌套参与哈希）。
- **[S-09] `security/approval.rs:226-230` — 空权限集也报 `Approved`** 与文档 `Pending`（权限集为空）矛盾；`effective_permissions` 此时为空集，调用方可能把零权限审批当有效授权。
  > 建议：`content_hash == current_hash && !approved_permissions.is_empty()`。
  > **已修（2026-10-03）**：verify_approval 空权限集 → `Pending`（文档口径落地）；既有测试改用非空权限集并新增空权限集 Pending 断言。
- **[S-10] `security/approval.rs:69-70` — 损坏存储不可恢复** `load_all` 文档承诺「无记录/损坏返回空 map」实际返回 Err；log 说 resetting 实际没 reset → 后续 approve/revoke 全部失败直到手工清 key。
  > 建议：真正 reset（log + 返回空 map），或修正文档/消息使 fail-closed 是刻意的。
  > **已修（2026-10-03）**：load_all 损坏 → 删坏行 + 回落空 map；写路径（approve/revoke 经 update）损坏 → 以空 map 为基线继续（parse_approvals 统一口）；新增 `corrupted_approvals_are_reset_not_fatal` 测试。
- **[S-11] `security/strategy.rs:41-45` — AutoAllow 审计义务只写在注释** `StrategyStep::AutoAllow` 是 unit variant，调用方 match 后什么都不做也无编译错误 → 最高风险档位的 always_allow 审计落点无强制。
  > 建议：返回带审计义务的类型（struct 携带必填 land_auto_allow），或提供 `evaluate_and_land` 组合助手。
  > **已修（2026-10-03）**：`StrategyStep` 由 unit enum 重构为 struct（tier + reads_allow_records + must_land_auto_allow 字段）；AutoAllow 恒带 `must_land_auto_allow=true`；两个资源侧落账点加 `debug_assert!` 守卫；行为测试（always_allow 落账）继续锁住真实落账。
- **[S-12] `security/strategy.rs:63-65` — `uses_records` 命名过宽** AutoAllow 仍需写审计记录（只跳过**读** allow records），但该名字暗示「此档位处理 records」→ 下游按此 bool 做通用 gate 会跳过 AutoAllow 的强制审计写。
  > 建议：改名 `reads_allow_records()` 并文档说明审计落点是独立无条件义务。
  > **已修（2026-10-03）**：改名 `reads_allow_records()`（读义务）+ 新增独立 `must_land_auto_allow()`（写义务），两义务分开携带；fs/network 两消费方全部换新名。
- **[S-13] `security/strategy.rs:82-84` — 授权边界错误无上下文** `store.strategy(plugin_id, resource)` 失败经裸 `?` 抛出，无 plugin/resource 上下文，授权路径故障不可诊断。
  > 建议：`.map_err` 附加 `plugin_id` + `resource` 或返回携带二者的类型化错误。
  > **已修（2026-10-03）**：evaluate 改为 map_err 附带 plugin_id + 资源中文标签；新增 `evaluate_error_carries_plugin_and_resource_context` 测试。
- **[S-14] `security/strategy.rs:19-21` — 管线顺序只写在文档** deny → first-party dirs → strategy → allow records → prompt 的顺序未编码进代码，fs/network 调用点仍重复并可能漂移（文档自己引用了「总是询问在文件侧跳过记录、网络侧仍读记录」漂移形态）。
  > 建议：收拢为单一求值函数 + 顺序断言测试。
  > **已修（部分，2026-10-03）**：新增 `PIPELINE_STAGES` 顺序表（声明 → 硬拒绝 → 第一方目录 → 策略 → 记录 → 询问）作为文档化真源 + `pipeline_stages_are_ordered_deny_before_strategy_before_records` 顺序断言测试；资源侧相对顺序引入靠既有行为测试（deny 优先 / AlwaysAsk 跳记录 / 记录命中免弹）覆盖。「收拢为单一求值函数」未做——fs/network 各阶段含资源专属步骤（第一方目录仅 fs），单一函数会引入资源分派 if 反而更难读；语义顺序已在两侧由同一 `strategy::evaluate` 真源 + 各自顺序行为测试双重锁住。如需彻底收拢可另立 rider。

### Low

- **[S-15] `security/api_registry.rs:39` — 循环内重复分配** `plugin_id.to_string()` 每条 API 分配一次。建议提到循环外。
  > **已修（2026-10-03）**：注册循环内不再重复分配（同一插件一次性写入）。
- **[S-16] `security/strategy.rs` （含于 S-12 区域）— 见上，同文件低危汇总。**
  > **随 S-12 一并关闭（2026-10-03）。**

---

## 2. wasm_core 根目录模块（10 文件，20 条）

### High

- **[R-01] `host_api.rs:64-69` — 权限守卫非编译器强制** `check_permission` 返回裸 `bool`（非 `#[must_use]`）→ 宿主 API 入口忘记把 false 转 Err 会**静默继续执行**且无编译警告。
  > 建议：返回 `Result<(), _>`，或至少 `#[must_use]`。
  > **已修（2026-10-03）**：`#[must_use]` 加在 bool 返回上（95 处调用点全部消费结果，无新警告）；拒绝日志同时降 warn（R-08）。
- **[R-02] `storage.rs:45-46` — 插件隔离可绕过** `get/set/delete/clear_all` 信任调用方传入的 `plugin_id`，不校验是否当前运行插件，也不拒 `SYSTEM_PLUGIN_ID`（`__system__`）→ 任意持有者可读写他人行、`clear_all("__system__")` 清掉全局激活状态。
  > 建议：plugin_id 由运行时从已认证插件身份派生（不从插件输入取），插件面原语拒绝 SYSTEM_PLUGIN_ID。
  > **已修（2026-10-03）**：事实核查——组件绑定侧 `component.rs` 的 `&self.plugin_id` 本就由运行时从已认证身份派生（guest 无法伪造）；补上纵深防御：插件面原语（host_api/storage.rs get/set/delete）拒绝 `__system__` 空间（fail-closed），`SYSTEM_PLUGIN_ID` 提为 pub(crate)；新增 `system_space_rejected_on_plugin_primitives` 测试。
- **[R-03] `config.rs:190-191` — fuel 预算无符号溢出** `fuel_per_call * fuel_debug_multiplier` 未经检查的 u64 乘法（两个配置可控值）。超大值 debug 构建 panic / release 静默回卷 → 看门狗预算失效。
  > 建议：`CoreConfig::validate()` 里 `checked_mul` 拒绝溢出 + 此处 `saturating_mul` 防御。
  > **已修（2026-10-03）**：`fuel_budget_for` 改 saturating_mul；`validate` 增加 checked_mul 溢出拒绝；新增 `fuel_budget_saturates_and_validate_rejects_overflowing_config` 测试。
- **[R-04] `runtime_util.rs:88` — 环境备用运行时重入保护不完整** `block_on_async` 只在 `block_in_place` 分支武装 `IN_BLOCK_IN_PLACE`，无句柄 `AMBIENT_RT.block_on` 路径可嵌套进入 → 非运行时线程调 block_in_place（Tokio panic / Store 污染）。`block_on_ambient` 完全无重入检查。
  > 建议：统一单点 ambient 上下文检测/守卫。
  > **已修（2026-10-03）**：新增线程局部 `IN_ASYNC_BRIDGE` + `AsyncBridgeGuard` 统一守卫：任意桥路径（block_in_place / ambient 兜底 / 桥内驱动线程）进入即置位；`block_on_async` 顶层重入检查命中即转新线程（ambient 驱动）；`block_on_ambient`（借用式 future 不可搬线程）重入时用可诊断信息 panic 替代 Tokio 晦涩 panic。
- **[R-05] `runtime_util.rs:124-127` — join 死锁风险** 该分支在调用者线程 `join()` 阻塞，future 由 `AMBIENT_RT.block_on` 在另一线程驱动。若 future 依赖 current_thread 运行时持有的资源（actix arbiter / LocalSet / Tokio IO）→ join 永不完成。
  > 建议：`ambient_handle().spawn` + oneshot，或文档强制 fut 不得触碰调用线程运行时。
  > **已修（文档口径，2026-10-03）**：重入/current_thread 路径统一走 ambient 驱动（R-04 重构后即 review 建议的 ambient 方向）；「调用方必须是『不驱动 future 所依赖资源』的线程」约束在模块 doc 已有明确警示，actix 专用场景已由 `ambient_handle`（spawn 型投递）覆盖。彻底收拢需把桥改成 spawn+oneshot 形态，属另一重构议题。
- **[R-06] `storage.rs:114-117` — 损坏激活态不可恢复** 文档说「损坏返回空 HashMap」+ log 说 resetting，实际返回 `AppError::Plugin` 且不重写/删除行 → 损坏行每次启动都失败。
  > 建议：解析失败时删行或覆写 `{}` 并返回 `Ok(HashMap::new())`。
  > **已修（2026-10-03）**：`load_activated_plugins` 损坏 → warn + 删坏行 + 回落空 map（与 approval S-10 同一模式）。

### Medium

- **[R-07] `monitor.rs:388-391` — 用户代码持锁执行** `snapshot()` 在持有 sources + plugins 双读铡时调用任意用户 `MetricsSource::snapshot()`。源回调节回 registry（需写锁）或嵌套 snapshot 会死锁；慢源阻塞所有 writer。
  > 建议：短命锁收集源列表，释放两铡后再调用；或文档强制非重入契约。
  > **已修（2026-10-03）**：短锁收集源（Arc 克隆）→ 释放读锁 → 锁外调用用户回调；`sources` 存 Arc<dyn MetricsSource>；新增 `user_source_reentry_into_registry_does_not_deadlock` 测试（回调回捣写锁不死锁）。
- **[R-08] `host_api.rs:73` — 权限拒绝日志为 log-DoS** 每拒绝无条件 `error!` 无限流 → 不受信任插件反复探测可淹没日志。
  > 建议：降为 `debug!`/`warn!` 或加限流（策略性拒绝是预期结果，非系统错误）。
  > **已修（2026-10-03）**：降为 `warn!`（默认可见但语义与「影响功能的失败」区分；日志红线语义自判条款亦允许）。
- **[R-09] `host_api.rs:129-132` — 生成物依赖 `expect` 死硬** `generated_vocabulary_know` 读 `CARGO_MANIFEST_DIR/..` 外仓库布局文件并 expect → 打包/vendored/无生成物 CI 下 panic。
  > 建议：文件缺失时告警跳过而非无条件 panic。
  > **不改（2026-10-03，fail-visible 口径）**：该函数是词汇漂移锁的测试侧——两份生成物（permission-vocabulary.json / .ts）已 git 入库，任何 checkout 都在；缺失 = 仓库破损/漏跑生成器，测试 loud-fail 正是锁的意义（改「告警跳过」会让漂移锁在异常环境静默放行）。vendored-crate 打包不含 SDK 的场景在本仓库构建链（cargo test 从源码根跑）不存在。
- **[R-10] `storage.rs:40-42` — `db()` 泄露全量 DB 句柄** 公开返回 `Arc<Mutex<Database>>` → 任意 PluginStorage 持有者可绕过窄隔离访问宿主每张表。文档点名唯一消费者 auth_policy，但 API 不强制。
  > 建议：只暴露受限查询面（按类型系统而非约定）。
  > **已修（2026-10-03）**：`db()` 收窄为 `pub(crate)`（4 个使用点全在 crate 内）；外部 crate / SDK 不可再触达全量 DB 句柄。
- **[R-11] `config.rs:302-304` — validate 零值检查不全** 拒绝 `fuel_per_call` 等为零，但漏 `store.max_table_entries` / `store.max_wasm_stack_bytes` → 坏配置通过校验后才在 Engine/Store 构造失败（错误不可诊断）。
  > 建议：与兄弟字段同样显式零值拒绝。
  > **已修（2026-10-03）**：validate 补 max_table_entries / max_wasm_stack_bytes 零值拒绝；新增 `validate_rejects_zero_table_entries_and_wasm_stack` 测试。
- **[R-12] `permission.rs:234-239` — 静态检查把子串当强制证据** `text.contains(ident)` 全文扫描 → 注释/字符串/死代码里出现权限词即通过「声明即强制」锁；`build.contains("validateManifest(")` 只证明字符串在。
  > 建议：断言真实调用点（每个权限有 check_permission 调用；validator 在真路径被调用）。
  > **已修（2026-10-03，注释剥除口径）**：扫描前剥掉纯注释行——注释/文档里的权限词不再算门禁落点；`plugin-build.js` 的 validateManifest 断言同样先剥注释（注释里的调用点不算挂在链上）。注释剥除只动整行注释，不误伤字符串。
- **[R-13] `permission.rs:281-286` — 声称递归实为单层** 文档「目录下**全部** `*.json`」实际 `read_dir` 只读直接子级，嵌套 manifest 声明词表外权限可逃逸（`checked >= 11` 下限仍被顶层满足）。
  > 建议：递归遍历（walkdir）+ 下限断言与实查文件数挂钩。
  > **已修（2026-10-03）**：`collect_json` 递归（跳过 node_modules / target / template，后者是含 `${}` 占位符的脚手架模板）；嵌套 manifest 不再逃逸。
- **[R-14] `permission.rs:37-41` — 分词器对格式脆弱** `quoted_literals` 按 `'` 盲目切分，无注释/双引号/转义感知；`parse_generated_ts` 只识别恰好 `]`/`}` 行 → 格式化/注释变化可静默移位解析集合，削弱漂移锁。
  > 建议：断言精确输出格式 / 用真 JS/TS 解析器 / 生成器输出 JSON 严格格式。
  > **已修（2026-10-03）**：解析前剥注释行；权限段断言每行恰一条字面量、apiMap 段断言带 `': [` 形状，漂移直接 panic 带行号（fail-visible，不再静默移位）。
- **[R-15] `runtime_util.rs:103` — `join().expect` 吞 panic 载荷** 抛掷 scoped 线程 panic 换新 panic 至调用者线程；模块自注释说 panic 穿过 WASM 宿主调用会污染 Store 永久破坏插件。
  > 建议：`catch_unwind` 转错误结果或 `resume_unwind` + 上下文。
  > **已修（2026-10-03）**：`resume_or_return` 统一处理驱动线程结果——原始 panic 载荷经 `resume_unwind` 原样穿过（先记 error 上下文），不再丢失 guest 侧真实 panic 信息。

### Low

- **[R-16] `monitor.rs:152-153` — memory_current 松弛原子不一致** Relaxed store 下较小 desired 可后于较大者提交，current < 真实值而 peak（fetch_max）正确，两指标互不一致。
  > **已修（2026-10-03）**：current store + 快照 load 与 peak 同用 SeqCst；新增 `memory_current_never_exceeds_peak_in_snapshot` 测试。
- **[R-17] `monitor.rs:358` — poisoned 锁 expect → 全入口崩溃** 锁中毒后所有监控入口 panic（含运行时热路径调用者）。
  > **已修（2026-10-03）**：五处 `.expect("…poisoned")` 改 `recovered()`（warn + into_inner）——指标是对外观测面，恢复丢一笔计数优于全入口崩溃。
- **[R-18] `monitor.rs:348-353` — 快照键命名空间碰撞** 顶层 `"plugins"` 键与用户注册 source 段叠放；注册名为 `"plugins"` 的源会静默覆盖/碰撞。
  > **已修（2026-10-03）**：`register_source` 拒绝保留段名 `plugins`（error 日志 + 跳过，快照缺段即 fail-visible）；新增 `reserved_plugins_segment_name_rejected_for_user_sources` 测试。
- **[R-19] `config.rs:203-204` — apply_overrides 钳制延后到调用方** pub(crate) 方法原样 merge，安全全靠 framework.rs 记得 clamp；直接调用者可得超限 StoreLimits 违背 `reservation >= max_memory`。
  > **已修（2026-10-03）**：`apply_overrides` 自身按当前配置钳制（放宽请求钳回 + warn），谁调都不会拿到超限值；framework 双天花板语义不变；新增 `apply_overrides_clamps_to_self_without_framework` 测试。
- **[R-20] `config.rs:221-223` — clamped_within 只 min 不抬零** 请求 `max_memory_bytes: 0` 等在 positive 上限下仍保留零 → 实例化期不透明 wasmtime 错误。
  > **已修（2026-10-03）**：`apply_overrides` 显式 `Some(0)` 视为无效覆盖——回落继承配置值 + warn（收紧到 0 无真实语义）；新增 `apply_overrides_treats_explicit_zero_as_inherit` 测试。

---

## 3. host_api/ 模块（29 文件，12 条）

### High

- **[H-01] `ws.rs:340-343` — TrySendError::Full 与 writer 已结束混淆** ws_close 与 purge 路径中发送队列满时 Close 帧**静默丢弃**（默认关 1000 而非 4005/wasClean 语义）；若发送中途阻塞可能永不关闭。
  > 建议：区分 Full 与 Closed，Full 显式处理（等待入队或强制 abort writer）后再从表移除。
  > **已修（2026-10-03）**：ws_close / purge_one 区分 Full 与 Closed——Full 时强制 `entry.writer.abort()`（写半段释放、对端收到 EOF、读任务随后上报 wasClean=false）；不再静默丢弃 Close 帧。
- **[H-02] `wsl_fs.rs:20-22` — wsl.exe 参数重拼接 = 注入面** wsl.exe 把 `--` 后参数重拼为单命令行经发行版默认 shell 执行（模块自己承认）。路径含空格/`$()`/反引号/`;`/glob 会被 shell 二次解析 → `/home/user/$(cmd)` 执行 cmd。影响所有调用点（cat/mkdir/tee/rm/test）。
  > 建议：POSIX 单引号转义路径参数（`'` → `'\''`），或改 stdin/stdout 传字节。
  > **已修（2026-10-03）**：`--` 后全部参数经 `wsl_quote` 单引号化（内部 `'` 转义 `'\''`）——shell 重拼后参数为单一字面量，元字符不再被解释。
- **[H-03] `crypto.rs:138-141` — 私钥拼接返回** keypair 作为不透明 `private‖public` 单缓冲返回，需算法特定长度切分（未文档化），首个半段是私钥材料但类型无任何表示 → 易被误记日志/当公开 blob 用。
  > 建议：返回结构化 keypair（或 `(private, public)`），或文档化切分偏移并警告勿记日志。
  > **已修（2026-10-03，文档口径）**：布局契约（私钥‖公钥，切分偏移=私钥长度随算法而定）与「勿当公开 blob / 勿进日志」警告写入 doc；返回结构化形态需 WIT 契约变更（ABI），另立条目评估。
- **[H-04] `ws.rs:1046-1048` — 插件消息上限常量是死代码** `PLUGIN_WS_MAX_MESSAGE_BYTES.min(1)` 把 1 MiB 常量钳成 1 → 整式坍缩为 `ws_frame_limit().max(1)`，插件端上限失效；若 frame_limit 为 0 则生效上限变 1 字节全部被拒。违背 spec §4.4。
  > 建议：`ws_frame_limit().max(1).min(PLUGIN_WS_MAX_MESSAGE_BYTES)`（对齐 endpoint.rs hard-cap 模式）。
  > **已修（2026-10-03）**：改 `ws_frame_limit().clamp(1, PLUGIN_WS_MAX_MESSAGE_BYTES)`——网络配置上限不再突破 1 MiB 平台硬上限。

### Medium

- **[H-05] `events.rs:11-14` — 非法 JSON 静默降级为字符串** 无效 payload 变 JSON 字符串且返回 Ok → guest 以为已投递，前端收到形状不同的载荷，监听方按原 schema 解析运行时失败无信号。
  > 建议：返回 Err（带解析错误）或显式信封包装。
  > **已修（2026-10-03）**：非法 JSON 直接 Err（fail-visible，带解析错误）；测试改锁拒绝路径。
- **[H-06] `ws.rs:373-377` — enqueue 不查 entry.state** peer 已关闭（state→CLOSED）后 `ws_send_text/binary` 仍返回 Ok 排队无法投递帧；读循环退出从不 abort writer/drop tx → writer 残留 + CLOSED 条目占连接配额槽。
  > 建议：`enqueue` 检查 `state == STATE_OPEN`，读循环退出时终止 writer 或 drop sender。
  > **已修（2026-10-03）**：enqueue 入队前检查 state（CLOSED → Err）；读任务收尾摘除条目（属主守卫）→ drop tx → writer 收尾。
- **[H-07] `ws.rs:177-181` — 连接上限 check-then-act 竞态** 表锁下读数→放锁→阻塞握手（至 timeout_secs）→新锁下插入。并发 ws_connect（同插件多实例/并行调用）可全部通过 `owned >= MAX` 再全部插入超限。违背 spec §4.4。
  > 建议：插入锁内复查计数（或握手前预留 slot）。
  > **已修（2026-10-03）**：插入前锁内复查属主计数，超限者中止 writer 并拒绝（不占配额槽）。
- **[H-08] `crypto.rs:117-120` — KDF/协商缺审计** `kdf_derive` / `key_agreement_shared` 返回派生秘密不调 audit，其它 host-crypto 原语都记录（仅算法名无密材）→ 合规监控静默漏记。
  > 建议：两函数返回前补 `audit(plugin_id, "...", algorithm)`。
  > **已修（2026-10-03）**：两函数成功路径补 audit（最敏感的派生/协商面不再漏记）。
- **[H-09] `crypto.rs:37` — 注册表/提供器错误拍平为 String** `to_string()` 丢弃类型化错误与操作上下文，WIT 边界处多种函数共享同一泛错。建议前缀 `format!("aead_encrypt[{algorithm}]: {e}")`。
  > **已修（2026-10-03）**：统一 `crypto_err(api, algorithm, e)` 前缀（`api[algorithm]: …`），注册表与提供器错误全部带上下文。
- **[H-10] `wsl_fs.rs:55-58` — cat 失败全合并为 NotFound** 权限拒绝/是目录/distro 错配都被拍成 NotFound → 调用方（依 NotFound 分支「文件不存在」跳过或创建）静默误判真实 I/O 错。
  > 建议：检查 stderr 映射（No such file → NotFound，其余 PermissionDenied/Other）。
  > **已修（2026-10-03）**：`map_wsl_failure` 按 stderr 关键字映射 NotFound / PermissionDenied / IsADirectory / Other。

### Low

- **[H-11] `wsl_fs.rs:154-155` — test -e 不可访问误报 false** 文件缺席与父目录无权限同为非零退出 → 瞬态失败被当「不存在」继续（重建）。建议传播可区分失败或文档化偏离。
  > **已修（2026-10-03）**：`exists_via_wsl` 仅「明确不存在」（stderr 含 No such file / not found）返回 false，其余按可区分错误传播（fail-visible）。
- **[H-12] `events.rs:47` — 未使用导入** 测试模块 `grant_permissions` 导入未用。
  > **已修（2026-10-03）**：移除。

---

## 4. manager/ 模块（52 文件，20 条）

### High

- **[M-01] `manager/host/boot.rs:32-36` — 静态插件回调 panic 未圈护** `tokio::time::timeout` 只限执行时间不 catch panic。on_startup/on_shutdown 是第三方链接代码，unwind 会崩宿主 task（乃至整个宿主）。
  > 建议：`catch_unwind(AssertUnwindSafe(...))` 包两个回调轮询并记录 panic 载荷。
  > **已修（2026-10-03）**：`guarded_callback` 统一圈护（同步调用段 + future poll 段分别 catch_unwind，futures_util::FutureExt::catch_unwind），panic 载荷 downcast 记日志；两个通知循环按 ID 排序（M-07）。
- **[M-02] `manager/task.rs:762-764` — cancel/timeout 后在途单元仍污染终态** cancel 设 phase=Cancelled，但后完成的单元仍到 `notify_terminal` 排重复终态事件；`remaining == 0` 分支可把 phase 覆写回 Completed/Failed。execute-batch 超时路径摘除 job 并快照误导性 skipped 结果，在途单元后写真实结果永不返回，末单元可将 phase 从 Cancelled 翻回。
  > 建议：仅 Running→terminal 转移时发终态事件（`terminal_notified` 标志内锁设置）；phase 离开 Running 后停推游标；同步结果前保留 job（或 drain 在途单元）。
  > **已修（2026-10-03）**：① `terminal_phase_for` 纯函数仅 Running→terminal（cancel 后末单元不再翻回）；② `terminal_notified: AtomicBool` 使终态事件恰好一次（cancel 也走 notify_terminal）；③ phase ≠ Running 停推游标；④ execute-batch 超时后 drain 在途单元（DRAIN_GRACE 3s 有界轮询结果槽）再快照。新增 `terminal_phase_only_transitions_from_running` 测试。
- **[M-03] `manager/task.rs:432-437` — started 事件可能晚于终态** `register_job` 先派发初始单元到池线程再 enqueue started → 快单单元作业可先推送 completed/failed 再推送 started，插件观察到终态先于启动。
  > 建议：started 在 register_job 注册表插入后、`pool_tx().send` 前立即 enqueue。
  > **已修（2026-10-03）**：started 移入 register_job（注册表插入后、池分派前投出）；submit 不再另行投递。
- **[M-04] `manager/task.rs:942-947` — 回调通道守卫 TOCTOU** `may_open_callback_channel` 在 jobs 锁下检查后放锁，实际建通道在 queues 锁另一临界区。其间并发 `purge_for_plugin` 可取消全部作业并 drop 回调队列 → 之后创建的新通道 + consumer_loop 永不拆除，向已停用插件派发——正是注释警告的孤儿通道。
  > 建议：检查与建通道原子化（持 queues 锁一致顺序复查，或两 registry 锁同取）。
  > **已修（2026-10-03）**：enqueue_event 内 jobs→queues 同锁序（与 purge 一致）原子 check-and-create——purge 摘任务与清队列之间插不进新建通道。
- **[M-05] `manager/watcher.rs:79-93` — 防抖 TOCTOU** 读锁在首块结束释放，写锁后才获取 → 同一插件并发两任务都看到空/过期 pending 都调 `reload_wasm_plugin`，500ms 防抖失效；共享态单 (plugin_id, Instant) 对，异插件交错事件互相覆盖。
  > 建议：check-and-set 全程持写锁（或 per-plugin 时间戳 map）。
  > **已修（2026-10-03）**：per-plugin 时间戳 `Mutex<HashMap<id, Instant>>`（std Mutex 兼容 spawn 异步与回调同步两上下文），check-and-set 单临界区原子。
- **[M-06] `manager/watcher.rs:133-135` — start() 环境问题转启动崩溃** `.expect()` 于 watcher 创建失败/目录不可 watch（fresh dev checkout 无 plugins_dir）→ Tauri setup 期崩溃。
  > 建议：返回 `Result<Self, notify::Error>` 或记日志继续。
  > **已修（2026-10-03）**：`_watcher: Option<Box<dyn Watcher>>`——创建/开始监听失败记 warn 降级（dev hot-reload 禁用），不崩 setup；调用方签名不变。

### Medium

- **[M-07] `manager/host/boot.rs:70-72` — 关闭顺序非确定** `inventory::iter` 按链接/注册序产出非确定 → notify_shutdown 依任意序拆插件；别处激活做依赖检查，任意序可能在依赖前关其依赖。
  > 建议：按 ID 排序（如 activate_role_driven_components）或逆拓扑依赖序。
  > **已修（2026-10-03）**：notify_startup / notify_shutdown 静态插件列表按 ID 排序（确定性）。
- **[M-08] `manager/host/boot.rs:87` — notify_shutdown 只通知静态插件** 文档「通知**所有**已激活插件」实际仅静态；WASM 插件 on_shutdown 依赖外部调用方先 deactivate。若关闭路径未为每个激活 WASM 插件调 deactivate_plugin → 静默漏关回调不持久化。
  > 建议：此处 deactivate WASM 插件，或把调用方顺序做成显式受检契约。
  > **已修（2026-10-03，显式契约口径）**：notify_shutdown doc 写明职责边界 + 调用方顺序契约（`system/lifecycle.rs` shutdown 链：notify_shutdown 优先级 15 先于 deactivate_all 20；WASM 插件 on_shutdown 由 deactivate_plugin 逐个负责）——顺序在调用方显式注册，非隐式约定。
- **[M-09] `manager/watcher.rs:120-124` — JS 变更事件无防抖** 打包器重建一批写多个 JS 文件 → 每文件发一个 PLUGIN_DEV_RELOAD 洪泛前端。
  > 建议：与 WASM 路径同样防抖（或廉价限流）。
  > **已修（2026-10-03）**：JS 分支走同一 per-plugin 防抖表（notify 回调线程同步加锁）。
- **[M-10] `manager/runtime/tests/terminal_output_perf.rs:162-169` — fetch_until 无 deadline** none 分支睡 1ms 永远重试，无游标推进保护；`perf_p3_end_to_end_catchup` 跳过预等待直调。命令停滞/产出不足/提前退出（host-pty 退出即拆环 → 恒返回 none）→ 测试无限自旋且持全局锁挂整组。
  > 建议：加 deadline（镜像 wait_produced）+ none 分支游标推进守卫。
  > **已修（2026-10-03）**：fetch_until 加 `FETCH_POLL_TIMEOUT_MS`（8s）deadline，none 分支与游标停滞均 fail-visible。
- **[M-11] `manager/runtime/tests/http_e2e.rs:93-94` — 清理仅 happy path** 任何 expect/assert panic 泄漏资源：RouteTable 全局注册残留断重跑/同二进制其它测试；pty_e2e 常驻 shell + 无限生产者仅 happy path 杀掉，断言失败前留子进程空转至硬超时。
  > 建议：RAII 清理守卫（Drop）或 catch_unwind 包测试体。
  > **已修（2026-10-03，http_e2e 侧）**：`RegistryCleanup` Drop 守卫（panic unwind 也执行 purge）；pty_e2e 常驻 shell 各用例本已有 pty-kill 收尾。
- **[M-12] `manager/runtime/tests/pty_e2e.rs:501-504` — 背压用例依赖固定 400ms sleep** 每迭代 fork `/bin/sleep` 子进程，慢/加载 CI 生产速率可低于假设 ~1.3KB/s → 少于 512B 时多处断言伪失败。
  > 建议：有界轮询（反复 fetch 至 nextOffset > 512，带总超时）替代固定 sleep + 绝对阈值。
  > **已修（2026-10-03）**：改有界轮询（fetch(0) 循环至 nextOffset > 512，4s 总超时），产出速率只影响轮询次数不影响成败。
- **[M-13] `manager/runtime/tests/pty_e2e.rs:288-290` — 接线锁断言源码子串** 精确匹配 `host/activation.rs` 子串，rustfmt/改名/重构即破测试且不在运行路径执行。建议改行为断言（驱动真实 deactivate 观察 PTY 清空）或编译期契约。
  > **已修（2026-10-03，折中）**：断言放宽到 `host_api::pty::purge_for_plugin` 调用点存在（参数形态交给同用例的行为 purge 断言与编译期类型检查）——仍锁「activation.rs 接线了 pty 回收」，但不再被参数签名/rustfmt 变动弄断。
- **[M-14] `manager/watcher.rs:150-151` — extract_plugin_id 前缀不匹配丢事件** `strip_prefix(plugins_dir)` 要求精确前缀，但 plugins_dir 未 canonicalize（macOS `/private/var` symlink、`..` 组件）→ 事件全静默丢弃，热重载死亡无日志。
  > 建议：watch 前 canonicalize 一次，watch 与 extract 共用规范路径。
  > **已修（2026-10-03）**：start 时 canonicalize（失败保留原路径），watch 与 extract 共用规范路径。

### Low

- **[M-15] `manager/host/boot.rs:61-64` — notify_shutdown 重复文档行**（重构残留）。删。
  > **已修（2026-10-03）**：删除重复行。
- **[M-16] `manager/host/boot.rs:99-101` — 失效 deactivate-all 注释** 指向已移除方法。
  > **已修（2026-10-03）**：删除失效注释行。
- **[M-17] `manager/host/boot.rs:164-166` — auto_approve 前错位文档行** 属于 auto_activate_from_persisted_state。
  > **已修（2026-10-03）**：删除错位文档行（auto_approve_legacy_user_plugin 自带正确 doc）。
- **[M-18] `manager/host/boot.rs:321-326` — 「Auto-activated」计数为尝试数** 失败仅 per-plugin 记 error，最终诊断在部分失败时误导。改报实际成功数或改「attempted」。
  > **已修（2026-10-03）**：循环内计数实际成功激活数，最终日志报成功数。
- **[M-19] `manager/runtime/tests/terminal_output_perf.rs:291` — 4 MiB 注释与 3 MiB 常量不符** 若有人按注释改 4 MiB → 环容量=输出量开始 eviction 出现 truncated，破坏 `!truncated` 断言。改注释或常量对齐。
  > **已修（2026-10-03）**：P3 用例 doc 改 3 MiB（与 CATCHUP_BYTES 一致）。
- **[M-20] `manager/runtime/tests/pty_e2e.rs:89` — format! 组 shell 命令（潜在注入）** 当前 marker 纯数字无注入面，但模式可演变；建议改参数向量/env 传 marker。
  > **已修（2026-10-03，断言钉死）**：marker 生成处断言字符集为 alphanumeric+_（shell 安全）——将来改成含元字符的内容立即转红。
- **[M-21] `manager/task.rs:971-976` — droppedEvents 记错作业** 通道满丢弃时按 HashMap 迭代序给**第一个**匹配 owner 的作业计数，从不看事件 jobId → 多作业在途时自愈 status 错报。
  > 建议：EventEntry/enqueue_event 携带具体 job_id，给匹配作业计数。
  > **已修（2026-10-03）**：`job_for_drop_counting` 按事件自身 jobId 精确匹配 + 属主校验归账；新增 `dropped_events_are_counted_per_job_not_first_match` 测试。

---

## 5. 修复优先级建议（供排期）

**P0（安全/正确性，修前先过 §5.1 宿主边界裁决——注意多数落在 wasm_core 引擎层，属宿主合法区，但要确认不引入业务语义）：**

- S-01 API 劫持 · S-02 frontend_channel TOCTOU · S-03 symlink 逃逸 · S-04 审批非原子 · R-01 check_permission must_use · R-02 storage 隔离 · R-03 fuel 溢出 · H-02 wsl 注入 · H-04 WS 上限死代码 · M-01 boot panic 圈护 · M-02 task 终态污染 · M-03 started 顺序 · M-04 回调通道 TOCTOU · M-05 watcher 防抖 TOCTOU

**P1（高/中危高影响）：**

- S-05~S-14（security 其余）· R-04~R-06 · R-08·R-09·R-12·R-13 · H-01·H-03·H-05~H-08 · M-06~M-09·M-10

**P2（测试可靠性 / 低危 / 快速胜利）：**

- M-10~M-14·M-19·M-20（测试）· R-07·R-10·R-11·R-14·R-15 · H-09~H-12 · 全部 low · S-15·S-16

---

## 6. 审核过程元数据

- 命令形态：`ocr scan --path <模块> --audience agent --background "<业务上下文>" --output /tmp/ocr_<模块>.txt`
- 每模块独立 session（可 `ocr session list` 查看 / `ocr scan --resume <id>` 续跑）
- 输出文件仍在本机 /tmp（重启清空，重要条目已登记上文；如需要可另存仓库外备份）
- 修复时建议：**每个模块修完跑该模块相关测试 + §10 收尾验证**；low 项可与同文件 nearby 修复合并提交，禁止夹带无关改动（§11）