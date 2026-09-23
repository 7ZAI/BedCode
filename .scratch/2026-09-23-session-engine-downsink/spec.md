# 会话引擎整体下沉（Session Engine Downsink）

Status: **ready-for-agent**（2026-09-23 立项；同日用户裁决开放点 1/2/3 并给出开放点 4 定案，见 §5）
Date: 2026-09-23
范围: **桌面端为主**（`bedcode-desktop/src-tauri/src/session/`、`pty/`、`plugin/manager/wasm_runtime/host_impl/{session,terminal,lifecycle}.rs`、
`server/`、`commands.rs`、`events/`、`utils/session_*_bridge.rs`、`plugins/terminal-session/`、WIT/SDK）；
移动端按用户 2026-09-23 明确指令「不用管」→ **不设双端同步豁免的收窄承诺，改出「移动端受损清单」如实记账**
决策依据: 用户 2026-09-23 方向指令（四点 + 总原则「向微内核靠拢，只有 io / 文件 / 线程等 WASM 无法实现的才留宿主」）、
AGENTS §5（无业务内核 / 高内聚低耦合）、ADR 0022（裁剪线）、`.scratch/2026-09-10-plugin-kernel-roadmap/spec.md`（阶段 3/4）、
`.scratch/2026-09-22-pty-business-downsink/spec.md`（本专项的前半段，票 1/3 已 land，**票 2 并入本专项 P0**）
承接: 会话**编排**（命名唯一化 / config→launch / 两阶段决策 / 重启）已于 v21 下沉 `plugins/terminal-session/rust/src/launch.rs`（701 行）、
终端**渲染与写入管线**已于 2026-09-22 整体下沉（票 01–08）；本专项把**最后一块「会话真源」**（登记 / 状态机 / 生命周期分发 / 输出环 / 输入直通）挪出宿主。

> **路径基准提醒（2026-09-23 实测）**：对侧在途批次正把 `src-tauri/src/plugin/**` 重命名为 `src-tauri/src/wasm_core/**`
> （`wasm_core/host_api/*` 取代 `plugin/manager/wasm_runtime/host_impl/*`；`plugin.rs` / `plugin/bus.rs` / `plugin/config.rs` 等
> 已删，见 `git status` 93 项删除 + 新增 `wasm_core.rs` / `wasm_core/`）。本文件正文中出现的
> `plugin/manager/wasm_runtime/host_impl/session.rs` 等旧路径**以落地时实际结构为准**：
> 已验证等价物为 `wasm_core/host_api/session.rs`（1430 行）、`wasm_core/host_api/{terminal,lifecycle}.rs`、
> `wasm_core/manager/*`、`wasm_core/security/*`。**开工前先 `git log -10` + `cargo check --lib` 确认该批次已 land。**

---

## 0. 用户方向指令（原文要点，作为验收基线）

1. **PTY 留宿主但必须解耦**：pty 只提供基础服务接口（或满足业务的基础接口），不得与业务代码耦合。
2. **会话状态机 + 登记 + 生命周期分发应放插件端**；其他服务调用插件端接口或转发给插件；**同步阻塞的留在宿主**。
3. **输出环**：「性能红线」应经测试后判断，性能可以就迁插件。**输入通道**也应由插件转发给宿主 pty。
4. **移动端不用管**；总原则 —— 向微内核靠拢，插件实现大部分业务逻辑，只有 io / 文件 / 线程等 WASM 无法实现的才留宿主。

---

## 1. Problem Statement：现状与终态的差额

### 1.1 现状（2026-09-23 实测）

**宿主 `session/`（3759 行）= 会话真源**：

| 组件 | 行数 | 性质 | 用户判定 |
| --- | --- | --- | --- |
| `session_manager.rs` | 1210 | 会话登记 + 状态机 + 生命周期分发 + 注解槽 + 属主表 + resize 裁决 | **点 2：应下沉** |
| `session_output.rs` | 1325 | `GlobalOutputManager` / `UnifiedOutputQueue` / `SubscriberHandle`（业务会话输出环） | **点 3a：测试后定** |
| `input_line.rs` | 562 | `SubmittedLineTracker` 提交行重建 + `SessionInputListener` 观察点 | **点 3b：随输入通道下沉** |
| `session_components.rs` | 375 | `PtyRegistry` / `SessionInfoRegistry` / `CanonicalRendererRegistry` / `RendererSource` / `ResizeOutcome` | 随点 2 下沉（wire 类型保留在外层） |
| `session_event.rs` | 201 | `SessionInfo` / `SessionInfoView` / `task_fields_from_slot`（硬编码插件键名） | 下沉 + **键名映射随之删除** |
| `session_config.rs` | 19 | v24 后零业务方法的装配占位壳 | 直接删 |
| `session_lifecycle.rs` | 56 | `SessionLifecycleEvent` / `SessionLifecycleListener` | 下沉（**Creating 同步语义按点 2 留宿主**） |

**`pty/` 业务耦合（点 1）**：`pty_process.rs:47-49` 有 `PtyCommandSource::Business(SessionLaunchConfig) | Raw` 双线，
业务语义在 `pty/command.rs`（shell 包装）+ `pty/wsl.rs`（275 行，WSL 路径转换）。
`.scratch/2026-09-22-pty-business-downsink/` 票 1（Raw argv 路径）与票 3（`PtySlaveFdPolicy` 统一 ReleaseOnSpawn）已 land，
**票 2（删 build_command/wsl.rs/Business 变体）尚未实施** → 并入本专项 P0。

**消费方测绘（下沉的阻力面，逐项见 §3 分票）**：

- 宿主 server（**留宿主**，点 4）：HTTP `GET /api/sessions`、`POST /api/sessions/start|{id}/stop|{id}/resize|{id}/input`、
  `DELETE /api/sessions/{id}/remove`、`GET /api/sessions/{id}/history`（6+2 条）；WS `/ws/event` 的 `SessionControlAction`；
  WS `/ws/terminal/session/{id}` 控制帧通道（每帧输出）——**全部是移动端消费面**。
- `events/sync_handler.rs:56-223`：`SessionCreated` / `SessionStatusChanged` / `SessionStopped` / `SessionRemoved` 4 分支 → WS 广播。
- `events/forwarder.rs:33-43`：`subscribe_status()` → Tauri 事件 `session-status-changed`（**前端已零消费方**，注意前端注册名是
  `session:statusChange`，两侧本就不一致）。
- `system/lifecycle.rs`：`:254-258` shutdown 最先 kill all PTY；`:311-326` + `lib.rs:595-613` 关窗守卫依赖 `list_sessions()`。
- `commands.rs`：`list_sessions` / `get_session` / `write_to_session` / `send_special_key` / `resize_session`。
- `host_impl/session.rs`：`host-session` **12 个函数**（见 §4.1 表）。
- 前端：`stores/session.ts`（**无生产调用方，仅测试**）、`composables/commands/sessionCommands.ts`、
  `plugin/context.ts:141-187` 的 `SessionAPI`（`list`/`get`/`onStatusChange`）。
- 插件自身：`output.rs` 经 `host-session.output-ring-fetch` 拉输出（自持游标）、`launch.rs` 经 `host-session.create-with-spec` 创建、
  `actions.rs` 经 `host-session` remove/rename/resize、`lib.rs` 7 个 `session-*` 互调 api。

### 1.2 终态（微内核划界）

```
宿主（内核，只留引擎 + WASM 做不到的）
├── PTY 引擎：Raw exec + PtyRing + 终态门 + 回收        ← 点 1，唯一 PTY 出口
├── host-pty 原语（6 函数，v16）                        ← 宿主唯一的会话相关原语
├── 网络引擎：HTTP/WS server 骨架 + 认证/过滤链 + 连接注册表 + mDNS
├── 存储/安全/通信/wasmtime 运行时
└── 同步阻塞点（spawn 边界、系统关停）                  ← 点 2 明确保留

插件 com.bedcode.terminal-session（业务真源）
├── 会话登记 + 状态机 + 生命周期分发（新 session_registry 域）
├── 会话真源 → 插件私有库（新 sessions 表）
├── 输出消费环 + 自持游标（经 host-pty.ring-fetch）
├── 输入转发（host-pty.write）+ 提交行重建（自 input_line.rs 迁入）
├── 尺寸裁决（已在插件 actions.rs）
└── 会话编排（已在插件 launch.rs）
```

**宿主「零会话对象」是本专项的验收红线**：`src-tauri/src/session/` 目录整体不存在。

---

## 2. 硬约束（不解决就无法落地，必须先裁决/改造）

| # | 约束 | 证据 | 影响 |
| --- | --- | --- | --- |
| H1 | **`PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN = 8`** | `system/constants.rs:214` | ~~全量业务会话改走 `host-pty` 直接撞上限~~ → **已解（P1-b 前置 B）**：manifest 声明 `ptyQuota` + 宿主加载期区间仲裁（上限常量 64，越界拒绝不夹取），`spawn` 判据按属主声明；terminal-session 的实际声明值随 P1-b 接入 `host-pty.spawn` 同批写入 |
| H2 | **同实例串行红线（A0-3）** | AGENTS §7「同实例串行红线」 | 同插件实例同一时刻只允许一个 guest 调用；桌面终端窗口 + 移动端 + WS 通道**同时拉输出会串行化**。需 P3 出量化预算（每帧成本 × 端数），不能靠假设 |
| H3 | **WASM 不可重入、无 push 模型** | ADR 0022 D3（两条理由） | 输出只能「拉」不能「推」；**已由开放点 1 定案规避**——环本体留宿主 PTY 引擎（形态 B），不入插件内存 |
| H4 | **server 留宿主** | 点 4（网络引擎属 WASM 做不到） | 移动端所有会话面由宿主 server 收 → 会话**事实**面转发给插件；输出**字节**面按形态 B 由宿主直读 PtyRing，不跨边界 |
| H5 | **同步阻塞留宿主** | 点 2 明确；`dispatch_lifecycle_event(Creating)` 现在同步阻塞（`session_manager.rs:267-274`） | Creating「hook 就位后才 spawn」的时序在插件化后由**插件自己串行保证**（它自己先做 hook 再做 spawn），宿主只保留 spawn 原语的同步边界 |
| H6 | **会话 id 与 PTY 句柄是两个标识** | `host-pty.spawn` 返回 `pty-<uuid>`；现会话 id 由宿主 `create-with-spec` 预生成 | 迁移期需定义映射；`BEDCODE_SESSION_ID` 注入（`2026-09-22-pty-business-downsink` 开放点 1）随迁形态要定 |

