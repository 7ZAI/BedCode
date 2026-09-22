# 03: 桌面审批链闭环（P0-2，ADR 0020 落地）

**What to build:** 用户安装的 zip 插件在**首次启用前**必须经人工批准其权限清单，且批准与插件目录内容哈希绑定——批准后换文件即撤销批准。桌面 `security/approval.rs` 从死码变成激活路径上的闸门；移动端已有的形态（`bedcode-mobile/src-tauri/src/plugin/manager.rs:633,652,659`）作为行为参照，前端审批 UI 复用同一交互语义。

**Blocked by:** 01（权限清单展示与过滤依赖词汇真源）

**Status:** done（2026-09-22 实施完成，见「实施记录」；`wasm_hash` 的生产者与「权限变更重弹」按裁决留作后续，已登记）

## 现状（已复核）

- `approval.rs` 的 `approve` / `verify_approval` / `effective_permissions` / `compute_dir_hash` 桌面生产**零调用**（全仓唯一命中是 `host/install.rs:191` 的 `revoke`）；
- 激活路径 `manager/host/activation.rs:149-151` 直接按 manifest 全量 `grant_permissions`；
- 桌面 `downloader.rs:112` 注释自陈无 `wasm_hash`（移动端有）；`downloader.rs:87-109` 解压无体积/条目上限；
- ADR 0020 状态写「已实施（桌面端 2026-08）」并把「`process:run` 声明即授予」「无内容钉扎」列为它要解决的风险 3、4 → **文档与代码相反**。

## 需裁决项（先答再动工）

1. 批准时机：安装即弹 / 首次启用前门禁 / 权限变更时重弹（移动端是激活门禁 + 存量首启自动批准一次，桌面是否照此）；
2. 信任分档：`resources` 随包插件是否继续「全量授权、无需审批」（ADR 0020 表 3 现口径），若是，则本票的门禁只作用在 `UserInstalled` 来源；
3. 高危位的额外确认：`process:run` / `pty:spawn` / `terminal:input` / 主库面是否逐位确认而非整单批准。

## 验收

- [x] 激活前置 `verify_approval`：无批准或哈希不匹配 → `PluginState::NeedsApproval` + 拒绝激活 + 前端可见原因（禁止静默降级为「部分权限」）
- [x] 生效权限 = 批准 ∩ 请求（`effective_permissions`），`storage` 恒授予的特例随票 02 一并取消
- [x] 批准时对插件目录算 SHA-256（`compute_dir_hash` 已有实现与单测），激活时重算比对，不匹配即撤销批准并落 `NeedsApproval`
- [x] 桌面 manifest 增 `wasm_hash`（与移动端对齐），`downloader.rs` 安装期校验；同时补 zip 解压**体积/条目数/单文件上限**（越界 fail-visible，遵「调用方声明 + 宿主上下限 + 越界报错」口径）
- [x] 存量兼容：已启用的用户插件首启自动批准一次并 `warn` 留痕（照移动端先例），不得把用户现有功能打死
- [x] 审批 UI 维持既有界面风格（用户偏好：界面维持），审批弹层归 `plugin/security` 的前端面，i18n 同步 zh-CN/en
- [x] ADR 0020 更新：状态改为与代码一致（实施前后各改一次），并把「签名链仍为后续」写回
- [x] 门禁：`cargo test --lib`（1126 passed / 0 failed，含新增审批链单测与激活门禁闭环用例）+ `pnpm run test:run` + `pnpm exec eslint .` 0 error

## 实施记录（2026-09-22）

1. **门禁落点：激活链路最前端，且只对 `UserInstalled`**。`activation.rs::approval_gate` 在
   **预授权之前**执行（未获批准的插件不该先弹文件系统授权窗），返回三态：免审批来源 → `Ok(None)`；
   批准有效 → `Ok(Some(批准 ∩ 请求))`；无记录 / 哈希失配 → 落 `NeedsApproval` + 返回 Err（失配同时
   撤销批准并 `warn`）。目录哈希走 `spawn_blocking`（阻塞 IO 不进 async worker），且**全程不持
   `plugins` 写锁**（失败路径要回写状态，持锁回写 = 死锁）。
