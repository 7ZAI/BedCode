# T6 真机回归清单（桌面 ↔ 移动 文件传输）

Status: 待执行（**本清单由人执行**；静态门禁与产物重建已在 T6 内完成，见 §0）
Date: 2026-10-09
关联：`spec.md`（T6 票）· `docs/adr/0044-file-transfer-shared-business-core.md`

---

## 0. 机器已完成的项（无需人跑）

| 项 | 结果 |
| --- | --- |
| 移动端插件单测 | `cd bedcode-mobile/wasm-apps/file-transfer/rust && cargo test` → **29 全绿** |
| 桌面端插件单测 | `cd bedcode-desktop/wasm-apps/file-transfer/rust && cargo test` → **47 全绿** |
| 共享核单测 | `cd packages/bedcode-file-transfer-core && cargo test` → **72 全绿** |
| 移动端产物重建 | `node scripts/plugin-build.js --plugin com.bedcode.file-transfer` → 736,404 B，已同步 `resources/plugins/mobile/` |
| 桌面端产物重建 | `cd wasm-apps/file-transfer && pnpm run build`（vite → cargo wasm32-wasip3 → 注入 wasmHash → 复制）→ 1,009,305 B，已同步 `resources/plugins/desktop/`，`plugin.json.wasmHash` 已更新 |
| 接口面（桌面） | 重构前后产物 **105 条接口条目零差异** |
| 宿主原语调用集合（双端） | 与 HEAD **完全一致**（`diff` 无输出）⇒ 本重构未改插件对宿主的调用面 |
| ABI / 权限 / 前端 | 均未变（`plugin.json`、`permissions`、`wasm-apps/*/src/` 在 `git status` 中无 diff） |

**未跑（必须人跑）**：本清单全部用例——需要真机（Android + 桌面）与对端互连。

---

## 1. 前置

1. 两端都装上**本次重建后**的插件（移动端必须重装 APK 或推送新产物；桌面端确认 `resources/plugins/desktop/com.bedcode.file-transfer/` 为新产物）。
2. 两端同一局域网，且都启用 file-transfer 插件。
3. **日志抓取**（失败时唯一线索来源）：
   - 桌面：宿主日志（`RUST_LOG=debug` 起的 dev 实例；插件侧 `log_info`/`log_error` 进宿主日志）。
   - 移动：`adb logcat | grep -i "file-transfer\|bedcode"`。
   - 插件内部关键文案：`restored N transfers` · `marked interrupted` · `send queued by plugin-side concurrency gate` · `transfer store load deferred`。

---

## 2. 用例（逐条勾选；失败请记下「哪一步 + 日志片段」）

### A. 基础互传（双方向）

| # | 操作 | 期望 | 失败时看 |
| --- | --- | --- | --- |
| A1 | 移动端 → 桌面：发送 1 个文件 | 两端任务卡出现；移动端进度递增；桌面接收队列出现；结束后两端进历史 | 移动 `tasks-changed` 是否发出；桌面 `receiving-changed` |
| A2 | 移动端 → 桌面：多选 3 个文件 | 同一批（一个 batchId）一条任务卡，文件清单 3 条 | 卡片数是否被拆成 3 |
| A3 | 桌面 → 移动：发送 1 个文件 | 移动端弹接收确认（策略 ask）；接受后进度递增 | 未弹窗 ⇒ 看 `auto_answer` 分支与策略值 |
| A4 | 桌面 → 移动：发送目录 | 递归展开后的文件全部到达 | 目录未展开 ⇒ `peer_collect_outgoing` |

### B. 接收策略与落点（移动端形态差异）

| # | 操作 | 期望 |
| --- | --- | --- |
| B1 | 策略 = ask，接收时**拒绝** | 桌面端发送任务 `rejected`，原因透传（`rejectReason` = `UserRejected`） |
| B2 | 策略 = ask，接收时**超时**（等过超时秒数） | `rejected`，`rejectReason` = 超时类枚举 |
| B3 | 策略 = accept | 不弹窗直接接收 |
| B4 | 策略 = reject | 不弹窗直接拒绝，发送端 `rejected` |
| B5 | 超时值填 5 / 700 保存 | 实际推给引擎的是 **10 / 600**（钳制），UI 回读仍是原值 |
| B6 | **移动端落点**（本次改动关注点） | 收到文件落在 `MediaStore.Downloads`；**移动端不推** `set-download-dir`（若日志出现该调用即为回归） |
| B7 | 桌面端落点设置为某目录 | 桌面收到文件落在该目录（桌面**应当**推 `set-download-dir`） |

### C. 共享目录注册表（差异面①③）

| # | 操作 | 期望 |
| --- | --- | --- |
| C1 | 移动端添加共享目录（SAF 选择器） | 注册表出现该目录；对端能浏览到；推给引擎的载荷字段是 **`safTreeUri`** |
| C2 | 桌面端添加共享目录 | 同理，但载荷字段是 **`path`** |
| C3 | 删除目录后再重启插件 | 注册表持久化正确（移动 KV / 桌面 plugin-db 表 `shared_roots`），顺序保持加入序 |
| C4 | 推送失败场景（目录被删/无权限） | 注册表**回滚**并报错，不出现「UI 有、引擎无」的分裂态 |

