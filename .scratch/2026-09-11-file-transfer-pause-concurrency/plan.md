# 双端文件传输：并发限制 + 显式暂停续传 实现计划（2026-09-11）

## 现状差距（dev 分支审计）

| 需求 | 现状 |
| --- | --- |
| ① 批量同时上传 | ✅ 已有（pick-files 多选 → enqueue，每批一条任务） |
| ② 限制同时传输数量 | ❌ 引擎 `send_files_to_peer_with_policy` 每批立即 `drive_send_session` 并发跑，**无并发槽**；`concurrency` 字段两端 types/i18n 存在但无 UI、引擎零消费（默认 1，实际无限并发） |
| ③ 显式暂停续传 | ❌ 插件命令 `pause`/`resume`/`resume-all` 是 unsupported 占位；`TaskStateName` 无 paused；任务卡无暂停/继续按钮 |
| ④ 传输速度显示 | ✅ 已有（任务卡速率/ETA + 总速率 summary；接收卡缺速率，本期顺手补） |

## 架构决策

**并发槽 + 暂停/续传落点 = 双端宿主引擎 `peer_transfer.rs`**（desktop + mobile 同构）：

- 会话执行（dial/send_batch/进度/终态）、取消（CancelToken）、重试（retry 续传语义）、历史持久化**全部已在宿主**；插件只是快照消费者 + retryMeta 存储。并发调度放插件层需新增插件自持 pending 清单 + 重载恢复 + 事件竞态处理，且暂停仍必须调宿主 cancel——改动面更大、状态双真源风险高。
- 「限制同时传输数量」= 通用资源保护（transport.rs pending cap 已有同类先例），归宿主职责；插件设置仍为真源，经原语推送宿主闸门（与 receivingPolicy/encryption 同模式）。
- **断线自动 resumable**（spec 语义）本期保持现状：拨号失败 → failed 终态 → 用户 retry（retry 已按接收端偏移续传）。只做用户显式 pause/resume。
- resume 走「显式立即启动」：不占并发排队（用户显式操作优先）；新 enqueue 走并发闸门。
- paused 任务不占并发槽位；不落历史（仅 running/pending 之外非终态，跨重启由历史文件持久化保留，重启后显示 paused 不自动传）。

## 改动清单（每项双端同构）

> 状态：2026-09-11 全部实施完成，验证见文末。

### A. 宿主引擎 `src-tauri/src/peer_transfer.rs`（desktop + mobile）
1. `PeerTransferSettings`（peer_receive.rs）加 `concurrency: u8`（默认 3，1–8）；DTO 透出；set 通道更新（与 encryption_enabled 同机制）
2. 并发槽：
   - `PeerTransferState` 维护 running 计数（锁内扫描 status==running 即可，不引入新字段）
   - `send_files_to_peer_with_policy`：入队 `status="pending"` → 调 `pump_queue(app)`（running < concurrency 时取最旧 pending 启动）
   - 新 `start_pending_batch(app, batch_id)`：锁内校验槽位+状态 → 运行时快照 dial → `drive_send_session`（status→running）
   - `apply_terminal`/`settle_failed` 终态结算后调 `pump_queue`
   - `snapshot()`/`publish()` 包含 pending（前端排队态）
3. 暂停/续传命令（Tauri command + 注册 commands.rs）：
   - `pause_peer_transfer(batch_id)`：仅 running → `status="paused"` + `rate_bps=0`（**先改状态再 cancel token**，防终态事件先到）→ `CancelToken.cancel()`
   - `apply_terminal`：任务 status 已是 `paused` → 仅 `unregister_session` + 归还槽位（pump_queue），跳过终态结算/历史落盘
   - `resume_peer_transfer(batch_id)`：仅 paused → `status="running"`（**保留 transferred_bytes**）→ `register_session` 新 epoch → dial + drive_send_session（接收端按偏移续传）
   - `resume_all_peer_transfers()`：遍历全部 paused 逐个 resume（顺序 = 列表序）
4. `is_terminal_status`/`retryable` 不含 paused；历史文件读写保留 status=paused
5. 单测：pending 排队/槽位限制/终态推进/暂停中断/恢复续传（保留 offset）/resume-all

### B. SDK（desktop `packages/plugin-sdk-desktop/rust` + mobile `packages/plugin-sdk-mobile/rust`）
1. WIT `bedcode.wit` host-peer 加：
   - `pause-transfer: func(batch-id: string) -> result<_, string>`
   - `resume-transfer: func(batch-id: string) -> result<_, string>`
   - `resume-all-transfers: func() -> result<u32, string>`（返回恢复数）
   - `set-concurrency: func(n: u32) -> result<_, string>`（插件设置推送宿主闸门）
2. traits.rs `HostPeer` trait + wasm_host.rs 桥 + abi.rs 签名表 + permission.rs 权限登记
3. 宿主 host_impl（desktop `plugin/host/services.rs` 或同构位置 / mobile `wasm_runtime/host_impl/peer.rs`）桥接 Tauri command

### C. 插件 Rust（desktop + mobile `plugins/file-transfer/rust/src/`）
1. lib.rs：`file-transfer.pause`/`resume`/`resume-all` 命令接新原语（去掉 unsupported 占位）
2. peer.rs：`pause_task`/`resume_task`/`resume_all_tasks`（桥 + 错误透传）
3. transfer_store.rs：
   - `is_active` 语义：active_send_entries 含 paused（显示在发送队列）；paused 不算 running 槽
   - `mark_cancelled` 覆盖 paused（暂停后仍可取消）
   - merge_snapshot：引擎 paused 快照正常覆盖本地
4. settings_store.rs：concurrency 持久化 + set-settings 时经 `set-concurrency` 原语推送宿主

