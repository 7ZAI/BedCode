# 修复：插件二次启用后视图持有停用前旧 context（通道令牌已回收）→ 会话数据加载失败

2026-09-25 · 桌面端前端修复（无 Rust 改动）

## 现象

Terminal Session 插件「启用 → 会话视图打开（修改/保存配置）→ 停用 → 再启用」（或 dev
watcher 热重载、重启后再次启用）后，Session Center 视图 `onMounted` 的 `load()` 失败，
用户看到 toast「会话数据加载失败」+「发生了未知错误」。

## 日志证据（修复前）

```
WARN bedcode_lib::wasm_core::security::frontend_channel: [PluginChannel] 缺少有效通道凭证，插件面命令被拒绝 plugin_id=com.bedcode.terminal-session   ×11~12
ERROR frontend: [GlobalError] mounted hook: execute@http://localhost:1420/src/plugin/context.ts:73:20
```

临时在 `api_bridge::plugin_invoke` 加 TEMP-DIAG（已移除）实锤：被拒命令
`session.config.list` / `session.list` 携带的是**停用前旧令牌**（cred_tail 与本次激活新签发
令牌不同）——重新激活时新令牌已签发（`插件前端通道令牌已签发`），但视图用的仍是旧 context。

## 根因

- `PluginViewHost` / `PluginSettingsSection` 只在各自 `setup` 里 `provide('pluginContext', …)`；
- 旧实现的 props 变化 `watch` 里 re-provide 是**死代码**：Vue 的 `provide()` 只能在
  `setup` 同步调用，watch 回调里调用实测不生效（Vue 3.5，用 vitest 探针验证）；
- 插件二次激活换新 context 对象时，宿主路由组件实例复用（setup 不重跑）、props 未变
  （watch 不触发）→ 子树（重挂载后的视图）注入的仍是停用前 context，其通道令牌已在
  deactivate 时回收 → 插件面命令全员 `缺少有效通道凭证`。

## 修复（全部前端）

1. `src/plugin/registry.ts`：新增 `contextsIndex` 响应式投影（同 `viewsIndex` 模式）
   + `contextIdentity(pluginId)`（WeakMap 稳定对象 id，setContext/clearPlugin/clearContext
   时重建投影）+ `clearContext(pluginId)`（激活失败回滚用）。
2. `src/plugin/components/PluginContextProvider.vue`（新）：`setup` 里读当前 registry
   context 并 `provide('pluginContext', …)`，作 keyed Provider 用。
3. `PluginViewHost.vue` / `PluginSettingsSection.vue`：内容改由
   `<PluginContextProvider :key="registry.contextIdentity(pluginId)">` 包裹——context 对象
   身份变化 → Provider 重挂载 → setup 重新 provide 最新 context；无 context 时不渲染。
   删除死的 watch-provide。
4. `src/plugin/loader.ts`（四处：loadFrontendOnly / loadFrontendForAlreadyActivated /
   loadInline / reloadPlugin）：`registry.setContext(id, context)` 移到 `module.activate()`
   **之前**（activate 期间重新注册贡献面、宿主重挂载视图时能取到新 context）；失败分支
   `clearContext` 撤下预登记。

## 验证

- 新增回归测试 `src/__tests__/plugin/PluginViewHost.test.ts`（4 例：初始注入 / 停用摘除 /
  二次激活注入新 context / 同对象重设不抖）+ `registry.test.ts` 新增 `contextIdentity`
  响应式契约（3 例）；`SettingsView.test.ts` fixture 补齐真实时序的 setContext。
- 全量前端：84 文件 / 814 用例全绿（vitest --maxWorkers=2 防 OOM）；eslint 0 error。
- E2E（instrumented dev 二进制 + vite 直启）：会话视图打开状态下 touch 插件产物触发
  dev watcher 重载 → 重挂载视图 `config.list`/`session.list` 使用新签发令牌，全run
  0 `缺少有效通道凭证` / 0 GlobalError。
- Rust 侧零改动（api_bridge TEMP-DIAG 已复原，git diff 为空）；`cargo test` 本次未跑
  全量：dev 工作区存在并发 agent 在途未提交的 peer_net 重构（编译红，与本次修复无关，
  未触碰）。

