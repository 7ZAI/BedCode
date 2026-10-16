# 2026-10-16 git stash pop 冲突解决（8 个移动端宿主文件）

## 触发与状态
- 06:49 他方会话 `git stash pop` 冲突中止：8 文件 unmerged（stage1/2/3 在，无 MERGE_HEAD），stash@{0} 完好。
- 07:45 用户授权合并。解决方式：**8 文件全部保留 dev（ours/Updated upstream）侧**，stash 侧丢弃；stash@{0} 保留未 drop。

## 决议依据（每个冲突块的 stash 侧内容）
| 文件 | stash 侧 | 决议 |
|---|---|---|
| connection/traits.rs | 与 dev 收敛（44 vs 45 行，仅 `}` 差异） | 保留 dev |
| peer_net.rs（dev 删/stash 改） | BusMessage 直传改造 | 等价物已在 dev `src/peer/ports.rs`（同注释「不再逐字段转换」）→ 保留删除 |
| plugin/downloader.rs | 仅函数签名换行格式 | 保留 dev |
| plugin/manager.rs | **删除 dev SIGILL 修复 `mobile_core_config`**（eb243917c 线） | 保留 dev |
| system/commands.rs | 仅加两个空行 | 保留 dev |
| system/config.rs | 新增 `generate_token()`（无任何调用方，含 stash 内，死代码） | 保留 dev |
| system/constants/plugin.rs | **删除 ADR 0053 `PLUGIN_AUTOSTART_KEY_PREFIX`**（bdd96a16d） | 保留 dev |
| retired_mobile_receive_orchestration_lock.rs | 引用旧路径 `src/peer_receive` | dev 已收拢 `src/peer/receive`，保留 dev |

stash 其余 28 文件改动未进入工作区（pop 中止），stash@{0} 完整，随时可查/可 drop。

## 验证
- 8 文件与 HEAD diff = 0 行；unmerged 清零；全 diff 无 `<<<<<<<` 残留。
- 移动端宿主 `cargo check` ✓（8m16s）。
- `cargo test --test retired_mobile_receive_orchestration_lock --test peer_engine_consolidation_lock --test command_registration_completeness_lock` 3/3 ✓。

## ⚠️ 并发在途（另一活跃会话，未触碰）
- 07:45 解决后 07:52 发现 `plugin/downloader.rs`、`plugin/loader.rs`、`packages/plugin-sdk-mobile/rust/src/types.rs` 出现新未提交改动 =「platform 适配门禁」开发（`manifest.platform.supports(Mobile)`，types.rs 给 PluginManifest 加 `platform` 字段）。
- 移动端宿主 `cargo test`（lib test 目标）E0063：SDK 已加 platform、宿主 `src/plugin/manager.rs` cfg(test) 3 处（1492/1743/1820）初始化未同步——属该会话在途状态，待其完成；非本任务引入（manager.rs 已恢复 HEAD）。
