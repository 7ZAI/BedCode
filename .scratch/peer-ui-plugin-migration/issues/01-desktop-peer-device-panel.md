# 01 — 桌面插件·附近设备面板与连接管理

**What to build:** 在桌面端 file-transfer 插件的浏览主页面内，现有「在线设备切换下拉」重构为完整的附近设备面板：用户能看到同一网络内所有具备文件传输能力的 BedCode 节点及其三态（已连接 / 连接中 / 未连接），能对未连接设备发起连接、断开已连接会话、在多个已连接设备间切换活跃对端；拨号被拒或对端不可达时在对应设备行内看到明确的失败文案。本票只做插件前端，全部数据由前端 mock handler / devMock 种子驱动，在 dev-shell 中即可完整演示。

**Blocked by:** None — can start immediately

**Status:** resolved（阶段一实现完成；UI 冻结待 ticket 06 截图评审统一收口）

- [x] 设备列表展示全部具备文件传输能力的发现节点（含未连接者），三态视觉可区分
- [x] 点未连接设备发起连接：握手期间显示连接中且不可重复发起；成功后可设为活跃对端；已连接设备可断开
- [x] denied / unreachable 结果以行内错误文案呈现（i18n zh-CN 与 en 同步补充）
- [x] 发现缓存事件驱动全量替换；活跃对端下线自动兜底切换逻辑保留；WS 控制面连接态与对等连接态语义不混淆
- [x] 设备状态派生（connected/connecting/error 归并）为无头纯函数并有单测；编排 composable 单测覆盖 start 幂等、事件整表替换、连接/断开命令路由、失败如实上报
- [x] devMock 提供覆盖在线/未连接两态的设备种子，dev-shell 中可演示上述全部行为
- [x] 只改插件前端源码（组件/composable/i18n/types/mock），不触碰任何 Rust 与宿主文件

## Comments

### 2026-08-24 实现记录

**落地文件：**
- `plugins/file-transfer/src/composables/deviceState.ts` — 无头纯函数 `deriveDeviceRows`（三态归并 + 行内错误 + 活跃标记）
- `plugins/file-transfer/src/composables/usePeerDevices.ts` — 编排 composable（替代删除的 `usePeer.ts`，context 参数注入风格）；订阅 `devices-changed`（全量替换）/`connection-changed`（增量）；命令路由 `dial-peer` / `disconnect-peer` / `set-active-peer`；connOnline 仍由旧 WS 控制面事件驱动，与对等连接态隔离
- `plugins/file-transfer/src/components/PeerDevicesPanel.vue` — 面板组件（自绘，样式走 styles.css 的 ft-dev-* 类，token-bound）；顶栏入口角标改为已连接数，点击外部收起
- i18n 新增 `transfer.devices.*` 14 key（zh-CN/en 同步，MessageSchema 编译期同步保障）
- 测试：`src/__tests__/plugins/file-transfer/{deriveDeviceRows,usePeerDevices}.test.ts`（19 用例；放宿主测试套件插件子目录，复用既有 vitest glob，无需改宿主配置）

**devMock 协议归位（按 spec 决策 14）：**
- 桌面 SDK types.ts 新增 `PluginDevMock { peer?: PeerDevMock }` + `PluginModule.devMock?`（与移动端 SDK 同构）
- 种子数据在插件工程 `src/devMock.ts` 导出、入口 re-export；dev-shell 只做通用接线（registry 增 registerDevMock/getDevMock，loader activate 前注册/deactivate 清理；mock/file-transfer.ts 消费种子驱动命令与事件）
- 种子五节点覆盖：初始已连接+活跃 / 拨号成功(800ms) / denied / unreachable / 无能力可见不可连

**遗留给 ticket 07 收口核对：**
- `connection-changed` 事件契约定为 `{ nodeId, connected: boolean, deviceName? }`——当前桌面 WASM proxy 对 peer:connection topic 是原样透传宿主 `{nodeId, deviceName}`（connected）与 `{nodeId}`（disconnected），缺 `connected` 字段，需在 proxy 翻译层补齐
- `file-transfer.query-peer` 现返回 DiscoveredPeerDto[]（list_devices_raw 透传）、`list-peers` 返回旧 `{peers, activePeerId}`——前端 refresh 同时拉两者，契约不变
