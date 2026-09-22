# 07: fs_auth 白名单收敛与最小权限（P1-2）

**What to build:** 「三层校验」成为名副其实的分层：路径白名单是**配置数据**而非硬编码子串；插件白名单从「完全免弹窗」降级为「预授权到声明目录」；WASI preopen 支持只读档。修完后 `fs:read` 不再等于「静默读 `~/.claude/**`」，第一方插件也不再等于「全盘免弹窗」。

**Blocked by:** 01

**Status:** in-progress（2026-09-22：验收 1 / 2 / 4 / 5 / 6 / 7 完成；**第 3 项（preopen 只读档 + 五同步点）经用户裁决延到下一批**）

## 本批范围裁决（2026-09-22 用户裁决）

1. **声明目录真源 = 宿主侧具名清单**（票面第 1 项的第二形态），不是新增 manifest 字段、也不是内核配置项：
   `fs_auth::FIRST_PARTY_TRUSTED_DIRS`，逐条注释归属，**只对具名第一方插件生效**。
   理由：撞车最低（不动 SDK / 打包 CLI / 前端），且新增 manifest 字段会把「免弹窗特权」
   变成插件自助申报——那张表本身就得再设一道审批，收益为负。
2. **preopen 只读档延后**：它要 `component.rs` 的一行（`FsPerms::ReadWrite` → 按声明分档）+
   SDK TS/Rust + 打包 CLI + 前端合法集五同步点，而 `component.rs` 与 SDK 此刻在隔壁线手上。
3. **不可复用的教训（记下来免得下轮重犯）**：曾考虑把 `wasiPreopenDirs` 当免弹窗真源——
   它今天靠 `is_granted` 过滤才能建 preopen（`component.rs:1498`），若声明即等于免弹窗授权，
   那层过滤恒真，插件声明 `~/.ssh` 就能预打开。裁剪线上这是**倒退**，不作为候选方案。

## 现状（已复核）

- `fs_auth.rs:66` `path_whitelist` 恒为空 `Vec` 且无写入点；实际第一层是 `match_path_whitelist`（`:278-296`）对 canonical 路径做 `.claude/` 子串匹配（含「以 `.claude` 结尾」）→ 任何 `fs:read` 插件无弹窗读 `~/.claude/**`（Claude Code 配置目录，含凭据类文件），任意项目 `.claude/` 亦可写；
- 第二层 `plugin_whitelist`（`:75-76` 内置 session / file-transfer，`:114-121` 直接 `return true`）→ 这两个 id 对**任意路径**完全免弹窗，fs 上限只剩两个权限位；
- `:123`（持久化授权 `check_granted_path`，函数在 `:304`）与 `:133`（弹窗 `request_user_auth`，函数在 `:328`）都标「第三层」，注释层数与文档「三层」口径不符（实为四层：路径白名单 / 插件白名单 / 持久化授权 / 弹窗）；
- WASI preopen 一律 `FsPerms::ReadWrite`（`component.rs:1401`），无只读档；
- `manager/task.rs:291` 注释承诺「绝不从池线程触发弹窗」，但 task 单元与 `fs_read` 同链，生产路径未见抑制机制（待复核，若是则必须修）。

## 验收

- [x] `.claude/` 免弹窗规则移出代码：改为内核配置/设置项里的**路径白名单数据**（默认空），或改为「宿主已知的第一方集成目录」显式清单并逐条注释归属；改造后 `fs:read` 插件默认无法静默读 `~/.claude/**`（红测断言）→ 走**显式具名清单**（裁决 1）；死字段 `path_whitelist` 一并删除（不留无人走的分层），层数由四收为三
- [x] 插件白名单语义收窄：白名单只表示「激活时按 manifest 声明目录预授权、无弹窗」，不再是任意路径放行——`check()` 内改为继续走持久化授权判定 + 声明目录前缀比对；两内置插件的声明目录来自其 manifest / 设置页真源（file-transfer 共享目录、session 的 Agent 集成目录）→ 见「真源核对」：file-transfer 零 fs 消费者，条目直接删除
- [ ] manifest 支持 preopen 只读声明（`{path, readonly}` 或 `wasiPreopenDirs` 增只读形态），SDK TS/Rust + 打包 CLI + 前端合法集同步（AGENTS §7 五同步点）→ **本批不做**（裁决 2：`component.rs` 与 SDK 在隔壁线手上）
- [x] 层级注释与文档统一（三层 or 四层，code-map:137 与 AGENTS §7 一并改口径），错误/日志文案说明**命中的是哪一层**
- [x] 复核并处置 `task.rs:291` 注释：若池线程确实可能触发弹窗 → 改为「池线程只走 `is_granted` 无弹窗判定，未授权即 fail-visible 拒绝」；若不会 → 注释保留并在本票 Comments 记录判据 → **注释是假的**，已补真实现（见「task 弹窗判据核对」）
- [x] 回归：`fs_auth` 既有用例（含 canonicalize 前置、相邻目录前缀不误匹配）全部保留；宿主真实闭环 `cargo test` 全绿
- [x] 弹窗 30s 超时、UUID request_id、pending 清理行为不变（本轮确认无过期/重放问题，勿回退）

