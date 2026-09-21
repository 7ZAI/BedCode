# 04: 会话 / 终端域属主闭环（P0-3，终端命令注入）

**What to build:** 「先过权限门，再查属主」这条既有不变量补齐到会话与终端域：插件只能操作**自己创建**（或被显式授权）的会话，生命周期监听的注册需要权限。修完后，持 `terminal:input` 的第三方插件无法再向用户正在用的交互终端注入命令。

**Blocked by:** 01（权限位与门禁测试形态）

**Status:** done（2026-09-22 实施完成，见「实施记录」；注解槽的处置与票面略有出入，已单列说明）

## 现状（已复核）

- `host_impl/terminal.rs:16` `terminal_send` 只查 `terminal:input`，直接 `session_manager.write_input(session_id, ...)`，不比对归属；
- `host_impl/session.rs:453` `session_close`、`:506` `session_remove` 只查 `session:write`；`session_close` 的文档注释自己写「关闭**自己创建**的会话」，代码未实现；
- `host_impl/lifecycle.rs:11` `session_lifecycle_register` **无任何权限门**（对照同文件 `:26` 的 `session_input_register` 需 `terminal:observe`）→ 任意已激活插件可得全部 session id/名称，成为注入链的第一环；
- 反证「这是遗漏非取舍」：pty / ws / mdns 三域已有统一属主判定与文案（`host_impl/pty.rs:49`、`ws.rs:51,54`、`mdns.rs:91`），core-task 的任务记录带 `owner` 字段（`manager/task.rs:111,145`）——本票照此形态补齐会话/终端域即可，无需发明新机制。
- 待本票一并核实的两项（来源审查记录，未逐行复核）：`host_impl/peer.rs:61,141` 句柄表无 owner 列；`host_impl/session.rs:645` 注解槽为全局扁平 map，跨插件同键互相覆盖。

## 验收

- [x] **先落红测**：插件 B 用插件 A 的 session id 调 `terminal_send` / `session_close` / `session_remove` → 当前实现必须成功（这就是红），修复后必须 `not owner` 拒绝；照 `test_pty_isolation_and_contract_matrix_roundtrip` 的闭环形态
- [x] 会话创建时登记属主（`plugin_id`），`close/remove/rename/resize/annotate` 一律先权限后属主；`create-with-spec` 若允许指定 id（v19 重启编排用），需明确「同插件重启自己的」而非任意 id
- [x] `terminal_send` 增加属主判定；同时评估：终端输入注入是否需要与 `terminal:observe` 同级的**用户在场确认**（高危位，见票 03 裁决项 3）
- [x] `session_lifecycle_register` 补 `session:read` 门（与输入监听的 `terminal:observe` 分档保持：生命周期是元数据、输入行是明文凭据面）
- [x] 注解槽改按属主分命名空间（`plugin_id` + key），跨插件不可互相覆盖；`peer` 句柄表补 owner 列并统一错误文案 —— **注解槽换了实现路径，见实施记录第 5 条**
- [x] 界面维持：`com.bedcode.session` 插件的现有会话编排不因属主校验而 Broken（它本就是属主），WIT 函数签名不变则不 bump ABI；若签名要改（如加 requester 参数）→ 按 ADR 0022 双端偏离登记并说明移动端影响
- [x] 门禁：`cargo test` 全绿 + 相关 `session_e2e` / `terminal` 闭环用例全绿；`cargo check --lib --tests`（防假绿）

## Comments

- 2026-09-21 立项：来源 spec §4-P0-3。与票 03 的关系：票 03 截断「谁能拿到 `terminal:input`」，本票截断「拿到后能碰谁的会话」，两票正交、都要做。

### 实施记录（2026-09-22，票 04 done）

**1. 属主登记 = 内核新表，零线协议变化。** `SessionManager` 加
`session_owners: session-id → 创建方 plugin_id`（不透明串，内核只存不解释），
`create_session_from_spec` 加第 6 参 `owner: Option<&str>`，与会话记录**同批写入**
（创建失败已在上方 return，不会出现半态），`remove_session_with_source` 连带注销。
**刻意不放进 `SessionInfo` / `SessionInfoView`**：属主是宿主侧访问控制事实，
不是给前端/移动端的展示字段 —— 线协议形状零变化，符合「界面维持 + 老端忽略未知字段」。
WIT 函数签名也未动（`plugin_id` 本就是 Store state 由宿主写入并传给每个 host fn）→
**不 bump ABI**，移动端零改动。

