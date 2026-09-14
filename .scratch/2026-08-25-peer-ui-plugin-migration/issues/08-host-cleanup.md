# 08 — 宿主对等 UI 删除与接线清理（两端）

**What to build:** 插件已全面接管对等网络前端后，把两端宿主内的对等 UI 整体拆除：对等页面（桌面 3 页 / 移动设置下 5 页）、首连确认与批量请求弹窗宿主组件、六个对等编排 composable、路由注册、侧边栏内置菜单项（桌面）、设置分组入口（两端）、布局中的弹窗宿主挂载、peers 与 settings.peer* i18n 命名空间。完成后宿主里不再有任何对等入口，界面能力全部随插件启停。

**Blocked by:** 07

**Status:** done

- [x] 两端宿主源码树内 grep 不到任何对等页面组件、弹窗宿主与编排 composable 的残留引用
- [x] 路由表、侧边栏菜单（含排序常量）、设置分组、布局挂载点同步清理，应用正常编译运行
- [x] 宿主 locales 删除对等命名空间并从注册表中摘除；无悬空 i18n key 引用
- [x] 对等服务层不动：peer-net crate、Tauri 命令面、事件桥、host-peer 原语全部保留
- [x] 动手前核对工作区无并发会话在途改动混入风险（受保护路径按仓库规范处理）
- [x] 两端 code-map 文档同步修正（移除对等相关条目描述）
- [x] 被删宿主编排测试文件随之删除；两端前端测试套件可整体跑通（个别因删除需更新的断言在本票内修正）

## Comments

### 执行记录（2026-08-25）

**删除清单**（两端对称）：

- 桌面：3 页面（PeerDevicesView / PeerTransfersView / PeerRemoteFilesView）、2 弹窗宿主（PeerConsentDialogHost / PeerBatchDialogHost）、6 编排 composable（usePeerConsent / usePeerDevices / usePeerReceiving / usePeerRemoteFiles / usePeerTransfers / useTrustedPeers）、5 宿主编排测试、`locales/{zh-CN,en}/peers.ts`
- 移动：settings 下 5 页面（PeerDevices / PeerTransfers / PeerRemoteFiles / PeerReceiveSettings / TrustedPeers）、同款 2 弹窗宿主 + 6 composable、4 宿主编排测试、`locales/{zh-CN,en}/peers.ts`

**接线清理**：

- 路由：桌面摘除 `/peer-devices` `/peer-transfers` `/peer-files/:nodeId`；移动摘除 settings 下 trusted-peers / peer-devices / peer-transfers / peer-files / peer-receive 五条
- 布局挂载点：DesktopLayout.vue 与 MobileLayout.vue 的弹窗宿主 `<PeerConsentDialogHost />` / `<PeerBatchDialogHost />` 及 import 摘除
- 桌面侧边栏：useSidebarMenu.ts 删两内置项与 `BUILTIN_MENU_ORDERS.peerDevices/peerTransfers` 槽位常量，注释同步
- 设置分组：桌面 SettingsView.vue 整删「可信对端」「文件接收」两个 section 及撤销确认 Modal、receiving/trustedPeers 相关脚本逻辑与 Modal import；移动 SettingsView.vue 整删 `groups.peerNetwork` 分组（4 入口）
- i18n：两端 `locales/index.ts` 注册表摘除 peers 命名空间；`settings.ts` 删 `peer` / `peerReceive` 段（移动另删 `groups.peerNetwork`）；grep 复查无悬空 key 引用

**断言修正**：桌面 `useSidebarMenu.test.ts` 六处菜单 id 序列断言去除 peer 两项（含 order 150 插槽插入用例改为 devices(100)→150→sessions(200) 口径）。

**验证**：

- 桌面 `npm run test:run` 53 文件 / 482 用例全绿；移动 27 文件 / 268 用例全绿（插件侧迁移测试 `__tests__/plugins/file-transfer/*` 不受影响）
- 两端 `vue-tsc --noEmit` 通过；`vite build`（build:fast）通过；eslint 仅既有 warning 无 error
- 服务层零改动：git status 确认 `src-tauri/` 无本票新增变更（tauri.conf.json 为 07 票在途改动）
- code-map 同步：移动端移除「文件传输对端相关页」设置子页描述与导航表条目；桌面端 views/components/composables 描述本无对等条目无需改动