## 遗留

- 移动端同构检查未做（移动端未改；若移动端存在同款 PluginViewHost 需同步评估——
  桌面端确认无）。

## 回归与再修复（同日深夜）

**现象**：本修复上线后，点击侧边栏切换菜单（进出插件视图）时主区域**永久白屏**，
后续点击亦不恢复。

**日志证据**（dev `frontend.*.log`）：

```
[Vue warn]: Component inside <Transition> renders non-element root node that cannot be animated.
  at <PluginContextProvider key="ctx:1" plugin-id="com.bedcode.terminal-session">
  at <PluginViewHost pluginId="com.bedcode.terminal-session" viewId="session.pairing">
  at <KeepAlive max=8> at <BaseTransition mode="out-in"> at <Transition name="page" mode="out-in">
  at <RouterView> at <DesktopLayout> at <App>
```

**根因**：本修复引入的 `PluginContextProvider` 上一版模板只有裸 `<slot />` → 渲染根是
Fragment。宿主路由出口是 `<Transition name="page" mode="out-in">`，Vue 把 transition
hooks 沿 `renderComponentRoot` 逐层下传、最终挂在根 vnode 上——**Fragment 根既不参与
`unmount` 的 leave 流程，也没有可等待的元素** → out-in 的 afterLeave 永不触发 →
新页面永不挂载（白屏）。即：插件二次激活修复副作用（透传组件）踩了「Transition 子组件
必须渲染元素根」这条 Vue 硬约束。

**修复（仍全在前端）**：

1. `PluginContextProvider.vue`：加真实元素根 `<div :class="props.rootClass">`；
   `rootClass` 默认 `contents`（`display: contents` 不生成布局盒子 → 设置分组等纯内容
   场景布局零影响），`PluginViewHost` 传 `root-class="h-full"`（参与页面过渡动画，
   同时保证插件视图根 `h-full` 有高度可依）。
2. 文件头说明移到**模板之外**：模板根级 HTML 注释在 dev 编译下被保留，会与 `div`
   构成多根（Fragment），把根重新变成非元素节点——同一坑的第二种触发方式，已写进注释。
3. `PluginViewHost.vue` 增补 `root-class` 的取舍说明。

**验证**：`src/__tests__/plugin/PluginViewHost.test.ts` 新增 3 条锁——①Provider 渲染根
vnode 为元素（`typeof type === 'string'`，Vue `isElementRoot` 同判据）；②对照：裸 slot
透传组件的根不是元素（证明①有区分度，防止恒真断言）；③真实 `Transition(mode=out-in)`
挂载不产生 `non-element root node` 警告且内容可见。变异自检：把根改回裸 `<slot />` →
①③转红、②仍绿；还原后 7/7 绿。关联：`registry.test.ts` / `SettingsView.test.ts` 同跑绿。

## 多窗口互踩：终端窗口加载回收主窗口凭证（2026-09-26）

同族问题的第二个触发面：不是「插件重新激活换 context」，而是**另一个 webview 的页面加载
把主窗口的凭证整体回收**。

**现象**（用户报告）：桌面端启动终端会话（打开终端窗口）后，点击关闭报错。日志：

- `[terminal-session] Failed to resolve background image URL: {}`（独立小 bug，见下）
- `[PluginChannel] 缺少有效通道凭证，插件面命令被拒绝 plugin_id=com.bedcode.terminal-session` ×N
- 前端 `[GlobalError] unhandledrejection: execute@src/plugin/context.ts:73`

**日志证据**（`~/.local/share/com.bedcode.app/logs/`，UTC 时间，本地 +8）：

```
18:38:10.26  session created via host-pty (session_id=0a928b03…)      ← 主窗口「启动会话」成功
18:38:12.872 [PluginChannel] 前端通道会话已重置 had_loader_session=true revoked_tokens=4   ← 终端窗口页面加载
18:38:12.908 [useSessionWindows] Window stored, keys: ["0a928b03…"]
18:38:13.189 [PluginChannel] 前端 loader 会话密钥已签发（仅一次 = 终端窗口自己的 bootstrap）
18:38:45.405 [PluginChannel] 缺少有效通道凭证，插件面命令被拒绝 plugin_id=com.bedcode.terminal-session
18:39:05 / 08 / 15  同上 ×3
```