**2. 判定链顺序：先权限门 → 再参数校验 → 后属主。** `ensure_session_owner`
（`host_impl/session.rs`，`pub(crate)` 供 `terminal.rs` 复用）错误文案
`not owner of session: <id>`，与 pty / ws / mdns 的 `not owner of ...` 同形，
且**不回带真实属主 id**（不把别的插件身份泄露给调用方）。属主判定排在参数校验之后，
既保住既有「参数门」用例的语义（空 id / 空名 / 非正尺寸 / 非法 requester 仍各自报错），
也让越权探测拿不到「该 id 是否存在」的差别信息。无属主（内核/宿主自建）一律拒 —
— fail-closed。

覆盖的六个面：`session_close` / `session_remove` / `session_rename` / `session_resize` /
`session_annotate` / `terminal_send`（终端命令注入是 P0-3 攻击链的收口一环）。

**3. 契约收缩（必须显式记账）：未知会话不再幂等成功。** 旧实现里
`close` / `remove` 对不存在的 id 返回 Ok、`rename` 返回 `Session not found`；
现在未知 id 判不出属主 → 与非属主同一拒绝口径。取舍：幂等成功等于允许对任意 id
盲发删除且不留痕，与票面「只能操作自己创建的会话」冲突，故选择显性拒绝。
已同步改的既有用例：`session_close_missing_session_returns_ok` →
`session_close_of_unowned_session_is_denied`（并补属主成功对照）、
`session_remove_is_idempotent_and_clears_state`、`session_rename_roundtrip_and_missing_session`、
`session_annotate_param_and_existence_gates`、e2e `test_session_annotate_and_devices_closed_loop`。
**待实机复核**：插件侧对「二次删除」的错误呈现（历史上是静默成功）；
宿主错误串是英文，若前端直显需按 AGENTS §6 走 i18n —— 本票未动插件编排与 UI。

**4. `session_lifecycle_register` 补 `session:read` 门**（此前**零权限要求**就能装监听器
枚举全部会话 id / 名称 / 工作目录，是注入链的第一环）。分档与票面一致：
生命周期 = 元数据（`session:read`），提交输入行 = 明文凭据面（`terminal:observe`，ADR 0001）。
新用例 `session_lifecycle_register_permission_denied`；
`session_lifecycle_register_services_not_ready` 改为授予后仍报两阶段未就绪（证明门在后）。

**5. 注解槽：用「属主独占写入」达成票面目标，没改存储键形状。**
票面写「按 `plugin_id` + key 分命名空间」，实测两条约束相互冲突：
`annotations` 槽的**读路径**是内核对外形状
（`session_list` / `session_get` 透传 + 票 12 的 `SessionInfoView` 任务字段机械映射
`task_fields_from_slot`），一旦按插件分命名空间，内核就必须回答
「`taskStatus` 该取哪个插件的槽」—— 那是把产品语义塞回内核（违 ADR 0022）。
而跨插件覆盖的**洞**已被第 2 条闭合：非属主写不进去（用例断言注解未被改）。
⇒ 净效果 = 每个会话的槽只有属主插件可写，等价于「按属主分命名空间」的隔离目标。
将来若真需要「多插件共享注解」，再引入键形状 + 由插件层解释取哪一份。

**6. peer 句柄表补 owner 列**（票面「待复核」项，核实为真：`SessionEntry` 无 owner）。
`owner` 在 `peer_dial` 成功铸造句柄时写入；`peer_close` 与 `with_auto_redial`
（数据面四函数共用）解析句柄后判属主，非属主报 `not owner of peer handle`。
两个细节：① `peer_close` 的属主判定**刻意排在 `require_app` 之前**，
无头/引擎未就绪环境下越权探测也得到同一答案（用例锁这条顺序）；
② 拒绝时把句柄 `restore_session` 放回，非属主的一次探测不得把别人的连接摘走（用例断言仍在册）。

