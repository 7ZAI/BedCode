# 11: 票 07 补齐 —— opencode SQLite 适配 + codex 预留 + 数据清空

**What to build:** 实施母 spec 的票 07。`rust/src/usage.rs:48` 的 `ADAPTERS` 仍是 `["claude", "pi"]`，而 spec §2 把使用统计列为 v1、§4.5 数据源明写 opencode = SQLite。三件事：(a) opencode 适配器读 `~/.local/share/opencode/opencode.db`，进同一套归一 schema，看板与日志视图自动覆盖第三家；(b) codex 适配器按官方格式预留骨架 + 「已装 · 未初始化」状态（`CliCard` 现有五态要加第六态）；(c) 数据保留策略：全量保留 + 手动清空命令与 UI 入口。

**Blocked by:** None

**Status:** resolved（2026-09-27 实施完毕；spec §6.1 裁决走方案 A，母 spec §2 未动）

## 实机核实（2026-09-27，方案 A 成本评估的依据）

SQLite 结构（`sqlite3 file:…opencode.db?mode=ro` 只读打开，**445 MB / 22 张表**）：

| 表 | 行数 | 与本票的关系 |
| --- | --- | --- |
| `session` | **54** | 聚合真源，**扁平列，一对一映射 `usage_session`**（母 spec 票 07 写的「12 个」已过时） |
| `message` | 1 259 | 事件流输入，`data` TEXT 是 JSON blob |
| `part` | 6 132 | 事件流输入，`data` TEXT 是 JSON blob |

`session` 表列（前 20，恰好覆盖归一 schema 全部需求）：

```
id, project_id, workspace_id, parent_id, slug, directory, path, title, version,
share_url, summary_*, metadata, cost, tokens_input, tokens_output, tokens_reasoning,
tokens_cache_read, tokens_cache_write, revert, permission, agent, model,
time_created, time_updated, time_compacting, time_archived
```

实施要注意的四个实况（都与原票的乐观描述有出入）：

1. **`model` 列是 JSON 串**，不是模型名：`{"id":"space-bunny-free","providerID":"opencode","variant":"max"}` → 需解析后取 `id`（或 `providerID/id`）再入 `models_json`
2. **`cost` 全为 0.0**（54 行中 `cost > 0` 的有 0 行）→ spec §4.5「有则存、null 不估算」正好适用：opencode 的成本列在看板/明细里应显示为空而非 `$0.00`
3. **9 个 session 的 `tokens_input = tokens_output = 0`** → 空会话要有专门处理（要么入表但零值、要么过滤），不能触发除零或空图表行
4. **时间戳是 epoch 毫秒**：`time_created = 1790301670571`（= 2026-09-25）→ 与 claude/pi 的 ISO8601Z 不同，`usage_parse.rs` 要新增一个适配器入口，不能复用 ISO 解析

事件流的额外成本（claude/pi 是 JSONL 逐行直读，opencode 不行）：要从 `message.data` + `part.data` 两层 JSON 里归出 user/assistant/tool/system 四类事件。`message.data` 实测形态：

```json
{"parentID":"msg_0d64a7b1a0015zUi8T6v9tjx1Q","role":"assistant","mode":"build",
 "agent":"build","variant":"max","path":{"cwd":"/home/binblink/project/…","root":…}}
```

codex「已装未初始化」的实况佐证：`~/.codex/` 存在且有 `config.toml` + `hooks.json`，但 `find ~/.codex -name '*.jsonl'` = **0** —— 装了没跑过会话，与 spec §9 记的「0.153.4 已装未初始化」一致（版本号已变，探测态展示要跟实际探测值走，不写死）。


## 验收标准

- [x] opencode 真实会话（本机 54 个）进入看板与日志视图；db 文件变更触发增量重扫且**幂等**（重复扫描不产生重复会话）
- [x] 上面 4 条实况全部处理：model JSON 解析、cost 空值展示、零 token 会话、epoch ms 时间戳
- [x] codex 骨架 + 「已装 · 未初始化」状态在概览卡片正确展示，en 文案同样正确
- [x] 手动清空：两击确认；清空后看板 / 列表 / 来源扫描计数一致归零；`parse_watermark` 与 `usage_session` 同事务清
- [x] 缺 `sqlite3` / 库不存在 / 查询失败 三类空态与降级文案走 i18n（zh-CN/en 同步）
- [x] 445 MB 的 db 不被整文件读进内存（SQLite 源按 session 主键分页 / 流式，禁 `SELECT *`）
- [x] `cargo test`（插件 crate 125）/ `pnpm exec vitest run wasm-apps/agent-hub`（205）/ `tsc --noEmit` / 根 `pnpm exec eslint .` 0 error 全绿
- [x] 母 spec `.scratch/2026-09-13-agent-hub/issues/07-stats-opencode-codex.md` 状态同步为 resolved 并回填 Answer

