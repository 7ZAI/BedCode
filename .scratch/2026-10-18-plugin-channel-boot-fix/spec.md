# 插件通道凭证 + 启动期授权时序修复（2026-10-18）

来源：Windows 正式版 v3.0.0（安装于 `C:\BedCode`）首次运行全面报错排查（见
`.scratch` 同批会话记录与 `~/.pi/agent/memory/daily/2026-10-18.md`）。

## 1. 现象（Windows 正式版实测日志）

`%LOCALAPPDATA%\com.bedcode.app\logs\runtime.2026-10-17.log` 三次启动（20:31 / 20:32 / 20:38）全部同型：

```
20:32:39.046737 [PluginChannel] 前端通道会话已重置 webview=main reason=page-load   ← Started
20:32:39.127006 [PluginChannel] 前端 loader 会话密钥已签发（宿主面凭证）            ← 宿主前端 bootstrap
20:32:39.173965 [PluginChannel] 前端通道会话已重置 webview=main reason=page-load   ← Finished（把刚签发的密钥作废）
20:32:39.413481 [API] plugin_channel_token: loader 会话凭证无效
…
20:35:50.267667 [API] 非宿主面凭证，拒绝授权管理操作 command=plugin_fs_auth_respond
20:36:18.348975 fs_auth: batch auth request timed out   ← 30s 超时（用户点了「允许」但应答被拒）
20:36:22.492609 WASM activate() failed: 目录授权被拒绝：…\.bedcode/ai-chatbox
```

连锁结果：插件前端模块加载失败（terminal-session / agent-hub / ai-chatbox 界面缺失）、
文件授权弹窗应答全部被拒 → 插件启用失败 / 降级、授权总览页报错。

同一台机器每次启动 `后端初始化耗时: 32869ms / 30567ms / 31180ms`（正是 `fs_auth::PROMPT_TIMEOUT` 30s）。

## 2. 根因

### 缺陷 A：页面加载钩子在 Started 与 Finished 上都重置凭证域

`bedcode-desktop/src-tauri/src/lib.rs::frontend_channel_session_hook` 的 `on_page_load`
未区分事件类型；wry 对同一次导航回调两次（WebView2 `ContentLoading→Started` /
`NavigationCompleted→Finished`，WebKitGTK `Committed/Finished`，见 `wry-0.55.1`
`src/webview2/mod.rs:653,665`、`src/webkitgtk/mod.rs:477,480`；tauri-runtime-wry
`lib.rs:5028` 两者都透传）。

凭证域语义是「每 webview 每次页面加载重置一次 + loader 密钥**首个调用者生效**」
（`packages/bedcode-wasm-core/src/security/frontend_channel.rs`）。宿主前端
bootstrap（`bedcode-desktop/src/plugin/commands.ts::ensureHostCredential`，页面内缓存不刷新）
落在两次 reset 之间时，密钥立刻被 Finished 那次作废 → 整页所有宿主面 / 插件面命令被拒。

Linux 上同一段代码也在双 reset（`~/.local/share/com.bedcode.app/logs/runtime.2026-10-1*.log`
逐日可见），只是顺序为 `reset, reset, issue`（安全序）；Windows 正式版（单文件 bundle +
启动期 30s 阻塞把事件与 IPC 一起积压）落在坏序 → 必现。

### 缺陷 B：启动期同步等待交互式目录授权，阻塞窗口与前端

`bedcode-desktop/src-tauri/src/lib.rs` setup 里
`tauri::async_runtime::block_on(PluginHost::new(...))`，而
`PluginHost::new`（`packages/bedcode-wasm-core/src/manager/host.rs`）内部同步执行
`auto_activate_from_persisted_state()`（`boot.rs`）。首次运行（无授权记录）时 L3 插件激活
要弹目录授权，主线程被占 30s 等用户点击：

- 窗口 / 前端 30s 后才可用（用户可见的「启动卡死」）；
- 该弹窗在宿主前端注册监听器之前发出 → 事件丢失 → 即使有 UI 也无人应答 → 必然超时拒绝
  （Windows 首次运行实测 `PluginHost] Failed to auto-activate plugin … file access authorization rejected`）。

Linux 侧只因为授权记录早已落账（`bedcode.db::plugin_auth_records` 37 行 vs Windows 0 行）
才没触发；触发条件与平台无关。

