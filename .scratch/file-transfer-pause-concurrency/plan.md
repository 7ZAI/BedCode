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
