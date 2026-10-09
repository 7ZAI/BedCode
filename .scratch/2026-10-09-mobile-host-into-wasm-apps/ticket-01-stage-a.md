# 票 01 · 阶段 A 收尾 + 机制对齐 + QR 迁移记录（2026-10-09）

> 上游 spec：同目录 `spec.md`（v0.1）。用户裁决（2026-10-09）：
> ① 本次只做**阶段 A 收尾 + 验证**，旧宿主暂不动（阶段 B 退役待复核）；
> ② 旧前端机制（国际化 / 错误码 / 日志）必须**正确落到新前端**；
> ③ 遗留的 **QR 扫码配对迁移到 terminal-session wasm-app**。

## 1. 落点清单

| 面 | 文件 | 说明 |
| --- | --- | --- |
| A1 壳默认入口 | `src/router/index.ts` | `/` → `redirect: /mobile/shell`；旧 `/mobile/**` 过渡期保留（B 阶段删） |
| A2 壳注册通道 | `src/plugin/context.ts` | `ui.registerSurface/registerSlot/registerCapsuleItem/registerSettingsEntry`；appId 由宿主代填；运行面经 `PluginViewHost` 包装补 `provide('pluginContext')` |
| A2 契约面 | `packages/plugin-sdk-mobile/src/types.ts`、`src/plugin/types.ts` | 4 个 `Shell*Contribution` + `MobileHostApi` 连接/mDNS/生物扩容 |
| A2 解析优先级 | `src/shell/composables/useShellApps.ts` | ① 应用自带 surface → ② 数据源延迟解析 → ③ 空态 |
| A3 独立页面化 | `wasm-apps/{file-transfer,ai-chatbox}/src/index.ts` + `plugin.json` | 改注册 surface；停 toolbox/navTab；`ToolboxEntry.vue` 删；权限与 views 对齐；入口日志走 `context.logger` |
| A4 宿主页域 | `wasm-apps/terminal-session/src/host/**` | HostPage（设备→会话→终端）+ DevicesSection + SessionsSection + useHostPage + i18n + SlotCard + 测试 |
| A4 引擎事实投影 | `src/plugin/index.ts`、`src/plugin/connection-events.ts` | mobileApi 扩容：连接态/历史/动作、mDNS 原始事实、生物凭证状态、连接生命周期事件白名单 6 项 |
| A5 壳面接线 | `host/activate.ts` + `host/SlotCard.vue` | 运行面 + 首页「活跃会话」槽位（`host-sessions`，order 10） |
| **机制·错误码** | `host/errors.ts`（新） | `classifyConnectionError`（与 `src/utils/connectionError.ts` 逐字同源）+ `CONNECTION_ERROR_KEYS`（分类 → `hub.*`）+ `ensureCommandOk`（`{code,message}` 归一） |
| **机制·日志** | `host/useHostPage.ts` + 三个组件 | `context.logger`（`[host]` 前缀）；所有 catch 落日志（消除静默 catch / 未处理 rejection） |
| **QR 迁移** | `host/qr.ts`（新）、`host/components/QrScanner.vue`（新）、`DevicesSection.vue`、`useHostPage.authenticateWithQr` | 相机扫描 + 相册识别 + 照明 + 错误态 + 结果卡确认；`html5-qrcode@^2.3.8` 入插件依赖 |
| **缺陷修复（真实运行期 bug）** | `DevicesSection.vue`、`SessionsSection.vue` | 模板引用了 **9 个未声明的绑定**（`mdnsScanning` / `connectionHistory` / `isConnecting` / `connectingName` / `isAuthenticated` / `currentDevice` / `biometric` / `connectionStatus` / `pairingStage`）⇒ Vue 运行期 warn + 整块渲染缺失（mDNS 按钮、历史、配对、断开、生物全受影响）。修法：显式 `computed` 收口（SDK 的 `import('vue').Ref` 与本包 vue 类型跨包，模板静态解引用不生效，需本组件 computed） |
| **测试补齐（失败面）** | `host/__tests__/components.test.ts`（新，6 例） | 挂载两个区块组件断言真实渲染 + 动作；composable 单测覆盖不到模板绑定，正是上面漏检的面 |

## 2. 数据面映射（对齐 spec §2.4）

