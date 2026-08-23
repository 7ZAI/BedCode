# 10 — 前端：接收侧（询问弹窗/接收任务/落点设置）

**What to build:** 接收全流程 UI：传输批到达时按接收策略呈现——每次询问模式弹整批确认框（文件清单 + 总大小 + 发送方身份，接受全部/拒绝全部，超时自动拒）；进行中的接收任务与发送任务同页分列（正在发送 / 正在接收）；接收策略设置维持全局单开关（每次询问默认/直接接收/直接拒绝）+ 询问超时时长；桌面端新增接收落点文件夹设置（缺省 Downloads\BedCode\），逐次不弹窗。

**Blocked by:** 08, 05

**Status:** ready-for-human（双端代码与自动化测试完成；真机双端互传走查待人工）

- [ ] 真机：B 收到 A 的请求弹窗，展示清单/总大小/对方身份；接受后落下载目录
- [ ] 拒绝与超时路径 A 侧收到正确终态
- [ ] 直接接收/直接拒绝模式下无弹窗且行为正确
- [ ] 接收中任务可取消；正在发送/正在接收分组正确
- [ ] 桌面端可更换落点目录并即时生效
- [x] vitest 编排逻辑覆盖；i18n 双语同步；frontend-styles 自查通过（桌面 usePeerReceiving 10 用例 + 移动 9 用例；peers.transfers/settings.peerReceive 域 zh-CN/en 四文件同步）

## Comments

### 实现记录（issue 10，主 agent 直实现；与 issue 09 并行会话零侵入协作）

**背景**：执行期间检测到另一会话正实现 issue 09（发送侧宿主 `peer_transfer.rs` 与前端任务页已就位，其 DTO `direction:"receive"` 明确预留本票接缝）。为避免并行写冲突，本票全部改动落在独立文件或纯追加段，未改写其任何一行既有代码。

- **共享 crate**（packages/peer-net）：`TransferEvent` 三变体补 `remote: NodeId` 对端身份（接收方向=发送方），穿透 run_receive/receive_files_after_accept/run_send/drive_send/serve_pull/stream_pull_source 全部发射点——AlwaysAccept 策略无 OfferPending 阶段，Progress/Terminal 也必须带身份前端才能归组展示；`CancelToken` 增父子层级（`child(&global)`，装箱打断 async 递归 E0733）；`SharedDirHandler` 收敛进 `Arc<SharedHandlerInner>`：transfer 配置改 RwLock 支持运行中热更新（`update_transfer_config`，每连接取快照、在途会话不受影响）、新增按批取消注册表（batch_id→子令牌 + SessionGuard drop 摘除，`cancel_transfer(batch_id)`），原全局 `cancel_token()` 急停语义保留（测试 receiver_cancel 路径不回归）。cargo test：68 lib + 4 shared_dirs + 12 transfer_session 全绿
- **双端宿主新模块 `peer_receive.rs`**：引擎事件消费替换 peer_net.rs 的日志占位——OfferPending 登记 pending 任务（oneshot 回执入表）+ 发现缓存解析发送方名、Progress 节流入账（150ms 窗口）、Terminal 结算终态、通道关闭（节点停止）把在途接收如实落 failed；设置持久化 `transfer_settings.json`（策略 ask/always_accept/always_deny + 超时 10..=600 + 桌面落点覆盖），`register_handler` 在节点装配时以持久化设置纠正首份配置、变更经 `apply_settings` 即时热生效；命令面 `respond_peer_transfer` / `cancel_peer_receiving`（pending 视同拒绝、running 走按批取消）/ `list_peer_receiving` / `clear_peer_receiving_history` / 设置读写 / 桌面 `peer_pick_download_dir`。事件通道 `peer-receive-changed` 与发送侧 `peer-transfer-changed` 平行，DTO 复用其 `PeerTransferDto` 形状，前端合并双源列表
- **peer_net.rs 触点极小化**（3 处）：消费任务替换、`resolve_download_dir` 提权 pub(crate)、stop_locked 清 handler 句柄
- **前端**（两端镜像）：`usePeerReceiving.ts` 模块单例——事件驱动列表 + pending 批秒级心跳倒计时（归零自动回执拒绝，宿主 TTL 权威兜底）+ 纯函数 offerDeadline/remainingSeconds/pickCurrentOffer 导出供测；`PeerBatchDialogHost.vue` 询问弹窗（发送方身份+指纹/文件清单滚动区/总大小/接受全部/拒绝全部，关闭等同拒绝）挂载进双端 Layout；传输页接入「正在接收」分列（pending 倒计时行 + running 进度行）与合并历史（方向徽标区分，清空按钮双源并发）；设置面新增接收策略三段自绘分段控件 + 超时 chips + 桌面落点更换/恢复默认（SettingsView 区块 / 移动 PeerReceiveSettingsView 子页 + 路由 + 对等网络分组入口）
- i18n：`peers.transfers.{receivingSection,sendingSection,pending,askTitle,fromWithName,fileList,totalSizeLabel,acceptAll,rejectAll,countdown}` + `status.pending` + `settings.peerReceive.*`，zh-CN/en 双语 × 两端四文件同步
- 已知边界：接收终态仅会话内存（封顶 100），跨重启历史由发送侧 `transfer_history.json` 承载（spec 故事 26 的统一追溯由前端合并视图达成）；设置热更新对在途询问批不追溯（下一条连接生效）
- 验证：desktop vitest 483 全绿（含 usePeerReceiving 10 用例）、mobile vitest 246 全绿（含 9 用例）；desktop cargo test --lib 588、mobile cargo test --lib -j1 408 全绿（限并行规避 OOM）；eslint 改动面 0 error；frontend-styles 自查通过（token-bound、44px 触达、Teleport z-50、无原生控件外观、`--mobile-bg-hover` 不存在已改用 opacity 反馈）。无 Kotlin 改动，gradle 编译验证不适用
