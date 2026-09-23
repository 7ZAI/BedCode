# 03: 终端输入修饰与提交行观察面退役裁定（P2 残段）

**What to build:** 把「谁还能修饰/观察用户往终端敲的东西」这件事**定案并落地**：
内核里那条逐帧输入修饰钩子链与「注册了就能收到提交行」的观察注册表，在会话真源迁插件后
已经收不到任何真实流量；本票要么把它们干净退役，要么把跨插件观察能力换成一条有 ACL 的
总线广播——**不允许留在「代码在、永远不触发」的状态**，那是下一个断链的种子。

**Blocked by:** 01（人工基线先确认真机上这些面确实观察不到差异，才知道退役有没有代价）。

**Status:** done（2026-09-24 落地，裁定 = 退役；人工核验项待 01 基线复跑，见票末）

## 现状取证（2026-09-24 实测，开工时先复核）

- 修饰链的唯一生产调用点在内核写入管线里；该管线自 P1-b 起对插件会话不再有流量
  （桌面终端命令与移动端 WS/HTTP 输入都已改走窄转发层 → 插件 `session-input`）
- 宿主 `host-terminal` 的输入注入原语属主判定查内核属主表 → 对真实会话恒拒，零生产消费者
- `host-session` 的两条注册面（生命周期 / 输入观察）插件仍在 activate 时调用，
  但**没有任何东西再往里派发**（提交行观察已在插件自家写入管线内自驱）
- WIT 的 `terminal-hooks` 输入/输出导出：**无任何生产插件实现**（只有 SDK 默认 no-op），
  探针用例是唯一调用者 —— 开工时把这条再 grep 一遍确认，防止有插件在这期间落地了实现

## 验收标准

- [ ] 先做决定并写进票末：**退役**（默认推荐）还是**换成总线广播提交行 topic**。
      若选后者：topic 走既有属主命名空间与跨属主订阅拒绝规则，插件按声明订阅，
      且这条通道**必须**有真实消费者（无消费者的抽象不开）
- [ ] 插件 activate 不再调用两条注册面；宿主侧注册表与派发点、内核修饰链一并删除
- [ ] WIT 里随之失去意义的导出（终端钩子的输入/输出）在本票**只登记不动**——
      interface 级删除与 ABI bump 统一在 10 定稿（契约硬约束只 bump 一次）
- [ ] 行为回归：任务域提交行观察不变（受支持 agent 会话里敲一行仍生成任务记录，
      队列自身投递仍被跳过、不自触发循环）；01 的清单里 IME / 粘贴两项复跑无差异
- [ ] 为「删干净」上锁：源码扫描或存在性断言，防止后续票又把注册面接回来
      （插件任务域已有同形态的「不得再经已失效宿主面」扫描锁，照抄思路）
- [ ] 门禁：宿主 `cargo test --lib` + 集成 8 target 逐个串行全绿、插件 native 全绿、
      桌面前端全绿、根 `eslint .` 0 error，产物摘要变化后重跑（禁用旧产物）

## 边界与不做

- 不碰 `connections-list`（有真实消费者，走 04）。
- 不 bump ABI（统一在 10）。
- 不复活「宿主代写终端输入」的任何降级轨。

## Comments

### 2026-09-24 · 裁定 = **退役**（不换成总线 topic），并已落地

**裁定与理由**：票面给了两条路——「退役」或「换成总线广播提交行 topic」。选**退役**，因为
后者的开票条件自己写着「必须有真实消费者（无消费者的抽象不开）」：探明后**没有任何消费者**。
P1-b 真源下沉后：
- 创建 / 终态由 `com.bedcode.terminal-session` 自驱（`launch::spawn_session` 先行
  `flush_pending_restart` + `task::scheduled::handle_session_created`；
  `<owner>::pty:exit` → `session::on_pty_exit` → `interrupt_running_tasks_on_session_end`）；
- 提交行重建与任务域分发在该插件 `session::input_via_pty` 内完成（`handle_submitted_input`）。

即**兼容面的六个回调动作在自驱路径上逐条有等价实现**（grep 逐条核过），宿主那两条注册面
只剩「内核直连路径（测试）能触发」。换 topic 等于把一份没人要的抽象搬到总线上。

### 实施：宿主侧观察面整体拆除