### D. 插件前端（desktop + mobile `plugins/file-transfer/src/`）
1. types.ts：`TaskStateName` + `'paused'`；Settings.concurrency 默认 3
2. useTasks.ts：mapWireTask 状态直传 paused；+ `pause(id)`/`resume(id)`/`resumeAll()` 方法
3. 桌面 TaskPanel.vue：transferring 任务卡加「暂停」按钮（initiator=me 的 send 任务）；paused 显示「继续」+「取消」；有 paused 时显示「全部继续」；STATE_KEYS/CHIP_CLASS + paused（琥珀色）；接收卡 meta 补速率
4. 移动端 TaskCard.vue + TransfersTab.vue：同构
5. SettingsPanel.vue（桌面）/ SettingsPage.vue（移动）：并发数 stepper（1–8）
6. i18n zh-CN/en：pause/resume/resumeAll/paused/并发数 hint 等 key（同步 messages.ts schema）
7. devMock.ts 同步

### E. 验证
- 双端宿主 `cargo test`；插件 wasm32 `cargo check` + `cargo test`
- 双端插件前端 vitest + vue-tsc；根目录 eslint
- lens_diagnostics 无 blocker（完成后杀 LSP）
- 真机联调留待用户（无设备时说明）

## 风险与注意
- WIT 改动：host 函数按名 import，加新函数不破坏旧插件（无 ABI bump）；双端 SDK 独立 crate，逐字一致
- 移动端 `settings_store`/`peer.rs` 与桌面有少量差异（local_path 字段、SAF），逐文件对照
- 并发槽与「接收侧」（pull/serve）无关：只约束 send 方向发起会话

---

## 实施记录（2026-09-11 全部完成）

### 关键决策调整（与计划差异）
1. **concurrency 推送走 send 载荷脉冲**（非新原语）：`send_payload` 每元素携带 `concurrency`，宿主 `peer_send_files` 解析首元素并调 `set_peer_transfer_concurrency`——零 WIT 改动，与 encryption 同机制。**新 WIT 原语仅 3 个**（pause/resume/resume-all）。
2. **resume 也走并发闸门**（paused → pending → pump），与 enqueue/retry 统一「所有进入 running 的路径都过闸门」；排队的 pending 不占槽。
3. **异步递归打断**：apply_terminal 内 pump 改 `pump_after_settle`（tokio::spawn）——pump→start→settle→pump 链会无限递归（编译器 E0733 强制 boxing），任务化后安全。
4. **引擎任务不跨重启**（persist_history 已停写）：paused 重启语义由插件 transfer_store `mark_interrupted_on_load` 覆盖（含 paused）。
5. sdk 前端 i18n 的 paused/queued/pause/resume/resumeAll key **早已预留**，复用未新增；`ft-chip--pause`、`.ft-step-btn/.ft-step-value` 样式也早已预留。仅新增 `concurrencyMinus/Plus` aria-label key。

### 验证结果
| 项 | 结果 |
| --- | --- |
| 桌面宿主 cargo test | 617 全绿（peer_receive +1 concurrency roundtrip；peer_transfer +5 泵送裁决） |
| 移动宿主 cargo test | 278 全绿（peer_transfer 同构移植 + 泵送测试） |
| 桌面插件 cargo test | 33 全绿（paused 语义 + 并发缺省） |
| 移动插件 cargo test | 33 全绿 |
| 桌面 vitest | 613 全绿（+2 useTasks 暂停/恢复测试） |
| 移动 vitest | 360 全绿（useSettings 断言更新默认并发 3；useTasks +暂停/恢复路由） |
| vue-tsc（双端插件） | 净 |
| eslint 根目录 | 0 error（124 warnings 既有） |
| 双端插件 vite build | 成功 |
| lens_diagnostics | 无 blocker（见下） |

### 遗留 / 未做
- 断线自动 resumable（spec 语义）：保持现状（拨号失败 → failed → retry 手动续传）
- 桌面 SettingsPanel 并发 stepper 已加；移动端 SettingsSection 并发 section 已加（复用 ft-step-btn）
- 真机联调（暂停/恢复续传正确性、并发抢占）留待用户设备验证，清单见 followups P2
- 移动端 devMock 增加 paused 演示条目（桌面同）
- `resources/plugins` 为构建产物：dev worktree 新 clone 后需先跑插件 build 再宿主编译（本会话已从 uat 复制旧产物 + 双端插件 rebuild）

---

## 2026-09-12 真机联调发现 + dev-run 脚本修补（后续跟进）

### 真机现象与根因（用户联调报告：无法暂停 / 无并发任务 / 关闭无反应）

| 现象 | 根因 |
| --- | --- |
| 移动端无法暂停（logcat 8 次 `Command not found: file-transfer.pause (Plugin error: unsupported: transfer lifecycle is plugin-store managed)`） | 设备上插件 **wasm 旧版**（pause/resume 仍是占位实现）；前端 index.js 已是新版（dev-run watch 08:04:53 复制）→ 新 UI 调旧命令。**wasm 从未被重建**：dev-run 只 watch 前端，`pnpm run plugins:build` 未跑过 |
| 桌面端无暂停选项 | 测试时桌面端跑旧产物；08:13 重启 `tauri:dev` 时桌面端 `ensurePluginWasm` 因检查路径 bug（见下）永远判定缺失 → 自动补建出**新版** wasm，桌面端就此恢复 |
| 一个完成才出现另一个（无并行任务） | `file-transfer.enqueue` 一次调用把多选 paths 合并为 **1 个 batch**（batch 内多文件顺序传）→ 只有 1 条任务卡。并发槽只约束 batch 之间——符合设计，非 bug |
| 点击关闭无反应 | 旧 wasm `cancel` 参数语义与新版前端 `{taskId}` 不匹配（新版 wasm 已对齐，`peer.rs cancel_task` 读 `taskId`） |

### dev-run 脚本修补（双端，本次落地）

1. **移动端 `scripts/dev-run.js` 新增 `ensurePluginWasm()`**（对齐桌面端思路，升级为新鲜度检查）：
   - PLUGIN_WATCH_CMDS 每项补 `id`（插件 id）+ `wasmFile`（rust 产物相对路径）
   - 检测 `resources/{id}/*.wasm`：缺失或 **Rust 源码（插件 rust/ + SDK rust/，排除 target/dist/node_modules）mtime 晚于产物** → 自动补建（SDK CLI `build --rust-only --resources-dir ../../src-tauri/resources/plugins/mobile`，cwd=插件目录）；fail-fast
   - 启动序列：预检后、watch 前调用