| 旧宿主数据面 | 现通道 | 状态 |
| --- | --- | --- |
| 会话列表/起停删/直发输入 | `context.commands`（票 13）+ `mobileApi.activeSessions/sessionConfigs` | ✅ |
| 终端输出/输入/resize | `terminalView` + `mobileApi.openTerminalStream` | ✅（页内嵌 `TerminalView`，`back` prop 覆盖返回） |
| 配对/认证编排 | 插件命令 `request-pairing` / `verify-pairing-code` / `authenticate-with-qr` / `authenticate-with-biometric` + `context.events` | ✅ |
| **QR 扫码配对** | `QrScanner`（插件内 html5-qrcode）→ 结果卡确认 → `connectDevice` + `authenticate-with-qr` | ✅ 本轮迁入 |
| 连接生命周期事件 | `mobileApi.onConnectionEvent`（白名单 6 事件） | ✅ |
| mDNS 主机发现 | `mobileApi.mdnsServices/mdnsScanning/mdnsStart/mdnsStop`（原始事实投影；派生列表归插件） | ✅（D6 选 (a)，零 WIT） |
| 生物凭证状态 | `mobileApi.getBiometricKeyStatus/bind/unbind`（C4：只投状态与动作） | ✅ |
| 设置写面 | 仍留宿主命令，壳设置页调用（D5 选 (a)） | ✅ 未动 |
| 凭据镜像（token） | 红线：凭据零过境，留宿主 localStorage 镜像 | ✅ 未动 |

## 3. 机制对齐明细（旧前端 → 新前端）

| 机制 | 旧实现 | 新前端落点 | 差异说明 |
| --- | --- | --- | --- |
| 国际化 | `t('mobile.*')`（旧 locales）；插件经 `context.i18n` | 壳 `t('shell.*')`；宿主页 `context.i18n.t('hub.*')`；错误槽位存「i18n key 或原始文案」→ 渲染一律 `t(...)` | 键名域化（`hub.*`），**无硬编码文案**；双语文案树键集合由测试钉住（`activate.test.ts` C-P6 + `errors.test.ts` C-ERR-7） |
| 错误码 | `@/utils/connectionError.ts::classifyConnectionError` → `mobile.connection.{timeout,refused,unreachable}Toast`；业务 `code!==0` → `toast(message \|\| i18nKey)` | `host/errors.ts` 同源分类表（timeout/refused/unreachable/other）+ `CONNECTION_ERROR_KEYS` → `hub.*`；`ensureCommandOk` 归一 `{code,message}` | 插件不得 import 宿主 `@/`，故**复制**机制（同源契约由测试锁）；分类顺序、中文「超时」兼容、大小写敏感逐字保留 |
| 日志 | `logger`（frontendLogger）带 `[DevicesView]` 等上下文；catch 必 `logger.error` | `context.logger`（落宿主 tracing）带 `[host]` 前缀；`useHostPage` + 三个组件 + 扫码面板 catch 全覆盖 | 插件侧无 frontendLogger（属宿主模块），统一走 SDK logger；热路径（扫码逐帧）不记日志 |
| 壳层 | — | `src/shell/**` 原本已合规（L4 无静默 catch / L5 模板无中文 / `[Shell*]` 日志），本轮零改动 | 审计结论 |
| 类型检查 | 宿主前端 `vue-tsc`（build 脚本内） | 插件 `build` 脚本**不做类型检查**（`bedcode-plugin build` 只跑 vite）⇒ 本轮补跑 `npx vue-tsc --noEmit -p tsconfig.json`，修完 **0 error** | 后续改插件前端建议同法自查（构建绿 ≠ 类型正确） |

**QR 迁移安全口径**：二维码原文含 token ⇒ 只记解析结论（`reason`）不记原文；token 仅内存持有，连接 + 认证成功后立即清空；不使用 `console.*`（dev-shell mock 的历史 `console.log` 不属生产路径，未动）。

