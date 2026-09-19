# 01: PTY 引擎前置改造——退出码收集 + 输出汇可注入

**What to build:** 让宿主 PTY 引擎在**业务终端会话零感知**的前提下，补齐插件原语需要的两项地基能力：① 进程退出后能拿到退出码；② 输出读取线程可以把字节投递给**调用方指定的目标**，而不是只写进业务会话的输出总线。做完这一步，后续 host-pty 既能复用同一条读线程，又不必把插件私有输出灌进业务会话链路。

**Blocked by:** None（can start immediately）

**Status:** done（2026-09-19；桌面全量 `cargo test` 936 + 8 集成目标全绿，真 PTY 证据与变异自检见 Comments）

## 已定案的行为（spec D3/D4 + 摸底结论）

- **为什么先做**：现读的 PTY 读取线程把输出硬绑业务会话的输出总线，且 kill 路径直接丢弃底层 `ExitStatus`——不先解耦，host-pty 只能复制一份读线程（两套语义漂移）或污染业务链路（2026-09-17 刚重构完的背压链回归风险）。
- **输出汇可注入**：读取线程的投递目标改为参数/ trait 注入。业务会话继续走现有总线（默认实现），插件侧（票 02 的 PtyRing）走自备 sink。生产端「源零等待」原则不变——目标慢绝不把背压踢回 PTY 读取端。
- **退出码**：在 PTY 会话层补收集退出状态的能力，随生命周期事件带出（票 04 消费）。**禁止轮询 waitpid**；若底层绑定限制导致不可行，按 spec D4 降级为不带 exitCode 并在本票 Comments 记录原因，不阻塞主线。
- **边界**：本票不碰 WIT、不碰 ABI、不加权限、不建 PtyRing、不新增宿主能力面。

## 验收

- [ ] 业务终端会话链路（创建 → 输出 → 输入 → kill）行为零变化；既有真 PTY 会话链路集成测试全绿
- [ ] 真 PTY 单测：短命命令正常退出后可读到退出码；被 kill 与正常退出的状态可区分
- [ ] sink 注入单测：同一读线程分别投递到业务总线与自备 sink，字节内容与顺序均正确
- [ ] 无 ABI/WIT 改动；错误传播带操作上下文（AppError 规范），无 `let _ =` 静默忽略错误
- [ ] 桌面 `cargo test` 全绿；测试后清理残留 PTY 进程/线程（AGENTS.md §3）

## Comments

（实施记录追加此处）

### 2026-09-19 实施落地

**Status:** done（落地时的中间态为「code-complete，验证受阻」——并发会话在途改动曾阻塞全量验证，后已解阻；时间线见下「验证过程的时间线」，最终收口见票 07）

**改动文件**

- 新增 `src-tauri/src/pty/output_sink.rs`：`PtyOutputSink` trait（`async on_bytes(bytes, timestamp_ms)`）+ 业务默认实现 `SessionOutputSink`（构造 `OutputEvent` 并投递 `GlobalOutputManager`）。`session_id` 从读线程剥离到业务 sink，读线程不再认识业务总线。
- 新增 `src-tauri/src/pty/lifecycle.rs`：`PtyTerminated { status, exit_code, killed }` + `PtyTerminationGate`——**两路信号齐备才发一条终态事件**：① 读线程关闭（尾帧已投递）、② 子进程回收完成（退出码此刻可用）。回收走 `spawn_reaper` 专属 OS 线程持有 `Box<dyn Child>` 做阻塞 `wait()`，**无轮询**。
- `pty_reader.rs`：入参 `lifecycle_tx` → `sink` + `gate`；队列载荷 `OutputEvent` → `(Vec<u8>, i64)`；有序队列/源零等待语义原样保留。
- `pty_process.rs`：`PtySession` 增 `sink` + `slave_policy`；构造函数三分——`new`/`with_id`（业务线：默认 sink + `Hold`）、`with_private_sink`（插件私有 PTY：自备 sink + `ReleaseOnSpawn`，即票 02 PtyRing 的注入点）；`PtySessionState` 的 `pair` 拆为 `pair`（未启动）+ `master` + `slave_hold`；`start()` 把 `Child` 交回收线程并按策略决定 slave 去留；`kill()`/`Drop` 置 `kill_requested`；`subscribe_lifecycle()` 载荷改 `PtyTerminated`；openpty/spawn/resize/clone_reader 错误补操作上下文。
- `session_manager.rs`：生命周期处理器适配新载荷（状态映射逻辑不变，仍只取 `status`）。

