# 票 12 peer-net 半票：实施状态与收尾清单（未提交）

Date: 2026-09-23 05:1x（写于第三个会话正在重命名 `server/` 的当口）
状态：**代码写完、自己的 5 个编译错已清，但全量门禁跑不了，未提交**
为什么不提交：见文末「阻塞原因」。

---

## 1. 已完成到哪一步（裁决 1 = 选项 A）

`host-peer` 增两个引擎级节点生命周期原语，节点归属从「内核硬编码产品 id」改成
「**谁起谁停的属主记账**」。ABI 取 **v25**（对侧 v24 已提交在 `727041862`）。

| 层 | 文件 | 内容 |
| --- | --- | --- |
| 契约 | `packages/plugin-sdk-desktop/rust/wit/bedcode.wit` | `host-peer` 尾部追加 `start-node` / `stop-node`（函数级追加，桌面独有接口，移动端不跟演） |
| 版本 | `packages/plugin-sdk-desktop/rust/src/abi.rs` | `ABI_VERSION 24 → 25` + v25 版本注释 + `test_abi_version_is_v25` |
| SDK | `rust/src/host/peer.rs` / `rust/src/wasm_host.rs` | `HostPeer::peer_start_node` / `peer_stop_node` 声明与 guest 侧转发 |
| 引擎 | `src-tauri/src/peer_net.rs` | 新增 `node_owner: Mutex<Option<String>>` 记账 + 三个公开函数 `start_node_owned` / `stop_node_owned` / `release_node_for`；`stop_locked` 里**关停与清账同处**；**删除** `FILE_TRANSFER_PLUGIN_ID` / `plugin_transfer_activated` / `sync_node_with_plugin_state` |
| 宿主面 | `host_impl/peer.rs` + `component.rs` | 两个原语实现（`PERMISSION_PEER` 权限门 + 宿主注入 caller）+ WIT 接线 |
| 内核去身份 | `manager/host.rs`（删重复常量）、`manager/host/activation.rs`（两处 `plugin_id == FILE_TRANSFER_PLUGIN_ID` 外壳 → 按属主释放的通用钩子 `release_peer_node_if_owned`）、`lib.rs`（删 boot 对账调用 + 改注释） | 三处入口全部去掉产品 id |
| 上下文 | `wasm_runtime.rs` | 新增 `WasmHostContext::app_handle()`（`pub(crate)`）——理由见代码注释：属主清理钩子在 `manager/host/activation.rs`，不是 `wasm_runtime` 模块后代，读不到私有字段；旧路走 `AppContext::try_global()` 在 boot 期拿不到句柄 |
| 夹具 | `host/tests/runtime_preauth_test.rs` | `super::FILE_TRANSFER_PLUGIN_ID` → 测试文件本地 const（内核常量已删，测试夹具不该逼内核留着产品名） |
| 插件 | `plugins/file-transfer/rust/src/lib.rs` | `activate()` 末尾 `peer_start_node()`、`deactivate()` 末尾 `peer_stop_node()`；两处「引擎电源由宿主外壳驱动」的假注释改掉 |

## 2. 三条设计决定（都不是照票面字面做的，写清理由）

1. **起节点失败不翻成激活失败**（只 log_error）。旧外壳也是「只 log_error，激活结果由
   inner 决定」。改成 fail-fast 会让一次引擎故障变成整个插件不可用，是行为变更。
2. **`start-node` 放在 activate 的最后一步、`stop-node` 放在 deactivate 的最后一步**。
   等价于旧外壳的 `result.is_ok()` 门：前面任何一步失败都不会把节点带起来。
   但光靠这个还不够——见第 3 条。
3. **内核必须留一条按属主的补偿**（`release_peer_node_if_owned`，激活失败与停用成功两处走它）。
   旧实现「外壳只在成功时起节点」天然没有「插件起了节点随后自己失败」这个窗口；
   改成插件自起之后，不补这一条就会出现**插件未激活、本机仍在 `_bedcode-peer` 广播**。
   这条钩子按属主判断、不认产品 id，所以任何插件都能安全挂。

**属主规则**（`peer_net.rs` 注释里也写了）：无主可认领（含宿主命令面 `start_peer_node`
先起的情况）；同主幂等；他主拒绝接管与拒绝关停，且**错误文案不回带对方 id**
（与票 05 `ensure_handle_owner` 同口径）。

## 3. 立项时没列的第四、五个入口（票面漏的，实测找出来）

票 12 的 现状 表只记了「激活/停用两处外壳」。实际节点生命周期有 **五处**：

1. `activation.rs:85` 激活外壳
2. `activation.rs:589` 停用外壳
3. `lib.rs` boot 末尾 `sync_node_with_plugin_state` 对账
4. `peer_net.rs` `#[tauri::command] start_peer_node`（`lib.rs` invoke_handler 注册）
5. `peer_net.rs` `#[tauri::command] stop_peer_node`（同上）

第 4/5 条实测**前端与插件零消费者**（全 `src/` + `plugins/` grep 无 invoke），是宿主自己
保留的人工控制面。处理：**不动它们**（不是本票范围），但 `stop_locked` 清属主意味着
人工关停会把节点从任何插件名下解绑——这是对的（人工越权优先于插件记账），已写进注释。
> 待判（不在本票）：这两个命令按 ADR 0022 裁剪线早该随「内核不携带产品生命周期」一起
> 复审——它们能让一个没有插件在用 peer 的时刻把节点挂上广播面。