## 4. 验证证据（本轮实跑）

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 宿主页域测试 | `pnpm exec vitest run wasm-apps/terminal-session/src/host` | **6 文件 / 77 用例全绿**（+ 任务域 `src/task/__tests__` 3 用例） |
| 移动端全量 | `cd bedcode-mobile && pnpm run test:run` | **77 文件 / 831 用例全绿**（首次 75 文件全量时 1 例 mdns 用例重负载 flaky，单跑 + 复跑全量均绿） |
| eslint | 根 `pnpm exec eslint .` | **0 error**（110 warning 存量） |
| 插件类型检查 | `npx vue-tsc --noEmit -p tsconfig.json`（插件目录） | **0 error**（修掉 35 处模板绑定/解引用错误） |
| 插件产物 | `node scripts/plugin-build.js --plugin com.bedcode.terminal-session` | 成功；`index.js` 936 KB → **1.325 MB**（含 html5-qrcode）；产物内含 `hub-qr-reader`/`qrConnect` |
| 插件依赖 | `pnpm install`（插件工作区） | `html5-qrcode 2.3.8` 入插件 `package.json` + lock |
| 变异自检 | 5 处注入（分类顺序 / key 表 / 端口越界 / `accepted` 忽略 / 删 `connectionHistory` 绑定） | **5/5 变红**（连带用例共 10 例；删绑定复现了原 bug 的 Vue warn + 渲染缺失），还原后全绿并核 sha256 |
| Rust 侧 | 本票零 Rust 改动 | Rust wasm 仅因 `include_str!("../../plugin.json")` 随 manifest 重建 |
| 手工验证（未跑） | 真机相机扫码 + 壳默认入口 + 三 app 流转 | **未跑**：本机无 Android 设备 |

## 5. 遗留与下一步

**阶段 B 前置已完成**：任务域 UI 不再只挂旧宿主工具箱 —— 现注册壳内动态路由（`tasks`，host 页头模式自带返回）+ 应用胶囊菜单项（`task-page` → `ui.openPage('tasks')`），manifest 增 `ui:route` 权限；`wasm-apps/terminal-session/src/task/__tests__/activate.test.ts` 钉住（变异 2/2：路由 id / openPage 目标）。

**阶段 B 前置 ①（已完成）· 壳设置门类补齐**：`ShellSettingsScreen` 平台设置由 4 项扩到 **7 项** —— 新增「连接 / 认证 / 出站策略」三项（此前壳内无入口，删旧设置必造成功能回归），沿用既有设置页路由（`mobile-settings-connection|authentication|egress`，壳不重复实现引擎设置）；文案双语（`shell.settings.{connection,connectionHint,authentication,authenticationHint,egress,egressHint}`）；新增 `src/__tests__/shell/shellSettingsScreen.test.ts`（4 例：七项在场 / 新增三项路由 / 既有项口径不漂移 / 文案双语）+ `vitest.config.ts` 补 `define.__APP_VERSION__`（测试挂载壳屏必需，与 vite.config 同源）。变异 1/1（连接项跳错路由 → C-ST2 红）。

**阶段 B 退役的剩余阻塞（需裁决）**：
- **文件浏览器归属未定**：`CodeExplorerView`（`/mobile/files/:id`）需定 wasm-app 归处（file-transfer 还是另起 app）；`PresetTasksView` 与插件任务域页签（任务记录 / 定时任务）功能重叠，可随退役删除。
- 其余（设备 / 会话 / 终端 / 插件管理 / 任务 / 设置）已全部有壳内或插件内等价面 ⇒ 下一批可删：`MobileSwipeContainer` / `MobileNav` / `views/{DevicesView,SessionsView,TerminalView,PluginView,SettingsView,PresetTasksView,ToolboxView}`，并加防回接锁（路由名 / 视图符号 / 命令字面量）。

1. **「我的二维码」信息弹窗未迁**：旧 `ScanPanel` 底部该入口展示本机平台 + 默认端口（依赖宿主 `usePlatform` / `useMobileSettings` 投影），插件侧暂无平台投影 ⇒ 需要时先补 `mobileApi` 平台投影再迁（信息展示型，不阻断扫码主链路）。
2. **旧宿主过渡面**：`ToolboxView`（CodeExplorer）/ `SettingsView` 仍留旧路由（spec D1）。
3. **阶段 B（另票）**：旧 `/mobile/**` views + `MobileSwipeContainer` + 旧嵌入面退役删除；防回接锁（路由 / 视图符号 / 命令字面量）；锁索引 / code-map / CHANGELOG 双语同步。**需先真机复核阶段 A（D2 待裁决）**。
4. **真机核验清单**：壳默认入口可进；terminal-session 整体页「设备→会话→终端」全链路；**扫码配对（相机权限 / 相册 / 照明）**；file-transfer / ai-chatbox 独立页面不回归。

## 6. 阶段 B 退役执行记录（2026-10-09 收尾，handoff 交接）

