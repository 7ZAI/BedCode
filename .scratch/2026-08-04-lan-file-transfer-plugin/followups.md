# 内网文件传输插件 — 后续工作清单

> 实施会话（2026-08-05）完成 spec 步骤 1–5 与两端 UI 后的遗留项。已记录至 auto-memory（`lan-file-transfer-implementation`），此处为入库副本。

## 待办（按优先级）

### P1 — 上传方向收口
- [x] **桌面上传 UI（发送到手机）**（2026-08-07 完成）：宿主新增 `plugin_pick_files` 多文件选择命令（`plugin_pick_directory` 同构，fileservice 权限）→ SDK `FileServiceAPI.pickFiles()` + 前端 `pluginPickFiles` 封装 + 权限列表双端同步（permission.ts / SDK permission.rs）→ 顶栏「发送到手机…」按钮 → `enqueueUpload`（`direction=upload` + `localPath`，remotePath 取文件名，对端挂载根落位）。桌面端上传入队链路已闭环。
- [ ] **上传方向 E2E 验证**：上传钩子已修复（`host.fs_exists` 同名即拒）、session 流已实现，需真机双端实测（移动→桌面 10GB 级 + 桌面→移动）。
- [ ] **移动端存储访问 SAF 化**（方案见 `issues/08-移动端SAF存储访问改造方案.md`，2026-08-11 grilling 定稿）：不依赖 All Files（注定不可得）。v1 = 共享目录改存 SAF URI + Kotlin `SafTransferPlugin`（`listTreeChildren` 遍历 + `safToCache` 中转复制）+ 上传页共享目录文件列表 + MediaStore.Downloads 默认接收落点（私有回退）+ file_service 三端点 SAF 化（cache 中转）；Rust 引擎零改动。M1 上传 → M2 接收+共享 → M3 可选（上传 SAF 流直传 / 「保存到…」）。术语见 CONTEXT.md「文件传输」，架构决策见 docs/adr/0009。

### P2 — 宿主/SDK 缺口
- [x] **移动端 SDK 补 `fs_delete` 导出**（2026-08-07 完成）：新增 Kotlin `FileDeletePlugin`（gen/android + android-backup 双备份，已入 AGENTS.md 恢复清单）→ `android_plugins.rs` 注册/`delete_file()` 桥 → wasm_runtime 注册 `host_fs_delete`（fs:write 权限 + fs_auth Write 校验，非 Android 平台 std::fs 兜底）→ SDK `HostFs::fs_delete`（abi.rs 常量 + 签名表 + wasm_host 实现）→ 插件 `delete_part_file` 恢复真实删除。移动端取消下载现在会清理本地 `.part`。
- [x] **WASM 插件 trap 自动恢复（桌面端）**（2026-08-09 完成）：wasmtime 同步引擎下任何 trap 都会 `set_trapped()` 永久污染 Store，后续所有调用持续报 `cannot enter component instance`（消息总线只记录错误、插件永久失效）。host.rs 新增 `with_wasm_plugin_call` + `schedule_plugin_reload_after_trap`：所有 WASM 入口（on_message / invoke_command / on_upload_request / on_session_lifecycle / on_input_submitted）调用失败时释放实例锁 → 后台 deactivate→重新实例化→activate（复用 reload_wasm_plugin），30s 限频防重载风暴，失败置 Error 态；插件被停用时不擅自重载。同时 file-transfer 插件（两端）`state().lock().unwrap()` 改 poison 容忍，切断「一个 panic → 锁中毒 → 后续全 panic」连锁。配套测试 `test_component_trap_poisons_store_and_reinstantiate_recovers` 验证污染语义与重建恢复。
- [x] **http_fetch 响应体上限（两端，防 fuel trap）**（2026-08-09 完成）：http_fetch 等待阶段 guest 零燃料（fuel 只计 guest 指令），但响应体回传后 canonical ABI 拷入 guest 内存 + guest serde 解析会消耗单次调用 fuel 预算，无上限响应体可耗尽 fuel 触发 trap。非流式 http_fetch 响应体上限 32MB（`PLUGIN_HTTP_RESPONSE_BODY_LIMIT_BYTES`，桌面 constants / 移动 wasm_host 本地常量），流式读取超限即中止并报错引导 `stream:true`；大载荷（如 ai-chatbox）已走流式模式不受影响。配套测试两端各 2 个（小响应正常 / 超限拒绝）。
- [ ] **锁跨阻塞网络 IO 重构**：`schedule_and_start` 持全局 `Mutex` 时执行 `http_fetch` 握手（http_fetch timeout 120s/connect 10s），对端半连接时进度事件可停滞最长 120s、拖住移动端单 delivery worker。非硬死锁，真机若体验差再重构（prepare→execute→commit 三段式）。
- [ ] **fs_auth 白名单收敛**：`com.bedcode.file-transfer` 进白名单 = 任意路径放行。当前信任模型（内置首方 + 配对白名单）下接受；若未来开放 zip 安装同名插件需收敛为按 roots/download_dir 授权。