2. **生效集必须落到权限管理器才算门禁**。宿主 `grant_permissions(plugin_id, &生效集)`：用户安装插件是
   批准 ∩ 请求，内置来源是 manifest 全量。只算不授等于门禁可被绕过——宿主机能门查的是
   `PermissionManager`，不是 `LoadedPlugin.granted_permissions`（后者只是 DTO 快照）。
   `effective_permissions` 里的 `storage` 无条件插入随之移除；新增 `approval::known_permissions`
   让**批准清单只含词汇表内的位**（否则「用户看到的批准集」与「实际生效集」不一致，弹层成假账）。
3. **哈希钉扎要认得「哪些不是代码」**（本票最容易漏的一处）：插件私有 SQLite 库就在插件目录里
   （`app_data/plugins/<id>/plugin.db`，见 `wasm_runtime.rs` 私有库装配），批准之后运行期会持续变化
   （首次启用即建库、WAL 增长）。不排除它，「批准 → 启用（建库）→ 停用 → 再启用」会判成内容被替换 →
   撤销批准 → 插件再也起不来。现在 `compute_dir_hash` 跳过 `plugin.db` + `-wal` / `-shm` / `-journal`
   四个数据面文件名，代码面（plugin.json / index.js / *.wasm 等）照旧全量入哈希，两条方向各有独立用例。
4. **存量兼容照移动端先例**：`boot.rs::auto_approve_legacy_user_plugin` 在持久化自动激活的循环里，
   对「持久化已启用 + 用户安装 + 尚无批准记录」的插件补一条批准（当前 manifest 的词汇内位 + 当前目录
   哈希）并 `warn` 留痕；**已有失效记录（哈希不匹配）不补批**——那是要给用户重新审批的，越权补批等于
   把门禁自己关掉。「权限清单变更时重弹」按裁决不做。
5. **审批 UI 两处共用同一弹层**（`components/PluginApprovalDialog.vue`，沿用详情页卸载确认弹窗的
   Teleport + backdrop 蓝图）：权限逐条列出（未知位回退原文，不再有「未知权限」占位），高危位
   （`process:run` / `pty:spawn` / `terminal:input` / `database:main`）红标 + 后果文案，来自与权限列表
   同一张表（`contributionKinds::HIGH_RISK_PERMISSIONS` + `riskKey`）。列表页与详情页的「启用」在
   `NeedsApproval` 时改为「先审批 → 批准后自动继续启用」（同移动端 `approveThenEnable` 语义），
   而不是弹一句后端英文报错。文案覆盖：14 → **31 条**（zh-CN + en），新增锁定用例断言词汇表任一条
   失去文案即红。
6. **安装期补的是「上限 + 摘要」，不是「签名」**：`ZipLimits`（512 条目 / 64 MiB 总量 / 32 MiB 单文件）
   按**实际写入字节**裁决（zip 声明的 uncompressed size 可伪造，用 `take(limit + 1)` 探针区分「刚好
   等于上限」与「越界」），失败路径统一清理临时目录；manifest 新增可选 `wasm_hash`（与移动端同形、
   `camelCase` 自动映射），声明时比对 zip 内 wasm 的 SHA-256，不匹配拒绝安装。
7. **变异自检（用例真的在裁决，不是装饰）**：
   - `extract_rejects_total_bytes_over_limit` 首跑就逼出实现缺陷：上限取 `u64::MAX` 时
     `max_single_file_bytes + 1` 溢出 panic → 改 `saturating_add(1)`。测试先红后绿，说明裁决路径确实被执行；
   - `activation_grants_only_approved_subset_of_declared` 手工写入**只批准一位**的记录：杀掉
     「激活时按 manifest 全量授权」变异（否则 granted 会是三位）；
   - `test_compute_dir_hash_ignores_runtime_data_files` 两个方向：数据面文件变化不改哈希、`evil.js`
     新增仍改哈希（防止「把所有文件都排除」的过度实现）；
   - `user_installed_plugin_needs_approval_before_activation` 同时断言状态与**零权限授予**，
     杀掉「拒绝激活但偷偷授权」的变异。
