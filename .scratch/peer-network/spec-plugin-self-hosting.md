# Spec: file-transfer 业务自持与旧接口退役（issue 13 Phase 3–4）

Status: ready-for-agent —— 两处开放决策已于 2026-08-26 由用户裁决（见「Resolved Decisions」节）：A 取 A1（终态 host-peer = 13 原语）、B 取停写+一版读取

词汇表沿用 CONTEXT.md「对等网络」「文件传输」节；裁剪线与最终原语面依据 docs/adr/0022 v2 与 `.scratch/peer-network/issues/13-wit-host-peer-slimming.md`（Phase 1–2 已实施：host-peer 新增 dial-peer-endpoint / close / set-shared-roots 三原语、读对端四函数双态寻址、host-mdns / host-platform 就绪、ABI desktop v9 / mobile v7）。本 spec 是该票 Phase 3–4 的可执行规划：**把设备列表、共享目录注册表、任务队列与历史、接收策略设置、重试编排从宿主命令面下沉到 file-transfer 插件自持，随后退役全部旧 WIT 函数与宿主发现快照链路**。

## Problem Statement

Phase 1–2 完成后，宿主同时存在两套可用通路：旧命令面（list-devices / list-transfers / add-shared-directory 等 16 个待退役函数）与新原语面（13 个终态函数【A1 裁决后】+ host-mdns + host-platform）。file-transfer 插件仍全量消费旧命令面——其 WASM 翻译层（两端各约 400 行 peer.rs）做着纯 DTO 搬运，设备列表依赖宿主 DiscoveryCache 常热快照（`peer:devices` topic），任务与历史是宿主引擎内存态的投影。这正是 ADR 0022 要终结的局面：业务迭代同步五处、DTO 翻译两遍、第二个对等网络消费者无法进入。此外新原语虽已就绪但无人消费（dial-peer-endpoint / close / set-shared-roots 零调用方），契约正确性只被单测覆盖、未经真实负载检验。

## Solution

分两个阶段收口。**Phase 3（业务自持，双端）**：插件以 host-mdns browse 自建设备缓存（去重 / TTL / 展示名 / 能力位解读 + last-seen 持久化），以 set-shared-roots 全量推送接管共享目录注册表真源，以自有持久化层建任务队列与历史（bus `peer:transfer` / `peer:receive` 驱动状态机），接收策略三分支改为插件编排（ask 弹窗已有，auto 分支补自动应答），重试改为批元数据回放重调原语；期间新旧双写，任一时点可回退。**Phase 4（退役）**：删除 WIT 终态清单外全部旧函数及 host_impl 对应实现，退役宿主 DiscoveryCache 守护与 `peer:devices` 快照链路，数据面全面句柄化，双端同版发布 + 真机全链回归。

## User Stories（增量；既有故事见父 spec）

1. 作为任一端用户，文件传输插件停用后，本机的 mDNS 可发现性不变（自我广播随宿主节点存活），以便他人仍能看到本机——只是本机不再展示任何传输 UI。
2. 作为任一端用户，插件重新启用后设备列表应立即恢复（含离线设备的 last-seen 缓存标注），以便不必等待重新发现。
3. 作为发送方，发送失败后我点击重试，应从断点续传而非从头开始；应用重启后历史中的失败批仍可重试（源文件仍在原位时），以便大文件不白传。
4. 作为桌面端用户，我在插件设置中修改接收落点后，后续**推送**接收的文件落入新目录，以便自定义目录对两种接收方向都生效。
5. 作为任一端用户，升级安装后既有传输历史与本机设置保留，以便无缝过渡。

## Implementation Decisions

### 总原则

1. **阶段化双写**：Phase 3 全程新旧并存——插件每写一份自有状态就继续调旧命令面（或反之），前端切换按功能域逐个进行；Phase 3 验收标准即「拔掉任一通路，另一通路功能完整」。Phase 4 一次删除。
2. **数据层分层（双端不对称是现实约束）**：桌面端结构化数据（任务/历史/共享目录注册表）落 **host-plugin-database**（独立 SQLite，auto-task 先例）；移动端无 host-plugin-database，统一落 **host-storage KV**（JSON 快照 + 写入前整读改整写）。移动端数据规模上限可控（历史封顶 200 条、设备缓存 ≤50 条），KV 形态足够；若后续超限再立票补 mobile plugin-database（涉及 ABI bump，不在本 spec 范围）。
3. **引擎内部状态不迁移只断供**：引擎的 PeerTransferState / ReceivingStateStore / shared_dirs.json 照常工作（它们是数据面运转的必需品），退役的只是其**对外查询/管理接口**（list-transfers 等）。插件自有状态是「产品视图」，引擎状态是「运行时簿记」，二者在 Phase 3 期间短暂冗余、Phase 4 后各司其职。

