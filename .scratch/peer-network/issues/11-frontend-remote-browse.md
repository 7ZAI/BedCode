# 11 — 前端：远端共享目录浏览/下载

**What to build:** 浏览可信对端暴露的共享目录：进入对端文件页 → 目录树逐级下钻（目录优先、按名排序）→ 勾选单个/多个文件或整个目录拉取到本机下载目录，进度与取消复用任务体系。Android 存储权限过滤导致列表不全时展示 notice 提示（沿用既有语义）。

**Blocked by:** 08, 07

**Status:** ready-for-human（双端代码与自动化测试完成；真机双端互拉走查待人工）

- [ ] 真机双端：A 浏览 B 的共享目录并拉取文件成功落本机
- [ ] 目录下钻、多选拉取、进度与取消可用
- [ ] 拉取大文件中断后重试续传（复用 06 引擎）
- [x] 未信任节点不可见此入口；只读——无任何写操作入口
      （入口仅在已连接（互信）态渲染且要求 fileTransfer 能力位；命令面仅列根/列目录/拉取，
      线协议无写语义帧，结构性只读由 issue 07 保证）
- [x] vitest 编排逻辑覆盖；i18n 双语同步；frontend-styles 自查通过
      （usePeerRemoteFiles 双端各 14 用例；peers.files.* zh-CN/en 四文件同步；token-bound、
      自绘勾选钮、44px 触达、safe-area 避让自查通过）

## Comments

### 实现记录（issue 11，主 agent 直实现）

- **共享 crate**（packages/peer-net）：线协议新增 `RootsRequest`/`RootsResponse{dirs:[{id,name}]}`
  帧（JSON-tagged，无需动 kind 字节）——浏览方无从得知对端注册的 dir_id，先取共享根清单再逐根下钻；
  `BrowseResponse` 增 `#[serde(default)] filtered` 提示位（旧对端缺省 false，前后版本兼容）；
  `SharedSafAccess` 增默认方法 `may_hide_entries()`（空列表 + 宿主提示时置位，沿用既有
  notice 语义）；客户端 API `list_shared_roots` / `browse_shared_dir`（返回改为
  `BrowseListing{entries,filtered}`）/ `pull_shared_file`（增显式 batch_id 参数——宿主需以同 ID
  预登记接收任务行）；`SharedDirHandler` 增 `event_sender()` 访问器。cargo test 95 全绿
  （shared_dirs 4→5，含 roots 寻址闭环新测试）
- **双端宿主新模块 `peer_remote.rs`**（镜像）：命令面 `list_peer_shared_roots` /
  `browse_peer_directory`（每操作新拨号，单连接单请求契约）/ `pull_peer_files`（逐文件独立会话
  顺序队列：每文件独立 batch_id + 急停子令牌 + 会话发起前经 `register_remote_pull` 预登记 running
  任务行，Progress/Terminal 经既有事件通道自动入账接收表；拨号失败 `fail_task` 落终态防悬挂；
  单次上限 512 文件兜底）。取消接入：`cancel_peer_receiving` 先查 pull 令牌表再回退服务端会话表；
  节点停止 `clear_state` 取消本代根令牌中止在途与后续拉取。peer_net start/stop 各接线一处
- **前端两端镜像** `usePeerRemoteFiles.ts`：open 先取共享根清单（恰一根自动进入并等首屏加载）→
  面包屑下钻 → 勾选（文件点选/自绘勾选钮/全选仅作用文件）→ `pullSelection` 将所选目录经
  `enumerateDirFiles` BFS 递归展开（深度 16/数量 512 双闸，纯函数注入 browse 供测），与所选文件
  合并一次性入队；进度/取消在传输任务页正在接收分列呈现（复用任务体系），中断后重新拉取同一
  文件即断点续传（引擎继承）。页面：桌面 `/peer-files/:nodeId`（wb-toolbar/wb-btn 范式）+
  设备页已连接态「浏览文件」入口；移动 `/mobile/settings/peer-files/:nodeId`（SettingsSubPage +
  `--mobile-*` token + 44px 触达 + 底部选择条 safe-area 避让）+ 设置设备页镜像入口。
  权限过滤 notice：filtered 且空列表时展示警告横幅（沿用既有语义）
- i18n：`peers.files.*` 域 zh-CN/en 双语 × 两端四文件同步
- 已知边界：每次列目录/拉取均新拨号（LAN mTLS 握手数十 ms，v1 可接受；多请求复用连接属后续优化）；
  目录拉取大小估算仅含直选文件（目录展开后真实总量以任务行逐条呈现）
- 验证：packages/peer-net cargo test 95 全绿；desktop cargo test --lib 588、mobile cargo test
  --lib -j2 408 全绿；desktop vitest 497、mobile vitest 260 全绿（各含 usePeerRemoteFiles 14 用例）；
  eslint 改动面 0 error；frontend-styles 自查通过。无 Kotlin 改动，gradle 编译验证不适用
