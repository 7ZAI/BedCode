# 15: 任务域后端搬迁①——Agent 集成与会话状态

**父规格:** `.scratch/2026-09-19-terminal-session-plugin/spec.md` D2 / D3 / D8-P4

**What to build:** 会话一创建就自动装好 Agent 集成（写 hooks）、会话停止时把在跑任务标为中断、Agent 上报的状态与自动授权模式由 `com.bedcode.session` 后端记录并广播——这套语义从旧插件搬到新插件后端，并在搬迁时完成 async 化。旧插件的后端此时仍在原位但不再生效（expand 并存，contract 在 17）。

**Blocked by:** 10（会话编排已在插件，生命周期与输入面不再是宿主回调独占）

**Status:** done（2026-09-20；桌面 `cargo test --no-fail-fast` lib **1062/0** + 8 集成目标 + doc、插件 crate session **157/0** / auto-task 24 / file-transfer **58/0**、SDK **85/0**；桌面前端 **259 files / 759 tests**、根 eslint **0 error / 127 warning**（零新增）。实施记录、边界决定与取舍见 Comments）

- [x] Agent 集成模块（多 Agent hooks 安装 / 清理 / 模板版本触发重写）、会话状态与会话映射模块、Agent 能力与预设任务模块搬入新插件后端
- [x] async 化：整批文件遍历与长循环按 WASI 0.3 async 语义让出；不得在 async store 内出现无让出阻塞循环
- [x] 编排反转落地：插件作为会话编排方主动推进集成注入，反向回调兼容面保留但本插件不再依赖它驱动主流程
- [x] 能力面只用宿主基础服务：文件读写、插件私有库、事件广播、配置读取、定时器；不新增任何宿主命令
- [x] 与搬迁前实现做对照测试（同一输入 → 同一 hook 写入结果与状态推进序列）
- [x] 错误隔离验收：三域各自结果收口、禁止跨域持锁、定时器回调按命令名分域计数失败只降级本域
- [x] 新插件后端 `cargo test` + 桌面 `cargo test` 全绿

## Comments

### 2026-09-20 实施落地

**前置**：上一会话已把 `plugins/auto-task/rust/src/{agent,hooks,state,queue,preset}.rs`
搬入 `plugins/session/rust/src/task/` 并把 `crate::` 引用改到 `crate::task::*`
（CRLF 行尾，逐处 edit 不改行尾）。本会话接着做**接线 + async 化 + 对照与错误隔离**。

**1. 边界决定：queue.rs 随 state 一起搬（沿用上一会话的决定）**

`state.rs ↔ queue.rs` 是循环依赖（state 调 queue 的广播/在途判定/调度，queue 调 state 的
开关与建行），无法分开编译。故票 16 剩余范围为：`scheduled.rs` 搬入、`_http_endpoint`
（state / queue / scheduled 三路路由，path 段不改）、私有库表名前缀统一与幂等重命名迁移、
hook 脚本资源接线、广播面等价对照、队列状态机四态迁移与恢复路径单测。

**2. 编排反转的落点（清单第 3 项）**

- `activate()`：`task::ensure_schema_via_host()` 建六张表 + 注册**生命周期监听**与
  **输入监听**（`session_lifecycle_register` / `session_input_register`）——这是本插件
  自己推进编排的载体；失败只 warn 降级任务域，不打 Degraded（与票 08 配置面同口径，D7）。
- `on_session_lifecycle(Creating)`：agent 识别 → 读 `ConfigKey::NetworkPort` → 
  `ensure_agent_integration(...)`。`resource_dir` 由宿主在事件 payload 注入——**宿主无
  「插件自取资源目录」原语**，这是本插件拿到自身资源目录的唯一途径；对自己经
  `create-with-spec` 创建的会话同样会收到该事件，故「主动推进」与「回调兼容面」共用
  同一实现，不存在第二条注入路径。
- `on_session_lifecycle(Stopped)`：`interrupt_running_tasks_on_session_end`（不用
  Stopping：PTY 未终止时 agent 仍可能推送终态）。`Created` 留给票 16（定时任务域的
  会话就绪匹配）。
- `on_input_submitted`：空行 / `/` 命令 / 未受支持 agent / 已有在途任务四道过滤后
  `create_task_from_input`（与旧插件逐字同序）。
- `deactivate()`：`cleanup_all_agent_integrations`（与旧插件停用同语义）。

**3. 定时器（清单第 4、6 项）**

