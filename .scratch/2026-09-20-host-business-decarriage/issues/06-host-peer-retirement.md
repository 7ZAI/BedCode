# 06: 宿主 peer 编排退役（contract）

**What to build:** 在票 05 双轨阶段验收后删除宿主侧传输编排（发送/接收/远端浏览）与对应命令面、事件桥、历史落盘逻辑；对等网络功能全链路走 file-transfer 插件。宿主对等网络只保留引擎接入薄封装（节点身份/发现守护/连接表），实现「宿主侧业务代码清零」终态中对等网络部分的收尾。

**Blocked by:** 05（插件编排全量通过后才可删宿主实现）

**Status:** done（2026-09-21 收尾：④ 四件残留全部清账（encryption_enabled 删、未注册 command 属性剥离 + 死命令/DTO 删除、transfer_settings.json 孤儿定案良性、peer_net 模块头重写）；③ 取消接收断链补回 + `dismiss_pending_offer_*` 回归；宿主 `cargo test --lib` **1070/0**、vitest 全绿、eslint 0；票 02-04 遗留的组件 host-auth 记录面接线缺失、gateway 构造缺 auth 字段均已修。详见 Comments）

- [~] 全部契约面在插件路径通过后，宿主编排/命令面/事件桥删除（无中间态）——删除动作已在树里落地（见 ①），但「插件路径通过」尚无测试证据，且残留未清账（见 ④）
- [ ] 对等网络功能全链路真实 wasm 闭环 + 集成测试通过（发/收/暂停/恢复/浏览/策略/历史）——**未跑**；复核期间发现「收」方向的取消断链（见 ③），已按改名前语义补回，需闭环用例证实
- [~] 历史数据迁移收尾完成（无宿主副本残留、无孤儿文件）——宿主设置文件读写与 `peer_migration.rs` 已删、宿主不再持有历史/设置/任务真源（内存快照随节点生命周期），但旧 `transfer_settings.json` 的孤儿文件处置与升级路径尚未取证（见 ④-3）
- [x] 回滚仅剩 git revert 一种可能——本票全部改动都在未提交工作区，`git checkout` 掉 4 个新文件 + 恢复 4 个删除文件即可整体回退
- [ ] cargo test / vitest / eslint 0 error；收尾 lens_diagnostics 无 blocker——**未跑**（并发约束见 ⑤）

## Comments

### ① 工作区既有实现盘点（进入本会话时已在树里、未提交）

来源：上一会话（其日志无 peer 记录，进入本会话前 4 分钟最后落笔）。盘点结论——删除动作的**形状是对的**：

- `peer_migration.rs` 删除；`peer_receive.rs`/`peer_remote.rs`/`peer_transfer.rs`（3607 行）收敛为私有模块
  `peer_engine_receive/remote/transfer.rs`（2204 行），定位改为「host-peer 原语的引擎接入适配」
- Tauri 命令面：`invoke_handler` 只剩 `start/stop_peer_node` + `respond_peer_consent` +
  `list/revoke_trusted_peer` 五条；`set_peer_receive_policy` / `set_peer_download_dir` /
  `set_peer_transfer_encryption` 三条注销，函数体降为 `peer_net::*_for_plugin` 的内部真源
- 事件桥：收发快照改走 `peer_net::publish_bus_only` → `peer:transfer` / `peer:receive` 总线 topic，
  桌面 Tauri 前端不再有 peer 传输事件监听（`src/` 全域只听 `peer-connected` /
  `peer-disconnected` / `peer-consent-requested` 三条引擎连接态事件，宿主侧确有发射方）
- 前端：`bedcode-desktop/src/` 已零 `peer_*` invoke；file-transfer 插件 UI 全量走
  `context.commands.execute('file-transfer.*')`，manifest 声明 33 条命令与 `lib.rs` 分派表一致，
  覆盖发送/暂停/恢复/续跑/取消/重试/历史/批次/信任/远端浏览/设置/落点/共享根
- 数据面：宿主 `db/schema.sql` 无任何 peer 业务表；历史真源在插件 `transfer_entries` 表
  （`persist_entries` 本批改用 `plugin_db_execute_batch` 单调用内原子提交，替代跨调用 BEGIN/COMMIT）

### ② 本会话修复（票 06 范围内的残留）

1. **编译断裂 3 处**（`cargo check --lib` 直接失败，上一会话最后一笔改动导致）：
   `get_peer_receive_settings` 引用已删的 `PeerReceiveSettingsDto`、`apply_settings` 调用已删的
   `persist_settings` → 删除前者（零消费者）、移除持久化调用（宿主不再落盘设置）
