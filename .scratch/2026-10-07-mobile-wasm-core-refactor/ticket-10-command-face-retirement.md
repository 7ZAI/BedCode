# 票 10 · 宿主命令面收口 + 防回接锁（阶段 2 末票 · 传输调度面清零）

> 状态：**已完成（2026-10-08）**。门禁结果见 §6。**零 ABI 变更**、**零跨端协议变更**、
> **零 WIT 变更**、**零前端文件改动**、**零插件改动**（插件侧本票无调用点变化——票 06/07
> 就已把命令面全部让出）。

## 1. 目标与本票实际范围

spec 票 10：「宿主命令面收口 + 防回接锁」——宿主命令面
（`send_files_to_peer` / `list_peer_transfers` / `retry_*` / `resume_all_peer_transfers` /
`clear_*` / `get_peer_receive_settings` / `set_peer_transfer_encryption` /
`set_peer_transfer_concurrency` 等）注销 / 转薄转发；源码扫描锁
`retired_mobile_peer_transfer_orchestration_is_not_reintroduced`（移动版）+ 变异自检。

**逐项核实结果（不凭记忆，取自工作区实测）**：

| spec 条目 | 核实结论 | 本票动作 |
| --- | --- | --- |
| `send_files_to_peer` / `list_peer_transfers` / `retry_*` / `resume_all_peer_transfers` / `clear_*` | **票 06/07/08 已退役**（发送编排锁 13 needle、接收编排锁 12 needle 全绿；`resume_all_peer_transfers` / `resume_all_peer_receiving` 票 06/07 已删） | 零改动，锁已在册 |
| `get_peer_receive_settings` | 在册且**零消费者**（前端全仓无 invoke；设置真源在插件 settings store） | 删函数 + 删 `PeerReceiveSettingsDto` |
| `set_peer_transfer_encryption` | 在册且**零消费者**；加密裁决权已在插件（`send-files` 载荷逐项带 `encrypt`） | 删函数 |
| `set_peer_transfer_concurrency` | 在册且**零消费者**；唯一读者是 `peer_remote` 拉取队列并发闸门 | 删函数（字段与读点保留，见 §3.3） |
| 传输调度面（`cancel_peer_transfer` / `pause_peer_transfer` / `resume_peer_transfer` / `peer_pick_files`） | 仍注册，但零消费者；真入口已是 host-peer `close` / `pause-transfer` / `resume-transfer` 与 host-platform `pick-files` | 去 `#[tauri::command]` + 去注册，可见性收 `pub(crate)` |
| 远端浏览面（`list_peer_shared_roots` / `browse_peer_directory` / `pull_peer_files`） | 同上；真入口 = host-peer `list-shared-roots` / `browse-directory` / `pull-files` | 去属性 + 去注册 + 收 `pub(crate)` |
| `set_peer_receive_policy` | 注册在册、零消费者；但**是** host-peer `set-receive-policy` 的引擎实现 | 去属性 + 去注册，保留函数（收 `pub(crate)`） |
| 源码扫描锁 + 变异自检 | 三把锁（发送编排 / 接收编排 / 发现投影）在册，**均不覆盖命令面**——构件没删干净它们负责，构件还在但被挂回前端无人拦 | 新增第四把锁（命令面锁） |
| `stale_artifact_rebuild_hint` 判据扩展点名新 ABI | **本票零 ABI 变更**（WIT 一个字节未动），无新产物判据可扩展；票 06 的 `concurrency` 载荷 fail-visible（点名 ABI v12 重建）已在册且仍是唯一退役载荷判据 | 零改动，判据归位见 §5.3 |

## 2. 改动清单（5 文件，其中 1 新增）

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 宿主命令面 | `src-tauri/src/lib.rs` | `invoke_handler!` 注销 11 项 peer 命令（传输 4 + 接收设置 4 + 远端 3）；注释改述「票 09 + 票 10 后前端命令面为零」并点名两把真入口与防回接锁路径 |
| 宿主引擎 | `src-tauri/src/peer_transfer.rs` | 4 函数去 `#[tauri::command]`、`pub` → `pub(crate)`、文档补真入口（host-peer `close` / `pause-transfer` / `resume-transfer`、host-platform `pick-files`） |
| 宿主引擎 | `src-tauri/src/peer_receive.rs` | 删 3 命令函数 + `PeerReceiveSettingsDto`；`set_peer_receive_policy` 去属性收 `pub(crate)`、文档补真入口；退役说明落为「注（票 10）」段（含两字段为何保留） |
| 宿主引擎 | `src-tauri/src/peer_remote.rs` | 3 函数去 `#[tauri::command]`、`pub` → `pub(crate)`、文档补真入口；模块头「只读约束」段补「票 10 起无前端命令面」 |
| 防回接锁 | `src-tauri/tests/retired_mobile_peer_transfer_command_face_lock.rs`（新，4 用例） | ① 四个 peer 模块零 `#[tauri::command]` ② `invoke_handler!` 内零 peer 模块注册项 ③ 退役传输设置命令面字面量（4 needle） ④ 五个引擎原语仍在（反向断言，防「连原语一起删」后改走旁路） |
| 文档 | `bedcode-mobile/docs/code-map.md` | 对等网络节：命令面段改述为「票 09 + 票 10 后为零」+ 新增「传输设置字段（票 10 收口）」段；模块树两行纠偏（接收任务表已于票 07 退役） |
| 文档 | 本票 + `spec.md` + 双语 CHANGELOG | 票文档、spec 状态推进、变更条目 |