### Phase 3 步骤（每步独立可验证）

**步骤 1：设备缓存自持（替换 list-devices / peer:devices 消费）**

- 插件 activate 即 `mdns_browse(SERVICE_TYPE)`（服务类型字符串从 SDK 常量取，双端一致）；deactivate 时 stop-browse（宿主 purge 兜底）。
- 订阅 `mdns:found` / `mdns:lost`，设备缓存状态机：`found → 解析 txtRecords{ id, name, ver, cap } → 按 node-id upsert（addr 取 addresses 首 IPv4）`；`lost → 按实例名短指纹移除`；TTL 过期用**惰性清扫**（每次读取列表/收到事件时顺带清理 last_seen 超时项，不依赖定时器——移动端 manifest 无 timer 权限，桌面端也不为此新增）。
- 展示字段派生：name 缺失回退 instanceName 末段；能力位 `cap` hex 解析，bit0 = 文件传输（CAP_FILE_TRANSFER），无能力位节点在 UI 置灰不隐藏（父 spec 故事 2 的可见性语义）。
- last-seen 持久化：设备快照 JSON 写 storage 键（含 endpoint 三元组 nodeId/addr/port + deviceName + lastSeenMs）；activate 时先载入快照渲染列表（标注「最近可见」），再等实时事件刷新——缓解首屏空窗（ADR 0022 Consequences 已接受的取舍）。
- 连接动作改走 `dial-peer-endpoint({nodeId, addr, port})`，返回句柄由插件持有并映射到设备条目；断开走 `close(handle)`。denied/unreachable 错误串解析为既有三态文案（复用 usePeerDevices 现有分支）。
- 双写期：`peer:devices` 订阅保留但降级为对账源（差异仅记日志），前端列表完全由自建缓存驱动。

**步骤 2：共享目录注册表真源迁插件（替换 list/add/remove-shared-directories）**

- 注册表落插件数据层（桌面 plugin-database 表 `shared_roots(id, name, path_or_uri, created_at)`；移动 storage JSON 数组）；内置条目 `local-downloads` 不进注册表（引擎自行注入，插件 UI 只读展示）。
- 添加目录：桌面走 `platform_pick_folder()` + 用户可改展示名；移动走 `platform_pick_folder()`（SAF 树 URI）+ SAF 展示名。同根去重校验在插件侧做（引擎 replace_all 也会拒绝，双层防御）。
- 任何增删改后调 `set-shared-roots(dirs)` 全量推送（桌面条目 `{id,name,path}`、移动 `{id,name,safTreeUri}`）；推送失败如实报错并回滚本地变更（引擎拒绝 = 注册表无效，如路径已不存在）。
- 移除条目前检查：有条目正被浏览会话引用时给确认提示（浏览会话中断的 UX 兜底，引擎侧行为不变）。
- 双写期：旧 mount-local/update-roots 命令保留，内部改译 set-shared-roots。

**步骤 3：任务队列与历史自持（替换 list-transfers / list-receiving / history 族）**

- 数据模型：单表（桌面）/ 单键数组（移动）承载传输条目 `{ batchId, nodeId, peerName, direction, status, files[], totalBytes, transferredBytes, rateBps, detail, rejectReason, createdAtMs, updatedAtMs, retryMeta }`；`retryMeta` 仅发起方条目携带（send: `paths[]`；pull: `dirId + files[]`）。
- 驱动：订阅 `peer:transfer`（发送侧进度/终态全量快照事件）与 `peer:receive`（接收侧），按 batchId merge 进自有条目；引擎快照是进度真源，插件不做进度推算。
- 终态归档：status 进入 done/failed/rejected/cancelled 时写入持久层；封顶滚动淘汰（200 条，最旧先出）；清空历史 = 删除全部终态条目。
- 重启恢复：activate 载入持久层，其中 `running/pending` 态条目改标 `interrupted`（引擎重启后原批已死，如实呈现）；`interrupted` 与 failed 同样可重试。
- 发起：enqueue 改走 `send-files(sessionHandle, paths)`（句柄来自步骤 1 设备条目）；pull 改走 `pull-files(sessionHandle, dirId, files)`。返回 DTO 的 batchId 回填自有条目完成关联。
- 取消统一走 `close(batchId)`（传输句柄语义），替换 cancel-transfer / cancel-receiving 两个旧命令。

**步骤 4：接收策略编排（get-receive-settings 读接口退役；set-receive-policy / set-download-dir 保留为配置推送通道，A1）**