2. **桌面端 `scripts/dev-run.js` 修 bug + 升级**：
   - 原 `ensurePluginWasm` 用 `basename(dir)`（如 `file-transfer`）定位资源目录，但实际目录是插件 id（`com.bedcode.file-transfer`）→ **检查路径永远不存在 → 每次 tauri:dev 都无谓补建 wasm**（今天 08:13 出新版正是此 bug 的副作用）。改为按 `id` 定位 + 新鲜度检查，产物最新时不再重复构建

### 验证
- 双端 dev-run.js `node --check` 通过；eslint 0 error；CRLF 无回归（441/374 全 CRLF）
- 逻辑自测：真实产物全 FRESH（不触发补建）；红绿验证（临时目录）STALE/FRESH/target 跳过判定正确
- 移动端 file-transfer wasm 已重建（08:21:24）：`unsupported: transfer lifecycle is plugin-store managed` 计数 0，含 `pause-transfer`/`resume-transfer`/`resume-all-transfers` 原语

### 待办
- 重启 `tauri:android:dev`（重新打包 APK 安装）让设备加载新 wasm，再验证暂停/恢复/并发
- 产品决策未定：若期望「一次多选 → 多条并行任务」，需把 `enqueue` 改为逐文件 batch（当前每批一条任务为设计）
- 桌面端 `ensurePluginWasm` 原路径 bug 已修；`basename` import 若不再需要可后续清理（仍被 wasmDest 用于取文件名）

### 2026-09-12 追加：任务后自动切传输页 + 弹窗主题色（用户联调反馈）

| 需求 | 改动 |
| --- | --- |
| 移动端下载后自动跳传输 tab | `FileTransferView.vue downloadSelected()` 成功分支补 `tab.value = 'transfers'`（上传 uploadFile 已有同行为） |
| 桌面端批准接收（创建任务）后弹传输队列 | `FileTransferView.vue handleBatchApprove()` 改为 approve 成功后 `queueVisible.value = true`（上传/下载 handleUpload/handleDownload 已有） |
| 移动端弹窗按钮跟随主题色（不显示橙色） | `PluginDialogHost.vue` confirm 按钮：`variant==='warning'` 时原 `bg-[var(--mobile-warning)]`（橙 #f59e0b）改为 `bg-[var(--mobile-accent)]`（主题米白）；图标/图标背景仍保留 warning 橙做语义提示。排查结论：所有宿主弹窗（PluginGlobalDialog/PluginDialogHost/FsAuthDialog/ConfirmDialog）源码均已用 `--mobile-*` token，橙色即 warning 语义色（useTasks clearHistory 确认框 variant='warning'） |

验证：移动端宿主 dialogHost + file-transfer 测试 77 全过；桌面端 file-transfer 测试 90 全过；eslint 0 error；双端插件产物已重建（移动 index.js 含 pull-files 后 transfers 跳转）。PluginDialogHost 为宿主组件（bedcode-mobile/src），需重启移动端 dev 重编译 APK 生效。

### 2026-09-12 追加：启动会话 400 修复 + 拉取方向真正并行（用户联调反馈）

| 问题 | 根因 | 修复 |
| --- | --- | --- |
| 启动会话失败 `HTTP 400 Bad Request Content type error` | `useHttpApi.ts request()` 在 HTTP 收束 Rust 代理重构（6d5eeb18b）时丢失默认 `Content-Type: application/json` 头；桌面端 actix `web::Json` extractor 缺此头即 400 | `request()` 补默认头：`{'Content-Type': 'application/json', ...(options.headers||{})}`，调用方显式指定优先；字段命名无问题（server 端 `StartSessionRequest` 有 `#[serde(rename_all="camelCase")]`，前端传 configId 正确） |
| 多选文件下载串行（一个任务完成才出现下一个） | 宿主 `peer_remote.rs run_pull_queue` **顺序 for 循环**逐个 dial+传输（每文件独立 batch 但串行）；任务行随传输逐条登记 → UI 逐个出现 | 改**真正并行**：全部任务行先登记（UI 同步呈现），`tokio JoinSet` 并发执行，`Semaphore(concurrency)` 限流（默认 3，1..=8，与发送方向共用设置）；`PeerNetNode/StaticPeerRecord` 均 Clone，`dial(&self)` 共享引用安全；双端 `peer_remote.rs` 同步修改（保持 identical） |

验证：移动端 cargo 279 全绿、桌面端 cargo 608 全绿、vitest 378/378、eslint 0 error；双端 peer_remote.rs diff 仍 identical。宿主 Rust 改动需重启 `tauri:android:dev` 重编 APK 生效（10:02 已重启）。待真机验证：启动会话 + 多文件下载同时推进。

**教训**：HTTP 代理收束（fetch → invoke http_request）时 headers 从 `{'Content-Type': 'application/json', ...user}` 简化成 `user||{}`——默认头丢失无编译错误，只有真机 400 才暴露；此类收束应保留默认头合并语义。

### 2026-09-13 追加：桌面端 wasm 产物陈旧（pause 命令缺失）——已重建（用户反馈：双端暂停/取消不生效）

