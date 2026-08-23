# 09 — 前端：发送侧（表单/扇出发送/进度/传输历史）

**What to build:** 发送全流程 UI：从本机选择文件/文件夹 → 选择一个或多个具备能力的可信对端 → 扇出发送。领域模型为 N 条相互独立的传输批：每条独立批 ID、独立进度、独立过对端策略闸门，任一接收方拒绝不影响其余；「群发」仅呈现为一个来源动作下的多条任务。进行中任务展示实时进度/速率/取消；终态任务进传输历史（封顶滚动、可清空）。

**Blocked by:** 08, 06

**Status:** in-progress（双端代码与自动化测试完成；真机双端走查待人工）

- [ ] 真机双端：A 向 B 推送文件成功落 B 下载目录——**待人工走查**
- [ ] A 同时向 B、C 扇出：B 拒绝不影响 C 正常完成——编排独立性已有 vitest 断言（sendToPeers allSettled + 单台失败结果映射），真机走查待人工
- [ ] 大文件中断后从任务入口重试走断点续传——同批 ID 重试已接通（引擎侧断点语义属 issue 06 已验收），真机走查待人工
- [ ] 进度/速率/取消/终态在任务页如实呈现
- [x] 历史含发送与接收全部终态，封顶淘汰生效（存储层 direction 字段就绪，接收方向接入属 issue 10；封顶淘汰有 Rust 单测）
- [x] vitest 编排逻辑覆盖；i18n 双语同步；frontend-styles 自查通过

## Comments

### 实现记录（issue 09）

- 两端宿主 `src-tauri/src/peer_transfer.rs`（镜像新模块，发送命令面 = crate `send_batch` 薄封装）：
  - `send_files_to_peer(node_id, paths)`：发现缓存取记录 → spawn_blocking 收集源文件（目录递归保持相对形状、同名目标「名称 (2).ext」编号去重、512 文件上限）→ 后台会话拨号 + `send_batch`；命令即返 running 态 DTO
  - `cancel_peer_transfer` / `retry_peer_transfer`（同批 ID 重发 = 断点续传，仅 failed/cancelled/rejected；源清单重启后丢失则明确报错）/ `list_peer_transfers` / `clear_peer_transfer_history`
  - `peer_pick_files` / `peer_pick_folder` 宿主自有选源命令（不经插件门控）：桌面 plugin-dialog 多选/目录；Android 复用 SafPickerPlugin 真实路径解析（单文件逐次累加，目录发送按平台隐藏——SAF 树 URI 不能作发送源）
  - 事件桥 `peer-transfer-changed` 全量列表推送（复用 issue 08 范式）；Progress 引擎按 ≤64KiB chunk 发射，转发层 150ms 时间窗节流防 IPC 风暴
  - 会话代次（epoch）防护：重试换发令牌并递增代次，旧会话迟到的终态事件丢弃不覆盖新一轮状态
  - 历史持久化 `transfer_history.json`：终态落盘 tmp+rename 原子写、串行锁防竞态、封顶 200 滚动淘汰（最新在前保序裁剪）、格式版本 fail-fast；重启后历史可查但不可重试（sources 仅内存），UI 重试失败时如实反馈
- 前端两端同构 `usePeerTransfers.ts`（模块级单例）：start 幂等注册事件监听 + 首帧拉取兜底；`sendToPeers(paths, nodeIds)` 经 Promise.allSettled 每台独立发起——扇出互不影响在编排层成立；active/history computed 分区；cancel/retry/clearHistory 命令封装；`_resetPeerTransfersForTest` 用例隔离
- 页面：桌面 `PeerTransfersView.vue` 挂 `/peer-transfers` + 侧边栏内置项（order 160，托盘上行箭头图标）；移动 `views/settings/PeerTransfersView.vue` 挂 `/mobile/settings/peer-transfers`（SettingsSubPage 包裹 + settings-group 卡片 + `--mobile-*` token + 44px 触达）。发送表单 = 选源 chips + 目标设备自绘勾选多选卡（无原生控件）+ 扇出按钮；进行中区进度条 scaleX 合成层动画 + 速率/百分比/取消；历史区终态色点 + 拒绝原因文案 + 重试/清空
- i18n：新增顶层 `peers.transfers.*` 域 zh-CN/en × 两端四文件同步（含 status 五态与 rejectReason 五类 wire 值到文案的映射键）
- 适配在途改动：crate `TransferEvent` 各变体新增 `remote: NodeId` 字段（issue 10 接收侧前置），两端 peer_net.rs 既有事件消费者模式补 `remote: _`
- 验证：desktop `cargo test --lib` 588 全绿（含 peer_transfer 7 用例：重试资格/camelCase wire/去重/目录展开/历史 roundtrip+版本 fail-fast/封顶淘汰）、mobile `cargo test --lib -j 2` 408 全绿；desktop vitest 473（52 文件）全绿（含 usePeerTransfers 8 用例：扇出独立性/空参防御/命令接线/分区）、mobile vitest 239 全绿；eslint 改动面 0 error；vue-tsc 无新增错误（mobile 存量 useTerminalBuffer 在途改动报错与本票无关）；frontend-styles 自查通过（token-bound、scaleX 动画、无原生控件外观、对比度 token）。注：桌面 rustc 曾 OOM（内存受限环境），`CARGO_INCREMENTAL=0` 关增量后通过

(End of file)
