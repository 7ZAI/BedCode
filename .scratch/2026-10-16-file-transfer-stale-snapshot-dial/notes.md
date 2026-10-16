# 2026-10-16 双端文件传输：连接失败根因（陈旧快照拨号）+ 修复

> 用户现象：「双端文件传输 wasm app 依然无法发现对方建立连接；桌面端可以发现，
> 点击连接移动端没有反应」。

## 1. 现场证据（最新日志）

**桌面端**（`bedcode-desktop/.dev-logs/` + `~/.local/share/com.bedcode.app/logs/runtime.*.log`）：
- `peer discovered name=M2105K81AC ... peer=10.62.87.97:47613 short=a050655d` —— 引擎级 mDNS
  发现移动端**正确地址**；
- `WARN bedcode_server_peer_net: peer dial unreachable: dial peer failed on TCP connect: 192.168.1.4:47613: 连接超时` ——
  拨号却用了 **192.168.1.4**（旧网络残留地址）；
- 前端 `dial-peer failed`（host.internal）。

**桌面端持久存储**（`~/.local/share/com.bedcode.app/bedcode.db` → `plugin_storage`）：
`device_snapshot = [{"addr":"192.168.1.4","nodeId":"906bf087...","deviceName":"Redmi K70E","lastSeenMs":1791353884032(≈9天前), ...}]`
—— 旧身份（906bf087）+ 旧地址（192.168.1.4），而移动端当前身份 a050655d / 地址 10.62.87.97:47613。

**移动端**（06:21/06:32 会话）：`MulticastLock acquired`、`peer discovered name=binblink-PC peer=10.62.87.120:47613`
—— 发现正常；`list-remote FAILED: Command not found` 系「无活跃对端」时前端包装噪音（WASM 命令表
含 list-remote，peer::list_remote 因无连接报 no active peer）。

## 2. 根因链

1. `usePeerDevices.start()` 快照恢复**不过滤时间窗**：9 天前的陈旧条目（旧网络地址 / 旧手机身份）
   无条件上屏成为拨号候选；
2. 快照条目 nodeId（906bf087）≠ 实时 mdns-found 条目 nodeId（a050655d）→ `upsertDevice` 按 nodeId
   匹配不会覆盖 → 陈旧条目一直驻留可点；
3. `connect()` 直接按缓存 addr/port 拨号 → 192.168.1.4:47613 TCP 超时 → `unreachable`
   → 移动端从未收到任何拨号（「无反应」）；
4. 同步的「无法发现对方」感知：引擎发现正常（30s requery 窗口 + 对端节点未起），但首屏被
   陈旧条目占位误导 + 实时条目到达前列表为空。

## 3. 修复（双端同构改动，4 + 2 文件）

- `composables/deviceState.ts`：新增纯函数 `isWithinRestoreWindow(lastSeenMs, now)`
  —— 仅 TTL 窗口（DEVICE_TTL_MS=120s）内「近期可见」条目可恢复；
- `composables/usePeerDevices.ts`：
  a) `start()` 快照恢复加窗口过滤（陈旧条目不再上屏）；
  b) `connect()` 对 `restored`（未经本会话实时 mDNS 刷新验证）条目**拒绝拨号** +
     触发宿主即时重查（`requestHostRefresh`），实时条目接管后按实时 endpoint 拨号。
- 注释含 2026-10-16 实机证据；陈旧快照自愈（下次 live found 落新地址快照）。

## 4. 验证

- 移动端：`pnpm run test:run` 全量 102 文件 / **1225/1225 ✓**（新增 usePeerDevices 2 例 +
  deviceSnapshotWindow 纯函数 4 例）；
- 桌面端：`vitest run` 单进程 116/121 文件过；失败 4 例均为无关既有 flake
  （useTasks 全量并发下 5s 超时、单跑 14/14 ✓；terminalPreview 是 terminal-session 线，未触碰，
  孤立复现）；新增 deviceSnapshotGuard.test.ts 4 例 ✓；
- vue-tsc（双端 file-transfer tsconfig）0 error；eslint 改动文件 0 error；
- 双端前端构建 ✓（桌面 build:frontend；移动 plugin-build.js --plugin com.bedcode.file-transfer）。

## 5. 遗留 / 顺带发现

1. 桌面端 `useTasks`/`terminalPreview` 全量并发 flake（maxWorkers 高时甚至 worker crash
   ERR_IPC_CHANNEL_CLOSED——本机内存 13Gi 受限，全量建议 `--pool=forks --poolOptions.forks.singleFork`）。
2. 移动端 `Command not found: file-transfer.list-remote` 前端包装丢了底错误详情（无活跃对端时的
   噪音），不影响行为，可后续改善 extractInvokeErrorMessage。
3. 真机复验路径：桌面开 file-transfer → 不再出现「Redmi K70E(旧)」→ 移动端在线后以
   M2105K81AC 出现 → 点击 → 移动端应弹首连确认 → 接受后进入已连接。
