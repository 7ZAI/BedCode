# 移动端「插件 → wasm-app」命名对齐 + 未下沉业务代码审计

> Date: 2026-10-08
> 用户指令：「检查移动端和桌面端一样，将插件的概念改为 wasm-app；同时检查有些没有下沉的业务代码」
> 全部事实取自 2026-10-08 工作区实测（行号 / 消费者可复现），不凭记忆。
> **执行前置警告（2026-10-08 审查后更新）**：原警告的票 13/15 已提交落盘（`d585d5322` / `e2816bd78`，
> 均在 HEAD 历史）——重命名窗口已开。**当前仍存的并行在途**（不碰、不回滚，只在非重叠区做本任务改动）：
> 票 17 批次 2b（`src-tauri/src/plugin/wasm_runtime/*` 删除中，宿主垫片已切 `plugin.rs:46`）、
> 票 19 egress 收口（`egress.rs` / `plugin.rs` 在途）、能力域脱绑文档（根 `AGENTS.md` / `docs/code-map.md` 在途）。
> **审查修正记录**：§1.3 #12 原路径已随票 01–16 整核抽出（`4f02a3236`）迁至 `packages/`，改为真源；
> 触点清单补 3 项（#18 测试相对 import 8 文件、#19 第 4 把防回接锁、#20 注释类引用）。

---

## 第一部分 · 插件概念 → wasm-app（对齐桌面）

### 1.1 桌面现状（参照口径，勿扩大）

桌面端只有**业务应用源码目录**用 `wasm-apps/<app-id>/`；以下面**全部保留 "plugin" 词汇**，移动端对齐时不得改名：

- 插件机制：桌面宿主 `src-tauri/src/`（整核抽出后机制在根 `packages/bedcode-wasm-core/`）、前端 `src/plugin/`
- 开发 SDK / 夹具：`packages/plugin-sdk-desktop/`、`packages/plugin-*-test/`
- 运行时目录：`app_data_dir/plugins`（wasm-core `host_api/context.rs:656` 同用 `"plugins"`）
- 打包资源：`src-tauri/resources/plugins/`（含 `desktop` / `mobile` 两子目录，dev 窗口复制源）

### 1.2 移动端差异（本次要改的面）

移动端**业务应用源码目录** `bedcode-mobile/plugins/`（ai-chatbox / file-transfer / terminal-session 三个业务 app）
→ 改 `bedcode-mobile/wasm-apps/`，与桌面 `wasm-apps/<app-id>/` 同构。术语上「业务插件」→「wasm 应用」（机制性
「插件」保留）。app id（`com.bedcode.*`）、manifest 文件名 `plugin.json`、SDK 包名均与桌面一致，**不动**。

### 1.3 重命名触点清单（逐项核对，执行时按此改）

