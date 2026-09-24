# 08: 宿主会话命令面与前端会话 API 注销（读会话一律走插件）

**What to build:** 桌面用户看到的东西一件不变，但**宿主命令面不再有「会话」这件事**：
列出会话 / 取单条会话 / 写输入 / 特殊键 / 尺寸 这五个宿主命令连同前端那条会话 API 通道一起退役，
前端与插件界面统一改经插件贡献的命令通道取会话事实。做完之后，宿主里「谁还能直接问内核要会话」
只剩事件面（9）和待删的内核实现（11）。

**Blocked by:** 02（对外类型先迁出，命令面注销时才不会连带动到协议定义）。

**Status:** done（2026-09-24 落地；人工核验项待 01 基线复跑，见票末）

## 现状取证（2026-09-24）

- 这五个宿主命令已经全部转发到会话窄转发层（P1-b），命令本体只剩参数拆装与状态注入
- 前端消费方：终端窗口/侧栏读会话列表走插件贡献通道；宿主那条会话 composable 与 store
  **无生产调用方**（仅测试引用）；插件侧的 `context.session` 读面是另一条通道，需一并裁定归属
- 插件前端输入走宿主终端输入命令（P1-b 已改接转发层）——本票要把这条也换成插件自家通道，
  这样「终端输入」在宿主侧就没有命令面残留

## 验收标准

- [ ] 五个宿主命令从注册表注销；前端不再有任何一条路径能调到已注销命令
      （有锁：命令名在前端源码与注册表两侧都不出现）
- [ ] 宿主侧的会话 composable / store / 插件前端会话读面要么退役、要么改指插件命令面；
      裁定写进票末（保留壳会被 11 当成「还有消费者」）
- [ ] 终端输入命令（插件前端在用那条）改走插件命令通道；权限位与身份绑定的门禁
      **逐字保留**（身份令牌 + 激活 + 输入权限三段），迁移路由不等于放宽
- [ ] i18n：因注销而消失的文案同步删（zh-CN 与 en 一起），不留孤儿键；
      错误可见性不变（插件未激活仍显性报错）
- [ ] 桌面行为回归：01 的人工清单复跑，输入/尺寸/列表/通知四项无差异
- [ ] 门禁：宿主 lib + 集成 8 target 全绿、桌面前端全绿、根 `eslint .` 0 error、
      插件前端契约测试的 api/命令清单与 manifest 一致（四处 pin 不漏）

## 边界与不做

- 不删内核会话实现（11）、不删会话原语 interface（10）。
- 不改任何对外线协议形状。
- 不顺手清理与本票无关的宿主命令面。

## Comments

### 2026-09-24 · 落地：五个宿主命令 + 插件终端输入通道整族注销

**注销范围（六条命令）**：`list_sessions` / `get_session` / `resize_session` /
`write_to_session` / `send_special_key`（票面五个）**加** `plugin_terminal_send_input`
（票面验收第 3 条那条「插件前端在用」的终端输入命令）。

**为什么能整族删**：P1-b 之后这五条已经是「宿主命令名 → 窄转发层 → 插件互调 api」
的薄壳，而**壳里的消费者在这轮测绘里全部落在插件前端**——宿主前端唯一的读会话
路径（`stores/session.ts`）与命令封装（`composables/commands/sessionCommands.ts`）
都无生产调用方（只有测试引用）。故按票面「保留壳会被 11 当成还有消费者」的判据，
**一律退役、不留壳**。

### 裁定：`context.session` 数据面退役，窗口面留宿主

`src/plugin/context.ts` 的 `SessionAPI` 一刀切两半：

| 面 | 成员 | 裁定 |
| --- | --- | --- |
| 数据面 | `list` / `get` | **退役** → 插件读自家会话走命令通道（`session.list` / `session.get`） |
| 窗口面 | `predictTerminalSize` / `openTerminal` / `closeTerminal` / `isTerminalOpen` | **留宿主**（窗口本体 / 字体测量在宿主，spec D3；插件无法自实现，合裁剪线） |
| 事件面 | `onStatusChange` | 本轮**未动**——它是内核状态订阅驱动的 Tauri 事件（生产上对插件会话已无流量），归**票 09** 与订阅通道一并裁定；此面留到 09 不影响本票的「命令面干净」判据 |

同批退役 `TerminalAPI.sendInput`（宿主替插件导流输入）：留下的 `onOutput` /
`onInput` 是**观察面**（渲染管道输出投递与输入修饰链观察点），与写入面分属两侧。

### 插件侧补的三条自有命令

宿主命令面消失后，插件前端需要等价能力，故 `invoke_command` 新增三臂（**与既有
互调 api 共享同一实现**，不复制语义）：