- 策略键（policyMode / timeoutSecs / downloadDir / encryptionEnabled）落插件 storage（沿用现有 SettingsPanel wire 形状，key 不变，迁移零成本）。
- auto_accept / auto_deny：on_message 收到 `peer:receive` 待应答批事件后立即 `respond-transfer(batchId, true/false)`，UI 无弹窗；ask：现有 ConsentDialog/BatchRequestDialog 编排不动，倒计时到点插件主动 respond-transfer(false)。
- 引擎闸门 fail-safe 不变：DEFAULT_ASK_TIMEOUT_SECS 兜底仍在宿主（插件进程被杀/卡死时引擎照拒），插件倒计时只是提前于它的 UX 层。
- **双写**→ **配置推送（A1 裁决后的常态架构，非过渡态）**：策略键的产品真源在插件 storage；变更后经保留的引擎配置原语 `set-receive-policy` / `set-download-dir` 推送给宿主闸门/落盘（ADR 0022 v3 修订：二者是引擎安全闸门与落盘配置，非业务编排）；encryptionEnabled 同时作为 send 批参数携带（新通路）。Phase 3 期间旧 get-receive-settings 读接口仍可用作对账源，Phase 4 退役。

**步骤 5：加密与落点参数化（send/pull 载荷演化，零 ABI 变更）**

- send：paths-json 数组元素允许携带可选对象形态 `{ path, encrypt?: bool }`（纯 string 元素兼容保留，默认取插件设置值）——载荷是 JSON 字符串，形状演化不动 WIT 签名。
- pull：files-json 条目允许可选 `destRelPath`（落点子路径）；整体落点目录由引擎接收目录承载（A1 裁决：落点覆盖经保留的 set-download-dir 配置原语，无需 destDir 顶层字段）。
- 引擎侧解析兼容两种形态（serde untagged / 手工 fallback），单测覆盖混合数组。

**步骤 6：翻译层收敛**

- 两端 peer.rs 中仅服务旧命令面的 DTO 搬运函数删除（transfer_to_task / terminal_history / list_* 族等约 60% 体量）；新代码直接以 wire 形状构造原语入参、原样透传 bus 事件载荷。
- devMock 重造为 wire 形状种子（设备缓存含 TTL 过期样本、任务队列含 interrupted/failed/retryable 样本、共享根、待应答批）；dev-shell commands.execute 对新增内部命令名返回 mock（AGENTS.md：mock 数据归插件工程，dev-shell 只做接线）。
- i18n：新增文案（interrupted 态、last-seen 标注、重试失败原因）zh-CN/en 同步；composable 无中文硬编码。
- manifest：permissions 补 `mdns`（browse 原语门禁，Phase 2 已定义）；两端 plugin.json commands 清单按步骤 1–4 的新内部命令同步。

### Phase 4 步骤（退役收口，单版本双端同发）

**步骤 7：WIT 旧函数删除**

- 删：`list-devices` / `dial-peer`(node-id 版) / `disconnect-peer` / `cancel-transfer` / `retry-transfer` / `clear-transfer-history` / `list-receiving` / `cancel-receiving` / `clear-receiving-history` / `get-receive-settings` / `list-shared-directories` / `remove-shared-directory` / `add-shared-directory` / `pick-files` / `pick-folder` / `set-transfer-encryption`（A1 裁决：set-receive-policy 与 set-download-dir **保留**为引擎配置原语，不在删除清单）。
- 改：`dial-peer-endpoint` 更名 `dial-peer`（endpoint 语义转正）；四个双态寻址函数删 node-id 直呼分支（WIT 注释同步收紧）；`send-files` 返回值从 PeerTransferDto JSON 收窄为传输句柄字符串。
- 读对端函数句柄路由表升级：session 表条目从 `handle → node_id` 扩为 `handle → {node_id, addr}`（endpoint 在拨号时记忆），数据面按句柄寻址时优先复用活连接、连接已断则以记忆 addr 自动重拨（信任检查照走引擎握手）——这是退役 DiscoveryCache 的前置条件：引擎数据面不再查发现缓存。
- ABI 再 bump（desktop v10 / mobile v8）；双端 WIT 副本同步；SDK trait 收窄（HostPeer 删对应方法）。

**步骤 8：宿主侧链路退役**

- `peer:devices` topic 与 emit_json 桥接中的 devices 分支删除（bus_topic_for 映射收敛）；DiscoveryCache 守护（spawn_peer_mdns_daemon 内的 browse+快照比对推送）退役，**mDNS 自我广播保留**（register/advertise 与 TLS listener 同生命周期）；DiscoveryCache 本体保留为引擎内部结构者降级删除，依步骤 7 完成度决定（数据面不再读取后即可删）。
- 引擎侧 Tauri 命令面去留逐个评估：`start/stop_peer_node`（生命周期，保留）；`list_discovered_peers` / `dial_peer` / `disconnect_peer` / 共享目录 CRUD / transfer/receiving 查询管理族——主前端已无调用方（UI 已迁插件），除诊断需要外随 WIT 删除一并清理；`respond_peer_consent` / trust 设置族保留（首连弹窗若仍有宿主级兜底路径）。
- 引擎历史持久化（ensure_history_loaded 一族）：对外查询退役后转为纯内部簿记；按 B 裁决**停写并标记废弃**（保留一版读取以兼容回滚）；插件历史成为唯一产品历史。
- host_impl/peer.rs 收缩至终态函数 + handle 表；host_impl/mdns.rs、platform.rs 不动。