| 现象 | 根因 | 修复 |
| --- | --- | --- |
| 桌面端点击暂停/取消无反应（任务卡按钮存在但不生效） | 桌面插件 **wasm 产物陈旧**：`resources/plugins/desktop/com.bedcode.file-transfer/*.wasm`（09-12 08:49 构建）**不含** `file-transfer.pause/resume/resume-all` 命令字符串（grep -a 验证 0 命中），前端新 UI 调旧命令 → WASM "Command not found" → 前端静默吞掉；移动端 wasm（09-13 00:50 重建）已含 | 桌面插件 `node scripts/build.js --rust-only` 强制重编译（touch rust/src + 删 target wasm 破 cargo 缓存），产物 763844→764100 bytes，pause/resume/resume-all 字符串齐备；resources 与 target md5 一致 |
| 桌面端 plugin.json 未声明 pause/resume/resume-all | 24b0bb122 只给移动端 manifest 补了命令声明（且未提交）；桌面 manifest 缺 | 桌面 `plugin.json` contributes.commands 补三项（与移动端同构），重建后 resources 同步 |
| 命令失败零反馈（点击后无任何反应，无法定位） | `useTasks.pause/resume/resumeAll/cancel/retry` 无 try/catch，execute reject 成 unhandled rejection 被静默丢弃 | 双端 useTasks 各命令补 try/catch：`console.error` 带 batch_id 上下文；移动端额外 `context.dialogs.showToast(String(e),'error')`（沿用 sendFiles 既有模式） |

**教训**：cargo 增量缓存可能产出「mtime 新、内容旧」的 wasm（08:49 那次构建 lib.rs 已含 pause 但产物没有）——构建产物新鲜度不能只看 mtime，改插件 Rust 后应 grep -a 产物内命令字符串验证（本次用 `grep -aoc "file-transfer.pause" <wasm>` 快速校验）。

**验证**：桌面 vitest 90 全过、移动 vitest 73 全过、eslint 0 error（3 warning 为 TransfersTab 既有）、桌面 cargo peer_* 22 全过、双端 plugin.json 合法（python json.load）。

**待办**：
- 移动端 APK 需重新打包安装（`tauri:android:dev` 重装）——设备上 APK 内嵌旧 wasm/host，wasm 虽已重建但未进包
- 桌面端重启 dev（tauri:dev 或已运行则热加载 resources 新产物）后验证暂停/恢复/取消
- 真机复测：发送方向暂停→恢复（断点续传）、暂停中取消、并发闸门

---

## 实施记录 2（2026-09-14：wire 暂停/恢复协议 —— 修复下载方向三连 bug）

### 用户报障（真机联调发现）
1. 桌面端下载（拉取）时传输任务没有暂停/取消按钮
2. 移动端点击暂停 → 桌面端任务变成取消
3. 移动端点击恢复报错 `peer_resume_transfer: resume transfer: no paused send batch with that id`

### 日志证据（09-14 06:55 同一会话，桌面 UTC 22:55）
- 移动端：`peer transfer paused batch_id=pull-...` ×3 → 随后 `pull serve ended ... Cancelled { by_peer: true }` ×3 → 恢复报 `no paused send batch`
- 桌面端：接收进度在暂停时刻戛然而止 → `cancelling remote pull by host`（供流停止后拉取被取消）→ `peer receive session ended ... Cancelled`

### 根因
| # | 根因 | 位置 |
| --- | --- | --- |
| 1 | 暂停无 wire 信号：`TransferFrame` 无 Pause/Resume，暂停实现为 `CancelToken.cancel()` 掐断连接 → 接收端必然 Cancelled | peer-net crate + 引擎 |
| 2 | 接收方向无任何暂停能力：`peer_receive.rs` 无 pause 支持，接收卡只有取消（拉取场景桌面正是拉取发起方，按 spec §14.3 应有暂停权） | peer_receive.rs / TaskPanel.vue |
| 3 | serve（拉取供流）任务恢复被 `sources.is_empty()` 守卫静态拦截，且暂停只改状态不断数据 | peer_transfer.rs resume 守卫 + register_serve_task |

### 修复：协议级 wire Pause/Resume + 数据面门控（不掐断连接）

**暂停语义（关键设计）**：
- 暂停发起方门控「己方数据面」：发送端/serve 门控自身推流（连接保持）；接收端永不停止读（对端停发后读循环天然空闲），保证对端 Resume/Cancel 帧始终可达。
- wire Pause/Resume 帧双向可达：任一方向发起，对端置任务 paused/running 并抬发 `TransferEvent::Paused/Resumed`（宿主同步任务状态），不写回。

**改动面（双端同构 + peer-net crate）**：
- peer-net `transfer/message.rs`：`TransferFrame::Pause/Resume { batch_id }` 帧
- peer-net `transfer.rs`：`PauseSlot`（有界命令通道 + 门控位）、`SessionPause`、`PauseCmd`、`TransferEvent::Paused/Resumed`；`receive_files_after_accept` 拆读写半 + 暂停命令分支 + Pause/Resume 帧处理（消费方不门控读）；`drive_send`/`send_batch` 推流门控（`if !paused` select 守卫）+ 帧处理
- peer-net `shared.rs`：`stream_pull_source`/`serve_pull` 门控 + 帧处理；`SharedHandlerInner.pauses` 注册表 + `SharedDirHandler::set_serve_paused(batch_id, bool)`；`pull_shared_file`/`run_pull_session` 透传 slot
- 双端引擎 `peer_transfer.rs`：`pauses` 注册表（会话生命周期同步）；`drive_send_session` 挂 slot 传 `send_batch`；`pause_peer_transfer` 门控暂停（无活动会话回落取消令牌）+ serve 记账任务经 `handler.set_serve_paused` + 非发送任务回落 `pause_peer_receiving`；`resume_peer_transfer` 三态（Serve 门控 / Live 续流 / Redial 重新拨号）；`resume_all` 双路径；事件循环与 `drive_serve_events` 处理 Paused/Resumed 同步任务状态
- 双端引擎 `peer_receive.rs`：`is_terminal` 含 paused；`settle_terminal` 跳过 paused（同发送侧 apply_terminal）；`set_receive_pause_status`；`pause_peer_receiving`/`resume_peer_receiving` 命令；事件循环处理 Paused/Resumed
- 双端引擎 `peer_remote.rs`：`pulls` 注册表改 `PullSession { token, pause }`；`pause_pull`/`resume_pull`；`run_pull_queue` 创建 slot 传 `pull_shared_file`
- 插件前端：桌面 TaskPanel 接收卡（接收 tab + 全部 tab 混排）加暂停/继续按钮，`receivingStateName` 透传 paused；移动 TransfersTab 接收卡加暂停/继续（`receivingActions`），`receivingStateKey/Class` 加 paused；复用现有 `file-transfer.pause/resume` 命令（引擎回落接收方向）——零 SDK/WIT 改动