**退役删除（已执行）**：
- 旧宿主视图/组件：`src/components/{MobileLayout,MobileNav,MobileStatusBar,MobileSwipeContainer}.vue`、
  `src/views/{DevicesView,SessionsView,TerminalView,PluginView,SettingsView,PresetTasksView,ToolboxView}.vue`、
  孤儿组件（`ScanPanel` / `BiometricAuthDialog` / `PairingInput` / `PresetTaskCard` /
  `SessionConfigCard` / `SessionCard`）、`ToolboxEntry.vue`（ft 旧工具箱入口）。
- 随退役面失效的测试/夹具：`src/__tests__/views/{devicesViewScanFlow,mobileSwipeStride,
  pluginToggleConvergence,toolboxDeepChild,toolboxKeepAlive,toolboxViewSync}.test.ts`、
  `src/__tests__/integration/fixtures/{toolboxDeepChildHost,toolboxKeepAliveHost}.vue`。
- `App.vue` → 渲染 `<router-view />`（壳自承布局框，见 `ShellView.vue` 100dvh/安全区/祖先类框）；
  `src/router/index.ts` 删旧宿主路由（仅留 `/`→`/mobile/shell`、`/mobile/files/:id`（CodeExplorer 归属待定）、
  `/mobile/settings/*`（子页，壳跳转链接受）、`/mobile/shell`）。
- **退役 `registerSettingsSection` 全链路**（旧宿主设置区已无消费方）：`packages/plugin-sdk-mobile/src/types.ts`、
  `src/plugin/{context,registry,permission,types}.ts`、`dev-shell/{registry,mock-context,PluginsView}.vue`、
  fixture `mockLifecyclePlugin.ts`、`pluginReactivate.test.ts`、ft `src/index.ts`
  （改 `registerSettingsEntry` → `ui.openPage('settings')`）。
- `useLinkEncryption.test.ts` 改扫描 `wasm-apps/terminal-session/src/host/{utils.ts,SessionsSection.vue}`。
- 更新陈旧注释（ShellHost/ShellSheet/ShellTabbar 内对 MobileLayout/MobileNav 的引用说明）。

**防回接锁（新增）**：`src/__tests__/shell/retiredHostUIRetirementLocks.test.ts`（R1 路由名 /
R2 符号 / R3 壳等价物在场正面钉；跳注释、词边界/引号字面量、排除锁文件自身；覆盖全 src/ 含测试）。
变异自检 2/2：探针含 `mobile-devices` + `MobileSwipeContainer` → R1/R2 红；删探针还原 4/4 绿。

**验证证据（2026-10-09 实跑）**：

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 移动端全量 | `cd bedcode-mobile && NODE_OPTIONS=--max-old-space-size=4096 pnpm run test:run` | **72 文件 / 812 用例全绿**（含新 R 锁 4 例；用例数 835→812 系 6 个退役面测试文件删除的预期差）；上一轮 `useMdnsDiscovery.stopDiscovery` 1 失败**未复现**，确认重负载 flaky 而非真回归 |
| eslint | 根 `pnpm exec eslint .` | **0 error**（99 warning 存量） |
| 插件类型检查 | `cd wasm-apps/terminal-session && npx vue-tsc --noEmit -p tsconfig.json` | **0 error** |
| SDK 重建 | `pnpm --filter @binblink/bedcode-plugin-sdk-mobile build` | dist 重建成功（index.js 3.68 KB + dts） |
| 三插件产物 | `node scripts/plugin-build.js --plugin com.bedcode.{terminal-session,file-transfer,ai-chatbox}` | 三插件全部构建成功 |
| 防回接锁 | `npx vitest run src/__tests__/shell/retiredHostUIRetirementLocks.test.ts` | 4/4 绿；变异自检 2/2 红/绿一致 |
| 手工验证（未跑） | 真机核验壳默认入口 + 三 app 流转 | **未跑**：本机无 Android 设备 |

**遗留（非本阶段阻塞）**：
- `CodeExplorerView`（`/mobile/files/:id`）归属未定：路由 + 组件保留，待用户裁决（file-transfer vs 另起 app）。
- 插件侧旧嵌入面注册仍服役（非宿主回接）：task 域 `registerToolboxPage`（任务工具箱页，宿主无消费容器，
  数据源延迟解析兜底路径仍消费 registry）与 terminal 域 `registerTerminalView`（`pluginAppSource` ② 兜底）——
  机制保留，未列退役面，后续独立评估。
- 真机核验清单不变（壳默认入口 / 设备→会话→终端全链路 / 扫码配对 / 三 app 流转）。
