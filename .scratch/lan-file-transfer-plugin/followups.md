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
