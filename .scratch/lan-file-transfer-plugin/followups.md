# 内网文件传输插件 — 后续工作清单

> 实施会话（2026-08-05）完成 spec 步骤 1–5 与两端 UI 后的遗留项。已记录至 auto-memory（`lan-file-transfer-implementation`），此处为入库副本。

## 待办（按优先级）

### P1 — 上传方向收口
- [x] **桌面上传 UI（发送到手机）**（2026-08-07 完成）：宿主新增 `plugin_pick_files` 多文件选择命令（`plugin_pick_directory` 同构，fileservice 权限）→ SDK `FileServiceAPI.pickFiles()` + 前端 `pluginPickFiles` 封装 + 权限列表双端同步（permission.ts / SDK permission.rs）→ 顶栏「发送到手机…」按钮 → `enqueueUpload`（`direction=upload` + `localPath`，remotePath 取文件名，对端挂载根落位）。桌面端上传入队链路已闭环。
- [ ] **上传方向 E2E 验证**：上传钩子已修复（`host.fs_exists` 同名即拒）、session 流已实现，需真机双端实测（移动→桌面 10GB 级 + 桌面→移动）。

### P2 — 宿主/SDK 缺口
- [x] **移动端 SDK 补 `fs_delete` 导出**（2026-08-07 完成）：新增 Kotlin `FileDeletePlugin`（gen/android + android-backup 双备份，已入 AGENTS.md 恢复清单）→ `android_plugins.rs` 注册/`delete_file()` 桥 → wasm_runtime 注册 `host_fs_delete`（fs:write 权限 + fs_auth Write 校验，非 Android 平台 std::fs 兜底）→ SDK `HostFs::fs_delete`（abi.rs 常量 + 签名表 + wasm_host 实现）→ 插件 `delete_part_file` 恢复真实删除。移动端取消下载现在会清理本地 `.part`。
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
