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