**发现并修正 spec D4 的错误前提（用户裁决 = 方案 B）**

spec D4 假定「进程退出 → 读线程 EOF → 生命周期事件」，实测**不成立**：

- 证据 1：`script -qec "bash -lic \"cd '/tmp' && pwd && exit 7\""` → `rc=7`，进程确实自然退出。
- 证据 2：引擎探针 `reaped ok ExitStatus { code: 7, signal: None }` → 回收侧正常，退出码可得。
- 证据 3：`mark_reader_closed` 直到 15s 超时、`PtySession` 被 drop（slave fd 随之释放）后才打印 → **父进程只要持有 `pair.slave`，内核就不让 master 读返回 EOF**。原代码 `state.pair = Some(pair)` 正好把 slave 一直养着。
- 推论（既有事实，非本次引入）：业务线 `start_lifecycle_handler` 等的就是这条 EOF，所以**今天会话自然退出也不会翻 Stopped**，只有 kill/销毁才翻。

裁决取 B：**仅插件私有 PTY 释放 slave fd**（`PtySlaveFdPolicy::ReleaseOnSpawn`），业务线保持 `Hold` 以守住本票「业务链路行为零变化」。统一为 `ReleaseOnSpawn` 会让业务会话在自然退出时开始翻 Stopped 并下发状态事件（跨线产品行为修正），已登记为票 07 的抽取候选。

回归锁：`hold_policy_keeps_natural_exit_unobserved_until_killed`——若有人改默认策略，该用例失败并强制评估业务状态机影响。

附带修正：原 `start()` 取完 `process_id` 后直接 drop `Child`，子进程沦为僵尸不回收；现由回收线程 reap，满足 AGENTS §3「测试后无 PTY 进程残留」（实测 `cargo test pty::` 后无 `sleep 30`/`bash -lic` 残留、无僵尸）。

portable-pty 源码核实：Windows ConPTY 的 master/slave 共享 `Arc<Mutex<Inner>>`（`win/conpty.rs`），释放 slave 只是丢一个 Arc 克隆，不影响输入/输出管道；unix 侧 slave fd 正是挡住 EOF 的那个引用。**Windows 侧仍未在真机验证**（本机 Linux），票 07 文档需带此注记。

**行为契约**

| 契约 | 来源 | 规则 | 预期 |
| --- | --- | --- | --- |
| C-001 | 票面「退出码随生命周期事件带出」 | 读线程关闭 + 回收齐备 | 恰好一条 `PtyTerminated`，`exit_code` 取自回收侧 |
| C-002 | 同上（恰好一次） | 任一信号重复到达 | 不产生第二条事件 |
| C-003 | 尾帧完整性 / exitCode 可用性 | 仅一路信号到达 | 不发事件 |
| C-004 | 代码分支 | 信号到达顺序（EOF 先 / 回收先） | 结果一致 |
| C-005 | `wait()` Err 分支、线程启动失败 | 回收失败 | 仍发事件，`exit_code: None`（不挂死消费方） |
| C-006 | spec D2 生命周期语义 | `kill()`/`Drop` 之后 | `killed = true`；未 kill 为 `false` |
| C-007 | 用户裁决 B | `Hold`（业务） | 自然退出不发事件 |
| C-008 | 用户裁决 B | `ReleaseOnSpawn`（插件） | 自然退出即 EOF → 发事件且带退出码 |
| C-001s | 票面「投递目标改为参数注入」 | 业务默认 sink | 字节按序完整落入会话环（现役链路零变化） |
| C-002s | 同上 + ADR 0022 业务隔离 | 自备 sink | 字节按序落入自备缓冲，业务会话环零留痕、不注册总线 |
| C-003s | 源零等待契约 | 零订阅者 | 产出仍全量入环（既有回归护栏保留） |

**测试矩阵**（`pty/lifecycle.rs` 7 例 / `pty/output_sink.rs` 3 例 / `pty/pty_reader.rs` 6 例 / `pty/pty_process.rs` 新增 5 例真 PTY）