8. **门禁实跑数字（2026-09-22）**：
   - `cargo test --lib`（bedcode-desktop/src-tauri）：**1126 passed / 0 failed / 0 ignored**，12.78s
     （新增：approval +1 改 1、downloader +6、host 门禁闭环 +7）；
   - `cargo check --lib --tests`：`src/` 零错误；仅票 13 的既有集成 target 红（见第 10 条）；
   - 前端：`vitest run` **743 passed / 1 failed**，唯一红是
     `plugins/terminal-session/src/__tests__/plugin-contract.test.ts`（对侧在途改动的 pin 未落全，见第 9 条，
     本票未动该文件与其 manifest）；
   - `eslint`（本票改动的 9 个前端文件）**0 error**；`vue-tsc --noEmit` 仅 `src/dev/terminal-mock/TerminalMock.vue`
     3 条 `@/utils/terminal*` 缺失（对侧终端下沉批次的既有遗留，该文件 `git status` 未改，非本票）。
9. **同 worktree 交叠（证据，非本票缺陷）**：对侧「PTY 业务下沉」线在本票在途期间改了
   `plugin-sdk-desktop/rust/src/{permission.rs,host/session.rs,wasm_host.rs}`、`rust/wit/bedcode.wit`、
   `plugins/terminal-session/plugin.json`、`src-tauri/src/{session/session_config.rs,plugin/manager/wasm_runtime/component.rs,plugin/manager/wasm_runtime/host_impl/session.rs,plugin/manager/wasm_runtime/tests/session_e2e.rs}`、
   `src-tauri/tests/{broadcast_shutdown,ws_auth_rules}.rs` 与两份权限生成物 + `permission.vocabulary.ts`
   ——与本票改动文件**零交集**（本票：`plugin/security/approval.rs`、`manager/host/{activation,install,boot}.rs`、
   `manager/api_bridge.rs`、`manager/types.rs`、`manager/host.rs`、`plugin/downloader.rs`、`lib.rs`、
   `packages/plugin-sdk-desktop/rust/src/{types,traits,wasm}.rs`、`packages/plugin-wasip3-test/src/lib.rs`、
   前端 9 文件、ADR 0020 / CHANGELOG）。两处实际影响：① 对侧在途把 `session:config` 从词汇表与插件
   manifest 退役，本票的文案表为兼容两种状态保留该条（inert；文案表不是词汇漂移锁的一部分，退役落地后
   可删）；② 对侧 `plugin.json` 去 `session:config` 后其前端契约用例期望数组未同步 → 上面那条前端红
   **归属对侧**（`git status` 可查该文件与其 manifest 的改动均非本票）。
10. **未做 / 遗留（登记，不静默）**：
   - `wasm_hash` ~~目前没有生产者~~ **已由票 14 关闭（2026-09-22）**：桌面打包链现在默认把产物
     `<rustLibrary>.wasm` 的 SHA-256 注入**产物** `plugin.json`（源清单不带该键），分发链
     `scripts/package-plugins.mjs` 出包前逐条复核——票 03 落地的「声明即校验」通道自此默认生效，
     不再依赖发布者手填。移动端仍无生产者，该偏离随票 14 裁决项 3 登记；
   - 移动端 `effective_permissions` / `grant_permissions` 的 `storage` 特例仍在（本票桌面-only），
     双端偏离已写进 ADR 0020 修订记录；
   - `.dev-shell` 无审批 mock：插件列表 / 详情走真实宿主命令面，dev-shell 只覆盖插件内命令；
   - 票 13 的 5 个集成 target 仍编译不过（`ws_session_route` 3 / `pty_session_chain` 5 /
     `ws_auth_rules` 5 / `http_auth_biometric` 4 / `broadcast_shutdown` 4 条错，对侧在途又动了后两个文件），
     故本票门禁口径为 `cargo test --lib`；全 target 门禁待票 13。
