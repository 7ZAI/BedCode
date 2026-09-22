# 11: 结构归位与去重（消除复制即漂移面）

**What to build:** 同一职责只有一个实现：插件实例化一条路径、contributes 注册一条路径、权限真源一份、`downloader` 归位 manager；code-map 对模块职责的描述与实际一致。本票不改行为，纯收敛，但它是票 02/04/05 能安全落地的地基（否则一处修复要同步三处）。

**Blocked by:** 无（建议排在 02/04 之前）

**Status:** done（2026-09-22 实施完成并提交 `709d4c252` + `5d8bf9e93`；见「实施记录」。验收八项全勾，
遗留两项见 §C）

## 现状（审查记录，逐条落地前先自行复核）

| # | 问题 | 位置 |
| --- | --- | --- |
| 1 | loader 职责与文档不符：code-map:127 称 loader 做「文件扫描 + WASM 组件加载」，实际只做扫描 + manifest 校验 | `manager/loader.rs:96-132` vs `manager/host.rs:219-253` |
| 2 | 两条独立演化的实例化路径 | `host.rs:219-253` 与 `host/install.rs:43-88` 近乎复制 |
| 3 | contributes 注册三份手抄 | `host/register.rs:13-27`、`host/register.rs:83-99`、`host/wasm.rs:106-121` |
| 4 | `LoadedPlugin.granted_permissions` 只写不读（真源在 `PermissionManager`） | 写于 `host/activation.rs:151`，定义于 `manager/types.rs:16` |
| 5 | `downloader` 未归位 `manager/`（`plugin.rs:17-20` 已自标注待归位） | `plugin/downloader.rs`、`plugin.rs:15-21` |
| 6 | manifest 必填校验两份（loader 与 downloader 各一份） | `loader.rs:232-268`、`downloader.rs:62-74` |
| 7 | `host.rs` 2290 行中生产代码仅 `:1-451`，`:453+` 全为测试；`new()` 内联 WASM 加载应下沉 | `manager/host.rs` |

## 验收

- [x] 实例化收为一条路径（`host/install.rs` 与 `host.rs` 共用同一函数），行为等价并有用例锁定两条入口产物一致
- [x] contributes 注册收为一个函数，三处调用点全部改指它（`host/wasm.rs:106-121` 的手抄必须删除）
- [x] `granted_permissions` 死字段删除（含 `types.rs` 定义与相关测试断言），权限判定统一走 `PermissionManager`
- [x] `downloader` 迁入 `manager/`（安装职责归 core-plugin-manager），`plugin.rs` facade 与全部引用点更新；迁移不得改变行为
- [x] manifest 校验收一处真源
- [x] `host.rs` 测试体量拆到 `host/*_test.rs` 或既有测试模块，遵 AGENTS §6 文件规范（不用 `mod.rs`）
- [x] code-map:120-135 与 AGENTS §7 相关表述改为与实际一致（含 `security` 三层/四层口径、`loader` 职责、`downloader` 归位后路径）
- [x] 门禁：`cargo test` 全绿 + `cargo check --lib --tests`（删字段防假绿）+ `cargo fmt` / `cargo clippy` 自查

## 实施记录

### A. 已完成七项（1 / 2 / 3 / 4 / 5 / 6 / 8 的代码面）

