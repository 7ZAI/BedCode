# 11: 结构归位与去重（消除复制即漂移面）

**What to build:** 同一职责只有一个实现：插件实例化一条路径、contributes 注册一条路径、权限真源一份、`downloader` 归位 manager；code-map 对模块职责的描述与实际一致。本票不改行为，纯收敛，但它是票 02/04/05 能安全落地的地基（否则一处修复要同步三处）。

**Blocked by:** 无（建议排在 02/04 之前）

**Status:** ready-for-agent

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

- [ ] 实例化收为一条路径（`host/install.rs` 与 `host.rs` 共用同一函数），行为等价并有用例锁定两条入口产物一致
- [ ] contributes 注册收为一个函数，三处调用点全部改指它（`host/wasm.rs:106-121` 的手抄必须删除）
- [ ] `granted_permissions` 死字段删除（含 `types.rs` 定义与相关测试断言），权限判定统一走 `PermissionManager`
- [ ] `downloader` 迁入 `manager/`（安装职责归 core-plugin-manager），`plugin.rs` facade 与全部引用点更新；迁移不得改变行为
- [ ] manifest 校验收一处真源
- [ ] `host.rs` 测试体量拆到 `host/*_test.rs` 或既有测试模块，遵 AGENTS §6 文件规范（不用 `mod.rs`）
- [ ] code-map:120-135 与 AGENTS §7 相关表述改为与实际一致（含 `security` 三层/四层口径、`loader` 职责、`downloader` 归位后路径）
- [ ] 门禁：`cargo test` 全绿 + `cargo check --lib --tests`（删字段防假绿）+ `cargo fmt` / `cargo clippy` 自查

## Comments

- 2026-09-21 立项：来源 spec §2 与 §6 表末两行。