| 场景 | 类型 | 关键断言 |
| --- | --- | --- |
| EOF 后未回收 → 无事件；补回收 → Stopped + exit_code Some(0) | 反例+正例 | `TryRecvError::Empty` → `PtyTerminated` 全字段 |
| 回收后未 EOF → 无事件；补 EOF → 带退出码 | 反例+边界 | 顺序无关，`exit_code` 仍来自回收侧 |
| 两路齐备 + 重复信号 | 正例/幂等 | 事件恰好一条（第二条 `Empty`） |
| `wait()` 失败 / 无退出码 | 异常 | 事件仍发出且 `exit_code == None` |
| kill 请求位 true / false | 正反例 | `killed` 字段翻转 |
| 真 PTY 释放 slave + `exit 7` | 正例 | `exit_code == Some(7)`、`status == Stopped`、`killed == false` |
| 真 PTY 释放 slave + `true` | 边界 | `Some(0)`（与「取不到」的 `None` 区分） |
| 真 PTY `Hold` + `exit 7` | 反例（现役锁） | 500ms 内无事件；随后 kill → `killed == true` |
| 真 PTY `sleep 30` + kill | 反例 | `killed == true`、`is_running()` 转 false |
| 真 PTY + 自备 sink（`echo MARK`） | 正例+副作用 | sink 收到 MARK；`has_session(sid) == false`；exit_code Some(0) |
| 同一读线程 → 业务总线 / 自备 sink 各一次 | 正反例 | 9000B 负载分多块，字节序列全等 + 分块顺序全等；另一侧无留痕 |

**验收清单**

- [x] 业务终端会话链路行为零变化（`Hold` 策略 + sink 默认实现；pty 全套含既有真 PTY 用例 `write_str_reaches_process_output` / `start_spawns_real_process_and_reports_running` / `resize_after_start_does_not_panic` 全绿）
- [x] 真 PTY 单测：自然退出读到退出码（Some(7)/Some(0)）；kill 与自然退出可区分（`killed` 位）
- [x] sink 注入单测：同一读线程分别投递业务总线与自备 sink，内容与顺序均正确（9000B 多块全等等值断言）
- [x] 无 ABI/WIT 改动；错误带操作上下文；本票改动内无 `let _ =` 静默忽略（`kill()` 的 ctrl-c/exit 兜底改 `debug!` 记录、unix 强杀改 `match` 打点）
- [x] **桌面全量 `cargo test` 实跑全绿**：`936 passed; 0 failed`（lib）+ 8 个集成测试目标全 ok，其中 `tests/pty_session_chain.rs::pty_session_chain_flow`（创建 → 输出 → 输入 → kill 业务真 PTY 链路）为验收第 1 条的集成级证据
- [x] 测试后清理：无 `sleep 30` / `bash -lic` 残留进程、无本票遗留僵尸（`Child` 现由回收线程 reap）

**变异自检（已实跑，非预测）**

| 变异 | 结果 |
| --- | --- |
| `try_complete` 去掉 `reaped` 条件（EOF 即发） | 8 例失败：`reader_closed_alone_does_not_emit`、`reap_*`、`both_signals_emit_exactly_one_*`、`reader_closed_before_reap_*`、reader 3 例、`natural_exit_*` ✅ 杀死 |
| `ReleaseOnSpawn => Some(slave)`（fd 策略空转） | 3 例失败：`natural_exit_*`、`successful_exit_*`、`session_with_private_sink_*`（15s 超时）✅ 杀死；`hold_policy_*` 仍绿（正确，行为退化等同 Hold） |
| `killed: false` 常量 | 3 例失败：`emits_killed_when_kill_requested`、`kill_marks_termination_event_as_killed`、`hold_policy_keeps_natural_exit_unobserved_until_killed` ✅ 杀死 |

三次变异后均已还原。**未覆盖风险**：① Windows ConPTY 释放 slave 的真机行为未验证；② `pkill -9` 等外部杀手段无法在测试里与 `exit 1` 区分（设计上由 `killed` 位承担，仅限宿主发起的 kill）；③ `hold_policy_*` 用 500ms 负向窗口，理论上极慢机器不变（EOF 在 Hold 下不可能发生），但仍是一个时间型断言。

**验证过程的时间线（并发会话阻塞，已解）**

同一工作区有认证中心线（host-auth / wasip3 宿主 async 化）在途编辑，两次挡住 `--tests` 编译：先 `wasm_runtime/component.rs`（12 error，`Results: 'static` 不满足 + 测试里对未 `.await` 的 future 调 `is_err()`），后 `host_impl/auth.rs`（`crate::session::config` / `crate::plugin::fs_auth` 未存在）。本票全程未碰对侧文件（AGENTS §11 在途改动红线），`pty::` 全套在窗口期先跑绿，对侧收尾后全量复跑 EXIT=0。