| # | 收敛点 | 落地形态 | 锁 / 证据 |
| --- | --- | --- | --- |
| 2 | 实例化路径二合一 | `host/wasm.rs::instantiate_wasm_plugin`（**关联函数**——`new()` 构造 Self 之前就要建实例，拿不到 `&self`）：`rust_library` 空 → 无实例；wasm 缺失 / 加载失败 → `PluginState::Error` 入表；成功则返回实例 | `install.rs` 与 `host.rs::new` 两入口只落表；新增 `instantiate_wasm_plugin_covers_both_entries`（两条分支产物）+ `wasm_instantiation_has_two_call_sites_only`（源码锁：生产面 `load_plugin_from_file(` 只允许「共用入口 + `rebuild_wasm_instance`」两处） |
| 3 | contributes 注册三合一 | `host/register.rs::register_plugin_contributions` 为唯一实现；`register_manifest_contributions` 退化为「取 id 清单 → 逐个委派」（读数在读锁内完成，注册不持锁）；`wasm.rs::reload_wasm_plugin` 手抄六项删除 | `contributions_identical_across_entry_points`（两条入口注册面逐项相等 + 非空断言 + 幂等 + 未安装 id 静默返回）+ `registry_registration_has_single_call_site`（六个 `register_*` 调用点各只出现一次） |
| 4 | `granted_permissions` 死字段删除 | 定义（`manager/types.rs`）、四处构造（loader / 静态注册 / 测试脚手架 / types 自测）与唯一写入（`host/activation.rs`）全删；`grant_permissions` 由带绑定改为直接调用——授权结果仍落 `PermissionManager`（唯一真源） | `grep granted_permissions` 零残留；`cargo check` 无「字段不存在」错误 = 删除干净的直接证据 |
| 5 | `downloader` 归位 `manager/` | `git mv plugin/downloader.rs → plugin/manager/downloader.rs`；`plugin.rs` 删 `pub mod downloader;`（连同「暂挂…后续一并迁移」旧注释），`manager.rs` 新增 `pub mod downloader;`；唯一引用点 `install.rs` 改 `crate::plugin::manager::downloader::…` | 全仓 `plugin::downloader` 引用归零 |
| 6 | manifest 校验收一处真源 | `manager/validation.rs` 新增 `validate_manifest_required` + `parse_manifest_json`；loader 与 downloader 各删一份 id/name/version 检查。**取「重复项」而非「并集」**：安装侧独有的 id 反向域名校验、扫描侧独有的 TS-only `main` 校验留在各自入口 → 两条路径行为完全等价，零语义迁移风险 | 两处新增/修改点均无行为变化（错误文案逐字保留） |
| 1 / 8 | 文档与实际一致 | code-map：`downloader` 归位后路径与职责、loader **不做** WASM 实例化（点名 `instantiate_wasm_plugin` / `register_plugin_contributions`）、fs_auth **四层**（原写三层，实际是「路径白名单 → 插件白名单 → 已授权前缀（持久化）→ 弹窗授权」，代码里最后一道序号还笔误写成「第三层」，已一并修正）；AGENTS §7 同步 fs_auth 四层 | `fs_auth.rs` 模块头与最后一道层号注释修正 |

### B. 门禁（实跑，2026-09-22）

同 worktree 的对侧线一度把 `commands/` 九文件合并为单一 `commands.rs`（AGENTS §6 模块入口命名），
期间 `lib.rs` 的 `commands::session::…` 等引用全 E0433（57 项）→ 整个 crate 不可编译。替代验证手段
`cargo check --lib --tests --message-format short` 过滤 `src/lib.rs` 错误后零错误，靠它抓出过一处
漏删字段；待合并收尾后补实跑：

| 门禁 | 结果 |
| --- | --- |
| `cargo check --lib --tests` | **0 error**（含 8 个集成 target 的编译面） |
| `cargo test`（全 target） | **lib 1138 passed / 0 failed**；8 个集成 target 与 doctest 全绿；`[skip]` 计数 **0** |
| `cargo fmt --check` | 无差异（**未跑整 crate `cargo fmt`**，CRLF 纪律） |
| 变异自检 | ① `register_manifest_contributions` 改回手抄注册 → `registry_registration_has_single_call_site` **转红**；② wasm 缺失不再降级 `Error` → `instantiate_wasm_plugin_covers_both_entries` **转红**；其余 3 条保持绿 ⇒ 两条锁都承重 |

提交：`709d4c252`（17 文件：实现 + 新增 4 条锁 + code-map/AGENTS 文档）、`5d8bf9e93`（迁移删除侧
`plugin/downloader.rs`，与新增文件同票但单独成笔）。

**提交纪律实证**：本票涉及的文件里混有并行线改动（rustfmt 长行折返、模块声明重排、`commands/`
单文件化文档更新、import 排序）→ 整文件 `git add` 会卷走他人改动 ⇒ 改为 hunk 级精确暂存
（`git diff -U3 HEAD -- <f>` → 反向还原外来 hunk → `git hash-object -w` + `git update-index
--cacheinfo`）并用 `git commit -F msg -- <paths>` 限定路径；`plugin.rs` 因自己与他人改动落在同一
hunk 内，改为「取 HEAD blob 手工删除 own 行」再写 index。

**未做**：根 `CHANGELOG.md` 未记（本票是内部收敛，无用户可见行为变化；记账习惯见 `2026-09-21-*
` 各票）。

### C. 未做 / 遗留（不在本票范围，登记不擅自扩）

- 仓内其余「fs_auth 三层」措辞（宿主 `host_impl/fs.rs`、`plugin/security.rs`、插件侧
  `agent-hub` / `terminal-session` 的 Rust 与 TS 注释）仍是旧口径——票面第 8 项只点名
  code-map 与 AGENTS，此处不批量改写，留给后续统一。
- `loader` 是否进一步拆成 `manager/loader/` 子模块本票未做（只把职责描述改成与实现一致）。

## Comments

- 2026-09-21 立项：来源 spec §2 与 §6 表末两行。
- 2026-09-22：第 7 项（host.rs 测试体量拆分）由上一会话完成并提交 `9f6e967a7`；本会话接着完成
  1/2/3/4/5/6/8（详见「实施记录」），门禁被对侧 `commands.rs` 合并中间态阻塞，待补数字。
