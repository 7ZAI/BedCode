# terminal-session 插件 OCR 审核报告登记（统一修复待办）

**审核日期：** 2026-10-03
**审核工具：** alibaba/open-code-review（OCR 自管模式，sensenova / deepseek-v4-flash）
**审核范围：** `bedcode-desktop/wasm-apps/terminal-session/rust/src/` 全部 62 个 .rs 文件（32,086 行）
**审核方式：** 按领域分 7 批 `ocr scan`（audience=agent，输出落盘 /tmp/ocr_ts_*.txt）
**登记人/来源：** pi 会话（wasm_core 68 条修复后的延续任务）

> 目的：把 OCR 审核发现的**全部评论**逐条登记（含原文位置、严重度、类别、修复建议），作为后续统一修复的单一真源。未作任何过滤——low 级别也保留，修复时可按优先级取舍。**所有条目初始 triage 状态 = `open`**，修复后改为 `done` 并附 commit。

> 插件背景：`com.bedcode.terminal-session` 是桌面宿主的**会话/认证/配对中心** WASM 插件（wasm32-wasip3 组件，经 plugin-sdk WIT host-* 原语访问宿主）。会话真源（登记/状态机/生命周期分发/输入输出编排）；认证中心（JWT 签发验签、配对码/QR、p256 生物凭证验签、密钥环最多两代）；移动端远程终端连接的 HTTP/WS 端点（/api/auth/*、/api/sessions/*、/api/configs、事件通道 + 终端流两条 WS 端点）；task/ 为自动任务队列（pi/claude/codex/opencode 四家 agent CLI 适配器）。

## 0. 汇总

| 批次 | 路径 | 文件数 | 评论数 | high | medium | low | 输出 | 状态 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 认证中心 | auth_http + auth_records + pairing + keys.rs | 13 | 11 | 0 | 7 | 4 | /tmp/ocr_ts_auth.txt | ✅ 已登记 |
| 2 会话域 | session/ | 8 | 4 | 0 | 1 | 3 | /tmp/ocr_ts_session.txt | ✅ 已登记 |
| 3 任务核心 | task/queue + state + scheduled | 3 | 7 | 2 | 3 | 2 | /tmp/ocr_ts_task_core.txt | ✅ 已登记 |
| 4 任务适配器 | task/hooks + mod + agent + preset | 4 | 8 | 1 | 6 | 1 | /tmp/ocr_ts_task_adapter.txt | ✅ 已登记 |
| 5 HTTP/WS 面 | ws_* + sessions_http + http_routes + output | 6 | 6 | 0 | 4 | 2 | /tmp/ocr_ts_http_ws.txt | ✅ 已登记 |
| 6 根/引擎面 | lib + actions + devices + device_face + devices_events + environment + schema + launch + d10_contract_test | 9 | 28 | 2 | 15 | 11 | /tmp/ocr_ts_root*.txt | ✅ 已登记（d10 被 OCR 跳过，另手工审） |
| 7 配置/杂项 | config + consent + quick_actions + file_browse + policy + trust | 19 | - | - | - | - | /tmp/ocr_ts_config.txt | ⏳ |
| **合计** | — | **62** | **0** | **0** | **0** | **0** | — | 0/0 done |

## 批次明细

（每批次评论逐条登记，格式：`[ID] 文件:行 — 标题` + 类别/严重度 + 原文 + 建议 + 修复状态）

---

## 批次 1 — 认证中心（auth_http + auth_records + pairing + keys.rs，13 文件 / 11 条）

### High

（本批无 high，全部 medium/low。OCR 项目摘要称 biometric 4 条 high 与实际逐条严重度不符，以逐条为准）

### Medium

- **[T-A01] `pairing/mod.rs:13-15` [maintainability] — 文档自相矛盾：声明「唯一真源」又承认宿主保留降级轨** 文档先宣称「pairing 语义只此一份 / 双副本窗口关闭」，随即说明宿主 `src-tauri/src/utils/auth/` 保留降级轨且「仍须同步演进」——只要降级轨存在，语义就在两处，双副本窗口并未真正关闭，漂移风险是永久性的。措辞会误导后续编辑以为宿主侧可忽略。
  > 建议：改写为「规范实现在此；宿主降级轨必须保持行为等价——任何语义变更须同步镜像到 `src-tauri/src/utils/auth/` 并经等价测试门禁」。
  > **triage: open**
- **[T-A02] `pairing/mod.rs:4-5` [maintainability] — 迁移保证挂在不存在的测试上** 「两侧语义仍须同步演进」的门禁 `tests_pairing_equivalence` 在全仓不存在（仅本文件 2 处引用；唯一测试文件是 `d10_contract_test.rs`）。若该测试只在宿主仓库，插件 CI 无法强制等价约束 → 插件轨与降级轨漂移可静默溜过。
  > 建议：在本仓补等价测试（或 CI 强制跨仓检查），或写明宿主侧测试的具体位置/机制。
  > **triage: open**
- **[T-A03] `auth_records/store.rs:218-221` [bug] — upsert 的 ON CONFLICT(device_fingerprint) 不覆盖 id 冲突** 若新记录的 `id` 已存在于**不同 fingerprint** 的行上，该冲突目标不生效 → `auth_pairings.id` 唯一约束报错。mock 用 uid_hash 复用锚行 id（SQL 表达不了的路径）→ 调用方「总是提供新 id 或匹配 fingerprint」的不变量必须显式化。且 trait 文档写 INSERT OR IGNORE + 更新，实现是 upsert——文档需同步。
  > 建议：补 ON CONFLICT(id) 分支或显式化调用方不变量 + 修文档。
  > **triage: open**
- **[T-A04] `auth_records/store.rs:424-428` [maintainability] — mock 合并语义与真 upsert 分叉** (1) fingerprint 匹配时 mock 整条覆盖（`*existing = record.clone()`），真 SQL 用 COALESCE 保留 id/paired_at、入参 uid_hash=None 时保留旧值；(2) mock 做了 uid_hash 复用 id，真 store 未实现（trait 文档称 uid_hash 合并由 `super::ops` 调用方编排）。mock 下通过的测试可能在 wasm store 上失败或写坏数据。
  > 建议：mock 对齐真 SQL 语义，或把 uid_hash 合并下沉进 store 并在两处一致实现。
  > **triage: open**
- **[T-A05] `auth_records/store.rs:26-26` [maintainability] — plugin_meta 表依赖初始化顺序** `marker`/`set_marker` 读写 `plugin_meta`，但本 port 的 `SCHEMA` 从不建它（仅 `src/config/store.rs` / `src/schema.rs` 建）。若本 store 的 ensure_schema 先于 config schema 安装 → `no such table`。CREATE TABLE IF NOT EXISTS 幂等，建议也加入本 SCHEMA，或文档化并强制初始化顺序。
  > **triage: open**
- **[T-A06] `auth_http/biometric.rs:52-54` [bug] — new_nonce 生产路径 expect panic** `getrandom` 失败（宿主 RNG 暂时不可用）会 trap 整个 WASM 插件，而函数已返回 String 非 Result。违反 fail-closed 红线。
  > 建议：返回 `Result<String, String>`，`issue_challenge` 里 `let nonce = new_nonce()?;`。
  > **triage: open**
- **[T-A07] `auth_http/biometric.rs:100-101` [bug] — nonce 不匹配分支不消费挑战** 错误返回但挑战仍活跃，违反本模块文档「任一不满足 → 挑战作废并报错」「验证即消费，无论成功与否」。失败尝试应使挑战失效（移除或标记 used），保持严格单次使用 + fail-closed。
  > 建议：mismatch 分支 `registry.remove(fingerprint)`。
  > **triage: open**
- **[T-A08] `auth_http/biometric.rs:73-80` [performance] — 挑战注册表无界增长** 只在其后同一 fingerprint 被消费时清条目；已绑定设备签发后永不回来验证的挑战永驻 static map。
  > 建议：签发时机会性剪枝 `registry.retain(|_, c| now_secs().saturating_sub(c.created_secs) < BIO_CHALLENGE_TTL_SECS)` 或封顶 map 大小。
  > **triage: open**
- **[T-A09] `auth_http/biometric.rs:138-140` [bug] — 内部错误串泄漏** `verify_signature` 把 `verify_biometric_signature` 的原始错误（'Invalid public key: ...' 等）直传调用方，泄漏实现细节且破坏与宿主 1009 分类的用户可见文案对齐。
  > 建议：`.map_err(|_| MSG_SIGNATURE_INVALID.to_string())?` 后再做 `!` 检查。
  > **triage: open**

### Low

- **[T-A10] `auth_records/store.rs:180-184` [performance] — 全结果数组 clone** `rows.as_array()?...clone()` 后 `array.iter()`，每次调用复制整个查询结果集。
  > 建议：直接借用切片 `rows.as_array().ok_or_else(...)?.iter().map(mapper).collect()`。
  > **triage: open**
- **[T-A11] `auth_records/store.rs:92-93` [documentation] — 排序契约与实现矛盾** trait 文档称「倒序由调用方排序；此处原样返回」，但 wasm 实现 `ORDER BY connected_at DESC`、mock 也降序排。契约/实现/文档三方不一致（索引 `idx_auth_history_device` 就是为此排序建的）。
  > 建议：去掉 ORDER BY 或改文档使契约匹配实际。
  > **triage: open**

---

## 批次 2 — 会话域（session/，8 文件 / 4 条）

### Medium

- **[T-B01] `session/ops.rs:53-53` [maintainability] — is_legal 文档/契约分歧** 文档称「`Idle` 只能被 `Starting` 认领」，但 match 分支也允许 `Idle -> Stopped`（已实测确认：`Idle => Starting | Stopped`）。若直接关停 idle 会话是有意的，需更新文档并补测试钉死（如 `is_legal(Idle, Stopped)` true + 从 Idle 记录 transition 填 `stopped_at` 而 `started_at` 为 None）；若无意，该分支是未文档化的状态机漏洞应移除。
  > 建议：二选一——改文档 + 补测试，或删分支。
  > **triage: open**

### Low

- **[T-B02] `session/ops.rs:105-107` [maintainability] — now 时间戳无格式/顺序/单调性校验** `now` 原样写入 `started_at`/`stopped_at`/`updated_at`，坏钟或畸形串（`updated_at` 早于 `created_at`、`stopped_at` 早于 `started_at`、Idle 停掉缺 `started_at`）静默破坏数据完整性。本模块是确定性策略核心（「所有策略在这里可确定性断言」）。
  > 建议：域边界校验时间戳形状（畸形返回 Err）或显式文档化调用方不变量（now 必须是良构单调 ISO-8601）。
  > **triage: open**
- **[T-B03] `session/ops.rs:116-119` [maintainability] — new_record 绕过 UUID-v4 不变量** 任意 `id`/`config_id`/`name` 字符串直接入记录，`new_session_id` 集中的 UUID-v4 契约在此不强制——空串/非 UUID id 均可表示（store-restore 路径可能绕过）。
  > 建议：在域边界校验 id 形状（返回 Result 或 assert），或用枚举建模两阶段创建代替 `start: bool` 标志使非法组合不可表示。
  > **triage: open**
- **[T-B04] `session/model.rs:50-52` [performance] — wire_name 每次调用分配 String** 返回值集合是固定 `'static str` 字面量，却 `to_string()` 分配新 String 并返回 String（用于 SessionSummary.status 与 sync 事件状态串）。
  > 建议：返回 `&'static str`（或 Cow），调用方需要所有权处再 `.to_string()`。
  > **triage: open**

---

## 批次 3 — 任务核心（task/queue + state + scheduled，3 文件 / 7 条）

### High

- **[T-C01] `task/state.rs:460-463` [bug] — insert_task_row 吞掉 INSERT 失败，继续广播/派发** 写历史失败/缺行被当 log-only：`insert_task_row` 丢弃 INSERT 错误返回 `()`（已实测：Err 仅 `host.log_error`），调用方照常发布 `in_progress`（bus/emit/WS）与投影槽位——DB 无行却发幻影事件；status 更新路径 `UPDATE task_history` Err 或缺行（非 idle 状态）也落入广播尾 + `try_dispatch_next`。可致重复任务、往忙碌终端派发。
  > 建议：返回 Result/bool，失败/无行时短路广播、槽位投影与派发；`Err` 与 `Ok(0)`/无行分开处理。
  > **triage: open**
- **[T-C02] `task/scheduled.rs:333-336` [bug] — scheduled 任务 DB 变更结果全被 `let _` 丢弃** creating 迁移失败 → 任务留 pending、下个 tick 重复建会话；`add_task_with_source` 入队失败 → prompt 静默丢失却仍标记 executed；executed UPDATE 失败 → 任务卡 creating 直到 step-0 watchdog 翻 failed。以上都不进 `failures` 向量，tick 仍报 Ok(())，`executed` 也未等所有 prompt 入队后才标。
  > 建议：逐个检查 Result/affected-rows、记 failures/log；只有写成功后广播 creating/failed/executed；executed 在全部 prompt 入队后才标记。
  > **triage: open**
- **[T-C03] `task/scheduled.rs:404-404` [bug] — Err 被当成「会话不存在」→ 批量取消 pending** `view_via_host(...).ok().flatten().is_none()` 把宿主/DB 错误（Err）与会话不存在（Ok(None)）等同对待（已实测）。瞬时错误会把该 scheduled 会话的全部 pending prompt 批量取消并广播 cancel，用户任务被一次瞬时抖动销毁。
  > 建议：只对 `Ok(None)` 走取消路径；`Err` 记入 failures/log 按步骤失败处理。
  > **triage: open**

### Medium

- **[T-C04] `task/state.rs:548-554` [bug] — create_task_from_input 缺 WS 广播通道** bus + emit + 槽位投影都有，独缺 `broadcast_task_status`（create_task_from_dispatch / hook 路径 / interrupt_running_tasks_on_session_end 都用）→ 移动端收不到用户提交任务的 `in_progress` 事件，违背模块单点 WS 广播策略。
  > 建议：补 `broadcast_task_status(host, session_id, "in_progress", Some("User submitted input"), None)`。
  > **triage: open**
- **[T-C05] `task/scheduled.rs:76-79` [bug] — trigger_at 只查空不验格式** 文档要求 `"YYYY-MM-DD HH:MM:SS"` UTC，畸形值存为 pending；所有 due/missed/timeout 判定靠与 `datetime(...)` 输出的字典序比较 → 该任务静默永不触发且无法重置。
  > 建议：INSERT 前校验/解析格式（如 NaiveDateTime::parse_from_str），至少拒绝明显非法输入。
  > **triage: open**

### Low

- **[T-C06] `task/state.rs:300-304` [performance] — pending_count 循环内重复查询** 循环前已 UPDATE，迭代期间值不可能变，却每个排队 id 一次 DB 查询（N+1）。
  > 建议：提升为循环前单个 `let pending = ...`。
  > **triage: open**
- **[T-C07] `task/scheduled.rs:250-250` [maintainability] — 批量迁移广播空 job_id** step 0（creating 超时→failed）与 step 1（missed）一次 tick 可影响多行，payload 无 job 关联信息，前端无法正确更新。
  > 建议：按受影响 job 逐条广播或在 payload 带计数/ids。
  > **triage: open**

---

## 批次 4 — 任务适配器（task/hooks + mod + agent + preset，4 文件 / 8 条）

### High

- **[T-D01] `task/hooks.rs:1277-1279` [bug] — hook 命令用 POSIX env 前缀语法，Windows 上静默失效** `BEDCODE_PORT=<port> <cmd> ...` 只在 sh/bash 有效；Windows cmd.exe/PowerShell 下被解析成命令名，hook 启动失败——而本文件明确目标平台含 Windows（`python_interpreter_for(Some("windows"))` => `python`）。`build_codex_hooks_config` 同样问题。
  > 建议：平台适配形式（`cmd /c "set BEDCODE_PORT=...&& python ..."` / PowerShell `$env:BEDCODE_PORT=...; python ...`）或改走其他通道传 port。
  > **triage: open**

### Medium

- **[T-D02] `task/hooks.rs:1066-1066` [bug] — hook 谓词收到错误对象（整文件 vs 内层 hooks）** `hooks` 是整个解析后的 hooks.json 文件（真 hooks 在顶层 `"hooks"` 键下，如 1165 行 `hooks.get("hooks")` / `merge_codex_hooks(&existing_hooks, ...)` 所示），但 `is_codex_hooks_configured(&hooks)` 与 `codex_hooks_port_matching(&hooks, port)` 传整文件（已实测 1602-1630：遍历顶层键的 value 是对象非数组 → 恒 false）→ `needs_update` 恒 true，端口/版本跳过逻辑是死代码，每次建会话都重拷脚本重写 hooks.json。Claude 路径 1165 行正确传 `settings.get("hooks")`。
  > 建议：两个检查都传 `hooks.get("hooks")`。
  > **triage: open**
- **[T-D03] `task/hooks.rs:1650-1652` [bug] — 清理时全组保留/删除，混组丢用户 hook** 事件组保留仅当组内**所有** hook 命令都不含插件脚本名；一旦某组混入插件 hook + 用户 hook（手工编辑 settings.json 或第三方工具），整组被删。`merge_script_hooks` 的 ensure 侧同样全有或全无。
  > 建议：按单个 hook 粒度过滤——只删插件命令，保留同组内用户 hook。
  > **triage: open**
- **[T-D04] `task/hooks.rs:182-184` [bug] — settings.json 损坏时按 `{}` 继续 → 覆盖写丢全部用户配置** 注释声称目标是避免静默丢弃用户内容，但把解析失败当 `{}` 继续，函数随后把合并结果（插件 hooks + `{}`）写回损坏文件——其余用户配置键全部丢弃。`ensure_codex_hooks` 同样模式。
  > 建议：解析失败时中止报错（可选备份文件）而不是覆盖写。
  > **triage: open**
- **[T-D05] `task/mod.rs:862-865` [bug] — 配置读取失败静默降级为空列表** `.ok()` 丢弃错误 + `unwrap_or_default()` 让「读失败」与「无配置」不可区分（已实测）；`list_configs_with_support_via_host` 消费方（HTTP supported-agents、queue-add 支持门控）在瞬时 `list_via_host` 失败时会得出「无 agent 受支持」。
  > 建议：返回 Result 让调用方暴露错误，或至少 `host.log_warn` 后再回退。
  > **triage: open**
- **[T-D06] `task/mod.rs:212-215` [bug] — panic 破坏 D7 单域降级契约** 契约只对 `Result::Err` 生效：域闭包 panic 会穿出 `run_tick_domains`，剩余域不跑（async store 下可能拖垮实例）。
  > 建议：每步 `std::panic::catch_unwind`（记录为 TickFailure 继续跑——注意仅当目标以 unwinding 而非 `panic="abort"` 编译才有效），或显式文档化「panic 视为整个实例失败、不在隔离保证内」。
  > **triage: open**
- **[T-D07] `task/mod.rs:148-148` [maintainability] — ensure_schema_via_host 顺序只靠调用点纪律** 文档要求先跑 `crate::schema::migrate_via_host()`，但无强制；先调则 `CREATE TABLE IF NOT EXISTS` 建出空的前缀表 → 幂等改名迁移跳过这些条目，旧数据滞留旧表名（升级症状：列表变空）。
  > 建议：运行时强制顺序（查 schema-version/migration 标记行，未迁移则报错）或统一入口收敛两调用。
  > **triage: open**

### Low

- **[T-D08] `task/hooks.rs:1298-1298` [security] — hook_script_path 未转义插值进 shell 命令** `hook_script_path`（源自用户可控 `working_dir`）只加引号不转义拼进 shell 命令，存 hooks.json 后由 agent 在用户 shell 执行——含 `"` / 反引号 / `$(...)` / `;` / `&` 的项目路径可产生畸形或可注入命令。
  > 建议：转义 shell 元字符或校验路径后再拼命令串。
  > **triage: open**

---

## 批次 5 — HTTP/WS 面（ws_terminal + ws_events + ws_control + sessions_http + http_routes + output，6 文件 / 6 条）

### Medium

- **[T-E01] `ws_terminal.rs:729-733` [other] — on_session_terminated 持全局锁跨宿主调用** `iter_mut().map()` 闭包内持 `CONNECTIONS` 锁调用 `pty_ring_fetch` / `ws_send_binary_to_client` / `send_text`（已实测），违背本模块 `drain_for` 自己立的规矩（锁内只拷贝状态、锁外做宿主调用）。宿主调用阻塞（慢客户端满邮箱）或重入插件（send 触发 client-disconnect）→ 整张连接表与所有 drain/subscribe 回调被卡死（单线程运行时下死锁）。
  > 建议：镜像 `drain_for`——锁内快照 `(pty_id, cursor, watermark)`，锁外拉尾帧/发送，再回锁重置状态。
  > **triage: open**
- **[T-E02] `sessions_http.rs:133-133` [bug] — cols/rows 用 `as u16` 静默截断** JSON 整数 >65535 静默回绕成非法 PTY 尺寸（已实测）；resize 路径还把缺失/零值当 0 传给 `session_resize`，与 start 路径 cols/rows > 0 检查不一致。
  > 建议：`u16::try_from` + 范围校验，越界/缺失按业务错误拒绝。
  > **triage: open**
- **[T-E03] `sessions_http.rs:160-163` [bug] — device_name 缺省语义自相矛盾** 文档注释「无 deviceName → 桌面/缺省源」，但 start/stop/remove 三处 `sessions-refresh` 事件缺省回落 `"mobile"`（已实测），与 resize 路径把无设备名归为 `kind:"desktop"` 矛盾——桌面端触发的事件被误标 mobile。
  > 建议：缺省统一用 `"desktop"`（或统一语义单一解析器）。
  > **triage: open**
- **[T-E04] `sessions_http.rs:82-86` [bug] — 畸形插件响应静默变空成功** list 把缺失/非数组 `sessions` 变 `[]`（`unwrap_or_default`），history 把缺失/非数组 `data` 或缺失 offsets/historyBytes 变空字节/0——契约违规被伪装成 200 OK。
  > 建议：必填字段缺失/畸形时抛 `sessions_error`，只有真·空数组/空历史算合法。
  > **triage: open**

### Low

- **[T-E05] `ws_terminal.rs:400-401` [bug] — 订阅状态先提交，subscribed 回帧未确认送达** `send_text` 失败（满邮箱）→ 连接留在已订阅态（session_id 已设、cursor 0、watermark 重置）但客户端从未收到 subscribed → 其本地 ack 基线未清零，后续输出帧对不上基线；错误帧随后 best-effort 大概率同样失败。
  > 建议：回帧发送成功后再提交状态（或失败回滚），subscribe/unsubscribe 都处理。
  > **triage: open**
- **[T-E06] `ws_terminal.rs:627-627` [maintainability] — 退出竞态检测依赖宿主错误文案子串** `msg.contains("not found")` 一旦宿主改措辞，预期中的退出后竞态被静默重分类为硬 Err 并上传调用方（session_stopped/tail 路径降级）。
  > 建议：用宿主 API 暴露的错误 kind/code，或在宿主边界保留错误类型处做分类。
  > **triage: open**

---

## 批次 6 — 根/引擎面（lib + actions + devices + device_face + devices_events + environment + schema + launch，8 文件 / 28 条；d10_contract_test.rs 被 OCR scan 确定性跳过，见附录）

> 注：本批文件较多，OCR 实际分 3 次跑（device_face+schema / lib+launch+devices_events / actions+devices+environment），评论互不重叠。

### High

- **[T-F01] `schema.rs:152-154` [bug] — 迁移非原子且账本最后才写** rename_table/indexes_of/drop_index 中途失败（SQLITE_BUSY 等）或进程死在 rename 与 write_ledger 之间 → 已改名表永远不在账本里；重跑无法修复（表已无旧名 → 被算作 already_prefixed 跳过且不回填账本），回滚窗口永久丢失（死在 rename 与删旧索引之间还会留下重复旧索引——模块文档明确点名要避免的故障模式）。
  > 建议：每次成功 rename 后立即写账本（write-through），或整段迁移包单个事务。
  > **triage: open**
- **[T-F02] `devices_events.rs:154-158` [bug] — 持锁跨同步宿主回程** 临时 `MutexGuard` 活到 let 初始化结束：map miss 时 `.or_else(|| resolve_identity(...))` 的 `ws_connection_context` 同步宿主调用在锁内执行（已实测）→ connect/disconnect/clear_connected 全被卡住，宿主同步重入插件时非重入 Mutex 死锁。
  > 建议：先 `remove` 取回 remembered，释放锁后再 `or_else(resolve_identity)`。
  > **triage: open**

### Medium

- **[T-F03] `device_face.rs:300-302` [bug] — 静默缺省违背「fail loudly」契约** `config_seconds(NetworkPort, 0)` 把缺失/损坏的 network.port 变成 0；空 IPv4 fallback 产出 `"host": ""` 的 QR payload；宽松设备过滤把缺 id/deviceName/deviceFingerprint/pairedAt 的记录静默丢弃（损坏表现为「无已配对设备」）。
  > 建议：显式报错或至少表面化错配，而不是暴露空/缺省值。
  > **triage: open**
- **[T-F04] `device_face.rs:260-261` [bug] — pairedAt 按裸字符串排序** 字典序只在所有时间戳同 offset 时等于时间序；RFC3339 混合 offset（`...T00:00:00Z` vs `...T01:00:00+02:00`）会排错序，破坏「最新配对在前」契约。
  > 建议：解析为时间点排序（或至少归一化 UTC）。
  > **triage: open**
- **[T-F05] `schema.rs:286-288` [security] — 账本驱动的标识符未引号拼 SQL** `format!` 直插标识符无引号；迁移路径只用模块常量，但 `rollback_to_legacy_names` 把 `plugin_meta` 账本读回值喂进 rename_table/drop_index（已实测：`ALTER TABLE {} RENAME TO {}`）。被篡改/恢复的账本条目含双引号或 SQL 关键字 → 破坏或注入 SQL；损坏账本解析检查不拦（引号在 JSON 字符串内合法）。
  > 建议：标识符加引号（`ALTER TABLE "x" RENAME TO "y"`）或按 `[A-Za-z_][A-Za-z0-9_]*` 严格白名单校验后再拼。
  > **triage: open**
- **[T-F06] `actions.rs:449-474` [bug] — restart 非原子：先删旧再建新，失败即永久丢会话** `note_removed` + `pty_kill` 在 `spawn_session` 之前；spawn 失败 → 旧 PTY 已杀、新 PTY 未建，会话永久消失（已实测）。且 PENDING_RESTART 条目（464 行 push）失败路径不清理——同 session_id 日后重建会发幽灵 `session-restarted` 事件。
  > 建议：spawn 失败路径移除 pending 条目 + 回滚/恢复已删记录。
  > **triage: open**
- **[T-F07] `actions.rs:105-106` [bug] — parse_canonical 非对象 JSON 当「无 canonical renderer」** `session_json.get("canonicalRenderer")` 对任何非对象值（串/数/数组）都返回 None——畸形宿主载荷被静默当缺失，`decide_resize` 可能批准本不该有的未确认接管。
  > 建议：先校验 `is_object()`，非对象显式报错。
  > **triage: open**
- **[T-F08] `lib.rs:790-796` [bug] — 兜底 `_` 分支把终端帧送进 JSON 控制解析器** 任何未解析端点（含 resolve_endpoint_path 失败时的终端端点）落入 `ws_control::on_client_message`；终端帧是裸二进制 PTY 字节，被当 JSON 控制帧解析可误解析/误解，未知端点的控制帧也被接受。
  > 建议：显式匹配两个已知路径，其余返回错误/drop（fail-closed）。
  > **triage: open**
- **[T-F09] `devices_events.rs:133-136` [bug] — CONNECTED_DEVICES 仅按 client_id 键控且无条件覆盖** 无连接 epoch/实例；迟到的旧连接 disconnect 会消费**新**连接条目（在线时误关其 auth record / 发 device:disconnected）；重复/乱序 connect 盲目重插条目、重触 auth record、重发 device:connected → 幽灵记忆。
  > 建议：按连接 epoch/实例键控（或校验 fingerprint 匹配），connect/disconnect 幂等化：已有活条目不重触/不重发，只消费身份匹配的条目。
  > **triage: open**
- **[T-F10] `devices_events.rs:159-164` [bug] — resolve_identity 失败仅 debug 日志，持久 auth_record 永不关闭** 无记忆且 fallback 失败（模块自己文档化的反注册竞态）→ 直接 return，该设备 auth_records 持久条目永远 open——把「从未配对」与「身份丢失」混为一谈，且违背模块「任何让真源变 stale 的路径要 warn 级可见」的契约；`resolve_identity` 错误经 `.ok()?` 丢弃无上下文。
  > 建议：升 log_warn（含错误）+ 考虑对多次 miss 后遗留 open 记录加 reaper/age-out 策略。
  > **triage: open**
- **[T-F11] `devices.rs:152-159` [bug] — paired 合并忽略 conn.authenticated** 仅按 fingerprint + active pairing 合并，未认证连接报个可信设备的 fingerprint 就被渲染为 paired——不可信连接被误标成已识别设备。
  > 建议：合并门上加 `conn.authenticated`（与「在线判定」语义一致），或文档化为什么 fingerprint 匹配单独即权威。
  > **triage: open**
- **[T-F12] `devices.rs:161-164` [bug] — 归属仅靠 device_name，空名连接可认领全部空名会话** 无名字连接 fallback `""` 可认领所有 `canonicalRenderer.deviceName == ""` 的会话；同名连接各自重复计数同一批会话（跨行重复）。
  > 建议：空名连接不拥有任何会话 + 文档化/校验「一名一连接」假设。
  > **triage: open**
- **[T-F13] `environment.rs:18-18` [maintainability] — 宿主边界错误被拍平成裸字符串** `map_err` 只留 `e.message` 加固定前缀，丢 kind/code 与「哪个原语失败」；devices.rs 的 `records().map_err(|e| e)` 是 no-op 恒等闭包。
  > 建议：保留结构化错误/code（或 `?` 前加稳定 per-source 上下文）。
  > **triage: open**
- **[T-F14] `devices.rs:147-147` [maintainability] — 配对模型是位置化 4 元组** 位置 bool 是 is_active，两处使用（paired 合并 + connect_list_via_host 映射）都要记字段顺序，错序静默编译通过。
  > 建议：建 `PairingRow { id, device_name, fingerprint, is_active }` 命名结构体。
  > **triage: open**
- **[T-F15] `launch.rs:280-282` [bug] — 盘符分支非 ASCII 首字符 panic** `chars().nth(1) == Some(':')` 对 `"€:\\foo"` 也为真（多字节首字符 + 冒号），`&path[2..]` 按字节切非字符边界 → 运行期 panic；`working_dir` 来自配置 wire（不可信）可崩整个 WASM 插件（已实测）。
  > 建议：字节级冒号检查（保证首字符单 ASCII 字节，`path[2..]` 合法边界）或 `path.find(':')` 后处理 None/索引 != 1。
  > **triage: open**
- **[T-F16] `lib.rs:670-673` [bug] — session-input 互调契约字段名分歧** trait 文档声明 wire 形状 `{sessionId, data, special?}` 且 `special = true`，实现与命令侧文档期望字符串字段 `specialKey`——按文档发的 `special: true` 被静默忽略（走 commit-line 重建，改变特殊键输入语义）。
  > 建议：统一契约——实现支持 `special`/boolean，或修 trait 文档为 `specialKey`。
  > **triage: open**

### Low

- **[T-F17] `schema.rs:245-247` [bug] — column() 形状不匹配静默映射为空** `meta_get`/`read_ledger` 读到不可读但可解析的宿主响应 = 「无账本」→ 全量迁移覆盖上一份回滚账本。
  > 建议：返回 Result，意外行形状报错，防宿主 wire 契约漂移静默销毁回滚数据。
  > **triage: open**
- **[T-F18] `schema.rs:212-213` [bug] — write_ledger 无条件执行而 ledger_written 只跟踪 rename** 无条目 rename 时账本仍被写（清账/全歧义重写同账本），报告却称「未写」——与字段文档含义（账本是否被写过）矛盾。
  > 建议：write_ledger 只在有实际变更时执行。
  > **triage: open**
- **[T-F19] `lib.rs:728-729` [bug] — 非测试路径 expect("fresh token active")** `QrTokenManager::generate` 后无活跃 token 会 abort 整个 WASM 插件；函数已返回 Result，可传播。
  > 建议：`ok_or_else` 转可恢复错误。
  > **triage: open**
- **[T-F20] `launch.rs:801-801` [bug] — request.start 解析了但从不读** `build_launch_spec` 硬编码 `true`，显式 `start: false` 被静默覆盖为总是启动；字段文档仍宣传两阶段启动决策。
  > 建议：honor `request.start`，或删字段并文档化 wire 字段被忽略。
  > **triage: open**
- **[T-F21] `launch.rs:224-224` [maintainability] — LaunchSpec.name 永远空串占位** 无生产调用方填它（create_via_host 把 unique_name 单独传给 spawn_session），spec.name 在所有真实路径为空；若 spec 被序列化（宿主调用/事件/diagnostics）会话名为空。
  > 建议：create_via_host 在唯一名解析后填 name，或删掉真未用字段。
  > **triage: open**
- **[T-F22] `actions.rs:570-571` [security→设计评审] — requester/force 信任边界（已降级）** OCR 声称 requester/force 直接从客户端 JSON 反序列化可冒充；**实测修正**：`requester` 由 `device_name(device)` 派生，device 来自 **JWT claims + 宿主转发**（sessions_http.rs:35 文档），非客户端可控；`force` 是客户端弹窗用户确认后按设计重发（actions.rs:56 文档）。真实剩余风险：force 可被任一已认证设备用于接管他人会话的 canonical renderer，确认弹窗流程是否强制是唯一 force 路径需要设计文档化/复核（HTTP 面在宿主 TrafficFilterChain + 认证中心策略之后，非匿名可达）。
  > 建议：保持 requester 宿主注入；文档化 force 弹窗确认流程为唯一路径；`remove_via_host` 的 sourceDevice 同理复核。
  > **triage: open**
- **[T-F23] `actions.rs:585-589` [bug] — Apply 分支先 pty_resize 后 note_canonical** 注册表写失败 → PTY 已改尺寸但属主未更新，返回错误——终端状态与属主记录不一致；后续 resize 看旧 canonical 可能错误要求确认（或旧主绕过确认）。
  > 建议：先写属主记录（失败按关键错误），或注册失败时补偿回滚 resize。
  > **triage: open**
- **[T-F24] `devices.rs:158-158` [bug] — .find 取首个 active 配对，歧义时非确定** 若宿主允许同一 fingerprint 两个 active 配对（重配对遗留陈旧重复记录），渲染结果任意。
  > 建议：把 fingerprint 唯一性当宿主不变量（assert/校验），或收集全部匹配并排名/歧义报错。
  > **triage: open**
- **[T-F25] `devices.rs:184-184` [documentation] — 模块文档与行为分歧（计数含 Stopped）** 文档定义「该设备当前在看的会话」，owned 只按 canonical renderer 名过滤，Stopped/历史会话被计入（测试还锁定了「含 Stopped 历史」）。
  > 建议：文档对齐行为，或按会话状态过滤如果「当前」才是口径。
  > **triage: open**
- **[T-F26] `devices.rs:151-151` [performance] — clone().unwrap_or_default() 每次分配 String** 只为与 canonical_device 比较。
  > 建议：`conn.device_name.as_deref().unwrap_or("")`。
  > **triage: open**
- **[T-F27] `environment.rs:23-26` [maintainability] — cfg(not(wasm32)) 硬错桩实际编译进所有非 wasm 构建** 文档标注「native cargo test 路径」，实际 `#[cfg(not(target_arch = "wasm32"))]` 把硬错桩编译进每个非 wasm 构建（含未来的 native 二进制），非 test 调用方会拿到运行期错误而非编译期信号。
  > 建议：真 test-only 用 `#[cfg(all(not(target_arch = "wasm32"), test))]`；否则改文档为「通用非 wasm fallback」。
  > **triage: open**

### d10_contract_test.rs 手工审（OCR scan 确定性跳过 `*_test.rs`，0s 0 token，两次验证；规则未排除）

（待补：手工过一遍 1113 行契约锁测试后登记）