| # | 位置 | 改动 | 说明 |
| --- | --- | --- | --- |
| 1 | `bedcode-mobile/plugins/` | `git mv` → `bedcode-mobile/wasm-apps/` | 三个子目录整体移动；**在途票 13/15 提交后再动** |
| 2 | `AGENTS.md:16` | 路径基准行：移动端源码目录 `plugins/` → `wasm-apps/` | 该行同时列举两端相对路径 |
| 3 | `AGENTS.md:26` | 结构行：`移动 plugins/<plugin-id>/` → `移动 wasm-apps/<app-id>/` | 与桌面同一表述 |
| 4 | `.github/workflows/test.yml:238,243` | `bedcode-mobile/plugins/$p` → `bedcode-mobile/wasm-apps/$p` | 插件安装循环 |
| 5 | `.github/workflows/release.yml:292-297,491-496` | 同上 | 审查补漏：**292-297 为 working-directory 相对形式的插件安装循环**（文档原只列 491-496） |
| 6 | `bedcode-mobile/scripts/dev-run.js:166,176,186` | `dir: 'plugins/xxx'` → `'wasm-apps/xxx'` | `--resources-dir ../../src-tauri/resources/plugins/mobile` **保留** |
| 7 | `bedcode-mobile/scripts/plugin-build.js:6,28,49` | 扫描 `plugins/` → `wasm-apps/`（6 注释、**28 主扫描行**、49 报错文案） | 产物目标 `resources/plugins/mobile` **保留**（运行时面）；28 行 `resolve(ROOT, 'plugins')` 为漏列主行（审查修正） |
| 8 | `bedcode-mobile/vitest.config.ts:12,14` | include 路径 `plugins/terminal-session/src/terminal/__tests__/` → `wasm-apps/...` | |
| 9 | `bedcode-mobile/docs/code-map.md` | 约 15+ 处 `plugins/*` / `plugins/`（目录树、终端链路节、防回接锁索引、Quick Navigation）→ `wasm-apps/*` | 顺手把「插件源码」措辞改「wasm 应用源码」（机制节保留） |
| 10 | `bedcode-mobile/plugin-dev-mobile.md` | 约 5 处（`plugins/terminal-session/src/task/panel.css` 等 + 327 行内置应用路径表） | |
| 11 | 防回接锁 3 文件（`bedcode-mobile/src-tauri/tests/`）| 锁内源码扫描路径字面量 → `wasm-apps/`：`retired_mobile_auth_orchestration_command_face_lock.rs:69-73`、`retired_mobile_auto_task_plugin_lock.rs:165,178,200,214,222`、`retired_mobile_peer_discovery_projection_lock.rs:107` | 锁扫描的是**源码路径**，不改则锁逻辑指向不存在目录 = 静默失效 |
| 12 | `bedcode-mobile/packages/bedcode-wasm-core/src/test_support.rs:52` | 测试构建路径 `../../plugins/terminal-session` → `../../wasm-apps/terminal-session` | 原声明 `src-tauri/src/plugin/wasm_runtime/component.rs:911` 已随票 01–16 整核抽出失效（宿主 `plugin.rs:46` 只剩垫片）；真源迁移至 packages（审查修正） |
| 18 | `bedcode-mobile/src/__tests__/plugins/file-transfer/` **8 个测试文件** | 相对 import `../../../../plugins/file-transfer/...` → `../../../../wasm-apps/file-transfer/...` | 审查补漏：git mv 后相对路径断链（deriveDeviceRows / taskProgressColor / useConsent / usePeerDevices / useRemoteFs / useSettings / useTasks / useTrustedPeers） |
| 19 | `bedcode-mobile/src-tauri/tests/retired_mobile_session_control_face_lock.rs:124-130` | 锁内路径对 `plugins/terminal-session/...` → `wasm-apps/terminal-session/...` | 审查补漏：文档原列 3 把锁，实为 **4 把**（session.rs / commands.rs / plugin.json 路径对） |
| 20 | 前端 / 注释类引用（不功能断链，AGENTS §0 文档一致） | `plugins/terminal-session/...` → `wasm-apps/terminal-session/...` | 审查补漏：`src/utils/themeLabel.ts:6`、`src/views/TerminalView.vue:23`、`src/composables/useMobileConnection.ts`（注释）、`src/__tests__/integration/{session-flow,connection-flow}.test.ts` 注释、`src-tauri/src/system/constants/connection.rs:53` 注释、`src-tauri/tests/session_http_flow.rs:14` 注释、`plugins/file-transfer/rust/src/settings_store/tests/save_and_push_writes.rs:1`、`plugins/file-transfer/src/composables/useTrustedPeers.ts:11`、三 README 的路径引用 |
| 21 | `bedcode-mobile/vite.config.ts:122-123` | chunk 前缀判断 `startsWith('plugins/')` → `'wasm-apps/'` | 审查补漏：插件 chunk 路由判断（功能逻辑，源码移目录后 chunk 名随之变） |
| 22 | `bedcode-mobile/tailwind.config.js:8` | 内容扫描 `./plugins/**/src/**/*` → `./wasm-apps/**/src/**/*` | 审查补漏：git mv 后插件样式类不再被扫描（样式断链） |
| 23 | `bedcode-mobile/.vscode/settings.json:8` | 注释 `plugins/*/rust` → `wasm-apps/*/rust` | 审查补漏（RA 递归发现路径） |
| 13 | 根 `docs/commands.md:19,68,278` | `bedcode-mobile/plugins/<plugin-id>` → `<wasm-apps>` | |
| 14 | `docs/knowledge/wasip3-toolchain.md:185` | `bedcode-mobile/plugins/*` → `wasm-apps/*` | 文档描述行 |
| 15 | `.scratch/2026-10-07-mobile-wasm-core-refactor/*.md` | spec + 票文档的 `plugins/...` 引用 | .scratch 只在 dev 分支入库，执行批随改 |
| 16 | `docs/diagrams/bedcode-overall-architecture.*` | 仍引用 `bedcode-mobile/plugins/auto-task`（已随票 16 退役）| 属票 21 文档收口，非本票范围，登记联动 |
| 17 | `scripts/plugin-package-list.json` | 内容仅 app id（无路径），**结构不动** | CI/脚本的路径拼写改在 #4/#5/#6/#7 |

