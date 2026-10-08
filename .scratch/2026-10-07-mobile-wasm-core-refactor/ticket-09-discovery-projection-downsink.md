# 票 09 · 发现 / 设备列表投影下沉（阶段 2 末票 · 宿主残留收口）

> 状态：**已完成（2026-10-07）**。门禁结果见 §6。**零 ABI 变更**（WIT 仅注释纠偏）、
> **零跨端协议变更**、**零前端文件改动**。

## 1. 目标与本票实际范围

spec 票 09：「发现/设备列表投影下沉」——宿主 `peer-devices-changed` 快照指纹链路退役；
插件 device_bridge 自建缓存（订阅属主定向 mDNS 事件 + last-seen 持久化缓解首屏空窗）；
共享目录注册表 CRUD → 插件；`list_discovered_peers` / `list_trusted_peers` 收窄。

**逐项核实结果（不凭记忆，取自工作区实测）**：

| spec 条目 | 核实结论 | 本票动作 |
| --- | --- | --- |
| `peer-devices-changed` 快照指纹链路退役 | **issue 13 Phase 4 已退役**（`bus_topic_for` 无映射、无指纹比对任务）。但**插件仍在订阅**总线 `peer:devices` 做「规模对账」，而宿主已无发布者 = 死订阅 | 删插件侧死订阅与对账分支；锁住 topic 双向复活 |
| 插件 device_bridge 自建缓存 + last-seen 持久化 | **已落地**：前端 `deviceState.ts` 状态机（found/lost/TTL/能力位）+ `device_bridge` 快照经 host-storage 落盘（含 `lastSeenMs`），activate 首屏恢复标「最近可见」 | 零改动（核实通过，code-map 补记） |
| 共享目录注册表 CRUD → 插件 | 真源**已在插件**（`roots_registry`），但**宿主侧仍留着三个 CRUD 命令**——写镜像后会被插件下次全量推送覆盖 = 第二份真源 | 删宿主三命令 + 投影 DTO |
| `list_discovered_peers` 收窄 | 仍在册且**零消费者** | 删函数 + DTO |
| `list_trusted_peers` 收窄 | 函数被 host-peer `list-trusted` 复用（保留），但**Tauri 注册零消费者** | 去 Tauri 注册，保留引擎入口 |

**本票额外清退**（同属「发现 / 连接编排」命令面，且与 spec 票 04 §6 的移交项一致）：

- `dial_peer`（缓存解析版拨号）：票 04 §6 明确「命令面收窄留票 09」。ADR 0022 v2 已定「宿主不内藏
  node-id → 地址解析表」，而这函数正是那张表的使用面（唯一调用方是零消费者的前端命令面）。
  拨号真入口是 `dial_peer_endpoint`（host-peer `dial-peer`，endpoint 由插件设备缓存显式传入）。
- `start_peer_node` / `stop_peer_node`：零消费者，且**不认领属主**——与票 04 落地的「谁起谁停」
  属主记账直接冲突（无主启动会让插件停用后节点仍在跑、对端照样发现）。
- `disconnect_peer` / `respond_peer_consent` / `list_trusted_peers` / `revoke_trusted_peer`：
  去 Tauri 注册保留函数（真入口 = host-peer `close` / `respond-consent` / `list-trusted` /
  `revoke-trusted`，以及 `revoke_trusted_peer` → `disconnect_peer` 内部断连）。

**保留的引擎事实（非投影，不得删）**：`DiscoveryCache` 仍服务三处引擎内部用途——入站连接展示名
解析、首连确认落库元数据、endpoint 拨号的展示名兜底（缓存未命中时观察回退记录）。