| 命令 | 与哪条 api 同实现 | 用于 |
| --- | --- | --- |
| `session.list` | `session-list`（登记域视图 `{sessions: [...]}`） | `useSessionCenter.loadSessions` |
| `session.get` | `session-get`（单视图，不在册 → `null`） | `TerminalWindowView.loadSessionInfo` |
| `session.input` | `session-input`（`{sessionId, data?, specialKey?}`） | `TerminalPreview.onData`（键盘输入） |

为此把 `session-get` / `session-input` 的**入参解析抽成两个 free fn**
（`session_get_view` / `session_input_write`），api 与命令两条入口调同一份——
否则「两条路径的判据逐字相同」只能靠人眼维持，正是漂移源。

**门禁怎么算没放宽**：`plugin_invoke` 通道自带「身份令牌 + 已激活」两段（票据 32 /
P0-5 的既有门禁，逐字未改），写入侧的 `pty:io` 权限门仍在 WIT 层按 manifest 仲裁；
`terminal:input` 位的另一落点 `terminal.onInput` 保留（故该位不退役）。即**载体换了、
门数没少**：注销的宿主命令里那三段（credential / activated / `terminal:input`）
前两段搬到了插件命令通道、第三段由 WIT 权限门承接。

### 落地面（宿主）

- `commands.rs`：五条命令连壳删除（含 `ResizeOutcome` 导入）；
- `lib.rs`：六条注册摘除；
- `api_bridge.rs`：`plugin_terminal_send_input` 删除（连带其 `Store` 段注释更新）；
- 前端：删 `composables/commands/sessionCommands.ts` 与 `stores/session.ts`；
  `useDesktopCommands.ts` 去掉会话域 re-export（`SessionInfo` 类型改从
  `@/composables/model` 取）；`plugin/{context,commands,types}.ts` 三处去 API 面；
- i18n：`desktop.session.sessionStopped` / `desktop.session.stopFailed` 随 store 注销
  删除（zh-CN 与 en 同步），宿主 `session` 组只剩退出确认三项。

### 权限词汇同步（五同步点里本轮涉及的三点）

SDK `permission.rs` 的 `PERMISSION_API_MAP` 去掉 `terminal.sendInput`、
`session.list`、`session.get` 三个 API 名（**位不删**：`terminal:input` 仍由
`terminal.onInput` 门住，`session:read` 仍由四个窗口原语门住），随后跑
`gen:permissions` 重出 `bin/permission-vocabulary.json` 与
`src/plugin/permission-vocabulary.ts`。宿主能力清单 / host_impl 权限门 / `manifest-gen.js`
本轮**零改动**（删的是 API 名不是权限位，故无映射表变更）。

### 为「不许回接」上的锁（并修掉首版的一个真实漏洞）

`api_bridge::tests::retired_session_command_surface_is_not_reintroduced`：扫三个源码面
（宿主 `src-tauri/src`、宿主前端 `src`、插件前端 `plugins`，跳过 `node_modules`/`dist`/`target`），
非注释行命中六个退役命令名即红并打印 `文件:行:内容`。

**这条锁的第一版有漏洞，靠变异自检抓到**：前端针写成带单引号的 `'list_sessions'`，
而变异探针用的是 `invoke("list_sessions")`（双引号）→ **测试仍然绿**。收窄为
不带引号的名字匹配后复测：转红并精确点名
`src/composables/useDesktopCommands.ts:22`。还原后复绿。
（教训：字符串匹配型锁的针要按**语法自由度**选，不能按「我写代码的习惯引号」选。）

### 门禁实测（本票）

- 宿主 lib：**1147 passed / 0 failed**
- 集成 8 target **逐个串行全绿**
- 插件 native：**298 passed / 0 failed**；产物按新 SDK 重建（wasmHash `fe68b4a7…`）
- 桌面前端全量（`--pool=forks --maxWorkers=2`）：**80 files / 779 tests 全绿**
  （较 P1-b 基线的 81/794 少 1 文件 15 用例 = 删除的 `stores/session.test.ts`）
- 根 `eslint .`：**0 error**（120 warning 与基线同数）
- `permissionVocabulary.test.ts` 的 L1 门禁点基线 20 → **19**（三条 API 面注销），
  已在用例注释里记账「下降是已记账的退役，不是扫描失效」

### 记账

- 票面验收第 5 条「01 人工清单复跑（输入/尺寸/列表/通知四项）」**未跑**：01 人工基线
  本轮仍为零条观测。用户 2026-09-24 裁：本会话连前置一起做，人工验收项后置。据此本票
  在「人工核验」一项上**不算勾**。
- 尺寸裁决与特殊键**没有新增命令**：插件前端早已走自家 `session.action.resize`
  （`TerminalPreview.requestResizeImpl`），特殊键自对侧票 06 起也已是插件的
  `specialKey` 直写——宿主的 `resize_session` / `send_special_key` 两条壳注销后
  零功能缺口。