---

## 3. 分票建议

> **2026-09-24 起本节是「阶段与裁决记录」，不是工单。** P1-b 之后的实施工单已按纵向切片
> 拆成 `issues/01…12`，权威拓扑与依赖边见 **§3b 票据拓扑**；开工请读票文件。
> P0 / P1（含前置 A/B/C 与 P1-a / P1-b）已 landed，其记录原样保留在本节作为理由与踩坑存档。

### P0 — PTY 与业务解耦（**吞并 `2026-09-22-pty-business-downsink` 票 2**）

- 删 `pty/command.rs`、`pty/wsl.rs`；`PtyCommandSource` 收敛为 Raw（删 `Business(SessionLaunchConfig)` 变体，
  `pty_process.rs:47-49/252` 分支）；`create-with-spec` 只走 Raw 路径（旧插件产物兼容窗口按 pty-downsink spec 判定；
  本专项允许**直接切断**——插件产物随仓重建）。
- `command.rs` 的 200+ 行转义测试迁插件侧契约测试；修掉 scratchpad 记的两处 WSL 缺陷（正斜杠路径解析、单引号转义遗漏）。
- 验收：`pty/` 不含任何 shell 包装 / WSL 转换 / 产品环境分支；`PtyCommandSource::Business` 不存在。

**✅ 已 landed（2026-09-23）**——实施记录：

- **宿主删除**：`pty/command.rs`（348 行，`build_command`）整文件删；`pty/wsl.rs` 只留 `WslDistro` /
  `parse_wsl_list_output` / `list_distributions` / `decode_wsl_output`（`host-platform.wsl-distros` 原语），
  `windows_to_wsl_path` 与 `execute_command` 删（**235 → 151 行**，余下全是发行版列举与其解码/解析测试）。
  `pty_handler.rs`（93 行）整文件删
  （trait 无 `dyn` 消费者，`SessionManager::new_with_handlers` 随之退役 → `new()`）。
- **引擎面收敛为「只收 argv」**：`PtyCommandSource` 枚举删；`PtySessionState.command: Option<CommandBuilder>`；
  `start()` 单一路径（无 match、无 env 注入）。构造入口 `PtySession::with_command(id,name,cols,rows,cmd,sink)` +
  `with_private_command` 便捷版。**`pty/` 不再 import** `SessionLaunchConfig` / `ExecutionEnvironment` /
  `BEDCODE_SESSION_ID` / `SessionOutputSink`。
- **业务实现归位**：`session/session_manager.rs::launch_command`（业务翻译单点：argv / cwd 仅
  Windows+Linux 显式设置 / env 透传 / `BEDCODE_SESSION_ID` 注入 + 空 argv 显性报错）；
  `SessionOutputSink` 自 `pty/output_sink.rs` 归位 `session/session_output.rs`（随 `GlobalOutputManager`）。
- **旧产物不静默断流**：`SessionLaunchConfig.command_args` → **必需** `Vec<String>`（原 `Option`）；
  `resolve_launch_spec` 对缺省 / 空 `commandArgs` 与旧 `args` 字段显性拒绝（报错点名按新 SDK 重建）；
  `command` 降级为纯诊断串。`SessionLaunchConfig::new` / `with_environment` / `with_working_dir` 三个零调用者
  构造器一并删。
- **测试**：宿主 lib **1141/0**、集成 8 target 全绿（含 `pty_session_chain` 真实 PTY+WS 闭环）、
  插件 **227/0**。新增锁：`session_launch_config_requires_command_args`、
  `resolve_launch_spec_requires_command_args`（缺省/空/旧 args 三反例）、`launch_command_*` 三项
  （argv 透传 + env 注入 / cwd 仅原生环境 / 空 argv 拒绝）、插件侧
  `build_argv_wsl2_escapes_single_quote_in_working_dir`（自宿主 `wsl2_escapes_single_quote_in_working_dir`
  迁移的回归锁）。**变异自检 3 处**（均转红后还原）：① 插件 WSL 转义改直通 → 新锁红（diff 直接显示
  `o'brien` 未转义）；② `launch_command` 去掉 argv 守卫 → 拒绝用例红；③ `resolve_launch_spec` 缺省回退
  `vec!["bash"]` → 必填用例红。
- **既有缺陷记入（非本批次引入）**：`pty_reader` 读线程 `blocking_send` 入队尾帧后立即
  `mark_reader_closed`，而 `sink.on_bytes` 在**独立消费者任务**里异步消费 → **终态事件到达 ≠ sink 已收到尾帧**
  （原注释写「尾帧已投递完毕」，表述相反）。本次按事实更正注释，并把 `pty_process` 的
  `session_with_private_sink_bypasses_business_output_bus` 断言改为**有界轮询**（该用例原属 MEMORY 记录的
  flaky 名单，根因即此）。**残余风险**：这是引擎投递时序的真实间隙，若消费方在终态事件后依赖 sink 内容
  需自带等待；是否把「reader_closed 推迟到消费者排空后」作为引擎修复，另立项评估。
- **能力收窄（需用户复核）**：CMD 分支不可达（插件 environment 词表 `linux|wsl2|windows` 只映射
  PowerShell），宿主 `cmd.exe /K` 包装与危险字符拒绝随之退役；`WindowsShell::Cmd` 类型保留但无生产者。
- **未做（属 P1+）**：`SessionLaunchConfig` 仍含 `environment`（被 `launch_command` 用于 cwd 规则）与
  `command`（诊断）两个字段——两者随 `session/` 整体退役（P4）；`pty` 引擎已不依赖它们。