| 拆除对象 | 说明 |
| --- | --- |
| `SessionManager` 的 `lifecycle_listeners` / `input_listeners` 两张注册表 | 连同 `register_*` / `remove_*` / `dispatch_lifecycle_event` / `dispatch_input_submitted` 六个方法 |
| 生命周期分发调用点 4 处 | `create_session_from_spec` 的 `Creating`/`Created`、`kill_session_with_source` 的 `Stopping`/`Stopped`、`start_lifecycle_handler` 的终态 `Stopped` |
| `session/input_line.rs` **整文件删除** | `SessionInputListener`（观察扩展点）+ `SubmittedLineTracker`（提交行重建器）——观察面没了，重建器无消费者；行重建真源在插件 `session/input_line.rs` |
| `PluginHost::process_terminal_input`（逐帧输入修饰链） | `write_input` 改为**原样写字节**（`pty_registry.write_input(session_id, data)`）；修饰链是「宿主改名用户输入」的最后一处入口 |
| `PluginHost::process_input_submitted` | 提交行观察分发 |
| `wasm_core/manager/host/listeners.rs` **整文件删除** | `PluginLifecycleListener` / `PluginInputListener`（宿主事件 → 插件导出的桥） |
| `PluginHost::dispatch_{lifecycle,input}_to_plugin` + `is_activated_block` | 两个派发点 |
| `PluginServices::{register_session_lifecycle_listener, register_session_input_listener}` | trait 方法 + 全部 mock 实现 |
| 停用清理点 `activation.rs` | 原按 `plugin_id` 摘监听器的两行 |
| 插件 activate 的两条注册调用 | `plugins/terminal-session/rust/src/lib.rs` |

输出侧 `process_terminal_output` / `has_terminal_handlers` **未动**：它们服务的是业务输出环
（`session/session_output.rs`），属票 11 的删除面，不在本票。

### 关键裁决点：WIT 面「只登记不动」逼出的形态

票面要求 interface 级删除与 ABI bump 统一在 10，于是票 03 不能删 WIT 里的
`host-session.lifecycle-register` / `input-register`（删函数也是契约破坏，会破坏「只 bump 一次」）。
但注册表都拆了，这两个 host function 不能继续假装能注册。

**处置**：`wasm_core/host_api/lifecycle.rs` 降级为**显性失败的退役占位**——
调用即返回点明原因的 `Err`（`… is retired (session lifecycle dispatch moved into the
com.bedcode.terminal-session plugin); rebuild the plugin artifact with the current plugin SDK`）
并 `warn!` 留痕。三个理由：
1. 旧产物（≤ ABI v25）调用时拿到**可诊断的失败**而不是静默成功（票 10「旧产物失败形态可诊断」
   的前半段提前就位）；
2. 不留「代码在、永远不触发」的表象（它是**被调用且失败**，不是被遗忘）；
3. 契约面零变化 → 本票不 bump。

配套反向锁 `retired_register_surfaces_fail_visibly_with_rebuild_hint`：即便把历史权限位
（`session:read` + `terminal:observe`）全授予，两条面仍必须失败，且文案必须含
`retired` 与 `rebuild the plugin artifact`，两条面文案不得相同（分不清是哪条没了就白报）。

插件侧的 `on_session_lifecycle` / `on_input_submitted` **导出实现保留不动**：它们属 WIT
导出面（`interface events`），随票 10 与该 interface 的收口一并处理；本票只在其实现注释里
标注「派发源已退役」。WIT 侧同批加了**只登记结论**的注释（两条 import + 两个导出）。

### 为「删干净」上的锁

`wasm_flow_test::retired_session_observation_surface_is_not_reintroduced`：扫宿主
`src/**/*.rs`，非注释行出现 `register_lifecycle_listener` / `register_input_listener` /
`register_session_lifecycle_listener` / `register_session_input_listener` /
`dispatch_lifecycle_to_plugin` / `dispatch_input_to_plugin` 任一字样即失败并打印
`文件:行:内容`。注释行豁免（各模块的「为什么删」说明段落是**记账**，不是回接）。

### 门禁实测（本票）

- 宿主 lib：**1147 passed / 0 failed**（`[skip]` = 0）
- 集成 8 target **逐个串行全绿**：`build_manifest_smoke` / `broadcast_shutdown` /
  `pty_session_chain` / `ws_auth_rules` / `ws_session_route` / `server_integration` /
  `http_auth_biometric` / `link_crypto_http`(4)
- 插件 native：**298 passed / 0 failed**；产物按当前 SDK 重建（wasmHash `ec2a01b5…`）
- **变异自检 1 处**（新增锁）：临时在 `session_manager.rs` 加一行
  `pub async fn register_lifecycle_listener_probe(&self)` → 新锁转红并精确点名
  `session/session_manager.rs:166`，还原后复绿。

### 记账

- 测试数 1157 → 1147 的差额 = 删除的观察面用例（`input_line.rs` 整文件随删、
  两条派发用例、一条注册路径用例、`process_terminal_input` 透传用例、host_api 四条 → 一条）。
- 票面验收第 4 条里的「01 清单里 IME / 粘贴两项复跑无差异」**未跑**：01 人工基线本轮仍为零条
  观测（票 01 末 Comments）。用户 2026-09-24 裁：本会话连前置一起做，人工验收项后置由人跑。
  据此本票在「人工核验」一项上**不算勾**。