### 1.4 明确保留（与桌面完全一致，执行时**不要**动）

- `system/constants/plugin.rs`：`PLUGIN_STORAGE_DIR` / `APK_PLUGINS_DIR` / `PLUGIN_DATA_DIR` / `PLUGIN_DOWNLOAD_TEMP_DIR`
  均为 `"plugins"`（运行时 app_data_dir 面）
- `src-tauri/tauri.conf.json:53-55` bundle resources `resources/plugins/**/*`
- `src-tauri/src/plugin/`（Rust 机制）、`src/plugin/`（前端机制）、`packages/plugin-sdk-mobile/`、`packages/plugin-component-test/`
- WIT 接口名 / 权限位 / bus·events 话题命名

---

## 第二部分 · 未下沉业务代码审计（ADR 0022 B1–B6）

### 2.1 Rust 宿主残留（按明确度排序）

| # | 位置 | 内容 | 判据 | 状态 / 建议 |
| --- | --- | --- | --- | --- |
| R1 | `commands/mobile_commands.rs:15,49,56` | `SESSION_CONFIGS` 静态空内存库 + `list_session_configs_mobile` / `get_session_config_mobile`（**已注册** `lib.rs:287-288`）| **B1/B3**：宿主以业务名词定义 `SessionConfig`（`model/data.rs:11`）并对外暴露读写面 | **零消费者死代码**（前端/插件 grep 无命中；库恒空，无写入方）。随会话域下沉：读面收进 terminal-session 插件（会话配置真源 = 桌面 sync 事件），宿主命令退役 + 防回接锁 |
| R2 | `router/event.rs::MobileEvent`（28-133 行）| `SyncSessionCreated/StatusChanged/Stopped/Removed` · `SyncConfigCreated/Updated/Removed` · `SyncTaskStatusChanged` · `SyncSessionModeChanged` · `SyncTaskQueueChanged` · `SyncTaskScheduledChanged` · `PairingRequest/Paired/PairingVerified` 等**业务事件变体** + `start_event_forwarding` 转发 `ws_sync_*` 前端事件 | **B4/B6**：把 wire 结果翻译成产品事件形状 + 解释产品事件主动推前端 | 会话/配置/任务同步投影仍宿主持有。目标形态 = 插件事件面（host-events，对齐票 07 peer 事件桥范式）；事件名与载荷保持逐字一致则前端零改动。**未出票** |
| R3 | `peer_remote.rs:29,316-340` | `pull_peer_files` 多文件拉取：逐条 `for file in &files` 发起会话 + `PULL_FILES_CAP=512` 封顶 | **B2**：按产品语义推进的流程 / 封顶 | code-map 已自认「拉取编排本身仍是宿主 B2 遗留，见票 07/08 §5.2」；**未出票**。建议下沉 file-transfer 插件（拉取意图队列已插件化，`push_pull_intent` 先例在位）|
| R4 | `handler/plugin_event.rs`（563 行）+ `handler/system.rs` | WS 消息 → 前端事件（`MobileEvent`）的业务变体解释 | B4/B6 | 随 R2 一并迁移；传输面路由（`router/router.rs` / `registry.rs` / `context.rs`）留内核 |
| R5 | `system/config.rs::AppConfig.session`（`SessionConfig` 默认值 windows/wsl2）| 桌面形态会话默认配置残留 | B1/B5 | `get_app_settings` 有消费者（`src/stores/settings.ts:63` 节深合并），但 windows/wsl2 会话段移动端无用——**待确认**：确认后删字段或标记 deprecated（低优先）|
| R6 | `peer_net.rs`（1862 行）| 票 09 后剩余面 | — | 基本收口（DiscoveryCache 只作引擎事实，防回接锁在位）；列入复查项不列为缺口 |

### 2.2 前端宿主残留（旧 `src/`，AGENTS §6 只接受缺陷修复；业务面最终归各 wasm-app）

