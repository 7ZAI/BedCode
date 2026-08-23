# 08 — 前端：发现列表 + 连接/信任界面

**What to build:** file-transfer 插件前端接上新对等层：设备列表页展示发现缓存中的节点（名称 + 能力标记，文件传输能力有无一目了然）与在线状态；点击具备能力的节点发起连接——对端确认后进入已连接态；信任管理入口（列表/撤销）挂入设置。两端 UI 同构适配（桌面窗口 / 移动端安全区）。此票完成后用户第一次「看得见」对等网络。

**Blocked by:** 03, 04

**Status:** in-progress（双端代码与自动化测试完成；真机双端互连走查待人工）

- [ ] 真机双端：列表互见 → 点击连接 → 对端弹窗 → 接受 → 显示已连接——**待人工走查**（自动化链路已覆盖：拨号三态/事件摘除有 vitest 与 Rust 测试断言）
- [x] 无传输能力的节点在列表中可区分且不可发起文件传输（DTO `fileTransfer` 位标记 → 徽标区分 + 按钮禁用 + composable 防御拦截）
- [x] 信任管理入口可用：查看、撤销、撤销后再连重新走确认（issue 04 已落地，本票将移动端入口归拢进「对等网络」分组）
- [x] 列表随节点上下线自动刷新（发现缓存驱动，无需手刷）（宿主侧快照比对推送 `peer-devices-changed`，前端事件订阅免轮询）
- [x] 编排 composables/stores 有 vitest 覆盖；文案 zh-CN/en 同步；样式过 frontend-styles 自查

## Comments

### 实现记录（issue 08）

- 两端宿主 `src-tauri/src/peer_net.rs`（镜像改动）：新命令 `dial_peer(node_id)`（发现缓存取记录 → mTLS 拨号；denied/unreachable 为业务终态而非 Err，前端按态渲染文案）、`disconnect_peer(node_id)`（drop 句柄即 TCP 关闭）；`PeerNetRuntime` 补存 `node` 句柄（原装配后即弃，无拨号入口）；`PeerNetState.connections`（std Mutex HashMap）登记本机主动拨号的存活连接保持会话，重复拨号以新句柄替换旧连接；`stop_locked` 排空连接表并逐个发 `peer-disconnected`
- 发现缓存变更推送：宿主侧后台任务 `drive_discovery_push` 每 2s 对 `cache.list()` 做序列化串指纹比对，变化才 emit 全量列表事件 `peer-devices-changed`；节点停止补发一次空列表清空前端后任务退出，重启由 `start_locked` 重新拉起。**取舍**：不改共享 crate 的 DiscoveryCache 接口（保持最小缝），LAN 规模下轮询+比对成本趋零，满足「无需手刷」验收
- DTO：新增 camelCase `DiscoveredPeerDto`（含 `fileTransfer` = capabilities bit0）替换 `list_discovered_peers` 的裸 `DiscoveredPeerRecord`（snake_case 直跨 IPC 与 TrustedPeerDto 风格不一致；该命令此前零前端消费者，改形安全）；`DialPeerResultDto { status, deviceName }`
- 前端两端同构 `usePeerDevices.ts`：usePeerConsent 同款模块级单例——`start()` 幂等注册三类事件监听（devices-changed/connected/disconnected）+ 首帧主动拉取兜底页面晚启动；`connect()` 防御拦截（无能力/未发现/在途/已连接直接拒绝发起）并记录 `dialErrors` 终态反馈，命令面异常归一为 unreachable；`disconnect()` 乐观摘除徽标；`_resetPeerDevicesForTest()` 用例隔离
- 页面：桌面 `PeerDevicesView.vue` 挂 `/peer-devices` + 侧边栏内置项（order 150 紧随设备配对，WiFi 弧线图标，wb-toolbar/wb-btn 范式）；移动 `views/settings/PeerDevicesView.vue` 挂 `/mobile/settings/peer-devices`（SettingsSubPage 包裹 + ConfirmDialog 同款卡片风 + `--mobile-*` token + 44px 触达）；移动设置页新增「对等网络」分组（settings.groups.peerNetwork），设备列表与可信对端归拢同组
- i18n：新增顶层 `peers.devices.*` 域（title/subtitle/empty/capFileTransfer/capNone/connect/disconnecting/connected/denied/unreachable/trustHint 等）zh-CN/en 双语 × 两端四文件同步
- 已知边界：入站方向不登记已连接徽标（被连侧会话由 SharedDirHandler 自持，双向对称登记属传输票范围）；发起侧「已连接」为宿主句柄存活语义，TCP 死链的主动探测依赖后续传输会话心跳
- 验证：`packages/peer-net` cargo test 94 全绿；desktop `cargo test --lib` 576、mobile `cargo test --lib -j 2` 396 + 集成 21 全绿（mobile 全并行编译 rustc OOM，限并行度后通过）；desktop vitest 465（含 usePeerDevices 9 用例 + useSidebarMenu 期望更新至新内置项）、mobile vitest 231 全绿；eslint 改动面 19 文件 0 error；frontend-styles 自查通过（token-bound、44px 触达、无原生控件外观）。mobile target 18.5GB 超 15GB 规线已执行 `cargo clean`
