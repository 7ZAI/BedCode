# 11: 内核会话目录与装配链删除（宿主「零会话对象」达成）

**What to build:** 本专项的验收红线落地：**宿主里已经没有「会话」这个内核模块**。
删除内核会话目录（登记、状态机、生命周期事件机制、业务输出环、注解槽、属主表、
配置装配空壳），装配链与关停/守卫的引用改指引擎事实。做完之后，「谁是会话真源」
在代码层面只有一个答案：`com.bedcode.terminal-session` 的登记域。

**Blocked by:** 02（协议类型已迁出）、06（输出面已改直读引擎环，业务输出环才可删）、
10（宿主对外会话原语已消失，才没有回读入口）。
另需 07 的性能结论已定稿（环限额若需调参，调的是引擎环，别让删除票顺手改语义）。

**Status:** done（2026-09-24 落地；人工核验项待 01 基线复跑，见票末）

## 验收标准

- [ ] 内核会话目录整体不存在；`git grep` 在宿主源码内找不到「会话登记 / 会话状态机 /
      业务输出管理器」的引用（测试夹具一并清理，不许留 `#[cfg(test)]` 僵尸）
- [ ] 应用上下文的会话字段、宿主会话执行端、`host-session` 遗留实现文件清空；
      关停与关窗守卫只引用 PTY 引擎事实（存活句柄计数 / 全局回收）
- [ ] 注解槽机制随属主表一起退役：任务字段透传已活在插件登记域（P1-b 已改），
      宿主侧不得留「机械搬运键值对」的第二份实现
- [ ] 依赖内核执行端的测试夹具重写为插件背书（P1-b 已有互调播种 helper 与网关调用形态可复用），
      **禁止**为了省事把已删的内核入口复活成「测试专用」通道
- [ ] 事件面收尾：会话事件的宿主侧只保留「转发 + WS 广播」，无任何回查会话事实的代码路径（09 的锁继续有效）
- [ ] code-map 里内核会话一节整节删除，Quick Navigation 的「会话内核遗留线」条目移除；
      AGENTS §5/§7 的「遗留/待退役」措辞改为完成态
- [ ] 门禁全套：宿主 `cargo test --lib` 全绿（新基线数）、集成 8 target 逐个串行全绿、
      四插件产物重建后 `[skip]` = 0、桌面前端全绿、根 `eslint .` 0 error、
      01 人工清单复跑无回归、测试后无残留进程/端口
- [ ] 提交前自查行尾：`git show HEAD:<f> | grep -c $'\r'` 与工作区 CR 数一致，
      防止整文件行尾抖动混进删除 commit

## 边界与不做

- 不改插件侧任何语义（真源已在插件，本票纯删除）。
- 不动移动端。
- 不在本票做 P5 之外的文档（12 负责对外文档与路线图记账）。

## Comments

### 2026-09-24 · 落地：内核会话目录整目录删除（宿主「零会话对象」达成）

#### 一、删除面（3813 行 → 0）

`src-tauri/src/session/` 整目录 + 聚合模块 `src-tauri/src/session.rs` 删除：

| 文件 | 内容 | 处置 |
| --- | --- | --- |
| `session_manager.rs` | SessionManager（登记 / 状态机 / 属主表 / 注解槽 / 生命周期分发） | 删 |
| `session_output.rs` | 业务输出管理器 + 业务输出环 `UnifiedOutputQueue` + 内核订阅执行体的支撑类型 | 删（其中 4 个**共享**类型迁出，见下） |
| `session_components.rs` | 三个注册表（PTY / SessionInfo / 正统渲染端）+ `resolve_initial_size` | 删（外部消费者**零**，逐条 grep 过） |
| `session_lifecycle.rs` | `SessionLifecycleEvent` / `SessionLifecycleListener` | 删（派发源票 03 已无） |
| `session_event.rs` | `SessionStatusEvent`（状态广播） | 删（唯一生产消费者 = 已删的内核 status 兜底 watcher） |
| `session_config.rs` | `SessionConfigManager` | 删（`AppContext::config_manager()` **零调用方**） |
| `session.rs` | 聚合与重导出 | 删（`SessionStatus` / `SessionType` 的真源本就在 `crate::enums`） |

#### 二、共享类型迁出（不是「换地方放」）

业务输出环文件里有 4 个类型**不是会话专属**，删除时迁到唯一消费者身边：

- `SubscribeResponse` / `SubscriberHandle` / `SubscriberStats` → `server/websocket/terminal_ws/subscriber.rs`
  （它们是订阅执行体 ↔ 连接 actor 的契约，消费方只有 WS 订阅侧）；
- `MODE_REALTIME` / `MODE_BATCH`（双速传播模式）→ `server/websocket/terminal_ws/forward.rs`
  （`should_flush` 的输入就是它，原先挂在会话层只为「避免 session 反向依赖 server」）。

`UnifiedOutputQueue` / `OutputEvent` / `SessionOutputSink` / `GlobalOutputManager` /
`SessionOutputManager` / `PullSubscriber` 是**纯内核**形态，随目录删除不复存在。

#### 三、装配链与守卫改指引擎事实

- `AppContext`：删 `session_manager` / `config_manager` 两个字段 + 访问器 + builder 方法；
  `lib.rs` 不再构造二者、不再 `app.manage(...)` 它们；`PluginHost::new` / `WasmHostContext::new`
  的两个形参删除（5 处构造点同改）。
- `system/lifecycle.rs`：
  - 关停：原「业务线 `SessionManager::shutdown` + 引擎 `kill_all_registered` 双线」→ **只留引擎**
    （业务会话的 PTY 早就是引擎句柄）；
  - 关窗守卫：原「内核 `live_pty_count()` ∨ 引擎 `live_count()`」→ **只留引擎 `live_count()`**
    （内核那条自己就写着「P1-b 后生产为空，仅测试」）。