## Answer（2026-09-27 实施完毕）

### (a) opencode SQLite 适配

**通道选择：复用本插件既有的 `host-process`，同步跑 `sqlite3 -json -readonly`。**
备选是新增 WIT 原语，但 spec §2 非目标明写「不动 WIT / ABI」，而那要 ABI bump +
双端 WIT 副本同步 + 移动端影响评估；`host-process` 已是本插件安装域的既有通道
（`npm install -g`），git 域（terminal-session `file_browse::GitPort`）也用同款
`process_run_sync` 拿输出，属既有模式而非新依赖。SQLite 目标 JSON1 恒开（≥ 3.38）。

**新增 `rust/src/usage_sqlite.rs`**（新模块，只做取数与 SQL；解析仍在
`usage_parse.rs` 的纯函数里，合成层仍走既有 `upsert_session`）：

| 项 | 做法 | 实测 |
| --- | --- | --- |
| 聚合层 | 按 session 主键 **keyset 分页**（`WHERE id > ? ORDER BY id LIMIT 500`），封顶 20000 会话 | 54 会话 / 1 页 |
| 事件流 | `message × part` 联表按 `(mid, pid)` **复合游标**分页（同一 message 的后续 part 不能漏），每页 200 行 / 封顶 4000 行 | 最大会话 1329 part = 7 页 |
| 大 blob | `part.data` 最大 151KB（工具输出逐字回传）→ **在 SQL 侧** `json_extract` 展平成窄列 + `substr(state.output, 1, 600)` 截断 | 15.5MB raw → **977KB**（降 94%） |
| 坏 blob | 每个 `json_extract` 配 `json_valid()` 守卫（裸调遇非法 JSON 会中止整条查询，连已取到的行一起丢） | 用例钉住 extract/guard 计数相等 |
| 显式列 | 禁 `SELECT *`（opencode 升版加列会白搬数据） | 用例断言 |
| 零行 | `sqlite3 -json` 零行输出**空串**而非 `[]`，按空处理 | — |

**实机端到端验证**（把 Rust 生成的真实 SQL 抽出来，打到实机 445MB 库上跑）：

```
聚合层：分页数=1 累计=54 唯一=54 库中会话数=54 完整
事件流：分页数=7 累计行=1329 唯一(mid,pid)=1329 单调前进=True
        库中该会话 part 行数=1329 覆盖=1329 完整   累计字节=977552
```

即 **无重复、无缺口、单调前进、与库内行数精确相等**——不只是「跑通了」，是「数据对得上」。

**四条实况的处置**（全部有用例钉住）：

1. `model` 是 JSON **串** → guest 侧 `opencode_model_name` 解析取 `id`，备
   `providerID`；非 JSON 整串兜底（未来若改裸模型名仍能显示）；都不行才 `unknown`。
   **不用 SQL `json_extract`**：该列非 JSON 时会让整条查询报错。
2. `cost` 全 `0.0` → 按 spec §4.5「有则存、null 不估算」落 `None`（看板显示空而非
   `$0.00`）。0 在这里是「未上报」而不是「已知为零」。
3. 9 个零 token 会话 → 照常入库并计入会话数，但**不入 `models`**（否则按模型汇总
   多出一堆 0 消息的空模型）。
4. epoch ms → `time_created / time_updated` 直取，不经 `parse_iso8601_ms`。

**增量水位 = 内容指纹**（`parse_watermark.signature` 列，幂等迁移：SQLite 无
`ADD COLUMN IF NOT EXISTS`，故先 `PRAGMA table_info` 判定，同 `providers.rs` 的
api_key 迁移写法）。母 spec 写的是「mtime 变更触发」，但 **`host-fs.stat` 只给
`{size, isFile, isDir}`，WIT 无 mtime 原语**（usage.rs 水位注释里「WIT 无 stat
原语」说的就是这件事——stat 是 v19 后补的，仍无 mtime）。取
`{db 字节}:{-wal 字节}`：opencode 跑 WAL 模式，主库大小只随 checkpoint 增长，
**必须把边车一起纳入**，否则只写 WAL 未 checkpoint 的新会话会被漏掉。
指纹未变则整轮跳过（不跑 `sqlite3`）。