### P3 — 打磨
- [ ] **Unix rename TOCTOU**：下载 `try_exists→rename` 间目标被创建时 Android/Linux 会静默覆盖（`duplicate-name` 语义破坏）。可接受（窗口极小），如需彻底修复用 `renameat2(RENAME_NOREPLACE)`。
- [x] **KeepAlive 下全局弹窗悬浮到其他页面（修复 + 跨页入口，2026-09-10 完成）**：宿主 `DesktopLayout` 对路由视图包 `KeepAlive :max="8"`，切走后组件不卸载仅 deactivated；Vue 3.5 KeepAlive 去激活只把根元素移入离文档 storageContainer，**Teleport 到 body 的 fixed overlay 不受影响仍悬浮**——批量请求/首连确认弹窗（`ft-dialog-overlay` fixed inset-0 z-50）在用户离开文件传输面板后盖在任何当前页面上（样式错位观感 + 覆盖其他页面）。修复：新增 `usePanelActive`（**基于 onActivated/onDeactivated 生命周期**，dev-shell 无 KeepAlive 时钩子不触发恒 true，弹窗照常工作；不用路由路径比对正是为了 dev-shell 兼容）→ `BatchRequestDialog`/`ConsentDialog` 的 overlay `v-if` 加 `isPanelActive` 门控（state/倒计时保留，切回面板恢复剩余秒数）；待确认批跨页改状态栏项（`file-transfer.batch`，count + 点击跳转 PANEL_ROUTE，与 consent 状态栏项同模式；新 i18n `transfer.request.statusItem` 双端）；`maybeNotifyPendingBatch` 条件扩为「窗口不可见 **或** 面板未激活」发系统通知（面板激活 + 前台只靠弹窗不发）；接收中 toast 保持全页非阻塞横幅（spec 14.4 桌面 = 应用内横幅）。附带修复：i18n `messages.ts` 补齐 `recentSeen`/`interrupted` 两个 schema 漂移 key、`usePeerDevices` capabilitiesHex==null 防御（pre-existing TS 错误 + parseInt(undefined) 得 NaN 隐患）、死常量 `PER_FILE_TOAST_WINDOW_MS` 接入 pushToast。测试：`usePanelActive.test.ts`（KeepAlive 切走/切回 + dev-shell 恒激活）、`useReceiving.test.ts` +3（面板未激活发通知 / 面板激活不发 / 默认旧行为 + 同批去重）。全量 vitest 583 绿、vue-tsc（宿主）净、eslint 0 error。
  - **方案演进（同日再修，弃用门控方案）**：用户反馈「连接请求全局弹窗依然有问题」+ 截图复核——问题根源升级为「该出现时没出现 + 状态栏项图标被宿主当文本渲染」。**终态方案 = 宿主级通用全局弹窗组件**（用户指定方向「实现一个全局弹窗组件给插件用」）：新增 SDK `ui.showDialog(PluginDialogOptions)`（`packages/plugin-sdk-desktop/src/{global-dialog.ts, ui/PluginGlobalDialog.vue}`，宿主 + dev-shell 共用渲染组件，exports 子路径 `./ui/plugin-global-dialog`，权限 `ui:dialog` 三处同步：host permission.ts / SDK rust permission.rs / plugin.json）。能力：**预设模式**（title/message/icon/actions 按钮组 + primary/danger/ghost 变体 + navigateTo 跳转 + disabled 联动 + countdownLabel 倒计时）与**组件模式**（content 任意 Vue 组件 + provide('pluginContext') + props 热更新）双模式；**定时关闭可选**（timeoutSec / deadlineAt 二者择一，未提供即常驻）；FIFO 排队 + update()/close() 句柄。迁移：consent → 组件模式（index.ts 按 currentRequest 开关宿主弹窗，删除状态栏项机制与 PANEL_ROUTE）；batch → 预设模式（新 `useBatchPrompt` 状态机：先到先弹/已提示去重/TTL 超时由宿主执行，FileTransferView 经 deadlineAt=createdAt+timeout 保证迟到打开倒计时仍准确）；TrustedPeersSection 撤销确认框同样迁预设模式（顺带消灭第三个同类 Teleport 弹窗）。删除：`usePanelActive.ts` / `constants.ts` / `BatchRequestDialog.vue` / `transfer.request.statusItem`、`transfer.consent.statusItem`、`transfer.batch.*` 死 i18n key；styles.css 清理 ft-dialog-overlay/card/body 死类。验证：全量 vitest **604 绿**（SDK 19 + useBatchPrompt 5 新增）、插件/宿主 vue-tsc 净、eslint 0 error、lens 无 blocker。遗留：移动端同类问题仍未动（无报告）；「文件传输面板未访问过则收不到批请求」仍是既有缺口（useReceiving 视图作用域，未随本次升级）。
  - **遗留（未动）**：① 移动端插件同样的 Teleport 弹窗 + ToolboxView 层 KeepAlive（toolboxKeepAlive）存在同类风险，无报告未改，可复用 `usePanelActive` 思路；② `FileTransferView.vue` 模板引用且未定义的 `notice`（storageAccess 提示块死引用，v-if 恒 false，pre-existing）；③ `useReceiving` 去重记账与 `start()` 初始 refresh 空快照存在竞态（极小概率重复通知，pre-existing，测试已先收敛 refresh 再触发事件保证确定性）。

