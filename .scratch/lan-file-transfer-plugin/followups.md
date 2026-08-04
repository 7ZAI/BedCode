# 内网文件传输插件 — 后续工作清单

> 实施会话（2026-08-05）完成 spec 步骤 1–5 与两端 UI 后的遗留项。已记录至 auto-memory（`lan-file-transfer-implementation`），此处为入库副本。

## 待办（按优先级）

### P1 — 上传方向收口
- [ ] **桌面上传 UI（发送到手机）**：桌面 SDK `FileServiceAPI` 仅 `pickDirectory`，`enqueue` 上传需 `localPath`。需补桌面 SDK 文件选择 API（宿主 dialog 能力），接上 spec §9.1 顶栏「发送到手机…」按钮。
- [ ] **上传方向 E2E 验证**：上传钩子已修复（`host.fs_exists` 同名即拒）、session 流已实现，需真机双端实测（移动→桌面 10GB 级）。

### P2 — 宿主/SDK 缺口
- [ ] **移动端 SDK 补 `fs_delete` 导出**：移动端取消下载不清理本地 `.part`（spec §7.4）。桌面已走 `host.fs_delete`，移动 SDK `HostFs` 缺 delete。
- [ ] **锁跨阻塞网络 IO 重构**：`schedule_and_start` 持全局 `Mutex` 时执行 `http_fetch` 握手（http_fetch timeout 120s/connect 10s），对端半连接时进度事件可停滞最长 120s、拖住移动端单 delivery worker。非硬死锁，真机若体验差再重构（prepare→execute→commit 三段式）。
- [ ] **fs_auth 白名单收敛**：`com.bedcode.file-transfer` 进白名单 = 任意路径放行。当前信任模型（内置首方 + 配对白名单）下接受；若未来开放 zip 安装同名插件需收敛为按 roots/download_dir 授权。

### P3 — 打磨
- [ ] **Unix rename TOCTOU**：下载 `try_exists→rename` 间目标被创建时 Android/Linux 会静默覆盖（`duplicate-name` 语义破坏）。可接受（窗口极小），如需彻底修复用 `renameat2(RENAME_NOREPLACE)`。
- [ ] **错误透传**：`context.commands.execute` 把插件未激活/WASM trap 包装为 "Command not found"（与桌面既有行为一致）；duplicate-name 拒绝后 `retry` 必然再次失败（重试前需清理目标）。
- [ ] **移动端 i18n 编译期 key 同步**：桌面用 `MessageSchema` 接口强制，移动端是无类型对象，建议对齐。

## 待验收（spec §12 性能基准，需真机双端）
1. iperf3 `-P 4 -t 30` 双向标定链路上限 T，大文件顺序读写测磁盘 D
2. 单大文件（10GB 级）吞吐 ≥ 80% × min(T, D)
3. 多文件并发（默认 3）聚合 ≥ 75% × min(T, D)
4. 断点续传正确性：30%/60%/90% 强制中断（关 App/断 WiFi/锁屏），恢复后哈希一致
5. 并发抢占与恢复：传输中改并发数、对端插件停用再启用，无僵尸任务
6. 内存稳定性：全程宿主+插件内存增长 ≤ 100MB

不达标时启用文件内分片并发升级路径（HTTP/1.1 内）。
