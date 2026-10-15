# 移动端连接页 / 会话页「旧宿主形态还原」（2026-10-15）

## 目标（用户指令）

在**当前 wasm-app（`bedcode-mobile/wasm-apps/bedcode/`，插件 id `com.bedcode.terminal-session`）**
内还原旧宿主（`407e35e81:bedcode-mobile/src/views/{DevicesView,SessionsView}.vue`）的：

1. 样式布局（页头 / 底部双 CTA / 发现面板 / 连接历史 / 会话配置卡 / 会话行 / 二次确认弹窗）
2. 连接流程（手动连接必配对；历史 / 发现允许存量凭据静默换新；按 `preferredAuthMethod`
   选认证弹窗；扫码「连上 → 等 isConnected → QR token 认证」；关闭弹窗 = 断开）
3. 会话 ↔ 连接联动（认证成功 → 记历史 + 拉会话；会话页未连接空态 → 去连接；
   已认证 → 连接页「会话」入口）
4. 修复设置页「首选认证方式不回显」（应默认选中配对码）

## 关键改动

| 文件 | 改动 |
| --- | --- |
| `wasm-apps/bedcode/src/settings/SettingsPage.vue` | **回显 bug 修复**：分段按钮原比较 `item.key === opt.value`（设置键 vs 选项值，恒 false）→ 改 `ctrl.values[item.key] === opt.value`，并加 `aria-pressed`（a11y + 可断言） |
| `wasm-apps/bedcode/src/host/components/DevicesSection.vue` | 整体重写为旧 DevicesView 形态 + 旧连接流程（见下） |
| `wasm-apps/bedcode/src/host/components/SessionsSection.vue` | 整体重写为旧 SessionsView 形态（折叠配置区 / SessionCard / 二次确认 / 终端上限） |
| `host/components/{PairingInput,BiometricAuthDialog}.vue` | 自旧宿主同名组件迁入（文案改本域 `hub.*`） |
| `host/components/{SessionCard,SessionConfigCard}.vue` | 自旧宿主同名组件迁入 |
| `host/components/ScanPanel.vue` | 自旧宿主 `ScanPanel.vue` 迁入（载荷解析改走本域 `qr.ts`） |
| `host/components/ui/{Modal,Button,BottomSheet,LoadingDialog}.vue` | 自旧宿主同名通用组件迁入（BottomSheet 文案改 props 传入） |
| `host/components/QrScanner.vue` | **删除**（被 ScanPanel 取代） |
| `host/useRunTime.ts` | 自旧宿主同名言迁入（会话运行时长） |
| `host/useHostPage.ts` | 新增 `addHistory` / `removeHistory` / `mobileSettings` 投影 |
| `host/utils.ts` | 新增 `parseAddressWithDefault`（未写端口回落默认端口）+ `formatLastConnected`（相对时间） |
| `host/i18n.ts` | 按旧文案补齐 `hub.*`（128 键 双语全等）；删除随新界面引入、现已无引用的键 |
| `host/__tests__/*` | 契约按还原后形态重写（组件 12 例 / 纯函数 17 例 / composable 25 例 / 错误 key 锁 18 例） |
| `settings/__tests__/settingsPage.test.ts` | 新增：认证方式回显 3 例（默认配对码 / 落库回显 / 点击落库） |
| `app/AppRoot.vue` | 连接页与会话页自带页头 → 应用级头部只给工具箱 / 设置；传 `active`（复刻旧 onActivated/onDeactivated）；接 `open-connection` |
| `packages/plugin-sdk-mobile/src/types.ts` + `src/plugin/index.ts` + dev-shell mock + testKit | mobileApi 增 `addToConnectionHistory`（旧 DevicesView 认证成功记一笔的落点） |

## 与旧实现的有意差异（评审重点）

1. **扫码面板未迁**「我的二维码」信息弹窗（需宿主平台投影；插件侧暂无）——与旧 `QrScanner`
   遗留项同口径。
2. **历史写入时机**收敛为「认证成功」；旧实现在 `requestPairing()` 返回就写（配对码尚未验证
   就记了一笔）。
3. **会话可见性判据**用 `isPaired`（已认证）而非 `isConnected`（传输层）：未认证时受保护
   端点 401，列会话只会得空列表 + 报错噪音。
4. **打开终端预热**用 `terminal-session.prewarm`（只建连）而非旧 `subscribe`（整段回放，
   预热期页面通道未登记 ⇒ 注定丢弃，长会话可达数 MB）。
5. **新增「已连未认证」兜底**：`watch(..., { immediate: true })` 在重连恢复 / 挂载时已连但
   未认证的场景下按首选认证方式弹认证入口（旧实现该场景无入口 = 配对口不可达）。
6. 认证流程重入闸（`authFlowActive` + 弹窗在场判据）：连接流程与状态 watch 两条入口会同时
   在飞，无闸会重复 `request-pairing` / 弹两个弹窗。
7. 终端上限校验补回（`maxOpenTerminals`）：旧在宿主 composable，新界面改走插件命令时丢失；
   现由会话页启动前校验（运行中会话数 ≥ 上限 → toast 拒绝）。

## 验证证据（本地实跑）

- `bedcode-mobile && pnpm run test:run` → **92 文件 / 1122 用例全绿**
- `pnpm exec vue-tsc -p wasm-apps/bedcode/tsconfig.json --noEmit` → 干净
- `bedcode-mobile && pnpm exec vue-tsc --noEmit`（宿主前端）→ 干净
- 仓库根 `pnpm exec eslint .` → **0 error**（98 warning，均为既有基线）
- 未纳入自动化门禁：真机 / 浏览器手工核验（未跑）；`gradlew`（未改 Kotlin，不涉）；插件完整
  构建含 wasmHash（未跑，改动只在前端）

## 遗留 / 后续

- `packages/plugin-sdk-mobile/dev-shell` 的 `mock/mobile-api.ts` **缺 `readAllSettings` /
  `writeSetting`**（dev-shell 未进 CI 类型检查，属**既有**缺口，非本次引入）→ 已记 scratchpad。
- 未跑真机核验：连接页三态（已连 / 扫码 / 发现）、配对与生物弹窗、会话启动上限、设置回显。