**失败可见（spec §8 fail-visible）**：三类成因各有机器可读 code
（`sqlite3-missing` / `db-missing` / `query-failed`）写进 `adapters.opencode.error`，
前端按 code 查 i18n 出**不同**的话（装 sqlite3 / 先跑一次 opencode / 看日志）——
三者对用户的动作完全不同，压成一句「同步失败」等于没解释。
`opencode` **不进 `per_adapter` 累加器**（那是文件枚举口径，`files/parsed` 会失真），
槽位由 `write_opencode_slot` 整体写。错误轮**保留上轮 sessions 计数**——不清空已有
数据，只标明本轮未刷新。打开单个 opencode 会话**显性报错**而非返回空事件流（空数组
会把「sqlite3 缺失 / 库被删 / 会话真没了」压成同一个「无记录」）。

### (b) codex 骨架 + 第六态

`SESSION_ROOTS` 增 `("codex", ".codex/sessions")`（官方 rollout 路径
`YYYY/MM/DD/rollout-*.jsonl`，递归 find 覆盖三层嵌套）；`parse_by_adapter` 增 codex
分支。新增 `parse_codex_session`：计费取 `event_msg.payload.info.last_token_usage`
（**不取 `total_token_usage`**，那是累计，取了就重复计数），`input_tokens` 含缓存须
减出 `cached_input_tokens` 归 `cache_read`，codex 无缓存写入恒 0；模型名来自最近的
`turn_context` / `session_meta` 的 `payload.model`（`token_count` 自身不带模型，故
`current_model` 是有状态游标）；`token_count.info` 可为 null（本轮首次事件）、全零
（限流重发）→ 不计费。未知 `event_msg` 变体静默跳过（Codex 每版本都在加新事件），
坏行计入 `skipped_lines` 不中断整文件。
**本机未初始化过 codex 会话，格式依据官方 rollout 文档（`codex-rs/protocol` 的
`RolloutItem` 定义）实现，属预留骨架，实机初始化后校准**——母 spec §10 待办 3 仍成立。

**「已装 · 未初始化」是派生事实，不是特例分支**：`useUsage.cliSessionState(cli)` 按
`status === 'ok' && authGranted && (files + parsed + sessions) === 0` 判 `empty`。
三种**未定**状态（未扫描 / 未授权 / 扫描中 / 扫描失败）一律 `unknown` → 仍走常规
「已装」——把「看不到数据」说成「装了没初始化」是误报。
**双安装的 warning 优先于第六态**（`CliCard` 里 dual 先判）：双安装是可行动的问题
（PATH 生效的是哪一个），缺数据只是缺席信号。这一条是被测试逼出来的：先写的实现把
`empty` 判在前，A7-1 的边界用例立刻变红，随后调整了判定顺序。

### (c) 数据清空

新命令 `agent-hub.clear-usage-data`（`plugin.json` 同增）：`usage_session` 与
`parse_watermark` 经 `plugin_db_execute_batch` 在**同一事务**内删（宿主实现是
`unchecked_transaction` + 逐条 + 统一 commit，任一句失败整体回滚）。两者必须同生
共死：只删水位 → 下轮扫描整文件跳过，那些会话永远回不来；只删会话 → 重复入库。
状态侧一并归零（各适配器计数含自定义来源 + `activeSessions`——否则列表行会继续打
「当前」标记而库里已无对应会话），`status` 回 `idle` 以便下次 idle 回流重新自动扫描。
保留策略是**全量保留不自动过期**：数据量级在会话数级别，远不到需要自动清理的阀值，
擅自过期会让历史看板出现无法解释的断层。UI 在统计页就地两击确认（不弹窗），含重入
守门（清空中再触发直接拒，不发第二条命令），成功后重拉看板与两份列表，不留
「空看板 + 旧列表」中间态。

### 旧状态兼容

票 06 写下的 `adapters` 只有 claude/pi、`sources` 只有两条。`read_state` **按名增量
补齐**新槽位与新来源（codex JSONL 目录 + opencode SQLite 库），**不重建整表**——
重建会丢用户已添加的自定义来源。有用例钉住
（`legacy_state_backfills_without_losing_custom_sources`）。