- [x] **错误透传**（2026-08-07 部分完成）：`context.commands.execute` 包装行为保持既有约定（未激活/WASM trap → "Command not found"，与桌面一致，不改）。**retry 修复**：duplicate-name 拒绝后重试必然再失败的问题已解决 —— 下载方向 retry 时先删除本地目标文件 + 残留 `.part`（两端同构）；上传方向远端不可删（spec 禁止），重试前需用户在对端处理（代码注释已说明）。
- [x] **移动端 i18n 编译期 key 同步**（2026-08-07 完成）：新增 `messages.ts` `MessageSchema` 接口（82 key），zh-CN/en 标注类型、index.ts 收敛为 `Record<string, MessageSchema>`，与桌面端强制机制对齐。

## 待验收（spec §12 性能基准，需真机双端）
1. iperf3 `-P 4 -t 30` 双向标定链路上限 T，大文件顺序读写测磁盘 D
2. 单大文件（10GB 级）吞吐 ≥ 80% × min(T, D)
3. 多文件并发（默认 3）聚合 ≥ 75% × min(T, D)
4. 断点续传正确性：30%/60%/90% 强制中断（关 App/断 WiFi/锁屏），恢复后哈希一致
5. 并发抢占与恢复：传输中改并发数、对端插件停用再启用，无僵尸任务
6. 内存稳定性：全程宿主+插件内存增长 ≤ 100MB

不达标时启用文件内分片并发升级路径（HTTP/1.1 内）。

---

# v2.1 服务器归零（全手机发起传输）— 遗留项（2026-08-20，主 agent review 后）

> spec-zero-transfer.md + v2.1-zero-transfer-implementation-plan.md（施工图）已实现阶段①-④ + list 迁移，双轴 code-review（Standards/Spec）已逐条修复。以下为修复后剩余项。

## 待办（按优先级）