2. **测试残留**：`is_valid()` / `read_settings_file` / `write_settings_file` / `SETTINGS_FILE` 四个已删
   符号仍被 4 个用例引用（`cargo check --lib` 不编译 test cfg，故 ① 的 check 未曾暴露）→
   删除「设置文件 roundtrip / 旧字段向后兼容」三例（宿主设置文件已不存在，测它即测幽灵），
   并发边界保留并改名 `concurrency_bounds_are_one_to_eight`
3. **死命令面**：`clear_peer_receiving_history`、`peer_pick_download_dir`、
   `set_peer_transfer_encryption` 三个 `#[tauri::command]` 未注册且零调用 → 删除
   （落点选择已由插件 `file-transfer.pick-download-dir` → `host-platform.pick-folder` →
   `peer_net::pick_folder_for_plugin` 覆盖；加密开关由 `send-files` 载荷 `encrypt` 字段逐次下发）
4. **失效 intra-doc 链接**：`peer_net.rs` 模块头 `[\`crate::peer_transfer\`]` 改指
   `crate::peer_engine_transfer` 并改正「历史落盘/事件发射」的陈旧描述（改述为总线快照）；
   `plugin/task_data_migration.rs` 对已删 `crate::peer_migration` 的链接改为纯文字引用
5. **文档**：`bedcode-desktop/docs/code-map.md` 的「命令面」段落仍写「保留 set_peer_* 兜底 +
   其余函数体暂留一版，下版本删除」，与树内实际状态矛盾 → 改写为票 06 后的真实形态；
   顺手修正 peer 四行在目录树里的缩进错位

### ③ 发现的行为回归（改名时删断的链路，已补回）

`peer_close` 第 ③ 分支（接收侧句柄取消）被改成与第 ② 分支同一个
`peer_net::cancel_transfer_for_plugin`（发送侧取消），而 `peer_engine_receive::cancel_peer_receiving`
整体消失、`peer_engine_remote::cancel_pull` 因此成为 never-used（编译器告警暴露）。
后果：插件端「取消正在接收 / 取消拉取」只能命中发送表，在途接收批与远端拉取会话无法中止，
pending 询问也不再按拒绝回执。

按改名前语义补回：`peer_engine_receive::cancel_peer_receiving`（pending 视同拒绝并回执 false、
running 先查拉取会话再回退服务端会话表）+ `peer_net::cancel_receiving_for_plugin` 包装 +
`peer_close` ③ 改指该包装。**未做行为验证**——需票 05/06 的闭环与集成测试证实（见 ⑤）。

### ④ 残留未清账（下一轮随门禁一起处理）

1. `peer_engine_receive::PeerTransferSettings.encryption_enabled` 现已无任何写入方（恒 `false`），
   发送侧 `encrypt: None` 的兜底分支成为死路径 → 决定「删字段 + 兜底恒 false 显式化」或保留，
   不建议留现状
2. `peer_engine_*.rs` / `peer_net.rs` 内 18 个 `#[tauri::command]` 属性未注册（本会话只清了所在
   段的 4 个）→ 属性剥离后编译器会把真正的死函数全部亮出来，属机械清账
3. 旧 `transfer_settings.json` 孤儿文件：宿主不再读写，但升级用户数据目录里的历史文件无人清理，
   且删掉 `peer_migration.rs` 后不再有「宿主旧数据 → 插件存储」的搬运入口 → 需取证插件侧
   `settings_store` 首启是否自带缺省（不回读旧文件即视为「设置项归零」，与用户故事 3
   「历史数据一条不丢」是否冲突要判定）
4. `peer_net.rs` 模块头仍以 issue 08/09/11 叙事描述「命令面支撑」，与票 06 后的定位有落差（仅文字）

### ⑤ 门禁待跑与并发约束

本轮**未跑任何测试/构建门禁**，原因：同 worktree 内另一会话（pi）正实时执行票 02
（`plugins/session/**` + `packages/plugin-sdk-desktop/rust/wit/bedcode.wit` +
`src-tauri/src/plugin/quick_actions_migration.rs`），且 01:08 起其 WIT 追加已让
`component.rs` 两枚 trait impl 缺项报错（`run_sync` / `read_dir`、`canonicalize`、`stat`）——
此刻任何 `cargo test` 结果都无法归因。票 02 收尾后按顺序补跑：

- `cd bedcode-desktop/src-tauri && cargo test --lib`（重出插件产物后再跑，避免 S1 闭环静默 skip）
- `cd bedcode-desktop && pnpm run test:run`（`--pool=forks`，见项目记忆）
- 根目录 `pnpm exec eslint .` 0 error
- peer 全链路真实 wasm 闭环：发/收/**取消接收与拉取**/暂停/恢复/浏览/策略/历史（本会话 ③ 的回归必须有用例覆盖）
- 历史迁移幂等 + 升级后历史可回溯；文档同步补 CHANGELOG（票 05/06 条目，本会话刻意未写以免与票 02 撞车）