## Comments

- 2026-09-21 立项：来源 spec §5-P1-2 与 §2 `security/fs_auth.rs` 行。

### 真源核对（改这两层之前先把「谁在用」问清楚，结论与票面假设不同）

| 事实 | 判据 | 影响 |
| --- | --- | --- |
| `.claude/` 子串规则的**真实消费者不是 terminal-session**，是 `com.bedcode.agent-hub` | `plugins/agent-hub/rust/src/skills.rs::TARGET_SEGS` 把技能副本分发到 `~/.claude/skills` 与 `~/.pi/agent/skills`，规范库在 `~/.agents`；terminal-session 本就在插件白名单里（任意路径免弹窗），子串规则对它冗余 | 光删子串规则会**静默打断 agent-hub 分发**；票面把两者混在一条里，按票面字面做会漏 |
| `file-transfer` **零 fs 原语调用** | `plugins/file-transfer/rust/src` 内 grep `fs_read\|fs_write\|fs_read_dir\|request_dirs` 无命中（传输走 peer-net / host-filesrv） | 它的插件白名单条目是装饰 → 直接删除，不迁进新清单 |
| terminal-session 的文件浏览根是**用户选的任意目录** | `file_browse/source.rs` 逐调用走 `HostFs::fs_read_dir / fs_read / fs_stat / fs_canonicalize`，根来自会话 `working_dir` | 「声明目录前缀」这条模型覆盖不到它（段名不是固定的）；内核若去认「会话工作目录」就是把产品语义塞进 `fs_auth`（§5 红线）→ 唯一去处是让插件在公共前置里主动 `host-fs.request-auth`，用户授权一次后持久化 |
| 插件从不用 `fs_request_auth`（改造前不需要） | terminal-session 全仓 grep 无调用点 | 收紧必须由插件侧同时补上申请，否则行为是「含糊的 500」而不是「一次弹框」 |
| hook 写入面是**具名段**，可枚举 | `task/hooks.rs` 写 `<project>/.claude/{settings.json,auto_task_hook.py}`、`<project>/.codex/…`、`<project>/.pi/extensions/…`、`<project>/.opencode/plugins/…`，并清理全局 `~/.claude/settings.json` | 这半边能免弹窗保住：段名进新清单（`ProjectSegment` 形态） |

### 改造后的三层（原「四层」的第一层是死数据 + 子串 hack，合并后重编号）

1. **第一方具名集成目录预授权**（`FIRST_PARTY_TRUSTED_DIRS`）：两种形态——
   `Home("<家目录相对段>")`（组件前缀匹配，`~/.agents` 不覆盖 `~/.agentsx`）与
   `ProjectSegment("<目录段名>")`（自身或任一祖先段**全等**，`.claudex` / `x.claude` 都不命中）。
   取不到 `home_dir()` 时 `Home` 形态**不放开**（绝不让它退化成「任意位置的该段名」）。
2. **已授权路径前缀**（弹窗勾「记住」后的 `fs_granted_paths`）——不变。
3. **弹窗授权**（30s 超时、UUID request_id、失败/超时清 pending）——不变，验收 7 保住。

`check` / `check_batch` / `is_granted` 三条入口的免弹窗集合由同一个 `matched_layer` 给出
（旧实现是三处各抄一遍判据）；命中层随日志 `layer = first-party-dir | persisted-grant` 输出。

### task 弹窗判据核对（验收 5）

**注释是假的**：`manager/task.rs::execute_unit` 把 `fs.*` 单元直调 `host_impl::fs::fs_read/fs_write/…`，
那些函数走 `authorize_fs` → `SecurityFramework::authorize` → `FsAuthorizer::enforce` →
`fs_auth.check(...)`，也就是**会**在池线程上开弹窗并占住槽位最长 30s。按票面口径补真实现：
`ensure_unit_path_granted` 在单元执行前按管线**同一顺序**预判两件事——
① 声明闸门（缺 `fs:read` / `fs:write` → 返回与内层链逐字相同的 `"permission denied"`）；
② 目录授权（`is_granted`，未授权 → 点名路径与 `host-fs.request-auth` 出路）。
顺序写反是本批实测抓到的：先判目录会让「没声明权限」的插件收到「去申请目录」，方向全错
（`test_task_dual_gate_domain_permission_denied` 正是这条用例把它按住的）。

### 行为变更（用户可见，必须写明）

- **文件浏览 / git 面首次访问某个工作区会弹一次授权框**（此前静默放行）。插件在
  `resolve_working_dir_via_host` 这条公共前置里申请，已授权时宿主直接返回 true（同一张表，不重复弹）；
  被拒或无头环境下返回 **403 + `Not authorized: …`**，不再是含糊 500（`working_dir_error_response` 新增映射）。