- **协作事实取证（暂存区/提交被卷，勿改写对侧 commit）**：P0 实施期间对侧提交
  `69f7b316c`（`refactor(desktop): 内核模块 plugin 改名 wasm_core + constants 收敛`，08:59:58）
  **连带带走了本批次对 4 个「被改名路径」的在途编辑**——`wasm_core/manager/runtime.rs`、
  `wasm_core/host_api/terminal.rs`、`wasm_core/manager/runtime/tests/session_e2e.rs`（三者是
  `git` 识别出的 rename 目标，提交 rename 必然含其内容）与 `tests/pty_session_chain.rs`
  （同批 pathspec 内）。该 commit message 自述「未包含在途：PTY 解耦票（pty/*）」——意图正确，
  但 rename 的语义使其无法做到。**后果**：单看 `69f7b316c` 这个 commit 时仓库不自洽（含改后的
  `pty_session_chain.rs` 期望 `command_args: Vec<String>`，而 `enums/shell.rs` 的类型改动仍在工作区
  未提交 → 该 commit 单独 checkout 会编译红）。**处置**：按项目纪律不改写对侧历史；本批次剩余改动
  仍在工作区，二者合起来自洽（本机 `cargo test --lib` 1141/0 + 集成 8/8 全绿）。教训入
  MEMORY：对侧做 rename 类提交时，同仓库在途批次对「被改名文件」的编辑会被一并带走
  （`git add <pathspec>` 无法隔离 rename 的双侧内容）。

### P1 — 会话登记 + 状态机 + 生命周期分发下沉（**主票，最大块**）

插件侧新增 `session_registry` 域（建议 `plugins/terminal-session/rust/src/session/`）：

- 内存态：`session-id → { pty_id, name, status, created_at/started_at/stopped_at, canonical_renderer, annotations, owner }`；
  私有库 `sessions` / `session_annotations` 表（`ensure_schema` 幂等，跟随插件既有 schema 迁移链）。
- 状态机：`Starting → Running → Stopped / Error`，含自然退出（`host-pty` 的 `<owner>::pty:exit` 事件驱动，
  替代现宿主 `start_lifecycle_handler` 的终态门订阅，`session_manager.rs:434-485`）。
- 生命周期分发：`Creating / Created / Stopping / Stopped` 由插件内部广播 + 经 `host-events.emit_event` 对外
  （插件已有先例：`auth_http/mod.rs:472`、`task/state.rs:340`）。
- **尺寸裁决事实**：`canonical_renderer` 随会话记录入插件；`ResizeOutcome` 形状在两端 TS/Rust 保持不变
  （它已是 wire 类型，见 `session_components.rs:246-270` 的 camelCase 契约测试）。

宿主侧：**新增窄转发层**（替换 `utils/session_{create,action}_bridge.rs` 的「降级双轨」形态）——
server HTTP/WS 控制器与 Tauri 命令统一经它调插件 api，**不再有宿主降级执行器**（对齐用户 2026-09-22「同类漂移选退役」裁决）。

**P1 宿主侧 ✅ landed（2026-09-23）：会话窄转发层（宿主唯一收口点，行为零变化）**

新增 `src-tauri/src/utils/session_gateway.rs`：宿主侧**调用会话的唯一入口**（查询 / 创建 / 停止 /
移除 / 尺寸 / 输入 / 历史快照 / 输出订阅取消），模块文档内含「今日策略 ↔ P1-b 后」逐行对照表。
此前同一规则散在三处（桌面 Tauri 命令、移动端 HTTP 控制器、移动端 WS 服务各自直连
`SessionManager`，尺寸裁决的「插件优先 + 内核降级」只在桌面命令面存在而移动端两线直连内核）。

消费面全部改经本层：`commands.rs` 5 项（list/get/resize/write/special-key）、
`server/http/controllers/session_controller.rs` 6 端点 + history、`server/websocket/services/session_control.rs`
5 动作、`server/websocket/services/terminal_service.rs` 输入。

**今日策略逐字不变**（本批次是收口不是切换）：创建仍走插件编排（插件必需，无宿主降级）；
停止 / 移除 / 输入仍内核执行器；尺寸桌面路径插件裁决优先（不可用降级内核执行器含内核裁决分支）、
移动端信号路径内核裁决（函数签名不含宿主上下文——「移动端零改动」因此是**结构性**保证）；
历史仍读业务输出环。**未纳入本层**（各归其面）：事件形状与广播（`events/*`）、WS 终端通道的
状态订阅与输出存在性（`channel/terminal.rs`）——随 P3（形态 B）/ P4（事件下沉）收口。

测试：新增 4 项单测锁住今日语义（查询空/播种视图、停止翻 `Stopped` + 移除幂等、尺寸两条路径
四态、历史无环为 `None`）；**变异自检 1 处**（去掉内核降级轨 → 尺寸用例转红后还原）；
宿主 lib **1145/0**、集成 8 target 全绿（含 `pty_session_chain` 真实 PTY+WS 闭环）。

**未做（P1-b 真源切换）**：把本层内部实现由「内核执行器 + 插件桥」换成「纯插件互调 api」——
创建改 `host-pty.spawn`（插件自产 id + 注入 `BEDCODE_SESSION_ID`）、停止 / 移除 / 尺寸 / 输入改插件
api、历史改直读 `PtyRing`；届时 `utils/session_{create,action}_bridge.rs`（含 `Ok(None)` 降级轨）
与内核裁决副本一并退役。

**P1-a ✅ landed（2026-09-23）：插件侧会话登记域落地 + 双写（宿主仍是权威，行为零变化）**

新模块 `plugins/terminal-session/rust/src/session/`（`model` / `ops` / `store` / `registry` / `mod`）：

- `model.rs`：`SessionStatus`（与宿主 `enums::session::SessionStatus` **serde 形状逐字相同**，
  含 `{"error": …}` 形态）+ `SessionRecord`；形状锁定用例把八个取值逐一钉死。
- `ops.rs`：状态机（`is_legal` 矩阵 + 终态不可复活 + 同态幂等不刷时间戳）、记录构造
  （`start = true` → `Running` + `started_at` + 正统端初始归属 = 启动端）、活跃判据
  （与宿主 `filter_active_by_config` 同判据：`!= Stopped`）、id 生成（复用
  `config::model::new_config_id` 单点，插件侧 UUID v4 只此一处）。
- `store.rs`：私有库端口 + wasm 实现 + 内存测试替身；新增 `sessions` / `session_annotations`
  两表（登记进 `schema.rs::prefixed_table_names()`）；状态与归属写 JSON 文本（`Error` 载荷
  与 wire 形状无损）。
- `registry.rs`：内存镜像 + **写库先行**（写库失败不污染缓存）+ 惰性载入 + 稳定读序
  （`created_at, id`；不复制宿主 `HashMap` 迭代序不确定的缺陷）。
- `mod.rs`：激活装配 + 双写门面 + `session.status` 诊断字段。

**激活期对账**（`session::ensure_schema_via_host`）：清空上一进程遗留行——会话与 PTY 同生命周期、
宿主真源同为进程内存，留着只会让 P1-b 起的读取面看到幽灵会话。失败口径与配置面一致（只降级镜像、
不阻断激活），**但 P1-b 起该失败必须改判为「会话面不可用」的显性错误**（本批次唯一语义待收口点）。

**双写接线**（宿主权威不动）：`launch::create_via_host`（创建）、
`actions::{remove, restart, rename, resize}`（移除含注解槽 / 同 id 重建 / 改名 / 归属登记）、
`devices::annotate_via_host`（注解槽）、`lib::on_session_lifecycle`（`Created` → Running、
`Stopping` → Stopping、`Stopped` → Stopped）。镜像写入失败一律 `warn` 留痕并继续（D7 故障隔离），
**非法迁移由状态机显性报错**（不静默改成就近合法值）。观测面 = `session.status` 的
`sessionRegistry: {count, active}`（P1-b 的读面走互调 api，不用诊断面冒充）。

**已知缺口（P1-b 收口）**：宿主不经插件的路径不进镜像——移动端 HTTP/WS 的 `remove` 直连
`SessionManager::remove_session_with_source`，不派发生命周期事件；真源切换后这类路径必须改经插件，
缺口自然闭合。

**测试**：插件 native **251/0**（新增 23 项：wire 形状 / 状态机矩阵 / 时间戳语义 / 记录构造 /
id 形态 / 存储端口行语义 / 注册表读写穿与稳定序 / 注解槽）；宿主 lib **1141/0**、集成 8 target 全绿
（含 `pty_session_chain` 真实 PTY+WS 闭环）；`session_e2e::test_session_create_with_spec_closed_loop`
新增双写断言（两次创建 → `count = 2 / active = 2`）。**变异自检 2 处**（均转红后还原）：
① e2e 期望值改 3 → 红（诊断回执显示 `count:2`）；② `ops::is_legal` 终态出向放行 → 插件侧 2 项红。

**未做（P1-b）**：创建改走 `host-pty.spawn`（会话 id 由插件自产、`BEDCODE_SESSION_ID`
由插件注入、`pty_id` 落库）；插件读面 api；`host-session` 的 `lifecycle-register` /
`input-register` 退役（依附 P2 输入通道）。宿主窄转发层的**真源切换**见上一节。

**P1 前置 ✅ landed（2026-09-23）：引擎层 PTY 生命面（开放点 4 的一半）**

- `wasm_core/host_api/pty.rs` 新增两个**非 WIT** 引擎函数（不设权限门、不取参数）：
  - `kill_all_registered(bus)`：跨属主 kill + 摘除全部在册插件 PTY，逐条**按属主**补发
    `<owner>::pty:exit`（reason=killed）；
  - `live_count()`：在册句柄数（在册即存活——终态由退出监听摘除）。
  - 实现单点：`reclaim_handles` + `registered_handles(owner_filter)`，按属主回收与全量回收
    **共用**同一「摘除成功者才发布事件」不变量（票 04 单一发布者不变量的半段）。
- `system/lifecycle.rs` 关停钩子（优先级 10）接线：业务线 `SessionManager::shutdown()` 之后
  调 `kill_all_registered`——**插件已停用 / 超时 / trap 时仍能回收孤儿进程**（按属主回收依赖
  「插件停用流程被调到」，关机不保证）。P1-b 起业务会话也走 `host-pty`，届时只剩引擎层这一条。
- `SessionManager::live_pty_count()`：判据 `is_running() && !output_terminated()`，语义边界与
  今日关窗守卫口径**逐格对齐**（含只建不启的 `Starting`、含 `Running`、不含已 kill / 已自然退出）；
  两路合取不可省——业务线在册句柄**不随自然退出摘除**，而引擎 `running` 标志**只有 kill/销毁才翻下**，
  只看任一项都会把已退出的会话算成还在。
- 测试：lib **1148/0**、集成 8 target 全绿。新增锁：`live_count_includes_newly_registered_handles`
  （并行安全：只断言下界）、`live_pty_count_counts_unterminated_ptys`（四阶梯）、自然退出用例补
  「在册 ≠ 活」前提断言、`kill_all_reclaim_shares_impl_and_is_wired_into_shutdown`（结构锁：实现单点 +
  关停接线；**不做行为用例**——全量回收跨属主 kill，进程内并行跑会误伤兄弟用例夹具）。
  **变异自检 2 处**（均转红后还原）：① 判据去掉终结位 → 自然退出用例红；② 摘掉关停接线 → 结构锁红。

**P1-b 前置 A ✅ landed（2026-09-23）：`host-app.plugin-resource-dir` 原语（创建路径硬前置）**

- **WIT**：`host-app` 追加 `plugin-resource-dir: func() -> result<string, string>`——**函数级追加不 bump**
  （desktop 仍 v25，与 P4 的 host-session 整 interface 退役同批才 bump）。契约注释写清两条：
  ① 返回值 = 调用方**自己**的安装目录（`extension_path` 剥离 verbatim 前缀，与生命周期事件
  payload 的 `resource_dir` 同值）；② **不设权限门**（同 `host-platform` 口径：无可授予的权力，
  加门只会造出恒过的死门）。未加载插件 / 服务不可用 → `Err`（不静默返回空串）。
- **宿主**：`wasm_core/host_api/app.rs::plugin_resource_dir`（无权限门）→
  `PluginServices::plugin_resource_dir`（`runtime.rs` trait + mock 实现）→ `PluginHost` 真实实现
  （`services.rs` 读 `plugins` 映射 + `strip_verbatim_prefix`）→ `component.rs` 的
  `host_app::Host` 绑定。与生命周期监听器 `listeners.rs` 注入 payload 的那处**同源同形态**。
- **SDK**：`host/app.rs` 的 `HostApp` trait 加 `plugin_resource_dir`（唯一实现者 `WasmHost`，
  无插件侧 mock 需要同步）；`wasm_host.rs` 转调 WIT 绑定。
- **插件**：`lib.rs::on_session_lifecycle(Creating)` **不再**从事件 payload 取 `resource_dir`，
  改经 `host.plugin_resource_dir()` 自取；取不到 → `log_warn` + 跳过注入（不用空串拼路径去读
  一个不存在的文件）。这是本票的关键：**取目录的路径先独立于 `Creating` 事件存在**，
  P1-b 移除该事件时创建路径才不会被自己的下沉卡死。
- **测试**：宿主新增 3 项（`plugin_resource_dir_has_no_permission_gate` 锁「无权限门」口径；
  `plugin_resource_dir_returns_extension_path_of_registered_plugin` 走 `PluginHost::new` +
  最小 TS-only 插件，断言等于安装目录；`plugin_resource_dir_unknown_plugin_errors` 锁未知插件
  显性报错）。**变异自检 1 处**：实现改返回空串 → 路径用例转红后还原。插件 native **251/0**。
- **文档**：ADR 0022 修订记录 v12；CHANGELOG 双语「改进 / Improvements」各一条。
- **未做**：`SessionLifecycleEvent::Creating` 的 `resource_dir` 字段**保留在协议里**（宿主仍
  注入、`terminal-hooks` 等其它消费方不受影响），随 P4 事件面下沉一并处理。

**P1-b 前置 B ✅ landed（2026-09-23）：`host-pty` 配额按 manifest 声明（H1 解决方案）**

- **裁决（用户 2026-09-23 三选定案）**：① 守卫弹窗走「判据用引擎事实 + payload 异步问插件、失败回退空列表」；
  ② H1 取「manifest 声明配额 + 宿主上下限 + 越界报错」；③ 本批只做桌面闭环，移动端输出/历史面随 P3 恢复。
- **契约面**：SDK `PluginManifest` 新增可选 `ptyQuota`（`types.rs`，宿主与 guest 共用同一类型真源）；
  TS 镜像 `packages/plugin-sdk-desktop/src/types.ts` 同步（宿主侧前端 `src/plugin/types.ts` 是**缩减副本**，
  连 `resourceOverrides` / `wasmHash` 都不镜像 → 该处不加，不是漏项）。
- **仲裁分两层，各归其位**：
  - **构建期只校形态**（`bin/manifest-validate.js`：正整数），**不在 JS 里复刻内核上限数字**——复刻等于把
    配额判据拆成两处，改内核常量不会同步过去；
  - **加载期校区间**（`manager/validation.rs::validate_pty_quota`，挂在 `validate_manifest_required`
    这个两条装载入口共用的漏斗上）：`0` 或 `> PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN`（新常量 = 64）
    直接拒绝该 manifest，**不夹取**（与 `ringBytes` 同一口径）。判据落在加载期而不是 `spawn`：
    配额是自我声明的静态事实，拖到运行期等于把配置错误转嫁成「第 N+1 条会话创建失败」的产品故障。
- **引擎面**：`host_api/pty.rs` 新增 `QUOTAS` 表 + `register_quota` / `quota_of`；`spawn` 判据由常量改为本属主
  生效配额，越界文案点名**声明值**。登记表**无记录 = 默认档 8**（既有插件与无头测试上下文零迁移，
  行为与引入声明字段前逐字一致）。登记点与 `grant_permissions` 同处（`manager/loader.rs`，声明面唯一入口）；
  静态注册（inventory）插件不经 `host-pty`，故 `host.rs` 那处授权点不需要补。
  未做注销：配额随 manifest 覆写，卸载后残留条目无消费者。
- **测试**：宿主 lib **1157/0**、集成 9 target 全绿、`[skip]` 计数 0（闭环用例确实跑）；插件 native **251/0**；
  SDK `--lib` **114/0**；`eslint` 对我改的 TS 零输出；`rustfmt --check` 我引入的 3 处偏离已修（`loader.rs` /
  `host_api/pty.rs` 的 import 顺序偏离与 `manager/loader.rs` 测试段偏离是 HEAD 既有，未顺手格式化）。
  新增锁 7 项（`pty_quota_*` / `declared_quota_*` / `undeclared_plugin_falls_back_to_default_quota` /
  `quota_registration_is_wired_into_the_load_funnel` / `validation::pty_quota_*` /
  `manifest_required_check_propagates_quota_rejection`），含验收项「第 9 条会话可创建」的行为锁
  （声明 9 → 9 条全放行、第 10 条点名 `limit 9`）与**反向**锁（声明 2 → 第 3 条即拒，排除「与默认档巧合相等」）。
  **变异自检 1 处**：`spawn` 判据改回常量 → 两项声明用例转红后还原。
- **既有 flaky 修正（本批暴露，非本批引入）**：`live_count_includes_newly_registered_handles` 原断言
  `live_count() >= before + 2`，其中 `before` 是本线程读到的**全局**快照——兄弟用例随时在按属主
  spawn/purge，`before` 里属于别人的部分会在两次读之间消失，**判据正确也会红**（配额用例把在册条数
  拉到 10 后必现）。改为只断言「本属主在册集合 ⊆ 全局在册计数」这一并行安全不变量 + 逐属主精确断言，
  并补 `kill` 一条后的「摘除可见」阶梯。
- **门禁跑法记账**：宿主 `cargo test` 与插件 `cargo test` **并发**跑会让 4 个时序敏感用例连带红
  （`pty_declared_ring_backpressure` / `pty_exit_event_and_purge` / `perf_p2_guest_ring_fetch_batch_curve` /
  `session_annotate_and_devices` 读到兄弟用例的 WS 连接），单独复跑全绿——报绿前先确认没有并发 cargo。
- **未做**：`ptyQuota` 的实际声明方是 P1-b 的 terminal-session 插件（业务会话并发档位，随其接入
  `host-pty.spawn` 同批改 `plugin.json`）；本批只交付内核侧机制与锁。

**P1-b 前置 C ✅ landed（2026-09-23）：插件会话读取面（`session-list` / `session-get`）与 `SessionInfoView` 形状对齐**

新模块 `plugins/terminal-session/rust/src/session/view.rs`（+ `mod.rs` 两个 wasm 门面函数 +
`lib.rs` 两条互调 api），把 P1-b 真源切换时宿主窄转发层要读的那一面先建出来。**行为零变化**：
双写期宿主仍读内核登记事实，本面暂时无消费者。

- **形状红线**：`view_json(record, annotations)` 是 `SessionInfoView` 线形状的**唯一产出口**。
  八个基础字段恒出现（宿主 `SessionInfo` 的两个 `Option` 没有 `skip_serializing_if` →
  缺值是 `null` 而不是「键不存在」，本面对齐）；`sessionType` 恒 `"pty"`；本域私有事实
  （`ptyId` / `canonicalRenderer` / `owner` / `updatedAt`）**不外溢**——历史上不曾在协议里，
  改视图时不得顺手加。
- **注解槽 → 任务字段的解释随真源同迁**：`taskStatus` / `taskReason` / `taskUpdatedAt` /
  `taskQuestions` 四键本就是本插件任务域写的，宿主 `task_fields_from_slot` 那份「机械转发」
  在 P1-b 后退役（同一批键名两处解释本来就是漂移源）。判据逐条对齐：空串 = 缺失、
  `taskQuestions` 是 JSON 文本（解析成数组）、非法 JSON 按缺失处置但**必须留痕**——
  `view_json` 保持纯逻辑（告警作第二返回值），日志在 wasm 门面打，native 可全量断言。
- **时间戳口径差（记账，非缺陷）**：内核 `chrono`（纳秒）vs 本域秒级 RFC3339（插件侧不引
  chrono）。两侧都能被 `DateTime<Utc>` 反序列化、前端按秒渲染；双写期同一会话尾数不同，
  切换后只剩一份。对齐锁因此按秒比较。
- **四处 pin 同步**（api 计数是原子的，漏任一处即漂移锁翻红）：`plugin.json`（27→29）、
  插件 `lib.rs::declares_only_landed_domain_surface`、插件前端
  `plugin-contract.test.ts` 的 `manifest.api` 逐字清单、宿主 `session_e2e` 计数断言 +
  新增读取面两条存在性断言。
- **测试**：插件 native **259/0**（新增 8 项视图用例：八字段恒在 + `stoppedAt: null`、
  字段集合精确相等、空槽无字段、原样透传、空串视为缺失、questions 解析、非法 JSON 告警、
  `Error` 载荷形态）；宿主 lib **1157/0** + 集成 9 target 全绿、`[skip]` = 0；
  插件前端 contract 21/0；`eslint` 对 `plugin-contract.test.ts` 报「文件被 ignore 规则排除」
  （插件前端不在根 lint 范围，非 error）。
  最有价值的一处是 `session_e2e` 的**跨真源对齐锁**：闭环里创建两条会话 → 经插件 `annotate`
  写槽 → 调 `session-list` → 逐项反序列化进宿主 `SessionInfo` 并与内核 `get_session` 的
  id/configId/name/status/sessionType/createdAt/startedAt/stoppedAt 逐字段比相等，
  另锁 `session-get` 未知 id → `null`、缺 `sessionId` → 显性 `Err`。
  **变异自检 1 处**：插件视图把 `configId` 写成 `cfgId` → 对齐锁转红（diff 直接显示两侧值），
  还原后产物摘要回到 `5474808…` 同值，证明确已还原。

**P1-b 阻塞链勘察（2026-09-23，开工前必读）**

1. **真源切换不可半翻**：创建改 `host-pty.spawn` 之后宿主 `SessionManager` 不再持有会话，
   list / get / stop / remove / resize / input / 输出 / 增量事件 / 关停守卫**同时**失去事实来源。
   半翻会让**桌面端**立刻不可用（移动端允许损坏，桌面端要求功能等价）——故 P1-b 必须整批做。
2. **需要的 WIT/ABI 增量（桌面 v26；P4 的 host-session 退役同版本）**：
   - **`host-app.plugin-resource-dir`**：插件今天拿自身资源目录的**唯一**途径是
     `on-session-lifecycle(Creating).resource_dir`；创建改由插件发起后该事件不再产生 →
     `task::hooks::ensure_agent_integration`（hook 脚本源）失去输入。**这是创建路径的硬前置**
     （合裁剪线：宿主知道插件加载路径、零业务语义）。
   - **插件读面 api**（`session-list` / `session-get`）：宿主窄转发层的查询实现改指它们
     （`manifest.api` 27 → 29，四处 pin 同批：`plugin.json` / 插件 Rust 契约用例 /
     插件前端 `plugin-contract.test.ts` / 宿主 `session_e2e::test_session_plugin_artifact_lifecycle`）。
   - **缺两个互调 api**：插件已有 `session-remove` / `session-rename` / `session-resize`，
     但**没有「停止（kill）」与「输入写入」**（今天只有插件命令面 `session.close` 与宿主的写入管线）。
   - `host-pty.spawn` 的**宿主广播声明**字段（P3 子票，§4.2）：`session-id → pty 句柄` 引擎侧只读映射，
     供宿主 server 直读 `PtyRing`（移动端输出面恢复的前提）。
3. **宿主侧同批必做**：关窗守卫判据切「PTY 计数 > 0」（`live_pty_count` 已就位）；
   桌面输出 `output.rs` 由 `host-session.output-ring-fetch` 切 `host-pty.ring-fetch`；
   桌面输入经窄转发层改走插件 api；桌面增量事件改由插件经 `host-events` 发布。
4. **必须先裁决的一处（P1-b 开工前置）**：关窗守卫的**对话框 payload** 是「运行中会话名列表」，
   今日取自 `list_sessions()`（真源切换后为空）。两条路：(a) 关闭路径问插件（与开放点 4
   「不得阻塞关窗」冲突，但关闭路径本就是 async）；(b) 弹窗改「有 N 个会话在运行」计数口径
   （前端改动）。**未裁决前不动守卫判据切换**（判据已就位，可后置）。
5. **移动端受损（允许，逐条记账）**：`GET /api/sessions/{id}/history`、`/ws/terminal/session/{id}`
   的控制帧与输出订阅（依赖 `GlobalOutputManager`，随 P3 直读 `PtyRing` 恢复）、
   `SyncPayload::Session*` 增量推送（随事件下沉恢复）。

**P1-b 开工状态（2026-09-23 收尾记账，下一场会话直接从这里起）**

上面五条阻塞的当前状态：① 真源不可半翻 → **仍是硬约束**（整批做）；② WIT/ABI 增量：
`host-app.plugin-resource-dir` ✅（前置 A）、插件读面 api ✅（前置 C：`session-list` /
`session-get` + `SessionInfoView` 跨真源对齐锁）、**「停止（kill）」与「输入写入」两条互调 api
仍未建**、`host-pty` 宿主广播声明仍属 P3；③ 宿主侧同批必做四项（守卫判据 / 桌面输出切
`host-pty.ring-fetch` / 桌面输入经插件 / 增量事件经 host-events）均未动；④ 守卫弹窗**已裁决**
= 判据走引擎 `live_count()`、payload 异步问插件且失败回退空列表（见 §5 定案）；
⑤ H1 配额机制 ✅（前置 B：`ptyQuota`），但 terminal-session 的**实际声明值**与
`pty:spawn` / `pty:io` / `dependencies: ["host-pty"]` 尚未写进 `plugin.json`（随创建路径切换同批）。

已核实的实施要点（省一轮考古）：

- **插件已有 argv 全套**（`launch.rs::build_argv`：bash -lic / wsl.exe 前缀 / PowerShell
  包装 + 单引号转义 + WSL 路径转换），切 `host-pty.spawn` 只是换送口的字段名与 env 注入位置，
  不需要重写 shell 语义。
- **两阶段 `start=false` 在生产已不可达**（v21 起唯一生产者恒 `start = true`；余下调用全是测试），
  但 `create-with-spec` 的线形状仍允许它——切换时按「记录先落、PTY 后起」建模即可，
  无需为不可达分支保留双态。
- **`create-with-spec` 是 fire-and-forget**：宿主返回 id 时会话尚未真正建立（异步任务里落库）。
  插件自持 spawn 后这一步变同步可见，**回执语义因此变强**，不需要补偿等待。
- **`kill_session` 的 `old_status` 硬编码 `Running`**（`session_manager.rs:807` 区），
  插件状态机更严（终态不可复活）；迁移时按插件语义走，记账说明差异即可。
- **`send_special_key` 绕过钩子链与提交行重建**（只写字节、不翻 Running）——迁移输入面时这条
  不对称要逐字保持，否则特殊键会污染任务域的提交行观察。
- **`host_api/terminal.rs::terminal_send_input`** 是插件前端今天的输入入口（带
  `terminal:input` 权限门 + `ensure_session_owner`）。真源切换后宿主不再有会话归属可查，
  这条要改为转插件 api（或插件前端改走自家命令面）——是本批唯一的前端契约触点。

**门禁与协作事实（本批踩过的坑，下一场照此跑）**：宿主 `cargo test` 与插件 `cargo test`
**禁止并发**（4 个时序敏感用例连带红，单跑全绿）；跑宿主测试前先
`node plugins/terminal-session/scripts/build.js` 并核 `[skip]` = 0；`session_e2e.rs` /
`CHANGELOG.md` / `plugin.json` 此刻有对侧（存量迁移链退役 + peer_net 目录搬迁 + tailwind 收敛）
在途内容——按 pathspec 提交前必须逐文件核归属，必要时走「HEAD + 仅本票 hunk」的 plumbing 合成
（本次 776b29816 即为此法），且 plumbing 提交后**必须刷真索引**，新文件还要带 `--add`。

**P1-b ✅ landed（2026-09-23 深夜起，跨两场会话收口）：会话真源切换**

上面「开工状态」的 ②③⑤ 三项已闭合，① 按「整批做」执行（一次性切，无半翻中间态）。

**插件侧（真源 = `session/` 登记域）：**

- **创建改走 `host-pty.spawn`**（`launch.rs`）：会话 id 由插件自产（复用 `config::model::new_config_id`
  的 UUID 单点）、`BEDCODE_SESSION_ID` 由插件写进 spawn 的 `env`（裁决 ③ 落地；
  `.scratch/2026-09-22-pty-business-downsink` 开放点 1 的旧结论「倾向宿主原语化」随本批作废）、
  cwd 规则（仅原生环境设置）随 argv 一起由插件算、ring 走宿主默认容量。共用核心 `spawn_session`：
  Creating 期的 agent 集成**先于** spawn（H5 的时序由插件自串行保证）→ spawn → 登记 → Created 逻辑 → 广播。
- **`session/mod.rs` 由「双写镜像」升格为真源**：`note_*` 写失败**显性报错**（D7 故障隔离只保留在
  「广播」这一环，尽力投递 + `warn`）；`close_via_pty`（登记 Stopping + `host-pty.kill`，终态一律由
  `pty:exit` 收尾——**SessionStopped 的唯一广播点**是 `on_pty_exit`，避免 kill 与退出双发）；
  `input_via_pty`（提交行重建 + 任务域分发 + `pty_write`，`special` 标记绕过重建，逐字保持开工要点 5
  的不对称）；`summary_json_for`（SessionCreated 自携带概要）；`internal_records_json`（插件内部读
  原始记录含 `ptyId`/`owner`，不经互调面出网）。建表失败随本批**改判为阻断激活**（P1-a 记的「唯一
  语义待收口点」在此收口）。
- **P2 核心被本批吸收**：新 `session/input_line.rs`（`SubmittedLineTracker` 自宿主迁入）——输入改走
  插件后宿主的提交行重建必然断链，不可能留给 P2 单独做。
- 动作面（`actions.rs`）全部改走登记域 + `host-pty`：`remove` 保持**幂等**（未知会话仍 `Ok` 且仍广播
  `SessionRemoved`——多客户端刷新依赖该语义），`close` 保持 fail-visible（未知会话显性报错）。
  输出面（`output.rs`）改 `host-pty.ring-fetch`；设备 / 文件浏览面改 `internal_records_json`；
  任务域（`task/{state,queue,scheduled}.rs`）改登记域视图。
- manifest：api 27→31（`session-list` / `session-get` 于前置 C，`session-close` / `session-input` 于本批）、
  权限 +`pty:spawn` / `pty:io`、`dependencies: ["host-pty"]`、**`ptyQuota: 8`**（开工状态 ⑤ 的实际声明值：
  与退役前的内核上限同档，不借下沉之机放大）。

**宿主侧：**

- `utils/session_gateway.rs` 重写为**纯插件互调 api**（list / get / start / close / remove / resize /
  input / special_key），插件未激活一律显性报错；`utils/session_{create,action}_bridge.rs` 删除
  （含 `Ok(None)` 内核降级轨与内核尺寸裁决副本）。`special_key` 仍在宿主做 `KeyCombo` → 转义字节翻译
  （引擎级终端转义表，随 pty 留宿主），以 `special: true` 交插件写入。`SessionInfoView` 只 `Serialize`
  → 反序列化拆两半场（`SessionInfo` + 任务字段手取）。
- 消费面（`commands.rs` 五项 / HTTP 六端点 / WS `session_control` + `terminal_service`）改注入
  `PluginHost` 并走新签名；关窗守卫判据改「存活 PTY 计数 > 0」（内核 `live_pty_count` OR 引擎
  `live_count`），弹窗 payload 异步问插件、失败回退空列表（裁决 ④）。
- 事件面：SDK `SyncEvent` +4 会话变体（**载荷自足**——宿主不再回查内核）、宿主 `DesktopSyncEvent`
  增携带字段 + `From` 转换、`sync_handler` 处理器「携带优先、内核回查兜底」（内核路径仍服务测试与
  旧生产者）。**未 bump ABI**：会话变体是 `broadcast_sync` 的 JSON 载荷增量，函数签名零变化
  （同 v22 `host-bus` 命名空间那种「不 bump 的行为变更」口径）。
- **actix 同步互调自锁修复**（`host_api/api.rs`）：`api_call` 的总线发布步骤包进 `block_on_async`
  ——否则在 actix current_thread worker 上 `tokio::spawn` 的消费任务永不投递，互调 5s 超时。
  教训入档：**凡 actix handler 里同步等互调 reply 都要走 ambient runtime**。

**输入面收口（本场补，前一场交接文档的漏项——验收红线「桌面端功能等价」的实际断点）：**

真源切换后有两处宿主入口仍打内核 `SessionManager`，编译与既有测试全绿但功能已死：

1. **桌面终端窗口键盘输入**：`TerminalPreview.vue:595` → `context.terminal.sendInput` →
   `api_bridge.rs::plugin_terminal_send_input` → 内核 `write_input` → 按会话 id 查**内核 PTY 注册表**
   → `NotFound`；前端 `void ...` 不接 rejection → 按键静默丢失。改走 `session_gateway::input`
   （→ 插件 `session-input`），门禁三段（身份绑定 / 激活 / `terminal:input`）逐字保留——该命令面
   迁移前就没有属主判定，此刻加判据反而是本批越界。
2. **任务队列下发**：`task/queue.rs` 两处 `host.terminal_send` → `host_api/terminal.rs::terminal_send`
   → `ensure_session_owner` 查内核属主表（唯一写点是 `host-session.create-with-spec`，插件已不调）
   → 恒 `not owner of session` → 每个出队任务刚投递就被标 `interrupted` 并丢弃。改为同实例内直接
   调 `session::input_via_pty`（零跨边界、属主天然成立、`handle_submitted_input` 过滤链已就位，
   与旧「内核写入 → `on_input_submitted` → 同一过滤链」语义等价）。
   `host-terminal.terminal_send` 就此**零生产消费者**，源码文档标注为 P4 退役候选（本批不删）。

**为什么既有测试没抓到**：`session_e2e` 此前只测创建 / 动作 / 注解 / 输出 / 任务域，**没有任何一条走输入面**；
`pty_session_chain` 走的是 WS 终端通道（对插件会话按受损记账）。教训：**切真源时必须把「谁还在读旧真源」
逐条列成消费方清单并对齐到用例**——绿灯只覆盖被断言过的接缝。

**测试（本场新增）：**

- `session_e2e::test_session_input_via_gateway_closed_loop`：真实 bash 会话上锁 5 条契约——
  C-01 普通输入被执行（探针串出现 ≥2 次：tty 回显 + `echo` 输出；只出现一次即「字节进了 PTY 但没被执行」）、
  C-02 未知会话显性报「会话不存在」（无降级轨）、C-03 Ctrl-C 经 `special` 通道写 0x03（tty 回显 `^C`）、
  C-04 非法键名在宿主翻译期即拒、C-05 Ctrl-D 退出 bash → `pty:exit` → 登记域收为 `Stopped`
  （输入面与终态链之间无断点）。**变异自检 M1**：`input_via_pty` 去掉 `pty_write` → 用例转红
  （失败信息直接给出环内实际字节流），还原后产物摘要回到同值，证明确已还原。
- `api_bridge::tests::terminal_send_input_delegates_to_session_gateway_not_kernel`（源码扫描锁）：
  锁「委托目标是转发层、命令体不再出现 `write_input(`、门禁三段仍在」。写它当场就抓到一次自匹配
  （整文件扫描命中锁自身的字面量），已收窄为命令体切片。
- `task::tests::task_sources_never_route_through_dead_host_session_surface`：任务域源码不得出现
  `host.terminal_send(` / `host.session_{get,close,list,annotate,output_ring_fetch}(`——防「顺手改回宿主绕一圈」。
- 集成测试 tracing 级别上限（5 处，前一场）：无过滤的 `fmt::layer()` 会把 wasmtime/cranelift 的逐指令
  TRACE 全落 stdout，cargo 捕获缓冲可撑到 **2.2 GB+ 直至 4 G 分配失败 OOM**（实测）。编译缓存命中时不产
  TRACE → 「改动前能跑」是缓存假象，插件一重建就炸。

**行尾修复（提交前必查，本场实测 5 个文件被整文件转 LF）**：
`src-tauri/tests/{broadcast_shutdown,http_auth_biometric,server_integration,ws_session_route}.rs`
与 `src-tauri/src/server/websocket/services/terminal_service.rs` 在 HEAD 是 100% CRLF，
上一场的编辑（含 python text 模式读写）把整文件静默转成 LF——`git diff --stat` 直接报
1443/1316/672/418 行变更，真实改动只有 1–3 行。已按二进制方式统一回 CRLF
（`d.replace(b'\r\n', b'\n').replace(b'\n', b'\r\n')`），复核后 `--stat` 回到
3/2/2/2/18 行，四个 target 重跑仍全绿。**自查判据**：
`git show HEAD:<f> | grep -c $'\r'` == `wc -l` 而工作区 cr=0 → 即被抹平。

**门禁实测（本场，2026-09-24）**：宿主 lib **1135/0**（`[skip]` = 0，产物摘要 `98b67b5e…`）、
插件 native **287/0**、桌面前端 `vitest --pool=forks --maxWorkers=2` **81 files / 794 tests 全绿**、
根 `eslint .` **0 error**（120 warning 不计门禁）。集成 8 target 逐个串行全绿
（`build_manifest_smoke` / `broadcast_shutdown` / `pty_session_chain` / `ws_auth_rules` /
`ws_session_route` / `server_integration` / `http_auth_biometric` / `link_crypto_http`(4)）——
**禁并行跑，OOM**。`cargo fmt` 只按文件核：`session_e2e.rs` / `task/mod.rs` 的 HEAD 既有偏离未顺手格式化，
本场新增行零偏离（`api_bridge.rs` / `terminal.rs` 单文件 `--check` 干净；CRLF 的 `queue.rs` 只经 Edit 改，
行尾 CR 数与行数一致）。

**未做（P2 剩余 / P3 / P4）**：`host-session.lifecycle-register` / `input-register` 两条注册面仍在
（插件 activate 仍订阅，降为兼容面，随 P4 删）；宿主 `process_terminal_input` 钩子链与
`dispatch_input_submitted` 对**插件会话**不再有流量（逐帧输入修饰与跨插件输入观察随 P4 一并收口）；
`SessionInfoView` 的时间戳口径与 `session-output` 面见 P3。

**移动端受损（P1-b 新增 M6–M9，正文见路线图「移动端受影响清单」）**：WS 终端输出通道对插件会话
`error(SESSION_NOT_FOUND)`、HTTP 历史 404（`GlobalOutputManager` 里没有插件会话的字节）、
内核会话对 WS/HTTP 列表面不再可见（内核登记已空）、`SyncPayload::Session*` 依赖宿主回查的分支退化。

### P2 — 输入通道转发 + 提交行重建迁插件

- `commands.rs::write_to_session` / `send_special_key` → 直接转发插件命令面 → `host-pty.write`。
- `session/input_line.rs` 的 `SubmittedLineTracker` 迁插件（纯状态机，无 OS 依赖）；`SessionInputListener` 观察点
  改为插件内部扩展点 + `host-events` 对外（ADR 0001 需补记「观察点宿主侧退役」）。
- 现宿主 `process_terminal_input` 插件钩子链（`session_manager.rs:583-587`）随写入管线一并归位插件。

### P3 — 输出面（**形态 B 已定案**，见 §5 开放点 1）

- **环本体留宿主 PTY 引擎**（`pty/pty_ring.rs` 的 `PtyRing`）：业务会话既然改走 `host-pty`（P1），其输出环天然就是
  `PtyRing`；内核 `GlobalOutputManager` / `UnifiedOutputQueue` / `SubscriberHandle`（`session_output.rs` 1325 行）
  **整体退役**——它承载的「游标 + ack + 快照 + 多订阅者」语义，拆分为：
  - 插件侧（桌面终端窗口）：`host-pty.ring-fetch` 游标拉取，**已在用**（`plugins/terminal-session/rust/src/output.rs:30-57`，自持游标）；
  - 宿主侧（移动端 WS 通道）：宿主 server **直读同进程内的 `PtyRing`**，零跨 WASM 边界。
- **子票：host-pty 追加引擎级广播声明**（`spawn` config 可选字段，见 §4.2）——宿主据此维护
  `session-id → pty 句柄`只读映射，WS 终端通道（`server/websocket/subscription.rs` / `terminal_ws/subscriber.rs`）
  由「`GlobalOutputManager::session()`」改为「按映射取 `PtyRing`」。**需同步修订 ADR 0022 host-pty 第 2 条措辞。**
- **必须出性能测试（不许估算）**：桌面端多窗口 + 移动端通道**并发**拉取同一会话的端到端吞吐与延迟；
  对照基线 v23 实测 40 µs/op、2.6 ms/MB（`.scratch/2026-09-21-terminal-output-consumer-perf/`）。
  新增关注点：`PLUGIN_PTY_RING_MAX_BYTES = 4 MiB`（`system/constants.rs:230`）在「长会话 + 多端」下的历史深度是否够，
  以及 `PLUGIN_SESSION_RING_FETCH_MAX_BYTES = 16 KiB`（:249）单次拉取上限对桌面端刷新率的影响。
- 移动端 `GET /api/sessions/{id}/history`（一次性历史，`session_controller.rs:243`）改读 `PtyRing` 快照。

### P4 — 退役清理与 ABI

- WIT：`host-session` **整 interface 退役**（12 函数，见 §4.1）→ **ABI bump（desktop v25 → v26）**；
  同步 `packages/plugin-sdk-desktop/rust/src/abi.rs` + CHANGELOG + AGENTS §7 + ADR 0022。
- 删 `src-tauri/src/session/` 整目录、`utils/session_config_bridge.rs`、`events/forwarder.rs`；
  `events/sync_handler.rs` 的 4 个 Session 分支改写（由插件经 host-events 发布，宿主只做 WS 广播转发）。
- `commands.rs` 5 个会话命令注销 + 前端 `composables/commands/sessionCommands.ts` /
  `plugin/context.ts` 的 `SessionAPI` 迁移（插件前端已有等价路径，见测绘 §4.6）。
- `system/lifecycle.rs` 关停与关窗守卫改经插件（见开放点 4）。
- `SessionConfigManager` 空壳随装配链解引用删除。

### P5 — 文档与记账

- ADR 0022 补记（本批次 + 「host-session 退役」+ 裁剪线新判据）；AGENTS §5/§7/§8；code-map；
  CHANGELOG；路线图阶段 4 状态推进；**移动端受损清单**（对照 `plugin-kernel-roadmap/spec.md` 既有的 M1–M5 格式）。

---

## 3b. 票据拓扑（2026-09-24 拆票定稿，取代上面 P2/P3/P4 的粗分）

实施票落在 `issues/01…12`（一票一文件，编号即依赖序）。**本节是权威拓扑**：上面 §3 的
P0–P5 是**阶段划分与裁决记录**，保留作为理由；开工请以票文件为准。P1-b 已 landed
（见 §3 P1 的 landed 记账），故 01–12 全是它的后续。

| 票 | 一句话 | Blocked by | Status |
| --- | --- | --- | --- |
| 01 | 桌面功能等价人工基线（无头测不到的那些） | — | ready-for-human（2026-09-24 agent 侧配套就绪：`baseline-preflight.sh` + `baseline-run-sheet.md`，起跑预演绿；等票 06 落地后由人跑） |
| 02 | prefactor：对外 wire 类型与终端转义表迁出内核会话目录 | — | done（2026-09-24） |
| 03 | 终端输入修饰/提交行观察面退役裁定（P2 残段） | 01 | ready-for-agent |
| 04 | 连接清单迁独立原语（不随会话 interface 一起死） | — | done（2026-09-24 落成 `host-connection` + `connection:read`） |
| 05 | PTY 引擎级宿主广播声明 + 会话→句柄只读映射 | — | done（2026-09-24 landed，字段定名 `hostBroadcastSessionId`） |
| 06 | 移动端输出通道与历史直读引擎游标环（M6/M7 收口） | 05 | ready-for-agent |
| 07 | 多端并发输出实测与环限额裁决（不许估算） | 06 | ready-for-human |
| 08 | 宿主会话命令面与前端会话 API 注销 | 02 | ready-for-agent |
| 09 | 事件面收口：删宿主回查兜底与内核状态订阅转接 | 01, 08 | ready-for-agent |
| 10 | 会话原语整 interface 退役 + ABI 25→26（**唯一**契约破坏点） | 03, 04, 08, 09 | ready-for-agent |
| 11 | 内核会话目录与装配链删除（宿主零会话对象） | 02, 06, 10 | ready-for-agent |
| 12 | 文档与路线图收尾 | 11 | ready-for-agent |
| 13 | 插件 api 防漂移镜像清单落后（file-transfer 编译红，dev 起跑 / CI 双堵） | — | done（2026-09-24 裁定选 B：`#[plugin_api]` 按声明方/消费方分判，消费方改子集校验；file-transfer trait 31→2，ADR 0017 v2） |
| 14 | wasip3 测试夹具落后 `pty_quota`（宿主 `--lib` 在 HEAD 即 6 红） | — | done（2026-09-24 复发面一次清干净：`PluginManifest` 加 `Default` + 六处字面量改 `..Default::default()` + 契约锁；宿主 `--lib` 全绿待对侧 subscription.rs 收敛后复跑） |

**票 13/14 的来源（2026-09-24 补记）**：票 01 人工基线本轮开跑即被挡（宿主 dev 起不来），
顺带暴露两处 **P1-b landed 时没跟演的周边消费者**——13 是另一个插件里的镜像 api 清单
（跨插件防漂移），14 是测试内 WASM 夹具的结构体字面量。两者都在 HEAD 态、与 01–12 的
改动面无交集，属**前置修复**：13 门住票 01（以及依赖 01 的 03/09）能否开跑，
14 门住任何以「宿主 `cargo test --lib` 全绿」为验收口径的票能否出示干净证据。

**拆票原则与取舍**（与仓库先例的差异，供复核）：

- 纵向 tracer-bullet：03/04/06/08 各自穿契约 + 宿主 + SDK/插件 + fixture + 测试，单票可演示；
  不按「宿主层 / SDK 层 / 文档层」横切。
- **prefactor 先立**（02）：协议类型住在内核会话目录里，删除票必然连累对外契约——先搬再删。
- **契约硬约束集中在 10 一个版本点**：P1-b 的会话事件是 JSON 载荷增量（不 bump）、
  05 的广播声明是 spawn 配置字段追加（不 bump），只有整 interface 删除才 bump，
  让插件作者只重建一次产物。
- 宽删除（11）走 expand–contract 的 contract 段：02 expand（类型搬出去）、
  06/08/09/10 migrate（消费面逐批改指）、11 contract（目录消失）。
- **07 是实测票不是实现票**：spec 的性能红线要求数据说话，纸面推导不算达标。
- frontier：02 / 04 / 05 无阻塞，可并行开跑；03 只等 01；**01 的实跑前置是 13**
  （起跑即红），且以「宿主 `--lib` 全绿」为验收口径的票需要先清 14。

**明确不属于本专项**：移动端适配（M1–M9 是它自己的输入清单，见路线图）、
`com.bedcode.terminal` 之类的插件拆分、以及把会话登记域再拆成更细的插件。

### 3b-1 与并发批次 `2026-09-24-host-crypto-business-downsink` 的边界（2026-09-24 对账）

那条线（宿主加密引擎化 + 业务残留下沉）与本专项**同日在同一个 worktree 上跑**，
三处会撞车，已按下述分工写进票面，改票前先读它的 §4.2 三分类清单：

| 撞车点 | 它的票 | 本专项的票 | 裁定 |
| --- | --- | --- | --- |
| 按键组合 → 转义字节的归属 | 06（下沉插件，宿主只收裸字节） | 02 原本要「把转义表搬去中立域」 | **归它**。02 已收窄为只搬内核目录内的视图类型；对侧 06 落地前网关特殊键路径照旧可用，本专项不预先挪位置 |
| `enums/` 下会话类型（状态 / 类型 / 概要 / 启动配置） | 07 / 08（收窄 + 下沉） | 02 / 11 | **归它**。本专项的票遇到定义在 `enums/` 的待搬类型只登记不动，等它落地后由 11 一并收 |
| WS 动作与同步载荷词表声明式化 | 09a / 09b / 09c（expand–contract） | 08 / 09 | **归它**。09 只删「会话事件的宿主回查兜底 + 内核状态订阅转接」，同一 switch 文件里的词表只注释登记不删 |
| ABI 版本号 | 03（`host-crypto` WIT + 权限位 + ABI bump） | 10（会话 interface 退役 + ABI bump） | **已裁定（2026-09-24 用户，串行）**：本专项 10 携带 **25→26**；对侧票 03 走 **26→27** 且 blocked-by 指向本票（它票面现按 v26 写，需对侧批次内自行改记 v27，本会话不代改对侧文件）。它的 07「随会话原语退役」的依赖方向与该裁定一致，无需调整 |

依赖方向补记：它的 07 blocked by 本专项的 10（会话原语剩余函数退役），
所以两条线**不是纯并行**——`host-session` interface 删除是它的入口条件之一。
它的 03 同理被本专项 10 门住（版本号串行）。


---

## 4. 附：现状契约与消费方清单

### 4.1 `host-session` 待退役函数（`packages/plugin-sdk-desktop/rust/wit/bedcode.wit:397-486`）

| 函数 | 行 | 引入版本 | 终态归属 |
| --- | --- | --- | --- |
| `list-sessions` | :398 | — | 插件自己登记 → 删 |
| `get` | :399 | — | 同上 → 删 |
| `create-with-spec` | :421 | v19 | 改 `host-pty.spawn` → 删 |
| `lifecycle-register` | :422 | — | 改插件内部 + host-events → 删 |
| `input-register` | :423 | — | 随 P2 → 删 |
| `close` | :427 | v7 | 改 `host-pty.kill` → 删 |
| `remove` | :431 | v19 | 插件内部 → 删 |
| `rename` | :434 | v19 | 插件内部 → 删 |
| `resize` | :441 | v19 | 改 `host-pty.resize` → 删 |
| `annotate` | :450 | v19 | 插件私有库 → 删 |
| ~~`connections-list`~~ | :459 | v19 | **票 04 已落成**：迁独立 interface `host-connection.connections-list`（权限换挂 `connection:read`）；`host-session` 上仅留同判据别名，随票 10 删 |
| `output-ring-fetch` | :485 | 票 04 | 改 `host-pty.ring-fetch` → 删 |

### 4.2 `host-pty` 现役函数（`bedcode.wit:605-639`，v16，终态唯一 PTY 出口）

`spawn` / `write` / `resize` / `kill` / `ring-fetch` / `is-running`（6 函数）+ `ring-fetch-result` record。

**形态 B 追加的引擎级声明（子票，已 landed 2026-09-24 / 票 05）**：`spawn` 的
config-json 增加可选字段（实施定名 **`hostBroadcastSessionId`** = 本句柄服务的会话
id；spec 暂名 `hostBroadcast` 弃用——布尔名配字符串值会误导，字段同时承担
「opt-in 声明」与「映射键」两个角色），表示「本句柄的输出允许宿主 server 只读订阅」
（插件私有 PTY 的 opt-in，非默认）。宿主据此在**既有句柄注册表**内维护
`session-id → pty 句柄`的只读映射（**不新增第二份表**：登记随 spawn、摘除随终态，
复用生命周期单点），供 WS 终端通道直读 `PtyRing`（零跨 WASM 边界）。
该映射纯引擎事实（无产品语义），符合裁剪线；ADR 0022 host-pty 第 2 条措辞已随
同批修订（「不注册业务输出总线 → 不默认注册；按 spawn 声明 opt-in 只读订阅」）。
**追加字段不 bump ABI**（函数级/字段级追加惯例，v19 先例）。

### 4.3 移动端受损面（**用户说不用管 → 记账，不改移动端**）

- HTTP：`useHttpApi.ts:217-258`（list/start/stop/resize/remove/input 六方法）、`terminal_link.rs:1758`（history）。
- WS：`/ws/event` 的 `SessionControlAction`（启动/停止/移除/尺寸/列表）、`/ws/terminal/session/{id}` 控制帧
  （auth/subscribe/subscribe_ok/history_end/resync/set_mode/session_stopped）。
- 同步事件：`SyncPayload::{SessionCreated, SessionStatusChanged, SessionStopped, SessionRemoved}`
  （`bedcode-mobile/src-tauri/src/enums/sync.rs:15-33`、`handler/sync.rs:21-63`、`useMobileConnection.ts:317-372`）。
- 前端 buffer 状态机：`stores/terminalBuffer.ts`（`idle→connecting→auth→history→live`）、`useTerminalSubscription.ts`。

---

## 5. 裁决记录（2026-09-23，用户定案）

1. **移动端输出面 = 形态 B（宿主直读 PtyRing）** ✅
   环本体留宿主 PTY 引擎，宿主 server 读同进程内的 `PtyRing` 给移动端广播 —— 零跨边界、性能最优。
   连带必做：`host-pty` 追加「按 spawn 声明 opt-in 为宿主广播源」的**引擎级**声明（见 §4.2）+
   **修订 ADR 0022 host-pty 第 2 条**「插件 PTY 不注册业务输出总线」的措辞
   （改为「不默认注册；按声明 opt-in 只读订阅」，理由：该映射只是「会话 id → pty 句柄」的引擎事实，无产品语义）。
   → 影响 P3（从「依赖定案」转为「形态已锁定」）。
2. **「移动端不用管」口径 = 允许损坏 + 如实记账；移动端代码本次零改动，桌面端改造完成后再开适配专项** ✅
   移动端受损面列 M 系列清单（见 §4.3）记入 `plugin-kernel-roadmap/spec.md`；本专项的桌面侧改动**不为其设形状冻结承诺**。
3. **会话 id 由插件生成** ✅：插件自生成会话 id，经 `host-pty.spawn` 的 config `env` 注入 `BEDCODE_SESSION_ID`。
   连带必做：**修订 `.scratch/2026-09-22-pty-business-downsink/spec.md` 开放点 1 的结论**
   （原结论「暂按引擎原语保留 / 倾向宿主原语化」→ 改为「插件持有，宿主不再预生成」）。
4. **宿主关停 / 关窗守卫（我方定案，随 P4 实施）**：
   - **关停**：不经插件——宿主 PTY 引擎层增加「全局 kill 所有存活 PTY」的引擎能力（现 `SessionManager::shutdown()`
     的等价物，`system/lifecycle.rs:254-258` priority 10 处替换）；插件的 `purge_for_plugin` 仍作停用回收。
     理由：关机时插件可能已停用/超时，引擎自持是 H5「同步阻塞点留宿主」的直接落点。
   - **关窗守卫**（`lifecycle.rs:311-326` + `lib.rs:595-613`）：判据从「`list_sessions()` 里有 Running 会话」
     改为**引擎事实「存活 PTY 计数 > 0」**——功能等价（有会话在跑 ⇔ 有活 PTY），且不引入会话语义，
     避免窗口关闭路径被插件异步调用阻塞。
5. **`connections-list`（WS 连接注册表）**：**不随 `host-session` 退役** —— 它是宿主 server 的连接事实、无产品语义。
   处置：迁到独立原语（候选 `host-events` 扩展或新开极窄 interface；实施期按消费方定），
   权限位 `session:read` 中该面的部分随迁。
   **落成（票 04，2026-09-24）**：取「新开极窄 interface」方案 → WIT `host-connection`（函数名沿用
   `connections-list`，因 WIT 保留字 `list` 不可用，故迁出零改名）；`host-events` 落选理由是它是
   **推**语义（emit/broadcast/publish）而本面是**拉**一次快照，且会把 v22 的 topic 命名空间裁决牵进来。
   权限位不是「随迁 `session:read`」而是**新立 `connection:read`**（域名同源，审计单值化），
   且旧 `host-session` 入口改为同判据别名——不留第二把钥匙；属不 bump 的行为变更（fail-visible）。

---

## 6. 验收标准

- [ ] 宿主 `bedcode-desktop/src-tauri/src/session/` 目录**整体不存在**；`utils/session_{create,action,config}_bridge.rs` 无降级分支
- [ ] `pty/` 无 shell 包装 / WSL 转换 / `PtyCommandSource::Business`；`host-session` 不在 WIT
- [ ] 桌面端功能等价：创建 / 两阶段启动 / 停止 / 移除 / 重启 / 改名 / resize 裁决 / 输入（含特殊键）/ 输出 /
      滚动 / IME / 终端窗口 / 通知种子化 —— 逐项与迁移前一致（前端集成测试接缝不变）
      ——**部分核**：创建 / 停止 / 移除 / 重启 / 改名 / resize 裁决 / 输入（含特殊键与
      「特殊键绕过提交行重建」的不对称）/ 输出拉取 已由 `session_e2e` 真实 bash 闭环锁定
      （2026-09-24 输入面收口同批）；两阶段启动 v21 起已退役（生产恒 `start = true`）；
      **滚动 / IME / 通知种子化 无头测不到，需要一次人工 `pnpm run tauri:dev` UI 复核才算勾上**
- [ ] 性能：多端并发拉取端到端吞吐与延迟有**实测数据**且达标（阈值 P3 出，含移动端通道）
- [x] H1（每插件 PTY 上限 8）已有解决方案并有测试（第 9 条会话可创建）——**机制已 land**（P1-b 前置 B：
      `ptyQuota` 声明 + 加载期区间仲裁 + `spawn` 按属主判据，含「第 9 条可创建」行为锁）；
      terminal-session 的**实际声明值**随 P1-b 接入 `host-pty.spawn` 同批写入 `plugin.json`
- [x] 会话 id 由插件生成并经 `host-pty.spawn` 的 `env` 注入 `BEDCODE_SESSION_ID`（宿主不再预生成）
      ——**P1-b landed**（`launch.rs::spawn_session`，闭环锁在 `session_e2e::test_session_create_with_spec_closed_loop`
      与 `test_session_input_via_gateway_closed_loop`）
- [ ] 形态 B 落地：宿主 server 直读 `PtyRing`（零跨 WASM 边界）；`host-pty` spawn 声明有测试锁
      （未声明的句柄**不得**被宿主广播面读到）
- [x] 关停走引擎层全局 kill（插件已停用时 PTY 仍被回收）；关窗守卫改用「存活 PTY 计数」判据
      ——引擎层全局 kill 随 P1 前置 landed；守卫判据随 P1-b landed（弹窗 payload 异步问插件、失败回退空列表）
- [◐] 生命周期对外事件（`session-status-changed` 等）形状不变，或有记账的破坏性变更清单
      ——**记账**：WS 同步面（`SyncEvent` 会话四变体）载荷自足、形状不变；Tauri 前端事件
      `session-status-changed` 仍由内核 `subscribe_status()` 驱动，对插件会话**不再有流量**
      （前端 `stores/session.ts` 无生产调用方 → 影响面为零），随 P4 事件下沉一并收口
- [x] `connections-list` 不随 `host-session` 退役（票 04 落成 `host-connection` + `connection:read`；宿主侧有单钥匙锁 / 字节一致锁 / 生成物词汇锁三条用例，插件侧派生视图回归用例零改动通过）
- [ ] ABI bump 同步四处（WIT / `abi.rs` / CHANGELOG / AGENTS §7）+ ADR 0022 补记（含 host-pty 第 2 条措辞修订）
- [x] `cargo test` 全绿、`pnpm run test:run` 全绿、`eslint` 0 error；测试后无残留进程/端口
      ——**P1-b 批次实测（2026-09-24）**：宿主 lib 1135/0（`[skip]` = 0）、集成 8 target 逐个串行全绿、
      插件 native 287/0、桌面前端 81 files / 794 tests、根 `eslint .` 0 error、`ps`/`ss` 无残留。
      **P3 / P4 收尾各自重跑一遍**（本项是「每批都要过」的门禁，不是一次性勾）
- [x] 移动端受损清单如实写入 `plugin-kernel-roadmap/spec.md`（M 系列格式）
      ——M6–M9 已写入（2026-09-24），含触发条件、状态与 P3/P4 恢复动作；本 spec §4.3 的
      测绘清单保持原样作为输入来源
- [ ] 权限词汇零漂移（本专项预期**删权限位** `session:read` / `session:write` / `session:config` /
      `terminal:observe` 中的一部分 → 必须跑 `gen:permissions` + 五同步点：SDK 词汇表 / 宿主能力清单 /
      host_impl 权限门 / `manifest-gen.js` 映射表 / 前端合法集）

---

## 7. 风险与控制

| 风险 | 控制 |
| --- | --- |
| 输出吞吐（H2 同实例串行）| 形态 B 已把移动端输出面移出跨边界路径（宿主直读 PtyRing）；桌面端走已实测的 `host-pty.ring-fetch`（v23：40 µs/op、2.6 ms/MB）。P3 仍需实测**并发**场景，但不达标时只需调拉取批量/频率，不需回退形态 |
| **H1 上限 8** 直接阻断多会话场景 | 与 P0 同批改 host-pty 限额策略（声明式配额），有测试锁 |
| 一次性大爆炸式改造（3759 行 + 12 原语 + 6 端点的删除） | 严格分票：P0 独立可 land；P1 先「双写」再切真源；P4 只做删除 |
| 宿主 server 转发成为所有会话操作的瓶颈 | P1 的窄转发层批量合并 + 低频操作不敏感；高频只出现在 P3 |
| 关机路径丢 PTY（插件已停用） | 依赖 `host-pty::purge_for_plugin` 兜底 + 开放点 4 裁决 |
| 移动端静默损坏无感知 | 受损清单 + 桌面端每个改动都要回答「移动端这条会不会 404」 |
| 与对侧在途批次撞车（pty / 命令面 / server 均活跃） | 动手前 `git status` + `git log -10` + `find . -newermt "-20 minutes"`；P0 与 `pty-business-downsink` 同文件，先确认该线状态 |