## 2. 改动清单（7 文件，其中 2 新增）

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 宿主引擎 | `src-tauri/src/peer_net.rs` | 删 5 函数（设备列表查询 / 缓存解析版拨号 / 共享目录列表 / SAF 新增 / 共享目录移除）+ 2 投影 DTO（`DiscoveredPeerDto` / `SharedDirDto`）+ 1 `From` impl + 2 节点启停函数；4 函数去 `#[tauri::command]`；模块头补票 09 段（含 issue 08 段标注退役）；5 处函数文档改述真入口；`bus_topic_for` 注释改述 topic 双向退役；import 清 `SharedDirRoot` |
| 宿主命令面 | `src-tauri/src/lib.rs` | `invoke_handler!` 注销 11 项 peer 命令（票 09 六项 + 票 06/07 遗留的 5 项传输/接收/远端命令本票**不动**，见 §5 遗留），注释改述「Peer Net 零前端命令面」 |
| 防回接锁 | `src-tauri/tests/retired_mobile_peer_discovery_projection_lock.rs`（新，161 行 / 4 用例） | ① 退役投影构件（7 needle）② 无主节点启停命令面（4 needle，覆盖函数定义与注册引用两种形态）③ `peer:devices` topic 双向（扫宿主 `src/` + 插件 `rust/src/` 全部 `.rs`）④ 拨号入口唯一性（`fn dial_peer(` 不得复活） |
| 插件 | `plugins/file-transfer/rust/src/lib.rs` | 删 `peer:devices` 死订阅（activate + deactivate）与 `on_message` 对账分支；模块头纠偏两处（注册表真源 = host-storage 单键，非 plugin-database；「双写期旧命令面保持可用」已过期） |
| WIT | `packages/plugin-sdk-mobile/rust/wit/bedcode.wit` | host-peer 接口文档 topic 表纠偏：`peer:devices` 换成 `mdns:found.<plugin-id>` / `mdns:lost.<plugin-id>` 属主定向事件（**注释级，零 ABI 变更**） |
| 文档 | `bedcode-mobile/docs/code-map.md` | 对等网络节：命令面段改述为「票 09 起为零」+ 新增「设备列表投影（票 09 收口）」段 |
| 文档 | 本票 + `spec.md` + 双语 CHANGELOG | 票文档、spec 状态推进、变更条目 |

## 3. 为什么删（不是 tidy-up）

1. **共享目录三命令 = 宿主侧第二份真源**。注册表真源在插件 `roots_registry`（host-storage 单键），
   插件每次增删都经 `set-shared-roots` 全量覆盖引擎镜像。宿主 `add_shared_directory_saf` 写进
   镜像的条目，会在插件下次推送时被静默抹掉——用户「添加的目录自己消失了」。这类双写不是洁癖
   问题，是可复现的数据丢失路径。
2. **`dial_peer` = 引擎内藏寻址表**。ADR 0022 v2 裁决「寻址来源由调用方持有」，插件已经按裁决
   传 endpoint；留着 node-id 解析版等于给「引擎替插件解析地址」留回接入口，且它的错误文案
   （`not in discovery cache`）正是 host_impl 自动重拨逻辑的**匹配字符串**（`with_auto_redial`
   判 `e.contains("discovery cache")`）——两条寻址路径并存会让重拨判据继续挂在退役面书上。
3. **节点启停命令面破坏属主记账**。`start_peer_node` 不认领属主，前端一旦调用就会留下「无主
   节点」：插件 `stop-node` 因非属主被拒，节点得等到进程退出才下线，对端全程照常发现本机。
4. **`peer:devices` 死订阅会污染排障判断**。插件 `on_message` 里有「宿主发现缓存规模 vs 前端
   自建缓存规模仅记日志」的对账分支。宿主无发布者 ⇒ 该分支永不执行 ⇒ 设备列表异常时看日志会
   以为「对账正常」。留着它等于给排障留假绿灯。

## 4. 与桌面 v31/v34 的差异（点名）

| 语义 | 桌面 | 移动端（本票后） | 差异 |
| --- | --- | --- | --- |
| 设备列表命令面 | 无 `list_discovered_peers`（桌面发现缓存只经 host-mdns 定向事件） | 同（票 09 注销） | 对齐 |
| `list_trusted_peers` | lib.rs **仍注册** Tauri 命令 | 去注册，只留 host-peer 原语 | 桌面那份注册是历史外壳（设置面在插件），移动端不再跟随——**桌面清理由桌面批次立项**，本专项 Out of scope 含桌面改动 |
| 共享目录注册表 | 插件真源 + host-peer `set-shared-roots` 镜像 | 同 | 对齐 |
| 节点启停 | 属主记账（审计票 12），无宿主命令面 | 同（票 09 注销最后两个无主命令） | 对齐 |

## 5. 已知遗留（点名，非静默）

1. **传输 / 接收 / 远端浏览的 5 个 Tauri 命令仍在册**（`cancel_peer_transfer`、
   `pause_peer_transfer`、`resume_peer_transfer`、`peer_pick_files`、`list_peer_shared_roots`、
   `browse_peer_directory`、`pull_peer_files`、`get_peer_receive_settings`、
   `set_peer_receive_policy`、`set_peer_transfer_encryption`、`set_peer_transfer_concurrency`）：
   票 09 只收口发现 / 连接编排面，传输面属**票 10**（宿主命令面收口 + 锁扩展到调度面）。
2. **`peer_remote.rs` 拉取编排仍在宿主**（逐文件队列 + 信号量并发）：判据 B2 命中但 spec 阶段 2
   未给它单独票（票 07 §5.2、票 08 §5.2 已两次记录，归属判断未变）。