### 测试与验证

| 载体 | 数量 | 内容 |
| --- | --- | --- |
| Rust `usage_parse` | +12 | opencode 归一 / model 四形态 / 零 token / 四条实况 / codex 聚合·模型切换·工具轮·坏行·零用量 |
| Rust `usage_sqlite` | +4 | SQL 字面量转义 / keyset + 显式列 / 联表游标 + `json_valid` 守卫数相等 / 错误分类 |
| Rust `usage` | +5 | 四适配器默认态 / 枚举分段排除 sqlite / codex 分派 / 槽位只碰 opencode / 旧状态补齐 |
| vitest `useUsage` | +14 | U10 降级清单 / U11 会话状态 / U12 清空（含重入） |
| vitest `components` | +17 | A7-1 第六态四分支 / A7-2 降级横幅 / A7-3 两击确认三态 / A7-4 来源口径 |

**变异自检 8 项全部杀死**：

| 变异 | 期望变红 | 实测 |
| --- | --- | --- |
| 去掉 `cliSessionState` 的 `authGranted` 闸门 | U11 | ✅ 1 failed |
| 把未登记 code 也收进 `adapterErrors` | U10 | ✅ 1 failed |
| 去掉 `clearData` 重入守门 | U12 | ✅ 1 failed |
| `scanCount` 去掉 sqlite 分支 | A7-4 | ✅ |
| 两击确认退化为单击即清空 | A7-3 | ✅ 3 failed |
| codex 取 `total_token_usage` | Rust | ✅ 3 failed |
| codex 不拆出缓存 | Rust | ✅ 2 failed |
| opencode `cost = 0` 也当已知花费 | Rust | ✅ 1 failed |

**验证**：

- 插件 crate `cargo test --lib` **125/125** 绿（实施前 100）
- `pnpm exec vitest run wasm-apps/agent-hub` **205/205** 绿（实施前 172）
- 桌面端全量 `cargo test` **950 passed / 0 failed**（11 target，lib 936），与票 05 基线一致
- `tsc --noEmit` 0 error；`eslint bedcode-desktop/wasm-apps/agent-hub` **0 error 0 warning**；
  根 `eslint .` 0 error（118 warning 为全仓既有，含 `.scratch/evidence/ah-contrast.mjs`
  一处上一轮遗留）
- `cargo fmt`：`usage_parse.rs` / `usage_sqlite.rs` 整体已格式化（前者 @HEAD 是干净的，
  故全量格式化只动我的代码）；`usage.rs` 只手改了**我写的** 6 处，该文件 @HEAD 原有
  7 处 fmt 差异、现余 5 处且**全在既有代码**（两处 import 行、`builtin` 判空、
  `mark_active_rows`、`scan_script` 用例）——不顺手格式化相邻既有代码（AGENTS §0）

### 未覆盖风险

1. **codex 会话解析未经实机校准**：本机 `find ~/.codex -name '*.jsonl'` = 0，格式
   依据官方 rollout 文档实现。母 spec §10 待办 3「codex 会话/配置格式实机初始化后
   校准」仍成立。
2. **opencode 依赖宿主 `sqlite3`**：Windows 默认不带该可执行文件。本机
   `/usr/bin/sqlite3` 3.46.1 已验证；缺失时走 `sqlite3-missing` 显性降级 + i18n
   提示（不静默无数据），但该端暂无数据。是否随包附带或改走宿主原语，留待后续裁决。
3. **无真机截图核验**：本轮未起 dev-shell + LingLong Chrome。新增的降级横幅、清空区、
   来源形态标记三处 UI 未做视觉验收；`ah-matrix.mjs` 离线矩阵已把 4 个新文字元素纳入
   受审清单（降级文案 / 清空说明 / 成功提示 / 来源形态），可离线复核对比度。
4. `parse_opencode_events` 的 `MAX_EVENTS`（5000）与 `MAX_EVENT_ROWS`（4000）两道
   上限关系未做交叉用例（4000 行 < 5000 事件，故实际由行数上限先生效）。
5. `usage_parse.rs:400` 有一处 `msg["content"].as_array().unwrap()` 触发
   `rust-unwrap` 提示——**上一行 `is_array()` 已守卫**，安全，且属既有代码非本票范围，
   未动（AGENTS §0 最小改动）。