## 3. 为什么删（不是 tidy-up）

1. **旁路命令面会破坏「任务真源在插件」的前提**。票 06/07/08 的全部裁决建立在一个
   不变式上：发送/接收任务的**唯一真源**是插件 `transfer_store::reduce_event`。宿主
   `invoke_handler!` 里那 11 个命令一旦可被前端 invoke，就出现第二条写入路径——前端
   可以不经过插件事件归约直接读宿主状态、或通过命令触发宿主行为。真机上前端此刻
   已无消费者，但**注册项本身就是一条可被随手接回的旁路**，且这类旁路在
   ADR 0022 判据下是明确的 B4/B5（宿主替插件决定业务形状与默认值）。
2. **零消费者是实测结论，不是推测**。逐条 `invoke` 搜索（`bedcode-mobile/src/**`），
   前端只调 `mdns_*` / `egress_consent_resolve` / `plugin_*` 三族；11 个 peer 命令
   在 `src/` 与 `src/__tests__/` 全无引用，Kotlin / gen-android 侧亦无引用（仅构建
   产物里有历史符号）。票 06/07 把设置面与任务面迁插件时，前端调用点已同批改完。
3. **命令属性与注册项必须同批摘**。引擎原语（`pause_peer_transfer` 等）仍被
   `host_impl/peer.rs` / `host_impl/platform.rs` 调用，函数必须留；但只要还挂着
   `#[tauri::command]`，它就是「随时可注册」的候选。所以本票的锁直接锁**属性本身**
   （而不是锁符号名）——这是与前三把锁的本质分工。
4. **三个设置命令是「宿主侧第二份写入面」**：`get_peer_receive_settings` 读的
   `PeerTransferSettings` 真源已在插件 storage；`set_peer_transfer_encryption` 与
   `set_peer_transfer_concurrency` 则让宿主持有**只有它自己会写**的字段。留着它们
   等于承诺一个宿主侧设置面，而它与插件 settings store 无同步通道——用户从插件设置里
   改并发，宿主引擎副本纹丝不动，行为面分裂（票 06/07 §5.5 已两次记录同款双真源）。

## 4. 与桌面端的差异（点名）

| 语义 | 桌面 | 移动端（本票后） | 差异 |
| --- | --- | --- | --- |
| peer 前端命令面 | `server/peer_net_cmds.rs` 仍有一小批窄转发（consent 应答等历史外壳） | `peer_*` 四模块零注册 | 移动端更彻底；桌面清理由桌面批次立项（spec §8 Out of scope 含桌面改动） |
| 传输设置读面 | 桌面设置面在插件 | 同 | 对齐 |
| 引擎原语可见性 | 桌面 `pub` 居多（历史外壳未清） | 移动端收 `pub(crate)` | 移动端更紧；不影响插件（WIT 是唯一插件面） |

## 5. 已知遗留（点名，非静默）

1. **`peer_remote.rs` 拉取编排仍在宿主**（逐文件队列 + 信号量并发 + 会话表）：
   判据 B2 命中，spec 阶段 2 未给它单独票（票 07 §5.2 / 票 08 §5.2 / 票 09 §5.2 已
   三次记录，归属判断未变）。本票**未扩大也未缩小**该面。
2. **拉取并发上限失去写入面**：`set_peer_transfer_concurrency` 退役后，
   `PeerTransferSettings.concurrency` 只剩读点（`peer_remote` 拉取闸门），取值退化为
   「磁盘既有值或缺省 3」。**无用户可见回退**（票 06/07 后前端已无该命令的消费者），
   但这是「宿主持有无写入方字段」的显式记录，不是静默降级。要让拉取并发可调需
   新增 host-peer 原语（ABI 变更）——属阶段 3/独立立项。
3. **加密开关的宿主兜底分支恒为 false**：`send_files_to_peer_with_policy` 的
   `encrypt_override == None` 分支读宿主 `encryption_enabled`，该字段写入侧已随命令面
   退役；插件侧始终逐项下发 `encrypt`（`peer.rs` 元素级载荷构造），故实际行为不变。
   兜底分支保留为「旧调用方兼容」——若将来真有不带 `encrypt` 的调用方，它退回明文
   而不是误加密，语义保守。**建议后续随调用点收敛一并删**（本票不改，避免扩大改动面）。
4. **`PeerTransferSettings` 两个字段成为只读残留**：`encryption_enabled` /
   `concurrency` 保留在结构体与磁盘格式里（`serde(default)` 兼容旧安装），不再是
   可配置项。保留理由：引擎侧仍有读点 + 旧安装持久化兼容位。