| # | 文件 | 内容 | 判据 | 状态 / 建议 |
| --- | --- | --- | --- | --- |
| F1 | `useMobileConnection.ts`（1225）| 连接管理 + 会话状态合并 + `ws_sync_*` 事件监听 + `sessionConfigs` 投影 | B2/B4/B6 | spec D3 已定「连接编排视图」归 terminal-session app；**未出票** |
| F2 | `SessionsView.vue`（539）| 会话列表 / 启停 UI | B2/B4 | 同 F1，随会话域迁插件视图 |
| F3 | `DevicesView.vue`（1191）| 配对流程 / QR 扫码 / 生物认证 / mDNS 发现 UI（消费 `useMdnsDiscovery` + `useMobileCommands` 的认证包装）| B2/B4 | 票 14 只下沉了**命令面**；视图本体未迁（spec D3 归 terminal-session app）。**未出票** |
| F4 | `useHttpApi.ts`（669）| file tree / session mode / task queue HTTP 封装 | B4 | 会话控制已随票 13 改插件面；文件浏览面未动 |
| F5 | `useFileTree.ts`（308）+ `useCodeHighlight.ts`（391）+ `CodeExplorerView.vue`（137）| 文件浏览 / 代码查看 | B2/B4 | **未出票**；归属 app 待用户裁决（file-transfer 还是 terminal-session）|
| F6 | `usePresetTasks.ts`（227）+ `presetTaskState.ts`（67）+ `PresetTasksView.vue`（587）| 预设任务（业务默认值 / 策略）| B2/B5 | 票 16 只并了 auto-task 面板，预设任务面未迁；**未出票** |
| F7 | `useNotification.ts`（342）| 任务 / 连接事件 → 系统通知 | B6 | **未出票** |
| F8 | `useMdnsDiscovery.ts`（135）/ `useMdnsAdvertiser.ts`（50）| 发现 UI 状态（App.vue + DevicesView 消费）| B4 | 与插件 `deviceState.ts`（peer 域）是**两套**：宿主这份服务于桌面连接/配对发现——随 F3 迁 |
| F9 | `useMobileCommands.ts`（511）| Tauri 命令封装族（含 `wsAuthenticateWithQr` 等）| — | 票 14B 在途已换插件命令面；收口时核对无退役命令字面量残留 |
| F10 | `ToolboxView.vue`（205）/ `PluginView.vue`（858）| 工具箱入口 / 插件管理 | 机制 | 机制面保留（壳 ToolboxView 运行面已就位，旧页待壳接管后退役）|

### 2.3 已下沉 / 已出票对照（趋势确认）

票 06-10 对等传输（发送/接收/发现投影/命令面收口）、票 11 host-websocket、票 12 终端协议客户端、
票 13 会话控制（在途）、票 14 认证编排（命令面已下沉）、票 15 终端 UI（在途）、票 16 auto-task 任务域、
票 19/20 egress 授权（在途）。**Rust 面收敛明显，前端业务视图是大头缺口**。

### 2.4 建议出票（待用户裁决）

- **票 A · 会话配置死代码清理（R1）**：`SESSION_CONFIGS` + 两命令退役，读面收进 terminal-session 插件；小票，可与票 13 收口合并。
- **票 B · 会话/配置同步事件投影下沉（R2+R4+F1+F2）**：`MobileEvent` 业务变体 → host-events 插件事件面；连接编排视图（useMobileConnection 会话段 / SessionsView）迁 terminal-session app。中票，依赖票 15 收口。
- **票 C · 配对/发现视图下沉（F3+F8）**：DevicesView 迁 terminal-session app（命令面已就位，纯视图搬运）。中票。
- **票 D · 拉取编排下沉（R3）**：`pull_peer_files` 循环 + 封顶迁 file-transfer（`take_pull_intent` 队列先例）。小票。
- **票 E · 预设任务 / 通知 / 文件浏览（F5/F6/F7）**：归属裁决后逐域出票。文件浏览归属需用户先拍板。

---

## 3. 执行建议

1. **命名重命名**：等票 13/15 提交落盘后按 §1.3 清单执行（git mv + 触点同步 + 防回接锁路径字面量 + 双语文档 + CHANGELOG）。
2. **下沉缺口**：§2.4 的票 A-E 按序排入 `2026-10-07-mobile-wasm-core-refactor` spec（票号顺延），或按用户裁决取舍。
3. 本审计的命名触点清单与桌面 `wasm-apps` 先例均已核验，执行时无需再探路。