### 验证
| 项 | 结果 |
| --- | --- |
| peer-net cargo test | 全绿（+2：发送侧 wire 暂停/恢复端到端、拉取侧 serve 门控暂停/恢复） |
| 桌面 cargo test --lib | 612 全绿 |
| 移动 cargo test --lib | 281 全绿（1 失败 = 既有 ABI fixture 断言 8vs9，与本次无关） |
| 桌面/移动插件 vitest | 658 / 398 全绿 |
| vue-tsc 双端插件 | 净 |
| eslint 根目录 | 0 error（既有 warnings） |
| 双端插件 vite build + resources | 成功（桌面 resources 已更新） |

### 已知边界 / 后续
- push 接收方向（对端主动发送）的接收侧本地暂停未实现（无 slot 写 Pause 帧）：桌面暂停按钮对 push 接收静默 no-op（拉取下载场景已完整可用）；对端发送会话仍可暂停（sender 门控 + wire 帧 → 接收侧任务同步 paused）
- 移动端 APK 需重新打包安装（host 改动未进包）；桌面端重启 dev 生效
- 上次会话遗留的 paused serve 记账任务（无活动会话）只能取消（恢复无对端可续），下次真机会话不再产生该态
- `TRANSFER_PROTOCOL_VERSION` 未 bump（帧为增量；双端同步部署，旧端收到 Pause 帧会协议违规——符合「协议改动双端同步」红线，不承诺旧端互操作）

---

## 实施记录 3（2026-09-14 真机复测：暂停链路彻底修好 + 双侧速率/进度一致 + 双端 UI 缺陷）

### 用户报障
1. 桌面端下载移动端文件：两端进度/速度不一致（同一传输应一致）
2. 桌面端：暂停不起作用；任务卡不显示传输速度；「右上角关闭」无反应
3. 移动端：进度条应是主题色而非灰色
4. 移动端下载时点暂停 → 任务消失并变成历史任务

### 日志证据（09-14 20:56~21:13 移动端 logcat / 桌面 runtime.log）
- 移动端 `pause receiving requested hit=true` → **同一毫秒** `receive session ended while paused state=Failed { transfer session failed (receiver): unknown transfer frame kind 0x75 }`（0x35/0x63/0xed 同形，四次不同会话全中）
- 桌面端同形：`pause receiving requested hit=true` → `unknown transfer frame kind 0xed`
- 之后恢复报 `peer_resume_transfer: resume transfer: no paused send batch with that id`（会话已被上面的解码错位打死，`pulls` 表已摘除）
- 桌面插件 DB `transfer_entries` 速率字段失真：`rateBps` = 416046000 / 551125612 / 706785729（**416~706 MB/s**，同批实际 ~7 MB/s）

### 根因（本轮）
| # | 根因 | 位置 |
| --- | --- | --- |
| 1 | **`read_frame` 非取消安全**却被直接放进 `select!`：暂停命令分支抢先时，已从 TLS 流取走的半截帧字节被丢弃 → 后续帧边界错位 → 对端（本端）读任意字节当 kind → `unknown transfer frame kind 0xXX` → 会话 Failed。这正是「暂停无效 / 恢复报无 paused 批 / 暂停后任务消失」的共同上游 | peer-net `transfer.rs::receive_files_after_accept` |
| 2 | 接收侧暂停**先发 wire 命令后置任务态**：引擎 Failed 终态事件先到 → `settle_terminal` 把 running 任务结算成 failed 并归档历史（真机「点暂停，任务直接变历史」） | 双端 `peer_receive.rs::pause_peer_receiving` |
| 3 | 插件接收视图只收 `status == "running"`：paused 的接收任务既不在接收列表也不在历史 → 「任务消失」（前端 `receivingActions` 的 paused 分支成死代码） | 双端插件 `transfer_store.rs::active_receive_entries` |
| 4 | 速率逐块采样（相邻两次采样只隔一个数据块）：快网上形成亚毫秒窗口，64 KiB/0.15 ms 被放大成数百 MB/s；两端采样时刻不同 → 屏幕上「同一传输两侧速率差两个数量级」 | peer-net `transfer.rs::RateTracker` |
| 5 | 桌面接收（下载）卡不带速率：`ReceivingTask` 无 `rateBps`、`mapReceivingTask` 不映射、卡上 meta 无速度/ETA | 桌面插件 `types.ts`/`useReceiving.ts`/`TaskPanel.vue` |
| 6 | 移动端进度条：`indeterminate` 与底色 class 二选一（不确定态填充无背景色 → 只剩灰轨道）；`pending/paused` 映射到 `ft-progress-cancelled`（--mobile-text-disabled 灰） | 移动插件 `TaskCard.vue`/`types.ts` |
| 7 | 队列面板无关闭入口（只能从顶栏按钮收起），用户找不到「右上角关闭」 | 桌面插件 `TaskPanel.vue` |

### 修复
- **peer-net（两端共享）**：新增 `spawn_frame_reader`（读半独立任务，只向主循环交付**完整帧**，通道接收取消安全）+ `ReaderTaskGuard`（任意出口 abort）；`receive_files_after_accept` 改经通道读帧，取消排空改消费通道；`RateTracker` 改**窗口平均**（500ms 窗口 + `sample_at`/`sync_base_at` 可注入时刻的纯内核）
- **双端宿主 `peer_receive.rs`**：`pause_peer_receiving` 先置 paused 再发 wire 命令（未命中则还原，杜绝假暂停）
- **双端插件 `transfer_store.rs`**：`active_receive_entries` 收 `running | paused`
- **桌面插件前端**：`ReceivingTask.rateBps` + 映射 + 接收卡速率/ETA；队列头右上角关闭按钮（`close` emit → `queueVisible=false`）；合计速率计入接收方向
- **移动插件前端**：`TASK_STATE_PROGRESS_CLASS` 活跃态统一 `ft-progress-active`；`TaskCard` indeterminate 只叠加动画不吞底色

