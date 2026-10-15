# 2026-10-15 移动端连接页（bedcode wasm-app）逻辑与交互修正

> 触发：用户贴出移动端日志
> `[HttpApi] HTTP error: 401 ... Plugin endpoint '/api/plugin/com.bedcode.terminal-session/supported-agents' requires auth "jwt"`
> + 「扫描设备报错，请优化连接页面逻辑与交互」。

## 1. 现场证据（真机 elish/duchamp，10-15 09:18–09:23）

- 09:18:48 `[MobileConnection] init() loaded credentials: null` —— 本次运行**没有已存凭据**。
- 09:19:22 点「扫描设备」→ mDNS 找到 `BedCode-binblink-PC 10.62.87.120:8767`。
- 09:19:23 点设备连接：探测 200 → WS `ws_connected` → 任务域立刻请求
  `supported-agents`（**此刻无 JWT**）→ 桌面端 401。
- 连接页此后停在错误状态：头部「已连接」+ 卡片「已认证」+ **「正在连接 …」永不消失**，
  且**没有配对口**（用户无法完成认证）。
- 截图：`before-connection-page.png`（09:23 拍摄，故障态）；`state-authenticated-0951.png`
  （09:51 拍摄，用户手动配对后的已认证态——**不是**修复后核验，仅作状态对照）。

## 2. 根因（4 条，互相叠加）

| # | 位置 | 问题 |
| --- | --- | --- |
| R1 | `wasm-apps/bedcode/src/host/useHostPage.ts` | `isAuthenticated = api.isConnected`（传输层已连）被当成「已认证」。`connected` 时认证分支抢先渲染，**配对口（`v-else-if`）永远不可达**；「已认证」是假的 |
| R2 | `wasm-apps/bedcode/src/task/activate.ts` | 任务域看 `isConnected` 拉 `supported-agents`（JWT 保护端点）：`connected` 时发请求吃 401，而 `connected → paired` 不翻转 `isConnected` ⇒ 白名单**本会话永远为空**（报错只是症状） |
| R3 | `src/composables/useMobileConnection.ts` | `isConnecting` 只在 `onPaired` / 出错 / 断开时复位；「已连上但未认证」时永久为 true ⇒ 「正在连接…」常驻 + 连接页按钮全禁用 |
| R4 | `wasm-apps/bedcode/src/host/useHostPage.ts` | 新宿主页只做 connect，没有旧 DevicesView 的「用已存凭据静默换新 JWT」一步；返回用户重启后永远停在 `connected`（事件通道建不起来、受保护端点 401） |

补充：`isConnecting` 在 `onConnected` 复位只覆盖手动连接路径；重连路径（`ws_reconnected`
自己置 true 直到认证完成）行为不变。

## 3. 修复

- **SDK 投影**（`packages/plugin-sdk-mobile/src/types.ts` + `src/plugin/index.ts`）：
  新增 `isPaired`（已认证事实，区别于 `isConnected` 传输层已连）与
  `authenticateWithStoredToken()`（宿主静默换新 JWT，凭据零过境）。
- **连接页状态机**（`DevicesSection.vue`）：
  - 连接中：显示目标 + **取消按钮**（`cancelConnect`）。
  - 已连未认证：配对口（设备名 + 「等待认证」chip + 提示 + 生物识别（已绑定）+ 请求配对/验码）。
  - 认证失败（配对码错 / 凭据被永久拒绝）：状态会变 `error`，配对口**保留**以便重试。
  - 已认证：仅 `paired` 才显示会话/生物管理。
  - 扫码连接跳过静默认证（自带一次性 token）。
- **任务域 / 工具箱**：门控与 watch 全部改 `isPaired`；工具箱首屏按认证态决定是否拉数据。
- **凭据镜像**：`authenticate()` 与重连成功路径在认证成功后回读宿主凭据写回
  localStorage 镜像（否则下次启动把已换下的旧 token 推回宿主，换新白做）。
- i18n：`hub.status.awaitingAuth` / `hub.awaitingAuthHint` / `hub.cancelConnect`（zh+en）。

## 4. 验证

- 本改动定向：host/任务域 85 用例、integration 43 用例全绿；
- `cd bedcode-mobile && pnpm run test:run` 全量：10:18 本改动单独在树时 91 文件 /
  1091 用例全绿；11:04 与**并发会话**（终端重放世代 / prewarm 线）改动同在树时
  91 文件 / 1102 用例全绿（两侧改动互不冲突，`components.test.ts` 各自改不同
  hunk：本任务只是新增用例，C-C6 prewarm 断言归对方线）。
- `vue-tsc --noEmit`（移动宿主 + bedcode wasm-app）0 error；根 `pnpm exec eslint .` 0 error。
- 真机：无（设备在 09:52 后从 adb 断开，未做修复后实机复验；用户工作区的终端会话
  仍在跑、不便主动断开触发未认证态）。
- 注意：改动**只在本任务文件内**，工作区其余（terminal store / cross-end-tests /
  docs / workflow / schemas / SessionsSection prewarm / plugin.json / rust link.rs）
  均为并发会话在途修改，未触碰。

## 5. 遗留 / 顺带发现

1. **静默换新 JWT 是新增行为**（旧 DevicesView 有、新宿主页漏迁）：连接后自动用宿主
   已存凭据认证，失败才落入配对口。若产品上希望「手动连接必须重新配对」，把
   `connectDevice` 的 `silentAuth` 默认改为 false 即可。
2. `host_events.emit failed ... event names: "plugin:com.bedcode.terminal-session:terminal-state"`
   —— 插件经 host-events 发的 `terminal-state` / `terminal-resync` 事件名含 `.`，
   被 Tauri 事件名校验拒绝（移动端日志反复出现，与本次改动无关，待单独修）。
3. `packages/plugin-sdk-mobile/dev-shell/src/mock/mobile-api.ts` 缺
   `readAllSettings` / `writeSetting`，dev-shell 的 `vue-tsc -p dev-shell/tsconfig.json`
   必红（HEAD 已如此，与本次改动无关）。
4. 修复后真机复验待补：设备从 adb 掉线（09:52 后）且用户会话在跑，未演示
   「未认证连接页」动态流程。复验路径：连接未配对设备 → 应见「等待认证」配对口 +
   【取消】按钮，配对/生物成功后进已认证态，任务白名单不再 401。