## 3. 修复

| # | 修复 | 落点 |
|---|---|---|
| A1 | 页面加载只在 `PageLoadEvent::Started` 重置凭证域（Started 先于本页任何 JS） | `bedcode-desktop/src-tauri/src/lib.rs` |
| A2 | 重置日志把 `had_loader_session` 提到 info（原来只在 debug；`revoked_tokens=0` 会误导——密钥被清但不计数） | `packages/bedcode-wasm-core/src/security/frontend_channel.rs` + `manager/host.rs` |
| B1 | 持久化激活移出 `PluginHost::new`，改由宿主在启动钩子里异步触发（非阻塞、不占 hook 超时） | wasm-core `manager/host.rs` / `boot.rs` + 桌面 `system/lifecycle.rs` |
| B2 | 授权弹窗队列 + 冷启动补投递：wasm-core 暴露待应答请求快照，宿主面命令 `plugin_fs_auth_pending`，前端挂载时补拉并排队逐个应答 | wasm-core `security/fs_auth.rs` / `manager/host/api_bridge.rs` + 桌面 lib 命令面 + `bedcode-desktop/src/{plugin/commands.ts,components/FsAuthDialog.vue}` |

A 修好「凭证被作废」；B1 修好「启动阻塞」；B2 保证「启动期授权请求确实能被应答」
（首次运行 3 个插件同时请求时，弹窗按队列逐个展示，不再互相覆盖）。

## 4. 接受判据

- Rust：`packages/bedcode-wasm-core` 与 `bedcode-desktop/src-tauri` 各自 `cargo test` 全绿；
  新增用例：`PluginHost::new` 不激活持久化插件 + 显式调用才激活；`pending_prompts` 正反例；
  页面加载判据单测 + 结构锁（钩子必须带 Started 门；`new` 不得回接阻塞激活；桌面必须存在
  延迟激活调用点）。
- 前端：`pnpm run test:run`（端目录）+ 根 `pnpm exec eslint .` 0 error；新增用例：
  弹窗冷启动补拉、队列逐个应答、并发取凭证只打一次宿主命令、取凭证失败可重试。
- 手工核验（未纳入自动化）：Linux 安装版启动一次，确认 `后端初始化耗时` 回到毫秒级、
  页面加载序为 `reset → issue`（无第二次 reset）、`plugin_channel_token: 插件面令牌已签发` 全绿。

## 5. 验证结果（2026-10-18 06:00 完成）

自动化：

| 套件 | 结果 |
|---|---|
| `packages/bedcode-wasm-core` `cargo test --lib` | **592 passed**（含 4 条新增：3 条 `pending_prompts` + 1 条构造器时序契约） |
| `bedcode-desktop/src-tauri` `cargo test --no-fail-fast` | lib 160 + 全部集成用例绿（新增 `frontend_channel_boot_lock` 4 条、lib 单测 1 条；`system_component_test` 10 条随新契约更新后绿） |
| 桌面前端 `pnpm run test:run` | **123 文件 / 1681 用例全绿**（FsAuthDialog 18 条、channelIdentity 7 条） |
| 根 `pnpm exec eslint .` | 0 error（96 warning 为基线噪声） |
| 已知环境性失败（与本改无关） | `a03_probe_products::a03_p1c_…`：需打包产物 `resources/plugins/desktop`，工作树里该路径是 dev 指针文件 |

端到端（Linux dev，`XDG_DATA_HOME` 隔离数据目录，避免干扰用户实例）：

| 观测 | 修复前（安装版，同一隔离场景） | 修复后（本分支 dev） |
|---|---|---|
| `后端初始化耗时` | **60484ms / 60727ms**（两次 30s 授权等待串行） | **458ms**（后台激活，窗口立即可用） |
| 页面加载重置次数 | 2 次（Started + Finished） | **1 次**（只有 Started，日志带 `had_loader_session`） |
| 凭证被拒 / 撞闸 | `loader 会话凭证无效` 等 | **0 次** |
| 冷启动授权请求 | 事件丢失 → 30s 超时拒绝 | 前端补拉成功：`pluginFsAuthPending() returned 1 pending request(s)` → 无人点击时仍按 30s 超时 fail-closed 收敛（预期） |