### 验证
| 项 | 结果 |
| --- | --- |
| peer-net cargo test | 109 全绿（+5：窗口速率×3、读帧取消安全反例、读半任务帧完整正例） |
| 桌面宿主 cargo test | 614 lib + 集成全绿 |
| 移动宿主 cargo test | 282 lib + 集成全绿 |
| 双端插件 cargo test | 34 / 34 全绿（+1 接收视图含 paused） |
| 桌面 vitest | 663 全绿（+4 TaskPanel 展示/交互、+1 接收快照速率与暂停态） |
| 移动 vitest | 403 全绿（+5 进度条配色契约） |
| eslint 根目录 | 0 error（119 warnings 既有） |
| 变异自检 | 回退 `active_receive_entries` 修复 → 新用例红（已复现并还原）；Rust 新增代码 rustfmt/clippy 干净 |
| 双端插件产物 | 已重建并核验：wasm md5 target==resources、index.js 含新键（`transfer.queue.close` / `fv2-progress-indeterminate`） |

### 已知边界 / 后续
- **双侧进度天然存在「在途差」**：发送端计「已写网络」、接收端计「已落盘」，差额 = socket/TLS 缓冲内未落盘字节（百 KB 量级，相对 GB 级文件 <0.1%）。本轮修的是「速率失真」（两侧差两个数量级）
- push 接收方向（对端主动发送）的接收侧本地暂停仍未实现（引擎 `run_receive` 传 `SessionPause::None`）：桌面接收卡的暂停对 push 批是明确失败（前端有 toast/日志），拉取下载方向已完整可用
- 无活动会话的 paused serve 记账任务（对端取消后遗留）恢复仍报 `no paused send batch`，只能取消/等重启标注 interrupted —— 本轮未改语义（避免「暂停即进历史」的观感回归）
- 桌面端「右上角关闭」按「传输队列面板右上角」实现；若用户指的是**应用窗口标题栏关闭**，属宿主 lifecycle 范畴（日志见 SIGTERM 后进程仍存活 ~20min 的线索），另开任务排查
- 真机复测：暂停/恢复（下载 + 上传）、暂停中取消、并发闸门、双端速率一致性；移动端需重装 APK（host 改动未进包），桌面端重启 dev（host 已改）

---

## 实施记录 4（2026-09-14 22:59 真机复测：一端显示暂停、对端仍在传输）

### 用户报障
一端显示「已暂停」，另一端仍在继续传输。

### 日志证据
- 移动端 `peer transfer paused batch_id=pull-1789397918412410218`（serve 记账批）×2；**两端都从未出现** `pull serve transfer paused`（serve 门控分支日志）
- 移动端 `pull serve ended ... Err(TransferProtocol { role: "share-sender", detail: "expected file_done after pull stream, got Pause { batch_id: \"pull-...649-1\" }" })`：对端（桌面）的 Pause 帧落在 serve 会话「等 FileDone」阶段 → 判协议违规 → 会话 Failed
- 桌面 14:59:44 `pause receiving requested ... hit=true`（拉取暂停命令本身命中）

### 根因（两条独立缺陷）
| # | 根因 | 后果 |
| --- | --- | --- |
| 1 | `pause_peer_transfer` **分支写反**：`let paused = task.sources.is_empty()`（= serve 记账任务）后 `if paused { 发送会话分支 }`，而 `set_serve_paused`（serve 门控）写在 `else`（= 真发送任务）。serve 任务查不到发送暂停句柄、也无取消令牌 → **什么都没门控却返回 Ok(true)** | 本端任务置 paused + 前端乐观标记 ⇒ 真机「本端已暂停、对端持续收数据」；反向（真发送任务）落 `set_serve_paused` 空查 → SDK 报错，但任务已被置 paused、数据照发 |
| 2 | Pause/Resume 帧只在「推流循环」内合法：等 StartFile / 等 FileDone / 等 BatchDone 阶段收到 Pause 一律协议违规 Err；本端暂停命令在这些阶段也不下发 | 对端在数据推完瞬间暂停（真机正是此刻）→ serve 会话 Failed；发送端在文件间隙暂停 → 对端收不到暂停、卡片停在传输中 |

### 修复
- **双端宿主 `peer_transfer.rs`**：暂停改为显式 `PauseRoute { Serve, SendSession }` 路由（纯函数 `pause_route(sources_empty)` + 回归用例 `pause_route_splits_serve_accounting_from_send_sessions`）；Serve 分支无活动会话时还原状态（防假暂停）；`resume_all_peer_transfers` 补 serve 分支（此前 serve 任务被 `continue` 跳过 → 能暂停却不能「全部继续」）
- **peer-net（双端共享）**：新增 `next_frame_handling_pause`（会话级暂停面）——控制相等待期间同时 ① 消费本端暂停命令并写 Pause/Resume 帧、② 容忍对端 Pause/Resume 帧（置位门控 + 抬发 Paused/Resumed 事件）后继续等本阶段期待的帧；替换 `drive_send` 4 处、`stream_pull_source` 2 处、`serve_pull` 前置等待 1 处控制相等待（原 serve 前置循环的内联 Pause/Resume 处理随之收敛到该函数）

### 验证
| 项 | 结果 |
| --- | --- |
| peer-net cargo test | 110 全绿（+1 会话级暂停面契约：本端命令下发 + 对端 Resume/Pause 容忍 + Cancel 不被吞） |
| 桌面宿主 cargo test | 616 全绿 ×2（+1 `pause_route` 路由护栏） |
| 移动宿主 cargo test | 283 + 集成全绿（+1 同上） |
| 变异自检 | ① 反转 `pause_route` → 新用例红（复现原 bug，已还原）；② waiter 不 `continue` 处理 Pause 帧 → 新用例红（`left: Pause / right: BatchDone`，已还原） |
| 其他 | 本轮无插件/SDK/WIT 改动，插件 wasm 与前端产物无需重建；`cargo fmt` 新增代码干净 |

