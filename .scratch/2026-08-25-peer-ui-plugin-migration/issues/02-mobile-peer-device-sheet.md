# 02 — 移动插件·附近设备面板（bottom sheet）

**What to build:** 移动端 file-transfer 插件浏览主页面的顶栏对端名变为可点：点开一个附近设备 bottom sheet，列出所有具备文件传输能力的发现节点及三态（已连接 / 连接中 / 未连接），支持连接、断开、切换活跃对端，失败行内文案。现有「自动挑首个可用设备」逻辑收敛为「保持当前选择 + 空选时兜底」。本票只做插件前端，devMock 驱动，dev-shell 可完整演示。

**Blocked by:** None — can start immediately（可与 01 并行）

**Status:** resolved

- [x] 顶栏对端名区域可点开设备 sheet；sheet 内列表展示全部可传输节点及三态
- [x] 连接 / 断开 / 切换活跃对端可用；连接中防重复发起；denied / unreachable 行内错误文案
- [x] 交互遵循移动端规范：--mobile-* token、44px 最小触控目标、返回键先关 sheet 再退目录（ onBackPressed 接管顺序正确）
- [x] 自动兜底逻辑调整后不回归：无活跃对端时仍能自动选中首个可用设备
- [x] 设备状态派生纯函数有单测；编排 composable 单测覆盖事件整表替换、命令路由、失败上报
- [x] devMock 设备种子（在线/未连接两态）可在 dev-shell 演示全部行为
- [x] 只改插件前端源码，不触碰任何 Rust 与宿主文件

## Comments

### 实现记录（2026-08-24，ticket 02 落地）

**落地文件清单**

插件前端（`bedcode-mobile/plugins/file-transfer/`）：
- `src/composables/deviceState.ts` — 新增，三态归并无头纯函数 `deriveDeviceRows`（与桌面 ticket 01 同构平移）
- `src/composables/usePeerDevices.ts` — 新增，编排 composable：订阅 devices-changed / connection-changed / device-connected(disconnected)，命令路由 dial-peer / disconnect-peer / set-active-peer；兜底策略实现「保持当前选择 + 空选时兜底」
- `src/components/PeerDevicesSheet.vue` — 新增，附近设备 bottom sheet（Teleport + ft-sheet 过渡 + safe-area 底距 + 44px 触控目标 + 状态点呼吸动画尊重 prefers-reduced-motion）
- `src/components/FileTransferView.vue` — 顶栏对端名改为可点触控区（chevron 提示），onBackPressed 接管顺序：设备 sheet → 队列 sheet → 退目录 → history.back；挂载/卸载 usePeerDevices.start/stop
- `src/composables/useTasks.ts` — 「自动挑首个可用设备」收敛：onDevicesChanged 只维护 peerOnline 粗粒度标记，不再强制 set-active-peer 覆盖用户选择，connOnline 不再被发现事件污染（仅 ws_* 驱动）；顺带移除 mapWireTask 中 Task 类型不存在的 `localPath` 字段（修复插件工程既有 vue-tsc 报错）
- `src/i18n/{messages,zh-CN,en}.ts` — 新增 transfer.devices.* 13 键（zh/en 同步）；两语言文件补 `satisfies MessageSchema` 使 schema 强制真正生效（顺带补齐 5 个历史遗漏 key 到 schema：pull.*×3 / queue.entry / task.open / task.remove）
- `src/devMock.ts` — 新增种子（小米 14 Pro 在线+活跃 / Pixel 9 拨号成功 / BedBox denied / Old Laptop unreachable / HomeNAS 无能力），与桌面 ticket 01 同数据
- `src/index.ts` — 导出 `devMock`

SDK / dev-shell（通用接线，`packages/plugin-sdk-mobile/`）：
- `src/types.ts` — PluginDevMock 增 `peer?: PeerDevMock`（桌面同构协议对齐），index.ts re-export
- `dev-shell/src/mock/file-transfer.ts` — 新增，按 pluginId 消费 devMock.peer 种子注册对等域命令 handler（query-peer / list-peers / set-active-peer / dial-peer / disconnect-peer）并模拟 devices-changed / connection-changed 事件（query-peer 执行时同步补发追平晚订阅）
- `dev-shell/src/loader.ts` — 激活时检测 `getDevMock(pluginId)?.peer` 存在即注册 mock，停用时清理定时器

测试（`bedcode-mobile/src/__tests__/plugins/file-transfer/`）：
- `deriveDeviceRows.test.ts` — 7 例：空快照 / idle 派生 / 三态优先级归并 / 错误呈现边界 / 能力位归一化 / 活跃标记约束
- `usePeerDevices.test.ts` — 13 例：start 幂等 / refresh 拉取 / 整表替换 / **保持当前选择不被覆盖** / 兜底切换命令路由 / connect 成功 / denied+unreachable 如实上报 / 防御拦截（未发现/无能力/已连接）/ 并发拨号去重 / 断开乐观摘除 / 连接态集合维护 / WS 控制面语义隔离 / switchPeer

**验证结果**
- `npx vitest run src/__tests__/plugins/file-transfer` — 20/20 通过
- `vue-tsc -p plugins/file-transfer/tsconfig.json` — 干净
- 根 `vue-tsc --noEmit`（覆盖 SDK types）— 本次涉及文件零错误（剩余错误均在宿主 Peer* 文件，属 ticket 03/08 迁移范围、HEAD 上既有）
- dev-shell `vite build` — 通过
- eslint（改动文件）— 0 error 0 warning

**遗留项**
- 移动端 WASM 薄代理尚缺 dial/disconnect/set-active 等命令转发与 peer:connection 桥接（ticket 03 收口）；真机链路待阶段三验证
- dev-shell 的 tsconfig 本身存在大量既有类型报错（模块解析不一致等），本次新文件已做到除该既有 TS2307 模式外零新增
- 顶栏 pill 文案仍走 connOnline（WS 控制面）语义，与对等传输连接态分离——如需改为按活跃对端连接态展示需产品确认