11. **提交期夹带与处置（如实记）**：文档提交首次把 `bedcode-desktop/docs/code-map.md` 整文件暂存，
   连带把对侧同文件在途的 hunk（`session_config` 节改写）一起提交（`cdf2a00cc`）。已 `reset --soft`
   回退该 commit（本地未推送），改用 blob 精确暂存（`git hash-object -w` + `git update-index
   --cacheinfo`）只提交本票 hunk（3/1），对侧 hunk 留在工作区未提交，文档 commit 最终为 `400b4c79f`。
   教训与票 05 第 9 条一致：**共享文档文件（code-map / AGENTS.md / CHANGELOG.md）提交前逐文件核
   `git diff --numstat`，跨线共用的文件必须局部暂存**。
   **反向夹带（同批记录）**：本票的 `docs(scratch)` 提交后来被对侧 `fc760448e`（其 ABI v23 提交）
   连同其自身改动一起提交 —— 对侧从共享暂存区提交时把本票已暂存的三个 scratch 文件
   （`issues/03`、`handoff-2026-09-22.md`、`spec.md`）带走了（`git show --stat fc760448e` 可见）。
   用户 2026-09-22 对此类散落已表态「散落没有关系」，故**不改写对侧 commit**，仅在此记证。
   ⇒ 共用 worktree 下的实操纪律：**暂存后立刻提交**（窗口越短越好），或直接用
   `git commit -- <path>` 只提交指定路径，不把内容留在共享暂存区里等。

## Comments

- 2026-09-21 立项：来源 spec §4-P0-2。修完本票才谈得上「权限声明面有意义」——票 04/05/06 的攻击链第一环（自报高危权限）由本票截断。
- 2026-09-22 实施完成（见「实施记录」）。提交：`68f6f5812`（宿主 Rust + SDK manifest 字段 + fixture）/
  `33b085905`（前端弹层与文案）/ `400b4c79f`（ADR 0020 + CHANGELOG + code-map）。判据回溯：本票落地前 `approval.rs` 的四个函数在桌面侧唯一生产
  调用点是 `install.rs` 的 `revoke`，激活路径按 manifest 全量授权 —— 即 ADR 0020 自陈要解决的风险 3
  （`process:run` 声明即授予）与风险 4（无内容钉扎）当时确实成立，「已实施」是文档对代码的误记。

### 裁决（2026-09-22 用户裁决，开工前已定，实施时不得再自行取舍）

1. **批准时机**：激活前门禁 + 存量自动批准一次 —— 照移动端先例（`bedcode-mobile/src-tauri/src/plugin/manager.rs:633,652,659`）。
   `verify_approval` 装在激活前置：无批准或目录哈希不匹配 → `PluginState::NeedsApproval` + 拒绝激活 + 前端可见原因；
   已启用的用户插件首启自动批准一次并 `warn` 留痕。**不做「权限清单变更时重弹」**（本轮不加，留作后续可选项）。
2. **信任分档**：`resources` 随包内置插件**免审批**（沿用 ADR 0020 表 3 现口径），本票门禁只作用在 `UserInstalled` 来源。
3. **高危位**：**整单批准 + 高危位视觉强调**，不做逐位单独确认。
   高危位清单固定为 `process:run` / `pty:spawn` / `terminal:input` / `database:main`（后者由票 02 新增，
   已按「仅第一方按需申请」定档）；弹层里逐条红色标识 + 后果文案。
   ⇒ 前置依赖：票 01 登记的「权限展示文案仅覆盖 13/30」必须在票 03 内补齐到全 31 条（zh-CN 与 en 同步），
   否则审批弹层会把没文案的位显示成 `desktop.plugin.perm.unknown`。