**步骤 9：数据迁移与兼容**

- 旧 storage 键兼容检查：引擎 settings 表中的接收策略/落点/加密键首次启动时导出到插件 storage（一次性迁移，版本戳防重复）；shared_dirs.json 内容反向导入插件注册表后置为引擎只读种子（引擎仍需它 serve 浏览请求，直至插件首推 set-shared-roots 覆盖）。
- 插件旧版历史不迁移（旧历史在引擎侧，本就非插件数据）；可信对端在宿主信任存储，不受影响。

### Testing Decisions

沿用父 spec 三条既定接缝，不新增第四条：

1. **双节点对打 harness**（packages/peer-net/tests/）：扩展四组引擎级行为——endpoint 记忆重拨（断连后凭 handle 记忆 addr 恢复）、close 三路路由（session/发送批/待应答接收批）、set-shared-roots 幂等替换后对端浏览可见性、resume 断点续传（既有，回归即可）。mDNS 多播不进断言，浏览器句柄测试入口 = 注入 ServiceInfo 事件通路（与 harness 发现注入同法）。
2. **无头纯函数单测**：设备缓存状态机（upsert/TTL 惰性清扫/TXT 解读/cap 位派生）、批元数据回放（retryMeta 构造与校验）、终态归档封顶淘汰、send/pull 载荷新旧双形态解析——全部独立直测，不依赖 tauri/wasmtime。
3. **插件 vitest**：usePeerDevices 改造后的自建缓存编排（found/lost/过期三路事件驱动）、接收策略三分支自动应答、重启恢复 interrupted 标注、设置双写一致性；统一 test:run 执行。

真机清单：父 spec issue 05–07 验收项 + 历史/设置迁移数据兼容 + 「插件停用后本机仍可被发现」+ 双端交叉（桌面↔桌面、桌面↔手机、手机↔手机各一轮全链：发现→连接→互发→断点续传→历史/设置核对）。

## Out of Scope

- 多目标扇出发送的 UI 编排（服务层天然支持，父 spec 故事 13 维持后续票）
- 移动端 host-plugin-database 补齐（ABI bump，另立票）
- 可信对端免询问白名单、传输限速、断点续传的暂停/恢复粒度细化
- 第二个对等网络消费者插件的落地（本 spec 只保证它「能进入」）
- 引擎传输协议 / TLS / 信任语义的任何改动

## Resolved Decisions（2026-08-26 用户裁决）

**A. 接收闸门配置通道 → 取 A1（推荐案）**

ADR 0022 v2 存在内部张力：Consequences 段说「插件的策略设置只是预配置该闸门的参数」（暗示存在配置通道），退役表却把 set-receive-policy / set-download-dir 列入下沉。裁决承认二者是「引擎安全闸门/落盘配置」而非业务编排，符合 ADR 自己的裁剪线，保留为终态原语：

- host-peer 最终原语面 = **13 个**（原 11 + set-receive-policy + set-download-dir）；get-receive-settings 仍下沉（读接口，插件自持）。
- 已在 ADR 0022 追加 v3 修订记录；issue 13 的「11 个」表述同步加注。
- 对用户的可见收益：推送接收的落点自定义完整保留（故事 21 不半失效）、auto_accept 在插件未激活窗口由闸门预配置接管。

**B. 引擎历史持久化 → 停写 + 保留一版读取**

Phase 4 步骤 8 执行：引擎侧历史文件停写并标废弃，保留一版只读兼容回滚；插件历史经 Phase 3 验收后即唯一产品历史。

## Further Notes

- 契约红线：ADR 0022 裁剪线（宿主能力不得携带业务语义）与 spec 决策 11（peer-net 是宿主核心服务）不再重复论证；本 spec 与二者冲突处以 ADR 为准并回头修本 spec。
- Phase 3 各步骤完成后跑 `npm run test:run` + `cargo test --lib`（双端）+ vue-tsc；涉 Kotlin 改动才跑 gradlew（本 spec 预期不涉）。
- 前端改动动手前加载 frontend-styles skill；截图评审流程仅在触及视觉布局时启用（本 spec 以逻辑重构为主，预期不触发）。
- 构建产物：插件源码变更后按统一脚本重建并同步打包资源（两端）。
- Phase 4 发布要求双端同版（ABI bump 跨端协同）；发布线操作遵守 Git Rules（只推 master/uat）。
