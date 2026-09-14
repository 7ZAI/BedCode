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