3. **「共享目录注册表 → `host-plugin-database`」与 spec 措辞有偏差**：实际落在插件
   host-storage（票 05b 后真源是主库 `plugin_storage` 表），不是 host-plugin-database 私有库。
   理由：条目规模 ≤ 数十、单键整读改整写，KV 形态足够；05c 已核实插件三方无 db 用量。语义目标
   （注册表真源在插件、不在宿主）已达成，机制选型偏差在此显式记录。
4. **`dial_peer_endpoint` 仍有「缓存未命中则观察回退记录」过渡桥接**：数据面函数内部按 node-id
   寻址并依赖缓存解析元数据，Phase 4 才退役（issue 13 排期，本专项不动）。
5. **真机双端互连未跑**：本票删除的是零消费者命令面 + 死订阅，行为面等价；但「设备列表真源
   完全在插件」的真机确认仍需真机（列入票 20 全量验收）。
6. **中文 CHANGELOG 票 07 / 08 条目仍缺**（两轮用户裁决「本轮只写英文」）：本票起恢复双语，
   07 / 08 的中文回补未做，漂移显式记在此处。

## 6. 门禁结果

| 门禁 | 结果 |
| --- | --- |
| 宿主 `cargo check --lib` | ✅ 0 error 0 warning（清掉 1 个本票引入的 unused import `SharedDirRoot`） |
| 防回接锁 4 用例 | ✅ 4 passed / 0 failed |
| **变异自检 4/4** | ✅ ① 注回设备列表 DTO + 共享目录三命令 → `retired_mobile_peer_discovery_projection_is_not_reintroduced` 红；② 注回 `start_peer_node` / `stop_peer_node` 函数定义 → `retired_host_peer_command_face_is_not_reintroduced` 红（**首轮只锁注册形态漏过函数定义形态，已把 needle 扩到 4 个后复跑转红**）；③ 注回 `fn dial_peer(` → `endpoint_dial_is_the_only_peer_dial_entry` 红；④ 插件注回 `"peer:devices"` 订阅 → `retired_peer_devices_topic_is_neither_published_nor_subscribed` 红。均已还原并复跑全绿 |
| lib.rs 注册回接 | ✅ 编译期双保险：在 `invoke_handler!` 注回 `peer_net::list_shared_directories` / `peer_net::start_peer_node` 直接 **E0433 编译失败**（`tauri::generate_handler!` 找不到 `__cmd__*`），比锁更早拦截 |
| 插件 crate `cargo test` | ✅ **58 passed / 0 failed**（与票 08 同数——本票插件侧只删死代码与文档，无行为用例增减） |
| **wasm32 门禁（`--rust-only`，spec §6 硬门禁）** | ✅ wasm32 release + componentize 成功（**725,420 bytes**；票 08 为 725,757，差值即删除的死分支）。唯一 warning 是既有基线 `entry_from_dto` 未使用 |
| 宿主 `cargo test` 全量 | ✅ `--no-fail-fast`：lib **374 passed / 6 failed**（6 个全在并行会话在途的 `egress.rs`，非本票文件，与票 06/07/08 基线一致）+ **集成目标全绿**（含新增锁 4 例、另两把锁 2+2、http_auth 17 / http_proxy 7 / mock_plugin_ws 14 / ws_protocol / terminal_stream / session_http / storage 锁各绿） |
| `rustfmt --check`（本票文件） | ✅ 新增块与新锁文件 clean；`peer_net.rs` 全文件有 **41 处既有格式漂移**（import 排序、`state.node_owner.lock()` 折行等，HEAD 即如此），按最小改动原则**未整文件格式化**（ticket 08 同款裁决） |
| `cargo clippy --lib --all-targets` | ✅ 本票两个文件**零告警**（现存告警全在 `plugin/saf_io.rs` / `connection/ws_client.rs` / `plugin/manager.rs` 等并行会话文件） |
| 前端 vitest / 根 eslint | ⚠️ **未跑**：本票零前端文件改动（宿主命令面零消费者已逐条核实），无 i18n key 增减 |
| 真机双端互连 | ⚠️ **未跑**：见 §5.5 |
| `cross-end-tests` | ⚠️ **未跑**：本票不改跨端协议（peer 线协议 / REST / WS / 总线载荷均未动；总线侧只删一条无发布者的订阅） |

## 7. 遗留的平行线提醒（非本票范围）

工作区同时存在另外两条并行线（**不碰**）：

- `.scratch/2026-10-07-capability-crates-to-root-packages/`：8 个能力域 crate 上提根
  `packages/`（git status 里大量 `bedcode-desktop/packages/*` → `packages/*` 重命名）。
- `.scratch/2026-10-07-mobile-wasm-platform/`：移动端 WASM 应用平台 UI 重构（设计原型）。
