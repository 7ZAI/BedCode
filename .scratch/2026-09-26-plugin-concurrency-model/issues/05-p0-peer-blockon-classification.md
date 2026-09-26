# 05 — P0-5：`host-peer` 的 23 处 `block_on_async` 逐个分类

**Type:** task（审计）
**Spec:** `../spec.md`（§11 前置待办 5 + §4.3 候选 ③）
**Blocked by:** None — can start immediately
**Status:** done（2026-09-26）——分类表见 `## Conclusion`（口径校正：真实调用点 22 处）；
「真等外部」短名单 3 纯 + 1 混合，供票 09；零生产改动

**What to build:** 把 `host_api/peer.rs` 的 23 处 `block_on_async` 逐个分类为「**本地 await**」（进程内锁/信号量/引擎内部 future，微秒级，无等待语义）或「**真等外部**」（对端网络往返、传输状态等待，时长不受本仓库控制），产出分类表作为 P4（票 09）候选 ③ 的证据。

分类口径（spec §4.1）：

- **本地 await** → 保持同步，不 async 化（C1✗）；
- **真等外部** → 进候选清单，P4 走 §4.1 判据（C2 是否显著 / C3 等待期间确有其它工作 / C4 是否不可替代）。

**工作内容：**

1. `src-tauri/src/wasm_core/host_api/peer.rs` 23 处 `block_on_async` 逐个登记：所在宿主函数、等待的具体对象（握的什么锁 / 等哪个 channel / 等对端什么状态）、是否跨 tokio 任务或跨实例。
2. 对每一处给出分类 + 一句话理由；「真等外部」的项额外标注：等待时长量级（若可估）、等待期间同实例其它交互是否存在现实堵死场景（即 C3 是否成立）。
3. 产出两张表：⑧ 全量清单（23 项逐一）＋ ⑨ 「真等外部」短名单（≤ 若干项），后者直接供给票 09。

**Out of scope:**

- 不改 `peer.rs` 任何代码。
- 不给 `database/auth/ws/pty/…` 分类（本票只管 peer——它是 P4 候选 ③ 和前 23 处的出处；其余候选在票 09 里处理）。

## Conclusion（2026-09-26，逐处分类完成）

**口径校正**：`rg block_on_async host_api/peer.rs` 的 **23 次命中里第 1 次是 `use` 导入行（`:15`）**，
真实调用点 **22 处**（`peer.rs` 共 666 行，全部在 `server::peer_net::*` 的既有 async 实现之上）。

### ⑧ 全量清单（22 处）

| # | 行 | 宿主函数（原语） | 等待对象（引擎侧） | 分类 | 理由 |
| --- | --- | --- | --- | --- | --- |
| 1 | 129 | `with_auto_redial`（`dial`/`send-files` 等共用的重拨兜底） | `dial_peer_endpoint` → `node.dial()` TLS 握手 + 协议协商 | **真等外部** | 对端网络可达性/响应速度不可控；重拨路径 |
| 2 | 153 | `peer_dial` | 同上 | **真等外部** | 同上（主路径） |
| 3 | 183 | `peer_close` | `disconnect_peer`：connections 表 remove + watch 关闭 + 入站 handler abort | 本地 await | 进程内表 + 通道信号；无网络往返 |
| 4 | 190 | `peer_close` | `cancel_transfer_for_plugin`：cancel_token.cancel() / serve 会话表 | 本地 await | 进程内令牌 + 注册表；`cancel_serve_session` 仅查表 |
| 5 | 198 | `peer_close` | `cancel_receiving_for_plugin`：pending 表 remove + pull 表 + handler.cancel_transfer | 本地 await | 同 4（纯表操作 + 同步调用） |
| 6 | 216 | `peer_respond_consent` | `respond_peer_consent`：consents 表 remove + trust DB 写 + oneshot send | 本地 await | 本地 SQLite + 进程内通道；远端等待方是**它方**不是本调用 |
| 7 | 227 | `peer_list_trusted` | `list_trusted_peers`：trust 句柄读 + runtime 快照（online cache） | 本地 await | 本地 DB 读 + 内存 cache；C1✗ |
| 8 | 238 | `peer_revoke_trusted` | `revoke_trusted_peer`：trust DB 删 + `disconnect_peer` | 本地 await | 同上（断开见 3） |
| 9 | 296 | `peer_send_files` | `send_files_for_plugin`：runtime 快照 → `spawn_blocking(collect_outgoing_files)` 目录递归 → register_session → `drive_send_session`（**spawn 后立即返回 batch-id**） | 本地 await | 无外部等待；**但目录递归是本地阻塞点**（见下「非候选但需记账」） |
| 10 | 318 | `peer_respond_transfer` | `respond_transfer_for_plugin`：pending 表 remove + oneshot send | 本地 await | 纯表 + 通道 |
| 11 | 336 | `peer_set_receive_policy` | `set_peer_receive_policy_for_plugin`：校验 + `apply_settings`（DB 写 / 句柄更新） | 本地 await | 本地配置写 |
| 12 | 353 | `peer_pause_transfer` | `pause_transfer_for_plugin`：`PauseSlot::send`（mpsc 命令通道）/ serve 暂停表 | 本地 await | 通道投递；通道满时有界等待（backpressure 属引擎内部） |
| 13 | 372 | `peer_resume_transfer` | `resume_transfer_for_plugin`：live 会话 `slot.send(Resume)`；**会话已中断 → `drive_redialed_session`（重拨 + 续传）** | **混合**（主路径本地 / redial 分支真等外部） | 主路径同 12；redial 分支网络不可控 |
| 14 | 406 | `peer_set_shared_roots` | `set_shared_roots`：`store.replace_all(&entries)`（内存 + mDNS 暴露面） | 本地 await | 内存 CRUD（真源已移插件侧） |
| 15 | 420 | `peer_list_shared_roots` | `list_remote_roots_for_plugin` → `dial_peer` + `list_shared_roots(conn)` | **真等外部** | 拨号 + 对端响应往返，且**每操作新拨号** |
| 16 | 442 | `peer_browse_directory` | `browse_remote_for_plugin` → `dial_peer` + `browse_shared_dir(conn,…)` | **真等外部** | 同上（对端目录列举往返） |
| 17 | 470 | `peer_pull_files` | `pull_files_for_plugin`：快照 + 校验 + `spawn_with_error_boundary(run_pull_queue)` → 立即返回文件数 | 本地 await | **不等待传输**（逐文件会话在后台任务发起） |
| 18 | 487 | `peer_set_download_dir` | `set_peer_download_dir_for_plugin`：`tokio::fs::create_dir_all` + settings 写 | 本地 await | 本地 FS + DB |
| 19 | 504 | `peer_start_node` | `start_node_owned`：node_owner 记账 + 引擎起节点（bind/mDNS） | 本地 await | 进程内生命周期（绑端口是本地系统调用，非外部事件） |
| 20 | 517 | `peer_stop_node` | `stop_node_owned`：属主校验 + 引擎停节点 | 本地 await | 同上 |
| 21 | 531 | `peer_active_transfers` | `active_transfers_for_plugin`：三张会话表聚合投影 + JSON 序列化 | 本地 await | 纯内存投影（同步函数，`async` 仅为统一形状） |
| 22 | 544 | `peer_collect_outgoing` | `collect_outgoing_for_plugin`：`spawn_blocking(collect_outgoing_files)` 目录递归 + 序列化 | 本地 await | 本地 FS 扫描（调用自身驱动）；见下记账 |