## 4. 验证到哪一步

- ✅ SDK：`cargo test --lib` **114 passed / 0 failed**，含 `test_abi_version_is_v25`
  （WIT 追加后 wit-bindgen 重新生成，guest 侧两个转发方法能编译即证明契约与 SDK 对齐）
- ✅ 宿主：我那 5 个编译错（`app_handle` 访问器放错 impl、`require_app` 返回所有权、
  `node_owner` 读函数的尾表达式借用）**全部清掉**
- ❌ 全量门禁**跑不了**：见下条

## 5. 阻塞原因（为什么现在不能提交）

第三个会话**此刻正在写这个仓库**，且正在做一次会破坏构建的重命名：

- `src/server/` 正被拆成 `core/` + `http/` + `websocket/` 三层，`git status` 里一片
  `RM`（重命名 + 修改）与 `A`，`websocket/conn.rs` 最后写盘 **05:11:21**（我 05:12 跑检查前一分钟）
- 该重命名当前**不自洽**：`src/server/websocket/{conn,terminal,channel/event}.rs` 引用
  `server::services` / `server::connection_types`，模块已移走、引用没跟上 → `cargo check --lib` 5 个 E0433
- `cargo-clippy` 与 `cargo` 进程在跑（05:12 实测）

在这种树上提交，等于把别人 1 分钟前的半截重命名一起进历史，而且我无法证明我的改动是绿的。
**收尾前提**：`server/` 三层化落定、`cargo check --lib --tests` 回到 exit 0。

## 6. 接手要做的事（按顺序）

1. 等 `server/` 三层化提交；`git status` 干净后按下面任一方式恢复本票改动：
   - **`pending-12-peer-net-v25.patch` 是干净且验证过的**：只含我改的 13 个路径，
     且对**纯净 HEAD 导出树**跑 `patch -p1 --dry-run` 逐文件 checking、零 reject 零 fuzz。
     恢复 = `git apply .scratch/2026-09-21-wasm-core-audit/pending/pending-12-peer-net-v25.patch`。
     （它是用「HEAD 内容 + 只按文本替换我的 hunk」重造出来的，对侧在途 hunk 已剔除；
     生成器按关键字分 hunks，`unresolved=0` 覆盖全部 13 个文件。）
   - 兜底：`pending/files/<原路径>` 是这 13 个文件的**目标内容整份快照**，
     patch 万一冲突就直接覆盖再手工核。
   ⚠ 但如果基线已经不是当初那个 HEAD（对侧又动了这些文件里的某一个），**先核冲突再 apply**——
   别拿一个 dry-run 绿的 patch 直接怼到漂移到一半的树上。
2. 补测试（本票的承重断言，一条都别省）：
   - 属主记账三态：同主幂等 / 他主拒绝接管 / 他主拒绝关停，且三条错误文案都**不含对方 id**
   - `release_node_for`：属主匹配才关停、不匹配 no-op 且不报错
   - `stop_locked` 清空属主（含人工命令路径）
   - 内核去产品身份的 **grep 防回归锁**（票 12 验收 1 后半）：扫 `manager/**` 生产代码
     不得出现 `file-transfer` / `com.bedcode.session` / `quick_actions` / `auto-task` / `pairing`
     —— 注意两件事：① 要**排除测试与 fixture**（票 07 第二批我在 `component.rs` 测试里用了
     `${home}/.bedcode/ai-chatbox` 字面量，票 03 在 `activation.rs` 测试里有
     `com.bedcode.terminal-session.pairing-code-generate` 断言，不排除则锁一上就红）；
     ② 排除注释还是连注释一起扫，要显式判定并写进测试注释——票 07 的「特权表项逐条注释归属」
     规则要求**某些注释必须点名消费者**，一刀切扫注释会与那条打架
   - 行为等价（验收 2）：激活→节点起、停用→节点停、激活失败→节点不起也不残留
3. 门禁（本机实测形态）：`node plugins/file-transfer/scripts/build.js`（ABI v25 重建产物）
   → 其余三插件逐个 → `cargo test --lib`（核 `[skip]` = 0）→ `cargo test` 全 target →
   `cargo check --lib --tests` → SDK `cargo test --lib` → `pnpm exec eslint .` →
   根 `pnpm run test:run`。**改 WIT 必须重建全部产物**，否则闭环用例拿旧 ABI 产物跑是假绿
4. 落 commit 时同步：AGENTS §7 的 WIT 计数口径（desktop v24 → **v25**，含「函数级追加是新批次」）、
   `microkernel-gap.md` 阶段 4 前置清单、CHANGELOG（三条 Breaking 判断：本票是**纯增量追加**，
   v24 及以下产物不受影响，但 file-transfer 必须按 v25 重建才用得上；移动端不跟演要写明）
5. **本票还剩的另一半没做**：验收 3「四个一次性迁移模块摘出 `plugin/`」——它要动
   `plugin.rs` + `lib.rs` 的四个挂钩点，且并发线正在往 `plugin/` **加**新迁移
   （`auth_records_migration.rs` 已随 v24 提交），方向相反，等他们收口后单独判。
   顺带把 roadmap「阶段 3 冻结前未完成清单」里那条死字段
   （`WasmHostContext::config_manager`，零读取点）一起删掉——用户口味是死代码本票删。