端到端运行额外发现并修复一处**新引入**的并发缺陷：授权弹窗挂载时补拉与 `pluginLoader.loadAll()`
同时调 `ensureHostCredential()`，各自 `invoke` 会让后到者撞上宿主「首个调用者生效」闸
（`frontend loader session already issued`，实测报错）。修法：模块内共享同一个在途 Promise
（失败不缓存，可重试），并补两条用例（并发只打一次命令 / 失败后重试）。

## 6. 诊断修正（实测口径）

Linux 侧用安装版在**清空授权记录**后复现了 B（`后端初始化耗时: 60484ms`、两条 `batch auth
request timed out`、`目录授权被拒绝`），但页面加载顺序仍是安全的 `reset, reset, issue`——
即：**B 跨端成立；A 的坏顺序（`reset, issue, reset`）是 WebView2 特有的事件/IPC 投递顺序**，
不是 30s 阻塞本身造成的。修复口径不变（重置只在 Started），但结论要按这个口径记，
不要把「坏顺序」归因给阻塞。

## 6b. 补丁：延迟激活引入的前端加载缺口（2026-10-18 06:40，用户追问暴露）

**现场**：B1 把持久化激活移到后台后，前端 `pluginLoader.loadAll()` 在启动时只查一次状态，
会把正在激活的插件当成 `activating` 而**跳过**；激活完成（可能 30s 后）没有任何东西再加载
它的前端模块 → 该插件界面**整会话缺失**（实测：种入 agent-hub 持久化启用态 + 无授权记录，
`前端 22:38:52 跳过（state=activating）` → `后端 22:39:20 激活完成` → 无人补加载）。

**修法**：激活完成时发一次信号，前端按需补加载。

| 落点 | 内容 |
|---|---|
| `packages/bedcode-server-base/src/constants.rs` | 新事件常量 `PLUGIN_ACTIVATED = "plugin:activated"`（payload `{ pluginId }`） |
| `packages/bedcode-wasm-core/src/manager/host/activation.rs` | 激活终态（成功/降级）后 emit 该事件；投递失败只 warn（不影响激活结果） |
| `bedcode-desktop/src/plugin/loader.ts` | 抽出纯判据 `pluginsNeedingFrontendLoad(manifests, loadedIds)`（`loadAll` 与补加载共用门禁）+ `loadMissingActivated()`（并发合并、幂等、错误自收口）+ `loadRunningPlugin()` |
| `bedcode-desktop/src/plugin/runtime-listeners.ts` | 注册 `plugin:activated` → `loadMissingActivated()`（不解释 payload，重查状态，能自愈早先漏掉的事件） |

**验证**（同一隔离场景重跑，无授权记录）：

```
22:38:50.595 后端初始化耗时: 472ms                      ← 启动不阻塞
22:38:52.309 前端 loadAll：agent-hub state=activating → 跳过
22:39:20.605 Plugin activated agent-hub（30s 授权超时 fail-closed 降级）
22:39:20.978 [PluginLoader] Loading 1 plugin frontend(s) activated after startup
22:39:20.978 Rust+TS plugin frontend loaded: com.bedcode.agent-hub   ← 界面无需重启即出现
```

新增用例：`src/__tests__/plugin/loaderActivationSync.test.ts`（5 条：判据正/反/边界 + 并发合并 +
句柄释放）。重跑：wasm-core 592 绿、desktop 全绿（仍仅 a03 环境红）、前端 124 文件 / 1686 用例绿、
eslint 0 error。

## 7. 范围外（记录待办）

- 网络授权弹窗（`NetworkAuthDialog`）的同类「冷启动丢事件 + 单槽覆盖」未在本批处理
  （启动期实际请求的是 fs 面；网络请求多在用户操作后触发）。
- 移动端不受影响：`PluginHost` / `on_page_load` 均为桌面宿主形态，移动自持
  `bedcode-mobile/src-tauri/src/plugin/manager.rs`。
- 30s 弹窗超时（`fs_auth::PROMPT_TIMEOUT`）本身不变：多插件同时请求时，排在队尾的
  弹窗仍可能在用户应答前超时 → 该插件保持未启用，用户可手动启用（fail-closed，不变更）。