- 第三方 `fs:read` 插件读 `~/.claude/**`（或任意同名段）不再免弹窗——票面红测断言。
- 第一方两插件失去「任意路径」特权：清单外的路径走弹窗 + 记住。
- agent-hub 只拿到 `~/.agents`、`~/.claude/skills`、`~/.pi/agent/skills` 三棵子树，
  **不是整个 `~/.claude`**（用例锁住这条，防将来"顺手放宽一层"）。

### 测试面迁移（旧用例靠特权跑，不是靠契约）

`host_impl/fs.rs`、`security/framework.rs`、`tests/task_e2e.rs`、`tests/session_e2e.rs` 里
把测试目录塞进 `.claude` 段来绕过授权的写法全部改掉，改走生产同款：`save_granted_path`
预置「已记住」记录（`FsAuthChecker::save_granted_path` 放宽到 `pub(crate)`，
`WasmHostContext` 加 `fs_auth()` / `permission()` 两个 `pub(crate)` 访问器）。
session_e2e 的工作区授权特意落在**工作区根**而不是会话目录：越界探测
（`../outside.txt`）必须先被宿主 exists 判 404、再由插件的容器检查判 403——
404/403 的分层语义是插件契约的一部分，授权根选窄一点会把 404 变成 500。

#### 对隔壁线的一次越界副作用（如实登记）

本批按文件跑 `rustfmt` 时，把两份**隔壁线在途**文件里他们的代码也重排了：
`wasm_runtime.rs`（`mod tests` 里两处折行）与 `tests/session_e2e.rs`（`load_plugin_from_file`
等多处折行）。代码语义未变、两种写法都过 `cargo fmt`，但那是他们的在途改动。

处置：提交时这两份文件**只提交自己的 hunk**——用 `git hash-object` 造「HEAD + 仅我的插入」
的 blob 写 index（`wasm_runtime.rs` +10 行访问器、`session_e2e.rs` +11 行授权预置），
工作区保留重排后的完整版本，`git commit -- <paths>` 限定路径，避免把他们的 `commands/*`
staged 删除或他们的在途 hunk 卷进本票。⇒ **他们的 diff 里因此多出若干纯格式化行**，
下次他们跑 `cargo fmt` 会收敛，无需回滚；本票不代为还原（回滚反而可能覆盖他们随后的编辑）。

教训（写进 memory 同条）：**rustfmt 只对自己独占的文件跑**；对共享文件要么用
`--check` 看不改，要么改完立刻按 blob 提交自己的 hunk，别让格式化顺手落到别人身上。

## 门禁

- 宿主 `~/.cargo/bin/cargo test`：lib **1154 passed / 0 failed**、8 个集成 target 全绿（含重建后的 terminal-session 产物）
- 插件 `plugins/terminal-session/rust` `cargo test`：214 passed / 0 failed
- 产物链 `pnpm run plugins:build`：EXIT=0（wasm32-wasip3 编译成立——`file_browse` 的改动全在 `#[cfg(target_arch = "wasm32")]` 下，native 测试看不见它）
- 前端 `pnpm run test:run -- --pool=forks`：79 files / 756 tests passed；根 `pnpm exec eslint .` 0 error
- `rustfmt --check`（按文件，用 `src-tauri/rustfmt.toml`）：本轮改动的 7 个文件 0 diff
- 变异自检三处，各自命中预期用例后还原复绿：
  ① 把 `.claude` 子串规则塞回第一层 → `third_party_cannot_silently_read_claude_dir` +
  `first_party_dir_rules_match_segments_and_home_prefixes` +
  `is_granted_covers_first_party_dirs_and_persisted_grants_only` +
  framework 的 `fs_first_party_integration_dir_allowed_third_party_denied` 四条全红；
  ② 段名匹配退化成子串 → 同上清单用例红（`.claudex` / `x.claude` 两条断言按住）；
  ③ 单元预检把目录判据提到声明闸门之前 → `fs_unit_gate_follows_pipeline_order_and_never_prompts` 红

### 遗留（本票不做，须另立项或下一批）

1. preopen 只读档 + 五同步点（裁决 2 延后）。
2. **`is_granted` 在 `wasm_runtime.rs:207` 与 `component.rs:1498` 之外再无消费者**：
   WASI 预打开只覆盖 manifest 声明目录，插件经 `host-fs` 的路径授权与预打开集合是两套——
   下次动 preopen 时一并判「要不要让它们对齐」。
3. **无头/无 AppHandle 场景一律拒绝**（本批未改，行为不变）：生产里 fs 弹窗需要前端在跑；
   若将来有 headless 服务形态，fs 面需要另设非交互授权通道。
4. 文件浏览的授权是**按工作区根一次**，切换工作目录会再弹一次——如实接受（这是「按需授权」的定义，
   不是缺陷）；插件没有也不该有「记住所有历史工作区」的特权。