**7. 红测证据（票面纪律）**：`session_actions_by_non_owner_are_denied`（close / remove /
rename / resize / annotate 五面 × 越权方，权限齐备只缺属主）与
`terminal_send_by_non_owner_is_denied` 先落；把 `ensure_session_owner` 临时改成恒放行后
两条均转红（前者报「非属主 close 必须被拒」，后者一路走到 `write_input` 撞
`AppContext not initialized` 而炸）——证明断言是**承重**的，不是恒真。已还原。
正例对照（防恒拒绝假绿）：属主自己 rename 放行、属主 close 放行、
`peer_close` 属主路径、以及缺 `terminal:input` 时报的是权限拒绝而非属主拒绝（两门可辨）。

**8. 会话 e2e 闭环的属主身份**：`test_session_task_domain_closed_loop` 与
`test_session_annotate_and_devices_closed_loop` 里由测试直接播种的会话，
现在必须登记成实际驱动它的插件（`com.bedcode.session`），否则插件自己的注解会被拒 —
— 这正是本票要的行为，用例随之修正（不是绕过）。

**9. 界面与编排维持**：桌面 4 插件全仓 grep 确认 —— 只有 `com.bedcode.session`
消费会话动作原语与 `terminal_send`（file-transfer 里的 `session-remove/rename/resize`
是 `#[plugin_api]` 防漂移镜像声明，注明「宿主命令面消费；本插件不消费」，无调用点）；
它同时声明 `session:read` / `session:write` / `terminal:input` / `terminal:observe`，
且**所有会话创建（桌面 UI、移动端 HTTP/WS、定时器）都经它的 `session-create` 编排**
（`utils/session_create_bridge.rs`，插件必需、无宿主降级），故它是全部会话的属主，
现有编排不 Broken。`session_id` 指定路径（v19 重启）保持「先 remove 再 create 同 id」，
宿主仍拒覆盖在册会话 —— 现在重启发起方也必须是原属主。

**门禁实测（2026-09-22）**：`cd bedcode-desktop/src-tauri && cargo test --lib`
→ **1091 passed / 0 failed**（含会话域 31 条与两条新闭环 e2e）；
`cargo check --lib --tests` 跑过，报出的编译失败**全部是本票之外的既有断链**
（`ws_session_route` / `pty_session_chain` / `ws_auth_rules` / `http_auth_biometric` /
`broadcast_shutdown` 五个集成 target 引用已退役的 `pairing_service` /
`QrTokenManager` / `SessionManager::from_database` / `restart_session`；
`git grep` 核实这些符号在 HEAD 的 src 里已不存在，且错误里无一处涉及本票改的符号）；
`pnpm exec eslint .` 0 error；前端 `pnpm exec vitest run --pool=forks --maxWorkers=2`
→ **76 files / 740 tests 全绿**。
rustfmt：本票改动的 LF 文件（session_manager / host_impl/session / terminal / peer /
session_e2e / sync_handler）全部 `--check` 零 diff；CRLF 文件（lifecycle.rs）保持纯 CRLF
（99 CR / 0 裸 LF），未整文件 rustfmt。测试后无残留进程。

**登记未做**：① 终端输入注入的「用户在场确认」按票面留给票 03 裁决项 3（本票不擅自加交互）；
② 移动端 SDK 的 host-session 面（v11）本就没有这些动作函数，属主改造不产生双端偏离，
无需登记偏离条目；③ 二次删除的 UI 文案 i18n 归属插件侧，待实机确认后另计。

## 补门项复核（2026-09-22，票 05 收尾时按交接 §4 追查）

交接文档记的「对侧新增会话域原语（`host-session.output-ring-fetch`，`cc74d76d9`）尚未过本票属主门，
下一条做会话域时补 `ensure_session_owner`」——**逐行核实后不成立，无需补门**：

- 实现已在：`host_impl/session.rs::session_output_ring_fetch` 内 `ensure_session_owner(...)`，
  且判定顺序与本票契约一致（权限门 `terminal:output` → 属主门），注释直接点名「票 04 P0-3」；
- 用例已在（同文件 tests）：`output_ring_fetch_permission_denied_without_terminal_output`
  （权限门先于属主/存在性——未授权插件连「会话输出是否存在」都不应可探知）、
  `output_ring_fetch_by_non_owner_is_denied`（越权方权限齐备仍被 `not owner` 拦，
  且属主侧报 `session output not found`，错误分档可辨）、
  `output_ring_fetch_roundtrip_and_catchup`（正路径与游标追平）；
- 结论：**本票无遗留尾项**。该条从「待补实现」降级为「已核实关闭」，
  交接文档同批改正，避免下一个会话重复补做或误开空票。