### P1 — 代码重构（v2.1 完成后遗留，不阻塞联调）
- [x] **desktop 插件 handshake.rs 直连死代码清理**（2026-08-20 完成）：整删 `handshake.rs`（445 行，list_remote/fingerprint/request_transfer/create_session/query_session/complete_session/cancel_session 及私有 helper）；删除 `commands.rs` 3 处旧兼容调用（cancel 两条 cancel_session + transfer_progress 的 complete_session，v2.1 guard 下本就不可达/必然失败）；`PeerStore` 瘦身（删 `base_and_auth`/`base_and_auth_for`/`base_url`/`has_file_transfer_mount`/`file_transfer_operations`/`is_peer_desktop`，`new()` 去参），`start_single_task`/`resume` 改用 `endpoint()` 保留 fail-fast。验证：插件 cargo check（host+wasm32）+ 11 测试全绿。
  **遗留观察**：Task.`upload_session_id` 现只写不读（曾仅被 handshake cancel/complete 消费），字段保留但断点续传不再依赖它；pull 取消时桌面本地接收 session 现由宿主 TTL 兜底（旧直连 cancel 在服务器归零后本就必然失败）。
- [x] **wire 决策/状态魔法串枚举化**（2026-08-20 完成）：新增 `TaskReason` 枚举（两端，`#[serde(from/into = "String")]` 保持 wire JSON 逐字节不变；已知值编译期拼写检查，动态透传宿主错误/对端任意字符串经 `Other(String)` 兜底保留原文）+ `TransferDecision` 枚举（accepted/approved/rejected 双值映射，intent ACK 与 transfer approval 共用）+ `IntentDirection` 枚举（pull/push）+ `Direction::as_str/from_str`。改造两端 commands.rs 全部 reason 赋值/比较点（about 30 处）、intent ack/approval/resolved 的 decision 判断、send_intent 参数。历史条目/接收任务 reason 字段类型同步。**遗留观察**：mobile `HistoryEntry.direction/state` 仍为 String（desktop 已是枚举）——双写发散属 P1 下一条护栏项；wire JSON 层（SyncEvent/SyncPayload DTO）保持 String 为两端契约正确形态，未改。验证：两端 cargo test（desktop 14 / mobile 21）+ wasm32 check + 两端前端 vitest（415/203）全绿。
- [ ] **default_true / FileTransferIntent / ListEntryDto 双写发散护栏**：两端独立 crate 无法共享定义，属架构必然；若表单字段再次扩展，review 时重点 diff 双端逐字一致性（本已双写 wire 测试）。

### P2 — 真机联调验收（需真机双端，spec §8.3 ↔ §12 性能基准）
- [ ] **阶段① 手机自主下载/上传**：10GB 级吞吐 ≥ 80% × min(T, D)；30%/60%/90% 中断续传哈希一致（upload 续传已修 Network 分支 session 保留）。
- [ ] **阶段②③ 审批四场景**：手机自主上传→桌面临时批卡接受/拒绝/超时；桌面 push→手机 ask 确认（accept/reject 策略自动应答走 `filesrv:intent_received` 订阅）/拒绝；pull 免审批信息性通知 + 桌面调批上下文自批准不 403。
- [ ] **阶段③ 协调者**：传输中手机退后台 30s → 桌面卡「对端离线」；重连后续传不重复字节（download 断点 = 手机本地游标 HEAD size+mtime 双因子；upload 断点 = 桌面 session received）。进度不超 100%（upload 断点历史仅补报一次 + append 内部累计）。
- [ ] **阶段④ 回归 + APK 对比**：删除 server.rs/actix 后两端 v2 全场景（四 tab/历史/通知 action/duplicate-name/peers）无回归；APK 体积前后对比记录到 map.md。
- [ ] **list 浏览闭环**：桌面浏览手机共享目录（`filesrv_list_remote` WS 往返 5s 超时）在真机多设备场景验证；手机 announce（port=0/纯挂载公告）后桌面 UI 正常显示共享目录。

### P3 — 已知限制 / 边界
- 传输层加密 MVP 明文直通（`PassthroughCipher`），密钥协商未实现——未来 AES-GCM 两端同源、方向无关（spec「传输层加密」Out of Scope）。
- AP 客户端隔离（同网设备不可达）场景 v2.1 整体失效，需回退全 WS 数据面（ADR 0021 已知边界）。
- 评审 13 的「目录浏览 list」已由本会话补 `FileListRequest/Response` wire 迁移（施工图原未安排，用户拍板）。