#### 四、移动端 WS 终端通道：内核兜底腿全部删除

票 06 之后WS 面是「引擎优先 + 内核兜底」的双分支，票 11 把兜底腿全部删掉，各点**只剩一条来路**：

| 位置 | 原兜底腿 | 现在 |
| --- | --- | --- |
| `subscription.rs` 订阅 | 无引擎句柄 → 回落内核 `SessionOutputManager` 建订阅 | 无句柄 = 订阅不存在 → `None` |
| `subscription.rs` 退订 | `GlobalOutputManager::unsubscribe` | 退休本连接的引擎句柄；成功判据取「退订前本连接是否订着」 |
| `subscription.rs` ack | 无引擎句柄 → 回落内核 ack 水位 | 无句柄 = 迟到的 ack → 丢弃并 `trace` |
| `channel/terminal.rs` 存在性 | `broadcast_handle_for_session` ∨ `has_session` | 只 `broadcast_handle_for_session` |
| `channel/terminal.rs` 停止通知 | 无广播声明时起内核 `status` watcher | 删除（会话一律经 `host-pty` 创建并声明广播） |
| `session_gateway::history_snapshot` | 回落 `GlobalOutputManager::snapshot_bytes` | `?` 直接返回 `None` |
| `session_gateway::unsubscribe_output` | 内核环退订 | 函数删除（连接侧随引擎终态帧自理） |
| `conn.rs` / `websocket_manager.rs` 清理 | 逐连接 `unsubscribe_all_for_client` | 删除（引擎句柄连接私有，随 `SubscriptionState::cleanup` 退休） |
| `terminal_service::handle_input` | `session_manager` 存在性门 | 参数删除（内层一律走 `session_gateway`） |
| `session_control::handle_control*` | `session_manager` 参数（只作 `is_some()` 门） | 参数删除 |

`terminal_ws/subscriber.rs` 的内核订阅执行体（`spawn_subscriber` 297 行 + `subscriber_loop`
+ `park_until_ack` + 其 14 条用例）删除；引擎路径保留并成为唯一形态。**误删后已还原**
`flush_planner` / `sleep_until_opt` / `ParkExit`（引擎循环同样在用——第一次划范围时按
「内核专用」误划进去了）。

#### 五、为「删干净」上的锁

`retired_kernel_session_domain_is_not_reintroduced`（`wasm_flow_test.rs`）：扫宿主
`src/**/*.rs` 非注释行，出现 `crate::session::` / `SessionManager` / `SessionConfigManager` /
`SessionOutputManager` / `GlobalOutputManager` / `SessionInfoRegistry` / `SessionOutputSink` /
`UnifiedOutputQueue` 任一字样即失败并打印 `文件:行:内容`。注释行豁免；两张锁自身
（`sync_handler.rs`、本文件）跳过——它们把标识符当字符串去匹配。

**变异自检 1 处**：在 `pty/pty_ring.rs` 插一行 `const _MUTATION_PROBE: &str = "SessionManager";`
→ 锁转红并精确点名 `pty/pty_ring.rs:183`，还原后复绿。

#### 六、门禁实测

- 宿主 lib：**1061 passed / 0 failed**（较票 10 的 1116 少 55 = 删除的内核订阅执行体用例 14 条、
  `session_manager.rs` 全部用例、`session_output.rs` 全部用例、pty 业务线用例若干，
  净加 1 条新锁）
- 集成 **7 target 逐个串行全绿**（`build_manifest_smoke` / `broadcast_shutdown` /
  `pty_session_chain` / `ws_auth_rules` / `server_integration` / `http_auth_biometric` /
  `link_crypto_http`(4)）
- 前端：**未重跑**（本票零 TS / 前端改动，沿用票 10 的 80 files / 787 tests 绿态）
- 插件侧未改动，native 用例沿用票 10 的 298 / 98 / 59 / 15

#### 七、诚实记账：一处覆盖损失（`ws_session_route`）

`tests/ws_session_route.rs` **整文件删除**。它的四个场景全部架在内核会话线上：
用 `GlobalOutputManager::register_session` 注册**假会话**让存在性校验通过、用 `OutputEvent`
往业务环灌输出、用内核 `status_tx` 发 `SessionStatusEvent` 触发 `session_stopped` 帧——
这三个入口都随目录消失了。

改造成引擎形态需要「在集成测试里造一个声明了 `hostBroadcastSessionId` 的引擎句柄」，
而 `host_api::pty` 的广播句柄**没有测试注入点**（只有 `spawn` 时登记），为测试开一个
等于往生产模块加测试通道——本票不做，留作后续项。

**损失的唯一覆盖**是「WS 会话路由的端到端接线」（存在性校验 → 认证 → TB v3 帧 →
session_stopped）：其中 TB v3 帧与终态帧在 `engine_subscriber_loop` 的单测里逐条覆盖
（`engine_history_replays_then_history_end_then_live` /
`engine_terminal_grace_drains_and_emits_session_stopped` 等 6 条），鉴权在 `ws_auth_rules`
覆盖；**未覆盖的是路由级的接线本身**。建议后续补一条引擎形态的集成用例（spawn 真 PTY
并声明广播 → 走同一条 WS 路由）。

#### 八、记账

- 票面验收第 3 条（01 人工清单复跑）**未跑**：01 基线本轮仍为零条观测。
  用户 2026-09-24 裁：本会话连前置一起做，人工验收项后置。据此本票在「人工核验」
  一项上**不算勾**。
- 顺带补了票 10 的一处遗漏：8 处测试授权清单里的 `"session:write"` 未随权限位退役清理
  （`grant_permissions` 传的是字符串，运行时无害但属词汇漂移），本票一并清扫。