清单第 4 项把「定时器」列为任务域的能力面、第 6 项要求「回调按命令名分域计数失败」，
故本票落 `session.task.scheduler-tick`（间隔 1s，与旧插件同——放宽会让「上一任务输出被
立刻清屏」），驱动 queue 的两个周期步骤。分发表 `task::run_tick_domains` 按域名顺序
独立执行，单域失败只登记该域、**不短路其余域**；票 16 的 scheduled 域并入同一张表，
无需新增定时器。

为让「按域计数失败」真实可观测（而非永远 Ok 的空壳），把 `send_due_clears` /
`check_executing_silence` 的私有库查询失败由 `.ok()` 静默改为冒泡 `Result<(), String>`
——**行级失败仍在循环内自愈**（回滚 pending + 广播 revert，语义不变），只有「这一轮连
在途项都查不到」才算本域降级。

**4. 注解槽写面（票 12 contract 的写入方归位）**

`state.rs` 的四个状态推进点（调度建行、输入建行、hook 推送、会话结束兜底）统一调用
`publish_task_slots`，写 `taskStatus` / `taskReason` / `taskUpdatedAt` / `taskQuestions`
四键——键名是内核 `task_fields_from_slot` 的机械映射 contract，漂一个字移动端字段就恒空。
`taskUpdatedAt` 取宿主时钟 `ConfigKey::CurrentTimeMs` 经 `rfc3339_from_unix` 转 RFC3339
（内核对外字段是 String，内核测试钉的形状是 `2026-09-20T00:00:00Z`）。写槽失败只 warn：
槽是投影，`task_history` 行才是本域真源（D7 单域降级）。

**5. async 化（清单第 2 项）**

`yield_guard` 只插在**无宿主调用的纯循环**上（每 64 次迭代读一次宿主时钟制造 await 点）：
`hooks::cleanup_all_agent_integrations` 外层配置循环（working_dir 为空时整轮 continue）、
`state::backfill_working_dirs`（纯内存回填，历史行可能上千）、`queue::send_due_clears` /
`check_executing_silence` 的行循环（continue 分支无宿主调用）。已有宿主调用的循环本身就有
await 点，不再叠加。

**6. 权限与命令面（D2 一一对应）**

新增六位：`fs:read` / `fs:write`（写项目级 hook）、`terminal:input`（队列下发）+ 
`terminal:observe`（提交输入行监听，ADR 0001）、`broadcast`（任务 / 模式 / 队列广播）、
`timer:schedule`。未声明 `terminal:output`（本域不消费输出回调，多一位即审计噪音）。
命令面新增 23 条 `session.task.*`（**非互调 api**，`manifest.api` 仍 21 项）。
pin 同步四处：`plugin.json`、插件 Rust 契约用例、插件前端 contract 用例、宿主产物生命周期用例。

**7. 本票不落 HTTP 面**：`_http_endpoint`（state / queue / scheduled 路由）是票 16 清单项
（「HTTP 端点 path 段一个不改」），票 15 的对照与验收走命令面 + 生命周期/输入回调接缝。

**8. 顺带修的既有断裂（票 14 的跟演漏项）**

`file-transfer` 测试 `MockHost` 缺 `platform_local_ipv4_addresses`（票 14 往
`HostPlatform` trait 追加函数时未同步该消费方的 mock），该 crate `cargo test` 直接编译失败。
已补 `unimplemented!()` + 注释（与同文件 `platform_wsl_distros` 同处理），file-transfer 回 **58/0**。

### 门禁

- 桌面 `cargo test --no-fail-fast`：lib **1062/0**（1061 基线 + 本票 1 条）+ 8 个集成目标
  全 ok（线协议用例零改断言）+ doc 1 passed / 2 ignored
- 插件 crate：session **157/0**（153 基线 + 4：tick 分域隔离、让出护栏、跨域持锁禁令、
  命令面接线与 agent registry 对照）、auto-task 24/0、file-transfer 58/0；SDK **85/0**
- 新增宿主闭环 `wasm_runtime::test_session_task_domain_closed_loop`（S1，真实产物 + 真实
  宿主原语 + 真实私有库）：agent 名单 / 预设任务往返 / 会话开关写入读回 / tick 两域执行 /
  Creating 对未适配 agent 零写入 / **输入提交 → in_progress → 注解槽 → Stopped → interrupted
  状态推进序列** / 未知命令显性报错
- 产物已重建入 `resources/plugins/desktop/com.bedcode.session/`（manifest 与源逐字同步）
