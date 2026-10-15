# 移动端真机反馈修复（2026-10-15：第二轮 5 项 + 第三轮追加 1 项，见 §6）

> 触发：用户真机测试反馈 5 条（1 认证 / 1 会话页 UI / 1 文件传输设置页 / 2 系统返回键）。
> 起点提交：`033b3e0e2`（连接页 / 会话页旧宿主形态还原）。

## 1. 连接历史连不上免配对（每次都要重新输入配置码）

**根因（两层，都要修）**：

1. **凭据镜像断链（主因）**：认证编排已迁插件命令面（票 14），
   `terminal-session.verify-pairing-code` / `authenticate-with-biometric` / `authenticate-with-qr`
   都不再经过宿主 `useMobileConnection` 的 `verifyPairingCode` / `authenticateWithBiometric`，
   而 localStorage 镜像只在那两处写。结果：宿主 Rust 侧持有 JWT，但前端镜像
   （`auth_session_token` / `auth_pairing_id` / `auth_fingerprint`）**永空** ⇒
   `authenticate()` 一进门就 `No stored credentials, skipping auth -> false` ⇒
   连接历史走「存量凭据静默换新」必然失败 ⇒ 弹配对口；重启后 `wsSetToken` 也无源。
   修法：`useMobileConnection.onPaired` 里经宿主窄读口 `ws_get_auth_credentials`
   回读凭据写镜像，再落已配对设备（`refreshCredentialMirror()` 复用既有函数）。
   该回调是**所有认证成功路径的必经点**（宿主 `ws_authenticate` 与插件三条路径都发
   `ws_paired`），所以放在这里一处即覆盖全部。
2. **判据竞态（次因）**：`startConnection` 用 `isAuthenticated`（= `connectionStatus === 'paired'`）
   决定是否弹配对口，但 `ws_paired` 事件到达宿主晚于 `ws_authenticate` 返回 ⇒
   静默换新成功也会被判成未认证而弹配对口。修法：`useHostPage.connectDevice` 返回
   `{ authenticated }`（以本次静默认证结果为准），`startConnection` 据此裁决；
   并加 `connectFlowActive` 闸，禁止「已连未认证兜底 watch」在连接流程在飞期间插手
   （它本来只服务重连 / 挂载恢复场景）。

## 2. 会话页标题右侧刷新图标与会话配置刷新图标重复

删掉页头刷新按钮（`hub.refreshSessions` 的最后一个消费者），保留会话配置区块头的
刷新入口；并删除两语言里已成死键的 `hub.refreshSessions`（含 errors.test.ts 的清单项）。

## 3. 文件传输设置页：标题压进安全区 + 底色跳色

设置页是**平台设置二级页**，但原实现用 `header: false` 裸渲染 + 插件自绘页头：
① Android WebView 里 `env(safe-area-inset-*)` 恒为 0（宿主用 JS 注入值），自绘页头拿不到安全区；
② 页面走 `.ft-app` 蓝灰 app 主题，插在宿主暖色设置流里跳色。
修法：
- 路由改 `header: true`（PluginRouteView 走宿主 `SettingsSubPage`：页头 / 返回 / 安全区 / 底色全归平台）；
- 插件侧删自绘页头，内容套 `.ft-app.ft-host-page`；
- `styles.css` 新增「平台设置页变体」：把 `.ft-host-page` 的结构色 token 重绑到宿主
  `--mobile-*`（强调色与锚点对比色保留应用身份）。选择器带
  `html:not(.dark) .ft-app.ft-host-page` 一条，否则压不住浅色主题的 `html:not(.dark) .ft-app`。

## 4. wasm-app 里系统返回后「应用无界面」/ 点运行中应用也「无界面」

**根因**：`ShellRegistry.clearSource` 连界面贡献一起清，而 `ShellView.onUnmounted`
会在**任何离开壳的路径**（进插件整页路由、WebView 历史回退等）调用它。运行面由
`context.ui.registerSurface` 在 activate 时注册、Disposable 随应用存活，被壳卸载清掉后
再也回不来：重挂只重拉清单（`contributions: {}`），而重新 activate 会因模块已加载
幂等早退 ⇒ 运行屏恒显示「应用尚未提供运行面」。
**修法**：`clearSource` 只清清单记录，贡献归应用运行时所有（随其 Disposable 回收）；
壳卸载不再销毁运行面注册。回归锁：`shellRegistry.test.ts` 两例 +
`useShellAppsSurface.test.ts` 一例。

## 5. 系统返回直接退出应用（设置页返回没回主屏）

**根因**：壳从没接过系统返回键 —— 无 JS listener 时 Android 走默认行为（finish Activity），
而壳内屏幕栈不写 URL（`switchTab` 重置栈、屏间流转不上历史）⇒ 子屏按返回一律退出应用。

**修法（新增平台兜底链）**：
- 新增 `src/plugin/back-button.ts`：宿主前端**唯一**原生 `onBackButtonPress` 监听 +
  两层分发（应用层 `context.ui.onBackPressed` → 平台层宿主壳兜底）。
  处理器返回 `true` = 已消费，链上终止；不消费则继续交给下一层。
  监听**按需持有**：无订阅者即摘除，恢复系统默认返回（主屏返回 = 退出应用）。
- 新增 `src/shell/composables/useShellSystemBack.ts`：兜底顺序
  通知栏 → 胶囊菜单 → 授权弹窗 → 屏栈回退 → 回主屏；都轮不到（在主屏且无覆盖层）
  不消费（交给系统退出）。**按需订阅**：只有确实有可退目标时才持有监听，
  否则常驻订阅会让「主屏按返回退出」永久失效。