### ⑨ 「真等外部」短名单（供票 09 候选 ③）

| 序 | 原语 / 入口 | 等待什么 | C1 必要 | C2 显著 | C3 有价值 | C4 不可替代 | 量级估计 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| ③-1 | `host-peer.dial`（`peer_dial` L153；重拨兜底 L129 同源） | 对端 TLS 握手 + 信任协商 | ✓ | ✓（对端离线时可达 TCP 超时；用户体感「点了没反应」） | ✓（等待期间设置面/其它设备浏览可继续） | ✓（现状无「拨号结果事件」形态；要非等待替代须新增事件面） | 百 ms ~ 数秒（对端离线时到超时） |
| ③-2 | `host-peer.list-shared-roots`（L420） | 拨号 + 对端根清单应答 | ✓ | ✓ | ✓（同实例其它命令，如取消/切换设备） | ✓（同上，无事件化路径） | 百 ms 级（**每次操作新拨号**，含握手） |
| ③-3 | `host-peer.browse-directory`（L442） | 拨号 + 对端目录列举应答 | ✓ | ✓ | ✓ | ✓ | 百 ms ~ 秒级（大目录） |
| ③-4 | `host-peer.resume-transfer` 的 **redial 分支**（L372） | 重拨 + 续传会话驱动 | ✓ | ✓ | ✓ | ⚠ 混合：主路径（live 会话）是本地通道——**要 async 化必须按分支切，不能整函数标 async**（§4.4 反模式「半 async 半 sync」⇒ 结论：本条**暂不立项**，等 ③-1/③-2/③-3 走完管线后再评估是否拆出 redial 专用原语） | 秒级 |

**结论**：候选 ③ 的收敛结果是 **3 个纯外部等待项（③-1/③-2/③-3）+ 1 个混合项（③-4 暂缓）**，
20 处维持同步（其中 17 处是纯本地表/通道/DB，3 处含本地 FS 阻塞：L296 目录递归、L544 目录递归、
L487 `create_dir_all`）。

### 非候选但需记账（不进 P4，但影响 P1 的 I2 表述）

- **`peer_send_files` / `peer_collect_outgoing` 的目录递归**（`spawn_blocking(collect_outgoing_files)`）：
  C1✗（本地 IO，由调用驱动）⇒ 保持同步；但在属主模型下它**占住调用方属主**（host import 内
  `block_on_async`）——大目录时表现为「插件暂时不响应其它命令」，与今天逐字等价（I5 保持），
  P1 不得声称已消除。**若将来出现体感问题**，处置是「插件先调 `collect-outgoing` 再 `send-files`」
  （已有原语，插件侧编排，ADR 0022：业务编排归插件），不是把宿主改 async。
- **`pause_transfer` 的 `PauseSlot::send`**：通道满时有界等待（引擎 backpressure）；
  C1 部分成立（等**引擎**消费，非外部），维持同步，记 ADR 偏离（P4 复盘时再看 ③-4）。
- `peer_active_transfers` / `peer_collect_outgoing` 两处 `block_on_async` 包的是**同步实现**
  （`async fn` 只是统一形状），可顺手去 async 化（P1/P4 重构时清理，非本票范围）。

**Acceptance:**

- [x] 22 处（口径校正：`rg` 23 次命中含 1 次 `use` 行）逐一有行：宿主函数 / 等待对象 / 分类 / 理由。
- [x] 「真等外部」短名单：3 纯 + 1 混合（每项带 C1-C4 标注与量级）。
- [x] 结论写回本票 + spec §11 第 5 项。
- [x] 零生产改动（只读 `host_api/peer.rs` + `server/peer_net*`）。

## 关联证据

- `src-tauri/src/wasm_core/host_api/peer.rs`（23 处 `block_on_async` —— 全 host_api 最高频）