5. **中文 CHANGELOG 票 07 / 08 条目仍缺**（两轮用户裁决「那两轮只写英文」）：
   本票起双语同步写入，07 / 08 的中文回补仍未做，漂移记在此处。
6. **真机双端互连未跑**：本票删除的是零消费者命令面（行为面等价），但「宿主 peer
   前端命令面归零」的真机确认仍需真机——列入票 20 全量验收。

## 6. 门禁结果

| 门禁 | 结果 |
| --- | --- |
| 宿主 `cargo check --lib` | ✅ 0 error 0 warning |
| 新锁 4 用例 | ✅ 4 passed / 0 failed |
| 前三把锁回归 | ✅ 发送 2 / 接收 2 / 发现投影 4，共 8 passed / 0 failed |
| **变异自检 4/4** | ✅ ① 挂回 `#[tauri::command]` 到 `peer_pick_files` → `peer_host_modules_have_no_tauri_command_attribute` 红 ② 在 `invoke_handler!` 注回 `peer_receive::set_peer_receive_policy` → **编译期 E0433 先红**（`generate_handler!` 找不到 `__cmd__*`，比锁更早拦截——与票 09 同款结论，故注册面锁无法构造「注册且能编译」的变异，其价值在属性锁被绕过时的第二道拦截）③ 注回 `get_peer_receive_settings` + `PeerReceiveSettingsDto` → `retired_peer_transfer_settings_command_face_is_not_reintroduced` 红 ④ 把 `peer_pick_files` 改名并同步调用方（保证可编译）→ `peer_transfer_scheduling_entrypoints_stay_engine_only` 红（命中 4 / 应为 5）。四次变异均已还原并复跑全绿 |
| 宿主 `cargo test` 全量 | ✅ `--no-fail-fast`：lib **374 passed / 6 failed**（6 个全在并行会话在途的 `egress.rs`，非本票文件，与票 06/07/08/09 基线**同数同款**）+ **全部集成目标全绿**（含新锁 4 例、另三把锁 2+2+4、http_auth 17 / http_proxy 7 / mock_plugin_ws 14 / ws_protocol / terminal_stream / session_http / storage 锁各绿） |
| 前端 vitest / 根 eslint | ⚠️ **未跑**：本票零前端文件改动（11 命令前端零消费已逐条核实，见 §3.2），无 i18n key 增减 |
| `cross-end-tests` | ⚠️ **未跑**：本票不改跨端协议（peer 线协议 / REST / WS / 总线载荷 / WIT 均未动） |
| 真机双端互连 | ⚠️ **未跑**：见 §5.6 |
| 插件 crate 测试 / wasm32 门禁 | ⚠️ **未跑**：本票零插件文件改动（`plugins/file-transfer/rust/` 未触碰），无新增 guest 代码路径 |
| `cargo clippy --lib --tests` | ✅ 本票四个文件**零新增告警**（现存告警全在并行会话文件：`plugin/manager.rs` 测试变量、`connection/ws_client.rs` 等；`peer_receive.rs:542` 的 `io::Error::other` 与 `peer_remote.rs:80` 的 doc 缩进两处告警**均为既有行、本票未触碰**） |
| `rustfmt --check` | ✅ 新锁文件 clean（首轮 `--check` 报了 `RETIRED_HANDLER_PREFIXES` 折行与文末空行，已按rustfmt 格式化复跑）；`peer_transfer.rs` / `peer_receive.rs` / `peer_remote.rs` 三文件**全文件零漂移**（本票新增块均clean，无需整文件格式化） |

## 7. 执行记录

- 变异备份：`.dev-logs/t10-{pt,pr,lib,plat}.bak`（工作区内，未入版本库）；还原后
  `git diff --stat` 确认 `host_impl/platform.rs` 零漂移。
- 变异注入均用**字节模式** Python 就地替换（非 heredoc 传中文长文本），规避
  bash heredoc 吞字；写回后逐文件确认 diff 形态正常、无 CRLF 漂移。
- 全量测试日志：`.dev-logs/t10-host-test-all.log`（工作区内，不入版本库）；
  收尾已确认无残留 cargo / rustc 进程。

## 8. 遗留的平行线提醒（非本票范围）

工作区同时存在另外两条并行线（**不碰**）：

- `.scratch/2026-10-07-capability-crates-to-root-packages/`：8 个能力域 crate 上提根
  `packages/`。
- `.scratch/2026-10-07-mobile-wasm-platform/`：移动端 WASM 应用平台 UI 重构（设计原型）。

## 9. 下一票

**票 11 · host-websocket 客户端域（ABI 11→12 的后续段，阶段 3 首票）**：WIT 新增客户端
域 5 函数 + SDK 绑定 + host_impl + 属主私有事件，为票 12（终端订阅协议客户端迁插件）
铺路。本票零 ABI 变更，故阶段 2 已全票落地完毕。