### 已知边界 / 后续
- 真机复现路径的端到端用例仍缺（Pause 落在「等 FileDone」需要毫秒级时序，回环下无法确定性构造）；本轮用「waiter 契约用例 + 变异自检」兜住机制，端到端留待真机复测
- push 接收方向的接收侧本地暂停仍未实现（`run_receive` 传 `SessionPause::None`）；serve/发送/拉取三条路径现已对称可用
- 宿主与 peer-net 均改动：桌面需重启 `tauri:dev`，移动端需重装 APK 才能生效

---

## 实施记录 5（桌面端「清空历史」点了没反应）

### 现象
桌面端传输队列 → 历史 tab → 点「清空历史」无任何反应（列表不空、无报错提示）。

### 排查结论（日志取证）
- 桌面 dev 运行**每次启动会重置当日日志**（`bootstrap.log`: `dev reset: replaced ... runtime/error/frontend`）⇒ 报障当次会话的日志已被下一次启动覆盖，无法回溯。
- 现存会话（23:09:57~23:11:13）里插件存储仅被**读**过一次（`restored 61 transfers`），`plugin.db` mtime 停在启动时刻 ⇒ **没有任何清空写入**，即命令没走完（或压根没到插件）。
- 代码链路逐段复核均为正确接线：`TaskPanel` 按钮 → `emit('clearHistory')` → `FileTransferView` `onClearHistory` → `useReceiving.clearHistory` → `plugin_invoke` → 插件 `file-transfer.clear-history` → `transfer_store::clear_terminal` → `flush`（持久化 + 四路事件）。构建产物内三处字符串（`clearHistory` / `file-transfer.clear-history` / `ft-history-clear`）齐备。
- 双端对比发现**桌面端是「未完成的双胞胎」**：移动端 `clearHistory` 有「二次确认 + try/catch + 成功后本地更新」，桌面端三样全无（无确认、无错误处理、UI 仅依赖 `history-changed` 事件刷新）。

### 修复（桌面端补齐 移动端同构能力 + 可观测性）
1. `composables/useReceiving.ts::clearHistory`：
   - 新增宿主全局弹窗二次确认（`context.ui.showDialog` + 危险动作按钮，先例：TrustedPeersSection 撤销；移动端同构版本已有确认）——点击立即有可见反馈；
   - 命令成功后 `await refresh()` 重拉三列表：以插件持久层为真源刷新 UI，不再单靠事件送达（事件丢失/组件重挂即表现为「点了没反应」）；
   - 失败 `console.error` 后**向上抛**：宿主全局 `unhandledrejection` 处理器会写入 frontend 日志（此前完全静默）。
2. 双端插件 `peer.rs::clear_history`：新增 `log_info("clear-history cleared N terminal entries")` —— 破坏性操作留痕，下次可直接区分「命令没到」与「到了没清」。
3. i18n：`transfer.history.clearConfirmTitle` / `clearConfirmBody`（zh-CN + en + schema）。

### 验证
| 项 | 结果 |
| --- | --- |
| 桌面 vitest | 666 全绿（+3：清空后重拉、失败上抛且本地历史不变、取消确认不下发命令） |
| 移动 vitest | 403 全绿 |
| 双端插件 cargo test | 34 / 34 全绿 |
| 变异自检 | 去掉「成功后重拉」→ 新用例红（已还原） |
| vue-tsc（桌面插件） | 干净 |
| eslint | 0 error（既有 warning） |
| 产物 | 双端 wasm + index.js 已重建并核验（wasm 含 `clear-history cleared`，js 含 `clearConfirmTitle`） |

### 已知边界
- 若下一次仍「点了没反应」，看两处即可定位：插件日志有无 `clear-history cleared N`（无 ⇒ 命令未达插件，属宿主/前端链路）、frontend 日志有无 `[GlobalError] unhandledrejection`（有 ⇒ 命令失败原因在内）。
- 桌面端**没有** `context.dialogs.showConfirm`（SDK 未提供，仅移动端有）：故桌面确认改用 `ui.showDialog` + 动作按钮实现，两端交互一致但实现不同源。

---

## 实施记录 6（2026-09-15：同一传输任务「双端都能立即暂停/恢复/取消」+ 双端链路集成测试）

### 需求（用户口径，优先于旧 plan 的方向拆分）
**同一个传输任务，任意一端都能立即暂停、恢复、取消**——不按「发送方才有权暂停」的方向切分，四端视角（发送方/接收方/拉取方/供流方）能力对称。

### 审计：四视角能力矩阵（动手前）
| 视角 | 暂停/恢复 | 取消 | 结论 |
| --- | --- | --- | --- |
| 发送方（push sender） | ✅ wire 帧 + 本地门控 | ✅ cancel token | 已具备 |
| 接收方（push receiver） | ❌ `run_receive` 传 `SessionPause::None` | ✅ | **缺口 1** |
| 拉取方（puller） | ✅ | ✅ | 已具备 |
| 供流方（pull serve） | ✅ `set_serve_paused` | ❌ 宿主按批取消寻址落空 | **缺口 2** |

补充缺口（审计中发现）：
- **缺口 3**：`send_batch` 在宿主未传暂停句柄时 `SessionPause::None` ⇒ 对端（接收方）Pause 帧**没有门控落点**，「接收方按暂停、发送方照传」的同类根因。
- **缺口 4**：「全部继续」只遍历发送方向，已暂停的接收任务不会被拉起。

### 修复（peer-net 引擎 + 双端宿主，全部双端同构）
1. **接收侧本地暂停**（`packages/peer-net/src/shared.rs` + `transfer.rs`）：
   - `SharedHandlerInner` 新增 `receive_pauses` 表（接收 batch_id → `PauseSlot`；与 serve 的 `pauses` 分表，避免两侧视角 batch_id 碰撞时错误路由）；
   - Offer 分支创建句柄 + `TableGuard` 随会话生命周期登记/摘除（原 `PauseTableGuard` 泛化为 `TableGuard<T>`，取消登记复用同一守卫）；
   - `run_receive` 增 `pause: Option<Arc<PauseSlot>>` 参数并透传 `receive_files_after_accept`（原硬编码 `None`）；
   - 新增 `SharedDirHandler::set_receive_paused(batch_id, paused)`（写 Pause/Resume 帧请求对端发送会话门控推流）。