---

## 已完成（2026-09-10）：传输面板终态自动归档历史（双端同构）

用户反馈：桌面传输面板「正在发送/正在接收」tab 里已完成/失败的任务应自动移到「历史」tab。

- **根因**：引擎 `flush()` 把**全部** send 条目（含终态）当 tasks 派发、receive 非 pending 条目（含终态）当 receiving 派发，终态同时出现在活动 tab 与历史 tab，违反 transfer_store 文档「终态归档」意图。
- **改动（双端 peer.rs / transfer_store.rs 同构）**：新增纯函数 `active_send_entries`（send && running/pending）与 `active_receive_entries`（receive && running），`flush()` / `list-tasks` / `list-receiving` 统一改用；终态只出现在 `history` 视图（history-changed / list-history 口径不变）。+1 测试 `active_views_exclude_terminal_entries`（两端各 29 绿）。
- **前端配套**：HistoryEntry 增 `retryable`（wire `retryMeta != null`，仅发起方条目）——重试入口从任务卡迁到历史 tab（桌面 TaskPanel 历史项加重试 mini-btn；移动 TransfersTab 历史卡 actions 加 retry）；两端 devMock/dev-shell mock 同步视图口径（终态种子归 history，含 retryMeta）。
- **移动端连带修复**：`checkSettledNotification`（队列全终态通知）原依赖终态条目滞留 tasks——改为历史 diff 驱动：历史新增 send 终态累计计数，tasks 清空时结算通知（初始/重载播种只记 batchId 不累计，防旧条目误报；全取消不打扰语义保留）。同步重写移动端 `useTasks.test.ts` 两个 settlement 测试到新数据契约（终态走 history-changed）。**坑**：wire 条目无 `id` 字段只有 `batchId`——累计去重集合若存 `e.id` 会让所有条目都撞同一个 undefined，第二次结算永远被跳过（debug 实锤：`seenHistoryIds=[null]`）。
- **验证**：两端插件 cargo test 29 绿；桌面 `pnpm run test:run` 全绿（61 files/569 tests）；移动 `pnpm run test:run` 全绿（42 files/360 tests，含重写的 settlement 用例）；插件前端 vue-tsc / eslint 干净。

## 已完成（2026-09-10）：下载进度条恒 0 + 取消原因未国际化（双端同构）

用户反馈：① 桌面下载时任务进度条一直是 0 但数据正常传输；② 移动端被取消的任务显示英文 cancelled by xxx。

- **进度条恒 0 根因（双端 peer_receive.rs）**：远端拉取任务预登记时总大小未知（pull spec 的 `size` 恒 0 → `total_bytes = 0`），而引擎 Progress 事件携带权威 `total`，`update_progress` 却丢弃该字段只写 transferred/rate——`totalBytes` 恒 0，前端进度恒 0%。修复：Progress 解构取 `total` 并入账（`total > 0` 时覆盖 total_bytes）；`settle_terminal` Completed 结算把 transferred 归整为满额（末条 Progress 可能略低于总量）。
- **取消原因未国际化根因（双端 peer_receive.rs / peer_transfer.rs）**：`settle_terminal` / `apply_terminal` 的 Cancelled 分支把人类文案（`cancelled by sender` / `cancelled by receiver` / `cancelled by self`）直接落 wire `detail`。修复：改发机器码 `cancelled-by-sender` / `cancelled-by-receiver` / `cancelled-by-self`（旧文本 wire 前端兼容映射）。移动端 TransfersTab 接收卡/历史卡原因走 i18n 映射（新增 `transfer.task.reason.cancelledBySender/Receiver/Self` zh+en+messages.ts schema）；顺带修复 TransfersTab 两个预存 vue-tsc 错误（FILTERS computed 泛型注解、历史 TaskCard 缺 `:reason` prop）——移动端插件 vue-tsc 从 2 错归零。
- **验证**：桌面宿主 cargo 596 绿；移动宿主 cargo 251 绿；移动插件 vue-tsc 0 错；移动插件 build 成功；eslint 0 error。

