# ADR 0053：移动端应用模型——持久化 ≠ 自启动（启动只扫描，点击才启动）

- 状态：**已裁决（2026-10-15，用户）**；实施已落移动端（本 ADR 记录口径与桌面未来对齐方向）。
- 日期：2026-10-15
- 相关：ADR 0046（移动宿主壳 surface-only：业务页面归 wasm app、壳只挂载）、
  ADR 0020（插件身份校验与审批）、ADR 0032（插件分类）、`CONTEXT.md` 术语
  「启用 / 自启动 / 激活」

## 背景

1. **一个偏好承担了两件事**。移动端插件只有 `plugin.enabled.{id}` 一个持久化偏好，
   却被同时当作：① 启动时前端模块预载的门禁（`src/plugin/loader.ts` 的
   `loadAll → loadManifests`）；② 启动时后端 WASM 自动激活的触发（`src-tauri/src/plugin/manager.rs`
   的 `load_all`）。于是「这个应用可用」（持久化）与「这个应用随宿主启动」
   （自启动）被混为一谈——持久化 ≠ 自启动。
2. **无法关闭，且症状可见**。旧插件管理页（`PluginView` 的启停 Toggle）随宿主壳
   （ADR 0046）退役后，前端已没有任何地方写 `plugin.enabled.{id}`；已落盘的
   `true` 因此永久生效，每次开机自动激活。真机日志实证：开机后
   `WASM plugin activated plugin_id=com.bedcode.file-transfer`，并因 file-transfer
   是 peer-net 节点的唯一消费方（`manager.rs::activate_impl` → `peer::net::ensure_node_started`）
   连带拉起 mDNS browse 与监听端口——用户没点过任何应用，后台服务却全起来了。
3. **桌面是另一套模型**。桌面端（内核 `packages/bedcode-wasm-core/src/manager/host/boot.rs::auto_activate_from_persisted_state`）
   按持久化激活状态自动激活业务应用（L3），且桌面宿主前端仍是旧的、启动流程未改。
   桌面插件更接近「常驻服务」，其启动即激活在当前阶段是期望行为。

## 决策

### D1 · 概念分离：启用（Enabled）与自启动（Autostart）是两个独立偏好

- **启用**：`plugin.enabled.{id}`，持久化偏好，语义收窄为「应用可用」。
  **不再驱动启动流程**。
- **错误回退**：插件上报错误时宿主置 Error 并**撤销自启动**（旧行为由「持久化未启用」
  承担该职责，解耦后归 `autostart`）——避免失败插件每次 boot 重试报错；同时继续
  写 `enabled=false` 作为「应用不可用」记录。
- **自启动**：`plugin.autostart.{id}`，新增持久化偏好，语义 = 「启动时自动激活
  该应用」。**缺省即关**（未设置按关处理，读取失败按 fail-safe 关）。
- 两者互不蕴含：启用不等于自启动，自启动也不要求先启用。

### D2 · 移动端启动流程：只扫描，不启动；点击才启动

- 启动只做 `scan_and_load`（扫描已安装 wasm-app、编译并实例化），**不激活任何应用**。
- `PluginManager::load_all` 只对 `auto_start_candidates()`（真源 = `plugin.autostart.{id}`）
  逐个 `activate`；默认集合为空。
- 前端 `pluginLoader.loadAll` 只预载自启动应用的前端模块；其余不预载。
  理由：前端模块加载会执行 `activate()`（注册运行面 / 启动常驻订阅），且
  `pluginLoader.activate` 以「前端是否已加载」短路后端激活——若 boot 预载了
  非自启动应用，点击时后端不会被激活，形成「前端已加载但后端未运行」的悬空态。
- 用户点击壳内应用 → `pluginPreauthorize` → `pluginLoader.activate`（后端激活 +
  前端模块加载）→ 应用运行。停止走 `deactivate`，下次启动不自动复活。
- 卸载同时清理 `enabled` 与 `autostart` 两个偏好（重装即全新，不继承旧开关）。

### D3 · 桌面本次不动，移动是未来对齐方向

桌面保持现状（旧宿主前端 + 持久化激活自动拉起的启动流程）。待桌面宿主前端改造时，
按本 ADR 的口径对齐：以「自启动偏好」而非「启用偏好」作为 boot 激活触发，
默认不自动启动。**本 ADR 不改桌面任何代码，也不承诺时间点。**

### D4 · 不改 WIT / ABI / 跨端协议

两个偏好都只是宿主侧持久化设置（`mobile_settings.json`），不新增插件可见能力，
不动 WIT 契约、ABI 版本与跨端协议。

## 命令面

新增（移动端，供壳的应用管理 UI 使用）：

- `plugin_is_autostart(pluginId)` → 查询自启动偏好（缺省 false）
- `plugin_set_autostart(pluginId, autostart)` → 写自启动偏好（只改偏好，下次启动生效，不做隐式启停）

既有 `plugin_is_enabled` / `plugin_set_enabled` 保留，语义为「应用可用」偏好。

## 后果与未决

- **已解决**：开机不再因历史 `enabled=true` 自动拉起 file-transfer / peer-net / mDNS；
  `autostart` 默认关满足「应用启动只扫描安装的 wasm-app，不启动」。
- **未决（后续批次）**：移动壳内暂无「开机自启」开关 UI，自启动偏好目前只能经事件/命令面
  或直接改 `mobile_settings.json` 设置；`plugin.enabled` 目前没有运行时消费者（保留为
  应用可用性记录），待壳的应用管理 UI 绑定或另行裁决是否退役。