2. **发送侧门控兜底**（`transfer.rs::send_batch`）：宿主未挂句柄时**会话内自建** `PauseSlot` ⇒ 对端帧永远有门控落点。
3. **供流侧按批取消**（`shared.rs`）：新增 `serve_cancels` 表（serve batch_id → 会话子令牌）+ `SharedDirHandler::cancel_serve_transfer`；`serve_pull` 登记、`TableGuard` 摘除。与既有 `sessions` 表（pull 以 **dir_id** 登记）分表——宿主按 batch_id 取消此前静默落空。
4. **宿主接线**（双端 `peer_receive.rs` / `peer_transfer.rs`）：
   - `pause_peer_receiving`：拉取句柄未命中 → 回落 `set_receive_paused`（push 接收批），双双未命中才还原状态（防假暂停）；
   - `resume_peer_receiving`：同构双路；
   - `cancel_peer_transfer`：无发送会话时先试 `cancel_serve_transfer`（供流批），否则维持原 pending 乐观结算；
   - 新增 `resume_all_peer_receiving` 并接入 `resume_all_peer_transfers`（「全部继续」覆盖发送 + 接收两方向）。

### 集成测试（新增 `packages/peer-net/tests/pause_symmetry.rs`，生产装配形态）
两端都挂 `SharedDirHandler`、真实 mTLS 回环直连，覆盖矩阵：

| 用例 | 覆盖 |
| --- | --- |
| `push_receiver_pause_stops_sender_and_resume_completes` | 接收方本地暂停/恢复；**发送端刻意不挂宿主句柄**（引擎兜底回归） |
| `push_sender_pause_syncs_receiver_task_and_resume_completes` | 发送方本地暂停/恢复 + 对端 `Paused`/`Resumed` 事件同步 |
| `pull_serve_side_pause_stops_stream_and_resume_completes` | 供流方本地暂停/恢复（serve 门控） |
| `pull_serve_side_cancel_interrupts_puller_and_keeps_partial` | 供流方按批取消：双端终态 + 拉取方 `.part` 保留 |

「立即生效」判据（外部行为）：暂停后 `.part` 尺寸必须在 `STALL_GRACE=5s` 宽限内停滞（双采样相等），期间不得出现终态；恢复后内容逐字节一致。

### 验证
| 项 | 结果 |
| --- | --- |
| peer-net cargo test | **114 全绿**（81 lib + 9 + 4 新增 + 6 + 13；连续 3 轮稳定） |
| 桌面宿主 cargo test | 639 lib + 集成全绿 |
| 移动宿主 cargo test | 283 lib + 集成全绿（磁盘满导致全量重建后复跑） |
| 双端插件 cargo test | 桌面 **36**（+2：事务形态 / 回滚）、移动 34 全绿 |
| 桌面 / 移动 vitest | 666 / 403 全绿 |
| eslint 根目录 | 0 error（119 既有 warning） |
| rustfmt | 新增测试文件 `--check` 干净；其余文件 diff 为仓库既有漂移（未顺手改） |
| 双端插件产物 | 桌面已重建 + dev 副本同步（md5 一致）；wasm 内含 `BEGIN IMMEDIATE` / `file-transfer.pause` / `clear-history cleared` |
| 残留进程 | 无 vitest / cargo test 残留（仅用户既有 Gradle daemon） |

### 变异自检（改坏 → 用例转红 → 还原，全部实测）
| 变异 | 结果 |
| --- | --- |
| `send_batch` 去掉会话内自建句柄 | 接收方暂停后传输直接跑完 ⇒ 「暂停期间出现终态：Completed」（复现真机现象） |
| Offer 分支不把句柄挂进接收会话 | 同上转红 |
| `set_serve_paused` 只报成功不下发命令 | 供流侧暂停无效 ⇒ 转红 |
| `cancel_serve_transfer` 不触发令牌 | 拉取方落 `Completed` 而非 `Cancelled{by_peer:true}` ⇒ 转红 |
| 插件 `persist_entries` 失败也 COMMIT | 回滚用例转红 |
| 插件去掉 `BEGIN IMMEDIATE` | 事务形态用例转红（首条语句断言） |

### 已知边界 / 后续
- **在途余量**：暂停指令生效后，socket 缓冲 + 帧通道内已发出的数据仍会落盘（百 KB 量级，LAN 上毫秒级耗尽），故判据是「宽限期内停滞」而非「零增长」；UI 侧任务立即置 paused（进度事件对 paused 任务不再改写状态）。
- **一次不可复现 flake**：本轮首次全量 peer-net 测试时 `transfer_session` 有 1 例失败（未留下用例名），随后 14 轮复跑（含 8 轮 transfer_session 单跑）全绿；判定为负载下偶发，记录待观察。
- **轮次 C 遗留未做**（用户本轮聚焦暂停能力，均不影响暂停链路）：`plugin_db_*` 宿主有界等待、`reload_wasm_plugin` 是否等待在飞 guest 调用（热重载打断写事务的根因加固）。
- **需生效动作**：桌面端重启 `tauri:dev`（host + peer-net 改动）、移动端重装 APK（`pnpm run tauri:android:dev`，host 改动未进包）；桌面/移动插件 wasm 本轮**无功能改动**（桌面因上一轮 persist_entries 已重建）。
- **真机复测清单**：① 桌面接收移动端推送 → 桌面暂停/恢复/取消；② 移动端接收桌面推送 → 移动端暂停/恢复/取消；③ 桌面拉取移动端 → 桌面暂停/恢复/取消 + 移动端（供流）暂停/恢复/取消；④ 两端同时操作同一任务（先后暂停/恢复，状态双端一致）；⑤ 暂停中取消（`.part` 保留，重试续传）。