- `ShellHost` 接线 + 卸载时 dispose；`FileTransferView` 的返回处理器改为
  已处理分支返回 `true`（否则壳兜底会同时退一屏）、应用内到头返回 `false` 交还平台。

## 验证证据（实跑）

- `bedcode-mobile && pnpm exec vitest run --no-file-parallelism` → **94 文件 / 1153 用例全绿**
  （基线 1122 + 本任务新增 31）
- `pnpm exec vue-tsc --noEmit`（移动宿主）· `-p wasm-apps/bedcode/tsconfig.json` ·
  `-p wasm-apps/file-transfer/tsconfig.json` → 三处 0 error
- 仓库根 `pnpm exec eslint .` → **0 error**（98 warning，与既有基线同数；未新增）
- **变异自检**（逐条证明测试能杀死错误实现）：
  ① 去掉 `onPaired` 的镜像回读 → `pairing-flow.test.ts` 新例红；
  ② `clearSource` 恢复清贡献 → 3 条回归锁红；
  ③ 分发顺序反过来 / 覆盖层不再优先 → `backButton.test.ts` 4 例 + `shellSystemBack.test.ts` 1 例红；
  ④ 修 `back-button.ts` 时发现并修掉真实缺陷：`ensureNative` 误引未定义符号 `dispatch`
     （被 catch 静默吞掉 ⇒ 整个返回机制失效），正是 C-B7/C-B8 用例先红灯暴露的。
- **未跑（手工项）**：真机核验（无 adb 设备在线）——待验：连接历史免配对直达、会话页单刷新入口、
  文件传输设置页安全区/底色、wasm-app 内系统返回退目录与回主屏、壳设置页返回回主屏、
  主屏返回退出应用。插件完整构建 / gradlew（未改 Kotlin）不涉。

## 附带说明（非本任务改动）

- `src-tauri/gen/schemas/{acl-manifests,desktop-schema,linux-schema}.json` 在工作区已有
  未提交改动（新增 `core:app:allow-exit` 权限项，生成物），与本次修复无关，未触碰。

---

## 6. 追加（2026-10-15 真机复验二）：wasm-app 内二级页按返回直接回平台主界面

**现象**：应用内的二级页（BedCode 的终端沉浸页 / AI Chatbox 的供应商配置页）按系统返回，
直接退回平台主界面，而不是先回到该应用的主页面。

**根因**：§5 只补了**平台兜底链**，应用层没人接管——bedcode / ai-chatbox 两个应用都没注册
`context.ui.onBackPressed`（只有 file-transfer 注册了）。系统返回 → 应用层为空 → 落到平台兜底
`nav.back()` → 弹掉 `app-run` → 平台主屏。二级页（如终端）在旧宿主是独立路由页，下沉成应用内
状态后没有对应的返回入口。

**修法（应用自持，AGENTS §6「业务不落壳」）**：

- 每应用一个返回栈模块（`wasm-apps/bedcode/src/app/appBack.ts`、
  `wasm-apps/ai-chatbox/src/utils/appBack.ts`）：按声明顺序找第一层「有可退界面」的层后退一步并
  消费（返回 `true`）；所有层都到头 → 返回 `false`（交平台兜底，不吞返回）。
- bedcode `AppRoot`：一层 = 终端沉浸页（旧宿主 `/mobile/terminal/:id` 独立页的等价物）——
  返回收掉终端回到应用主页面（页签容器，保留进入前的页签）。
- ai-chatbox `ChatView`：三层，优先级 = 插件设置弹层 → 对话列表抽屉 → 供应商配置页；
  配置页经 `defineExpose({ goBack })` 逐级后退（表单/模板 → 列表 → 聊天页），与页头返回按钮同一路径。
- 两应用清单各补 `"ui:back"`（= `ui.onBackPressed` 的权限位；缺失时注册即抛权限错误，
  与 file-transfer 既有声明一致）。**因此真机复验前需重装插件包**（清单变更）。
- SDK `types.ts` 的 `onBackPressed` 文档补一句：应用自持的二级页 / 浮层应由应用逐级收掉，
  回到应用主页面后不再消费。

**验证证据（实跑）**：

- 契约测试各 7 例（正例消费 / 反例到头不消费 / 多层优先级 / 连续返回逐级 / 层内多级 /
  Disposable 直通 / 清单声明 `ui:back`）；宿主 vitest **95 文件 1160 用例 ✓**（基线 1153）、
  ai-chatbox **8 文件 150 用例 ✓**（基线 143）。
- 变异自检（逐条实跑确认变红）：反转 `canGoBack` 条件 / 去掉 `goBack` 调用 / 恒不消费 /
  恒消费 / 反向遍历 / 清单去掉 `ui:back` / ai-chatbox 去掉 `goBack` → **7/7 被杀**。
- `vue-tsc --noEmit`（移动宿主）+ `-p wasm-apps/{bedcode,ai-chatbox}/tsconfig.json` → 0 error；
  仓库根 `pnpm exec eslint .` → 0 error（98 warning，与既有基线同数）。
- 两应用前端 `pnpm exec bedcode-plugin build --frontend-only` → 均成功（导入与打包链路可用）。
- `bedcode-mobile/src-tauri` `cargo test --test retired_mobile_auto_task_plugin_lock` ✓
  （plugin.json 改动不破坏既有清单锁）；未改 Rust，故未跑移动宿主全量 Rust 套件。
- **未跑（手工项）**：真机核验（需重装插件包）——待验：终端页返回回会话页、AI Chatbox 配置页
  返回回聊天页、应用主页面返回仍回平台主屏、平台主屏返回退出应用。

**未触及**：file-transfer 的返回处理（本就在场，语义一致）；其内联实现未改用同一模块
（避免改动在途的重构文件，行为已对齐）。