`revoked_tokens=4` 证明被打掉的是主窗口的 4 个插件令牌，而重置后只有**一次**重新签发
（终端窗口自己的 bootstrap）——主窗口不会重新 bootstrap（页面没重载）→ 此后主窗口所有
插件面命令全被拒。

**根因**：`FrontendChannelRegistry` 是 app 级全局单例（loader 密钥 + 令牌表各一份），而
`on_page_load` 钩子对**每个 webview** 触发（`lib.rs`）。终端窗口是独立 `WebviewWindow`
（`/terminal-window/:sessionId`），其页面加载把主窗口凭证整体回收——修复前的 `reset()`
无 webview 维度，是「单页面加载」假设的残留。

**修复（Rust，锚点 = 按 webview 分区）**：

1. `security/frontend_channel.rs`：凭证表加 webview label 维度——
   `loader_sessions: HashMap<label, key>`、`tokens: HashMap<label, HashMap<token, plugin_id>>`、
   `plugin_tokens: HashMap<(label, plugin_id), token>`；`reset(webview)` 只清该域；
   `resolve` / `authorize` / `issue_*` / `verify_*` 全部带 label；`revoke_plugin` 跨所有域
   回收（插件可在多窗口各有前端实例）。安全语义不降级：域内仍「首个调用者生效」，
   跨域凭证不可解析（比全局表更严）；同 label 窗口重建由新的 `on_page_load` 重置旧域。
2. `manager/host/api_bridge.rs`：6 条插件面命令注入 `tauri::Webview` 参数（Tauri 保证是
   调用发起方所在窗口），按 `webview.label()` 定位凭证域；日志补 `webview` 结构化字段，
   再出同类问题可直接从日志看出是哪个窗口被拒。
3. `lib.rs`：`on_page_load` 传 `webview.label()`。
4. `manager/host.rs`：`reset_frontend_loader_session(webview_label, reason)`。

**次因（独立小 bug，一并修）**：`Failed to resolve background image URL` —— 宿主
`src/plugin/terminal-host-capabilities.ts` 的 `getServerPort` 硬编码 8080，而
`/static/terminal-bg` 挂在宿主主服务器上（本机 8767，配置 `network.port`）→ URL 永远
不可达。修复：打开窗口时预取 `get_server_status` 写入 ref（getter 同步返回，插件的
`resolveBgImageUrl` 是同步调用）；背景图保存前先刷新端口（插件侧 watch 在设置落盘后
**同步**解析 URL，端口必须先就位）；预取失败回落 8080 且不抛（终端视图不因探测失败崩）。

**验证**：

- Rust `frontend_channel` 11 单测绿，含 4 条新增回归锁：
  `page_load_in_one_webview_keeps_other_webviews_credentials`（本次 bug 的正面复现）、
  `credentials_do_not_cross_webview_domains`、`revoke_plugin_covers_every_webview_domain`、
  `recreated_window_with_same_label_gets_fresh_session`。
  变异自检：把 `reset` 改回全局清空 → 3 条转红（含回归锁），还原后 11/11 绿。
- 前端新增 `src/__tests__/plugin/terminalHostCapabilities.test.ts` 4 用例（端口实值 /
  预取失败回落 / 保存前刷新顺序 / 无关保存不刷新）。变异自检两轮：getter 回硬编码 →
  C1+C3 红；去掉保存前刷新 → C3 红；还原后 4/4 绿。
- `src/__tests__/plugin/` 8 文件 42 用例绿（`channelIdentity.test.ts` 前端契约面不变——
  webview 由 Tauri 注入，前端参数零改动）。
- `cargo check --lib` / `cargo clippy --lib` 无新增告警；eslint 0 error；改动 Rust 文件全 LF。
- 全量回归：Rust 全量 + 前端全量（结果见当日 memory 2026-09-26）。

**遗留**：webview 关闭时不主动清理其凭证域（仅靠同 label 重建时的 `on_page_load` 重置）；
窗口对象销毁后无人持有旧令牌，风险可接受，如后续要收紧可在窗口事件里
`reset_frontend_loader_session(label, "window-closed")`。