## 已完成（2026-09-10）：移动端「打开所在文件夹」误报本机没有对应文件

用户反馈：移动端下载完成后打开所在文件夹提示「本机没有对应文件」，明明刚下载。

- **根因（设备日志实证）**：`DownloadsDirPlugin.openFileLocationByName` MediaStore 按名命中成功，但打开 `content://com.android.externalstorage.documents/document/primary:Download` 目录 URI 时抛 `SecurityException`（UID 10462 does not have permission…you could obtain access using ACTION_OPEN_DOCUMENT）——用户未授权「所有文件访问权限」（MANAGE_EXTERNAL_STORAGE），ExternalStorageProvider 目录 URI 对应用无权限；异常被前端 catch 统一渲染为「本机没有对应文件」（文件其实在公共 Download 目录）。
- **修复（DownloadsDirPlugin.kt）**：MediaStore 命中后先试打开目录，`SecurityException` 时降级打开文件本身（MediaStore 行为应用自有插入，content URI 免权限可访问）——用户至少能查看下载结果；真不存在才 reject「本机没有对应文件」语义恢复准确。
- **验证**：`./gradlew :app:compileUniversalDebugKotlin` BUILD SUCCESSFUL（离线加 --offline）。

## 已完成（2026-09-10）：双端记账 + 桌面接收方向图标修正

用户反馈：① 一方发送另一方接收时，只有主动发起的一方在传输列表显示任务，另一侧应显示（双端记账）；② 桌面端接收（下载）卡片的方向箭头显示为↑（应是↓，上=发送/下=接收）；③ 语义确认：绿=完成、红=失败。

- **双端记账（peer-net crate + 双端宿主）**：拉取发起方（对端）本就有自己的 receive 任务；缺口在**供流方**——serve_pull 的 Progress/Terminal 此前流入接收侧通道且 batch_id 不一致（Progress 用 `pull-{nanos}`、外层 Terminal 用 dir_id），宿主查无此批全部丢弃。改动：
  - peer-net：`TransferEvent` 新增 `PullServed { remote, batch_id, files, total_size }`；`SharedDirHandler` 增加独立 `serve_events` 通道（`new()` 第 5 参，push 接收与 serve 供流分流）；serve_pull 解析成功后发 PullServed、进度/终态统一经 serve 通道且 batch_id 同源（outcome 包裹错误路径，PullServed 之后必然一次 Terminal）；外层 dispatch 的 dir_id 伪批 Terminal 跳过（serve_handled 标志）。
  - 双端宿主：`peer_net.rs` 建 serve 通道并 spawn `peer_transfer::drive_serve_events`；`peer_transfer.rs` 新增消费循环——PullServed → `register_serve_task`（解析对端名，插 direction=send 任务，peer_name 经 discovery cache），Progress → `update_progress`（150ms 节流 publish），Terminal → `settle_serve_terminal`（状态映射同 apply_terminal，取消码 -receiver/-self，completed 归整满额）；两端 `drive_receive_events`/`drive_send_session` 补 PullServed 防御性忽略分支；`lib.rs` 根导出补 `FileMeta`；peer-net 集成测试 `shared_dirs.rs` 2 处 `SharedDirHandler::new` 调用补 serve 通道。
  - 效果：拉取时供流方「正在发送」出现方向=send 任务（含文件名/大小/进度/终态），发起方「正在接收」不变——两端各自记账。
- **图标修正（桌面 TaskPanel）**：接收卡（正在接收 tab + 全部 tab 两处）此前恒用 `ft-task-dir--up` + 上箭头 path；改为下箭头 path（`M12 5v14M19 12l-7 7-7-7`）+ 去掉 --up（绿底，与任务卡 download 同款）。移动端 TaskCard 本就正确（upload↑/download↓）。chips 现状已符合绿=完成/红=失败。
- **验证**：peer-net cargo 全绿（75+1+9+5+12）；桌面宿主 cargo 596 绿、移动宿主 251 绿；桌面 vitest 61/569 绿；桌面插件 build + eslint 干净。