### D. 浏览与拉取（T7=B 行为变化重点）

| # | 操作 | 期望 |
| --- | --- | --- |
| D1 | 移动端浏览桌面共享目录 | 目录树可下钻（断线自动重拨） |
| D2 | **移动端从桌面拉取 1 个文件** | 接收视图**立即**出现该批次行（`pull-started` 事件即建行），随后进度递增 |
| D3 | **桌面端从移动端拉取 1 个文件** | 同上——**这是本次的行为变化**：桌面原先要等旧快照才出行，现在事件到达即出行（T7 已落地）。若桌面拉取后**不出现任务行**，即为回归 |
| D4 | 拉取后该行「重试」 | 可重试（`retryMeta` 已按 `rel_path` 挂回——事件通路 `attach_pull_meta`，T7 已落地）；重试后历史仍只有一条（批次 ID 替换） |

### E. 暂停 / 恢复 / 取消

| # | 操作 | 期望 |
| --- | --- | --- |
| E1 | 发送中点暂停 | 状态变 paused；**残留的 Progress 事件不得把它打回 running** |
| E2 | 暂停后恢复 | 回到 running，进度从已传字节续（不归零） |
| E3 | 暂停中点取消 | 不落终态（终态事件只是中断确认）；取消后原因码 = `cancelled-by-self` |
| E4 | 对端取消（发送中，接收方按取消） | 发送端原因码 = `cancelled-by-receiver`；接收端 = `cancelled-by-self` |
| E5 | 移动端拉取中取消 | 原因码 = `cancelled-by-sender`（对端是发送方） |

### F. 重试判据（核内单点，两端同源）

| # | 操作 | 期望 |
| --- | --- | --- |
| F1 | 失败任务点重试 | 新批次 ID 替换旧行，状态回 running，写入 **不新增历史行** |
| F2 | 进行中任务没有「重试」入口 | UI 不出现（判据 `NotTerminal`）；若强行调用应报「仍在传输中」 |
| F3 | 「本端供流记账行」（对端发起、本端供流）不该可重试 | 点重试报「发起方元数据缺失」 |
| F4 | 失败任务重试成功后再失败，再重试 | 仍可重试（凭证未被消费掉） |

### G. 历史与中断标注

| # | 操作 | 期望 |
| --- | --- | --- |
| G1 | 传输中杀掉插件/重启 | 重启后进行中条目显示为**中断**（不再假装在跑）；日志出现 `marked interrupted` |
| G2 | 历史超过 200 条 | 最旧终态条目被淘汰（进行中不淘汰） |
| G3 | 清空历史 | 只清终态，进行中保留 |
| G4 | 接收完成后在文件列表点「打开/定位」 | 移动端落点文件可打开；桌面 `reveal-in-dir` 打开所在目录（**移动端不支持该项是设计，不应报错崩**） |

### H. 双端一致性抽查（本次重构的核心承诺）

| # | 操作 | 期望 |
| --- | --- | --- |
| H1 | 同一操作（如取消）在两端的原因码/状态文案 | **逐字一致**（同一份核内判据） |
| H2 | 同一失败场景在两端的历史归档时机 | 一致（终态即归档，卡片从活动队列消失） |
| H3 | 两端各自断网/重连后的 endpoint memo 行为 | 一致：断线摘句柄、保留 memo、下次命令自动重拨 |

---

## 3. 明确不在本次范围（避免误判为回归）

1. **桌面 `peer.rs` 的票 08 三项编排修正**（重试判据前置到调引擎之前 / 发送闸门统一走 `send_slot_open` / 排队批派发失败落带凭证的终态行）**已在桌面落地**（2026-10-09 裁决：以移动修正版为准；`retry_task` 判据前置 + 闸门、`MAX_SEND_ATTEMPTS` 重排队 + `insert_failed_send_entry`、拉取意图先入队 + 事件通路 `attach_pull_meta`、停用 `reset_volatile_intents`）。D3 / F 组用例即验证点。
2. 桌面 `manifest-gen` 的权限映射表问题（他人会话在途）：本次桌面产物经 **app 自带 `scripts/build.js`** 重建（vite → cargo → 注入 wasmHash → 复制），**未**走根 `plugins:build` 的 manifest 生成步骤。因本次未改 `plugin.json`/权限，产物 manifest 与改动前同源；若后续改了权限，必须回到根 `plugins:build`。
3. `tracing` 在桌面插件 `Cargo.toml` 中未被使用（HEAD 亦如此）——cargo 的 `unused_dependencies` 警告为既有，不是本次引入。

---

## 4. 结论填写（执行后补）

| 区间 | 结果 | 备注 |
| --- | --- | --- |
| A 基础互传 | 待填 | |
| B 策略与落点 | 待填 | |
| C 共享目录 | 待填 | |
| D 浏览与拉取 | 待填 | **D3 是本次行为变化的验证点** |
| E 暂停/恢复/取消 | 待填 | |
| F 重试判据 | 待填 | |
| G 历史与中断 | 待填 | |
| H 双端一致性 | 待填 | |
