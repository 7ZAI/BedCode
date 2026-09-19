# 04: 生命周期面——kill / 退出事件 / 停用回收

**What to build:** 插件的 PTY 有始有终：主动 kill 能终止并销毁句柄；进程因任意原因退出（正常退出 / 被 kill / 出错）时属主插件都能收到一条 owner 作用域的 `pty:exit.<owner>` 事件（含原因，退出码视票 01 能力）；插件被停用时宿主自动回收其全部 PTY，不泄漏孤儿进程，也不牵连同宿主上其他插件的 PTY。

**Blocked by:** 02（注册表与接线就绪）、01（退出码能力，经 02 传入）

**Status:** done（2026-09-19；kill / `pty:exit.<owner>` / 停用回收三面贯通并接线 deactivate，宿主 28 例 + e2e 3 条全绿，桌面 lib 988 绿；本票由本会话在并发会话 07:44 的在途代码上接手收尾（用户裁决），变异 M1/M3/M4/M5 杀死、M2 存活原因见 Comments；两条红项均属认证中心线在途，非本票）

## 已定案的行为（spec D2/D4/D10/D11）

- **kill**：属主校验 → 复用既有终止语义（优雅中断 + 兜底强杀）→ 摘注册表 + 释放 ring → 补发 `pty:exit.<owner>`（reason=killed）。
- **退出监听**：宿主守护订阅 PTY 会话生命周期状态迁移，收到停止/错误 → 发布 `pty:exit.<owner>`（payload camelCase：`{ ptyId, reason: "stopped"|"killed"|"error", exitCode? }`）→ 摘除注册表、释放 ring。
- **exit 即摘除**：不做延迟保留。竞争窗口写进 SDK/契约文档注释：插件应先把输出消费完（next-offset 不再前进）再等退出事件。
- **事件面只有这一条**：spawn 成败在返回值里（无状态事件）、错误直接上抛（无错误事件）；走 host-bus 既有 publish 路径，topic 内嵌 owner 使非属主物理订阅不到。
- **停用回收**：插件 deactivate 路径追加 `pty::purge_for_plugin(plugin_id)`（紧邻既有同类回收），只碰本人。
- **自愈契约**：bus 不缓冲不重放；晚订阅期间丢的事件由 is-running 快照补齐（票 03）。
- **SDK**：`PTY_EXIT` 常量 + `pty_event_topic(event, plugin_id)` 助手 + 订阅时序硬提示（activate 期订阅）。

## 验收

- [x] kill 后进程真实消失、句柄不可再用、ring 释放，且属主收到 reason=killed 的退出事件
- [x] 进程自然退出（短命命令跑完）→ 属主收到退出事件（票 01 交付 exitCode 时断言其值正确；降级则记录原因）
- [x] 非属主插件在自身订阅窗口内收不到他人 `pty:exit.<owner>`
- [x] deactivate 插件后：其全部 PTY 进程消失、注册表无残留、每条 PTY 补发 exit 事件；同宿主其他插件的 PTY 不受影响
- [x] spawn 失败路径不产生任何事件（回归断言，与票 02 一致）
- [x] 宿主单测（回收只碰本人 / 事件定向投递）+ fixture 断言全绿；桌面 `cargo test` 全绿；测试后无孤儿进程、无端口残留

## Comments

### 2026-09-19 接手收尾（并发会话在途代码之上）

**交接事实**：本会话 07:44 开工时，票 01/02/03 已 done/code-complete，而 `host_impl/pty.rs`（07:44:58）、`wasm_runtime.rs`（07:43:10）正被另一条会话实时写入票 04 的代码；该域当时 **17 例红**。用户在 07:55 裁决「我接手票 04」后由本会话完成，前序会话的对侧文件（`plugins/devices/*`、`utils/auth/*`）全程未碰（AGENTS §11）。

**接手时对侧已落地的部分**（保留原样，不重写）：`pty_kill` 实装、`exit_reason_of`、`pty_exit_payload`、`publish_pty_exit`、`spawn_exit_monitor`、`purge_for_plugin`，以及「start 之前订阅终态」「remove 成功者才发布」两条防御。

**本会话的四项改动**

1. **修掉生产级 panic（关键）**：`spawn_exit_monitor` 原用 `spawn_with_error_boundary`（内部 `tokio::spawn`）派生监听任务。但 WASI 预打开模式下插件调用跑在**无 runtime handle 的阻塞线程**上（`block_on_async` 的 ambient 分支注释即为此），`pty_spawn` 的同步路径直接调它 ⇒ `panic!("there is no reactor running")`，panic 穿透污染 wasmtime Store、插件整体失效——**不是测试专属问题，生产同样命中**。改为 `spawn_with_error_boundary_on(&ambient_handle(), ...)`，并把监听体抽成 `reap_and_publish`（命名 future、脱离缩进地狱）。这也是接手时 16 例 `host_impl::pty::tests::*` 红 + e2e「环空」的唯一根因（e2e 侧因 `pty_spawn` 在宿主函数里 panic → guest 拿到 error 载荷 → 拉取永远空）。
2. **停用回收接线**：`host.rs::deactivate_plugin_inner` 在 mdns / ws 回收之后追加 `pty::purge_for_plugin(plugin_id, &self.message_bus)`，**位置在 `remove_all_subscriptions` 之前**（否则补发的 `pty:exit` 已无人可投）。总线同一 `Arc` 由 `WasmHostContext::new` 传入（`host.rs:169`），与插件 `bus_subscribe` 用的同源。
3. **票 02/03 用例的载体迁移（「exit 即摘除」的必然连带）**：短命命令（`/bin/true`、`/bin/echo`、`/usr/bin/env`、`/bin/stty`）在断言前就被摘环，会撞 `pty handle not found` 而不是它要测的行为。统一换成常驻载体：常量 `ALIVE`（`sh -c "…; read go"`）与 `alive_with_output()`；三条 e2e 同理，并在结尾 `pty-kill` 清场（AGENTS §3）。具体三处判据升级：
   - `spawn_applies_declared_env_and_size_only` → `spawn_applies_declared_env_without_business_identity`（尺寸另有 `spawn_applies_requested_terminal_size` 常驻化用例承接），并删掉随本票失效的「kill 必回 not implemented」断言；
   - argv 免 shell 用例改以常驻 `sed` 为载体：宿主若做 shell 包装，`sed` 的参数会被截成 `s/^/literal;`、`echo PWNED` 另行执行，连续字面量 `literal; echo PWNEDbody` 不再可能拼出——判据比原来的 `PWNED` 计数更强；
   - `is_running_true_while_alive_and_false_after_natural_exit` → `..._handle_retired_after_natural_exit`：只断可确定的两端（alive=true / 摘除后 `Err(not found)`）。**但票 03 的变异 C-②（去掉 `!output_terminated()` 判据）原本就靠这一格翻红**——摘除语义把窗口压成毫秒级，端到端再也抓不住它。故把判据组合抽成纯函数 `running_verdict(running, output_terminated)`，新增 `is_running_verdict_covers_all_four_states` 四格真值表锁回该变异（M5 实证），`running` 不翻的引擎语义仍在票 01 的 `lifecycle::reader_closed_accessor_tracks_signal_one`。
4. **fixture 补 `pty-kill` 命令**（`packages/plugin-pty-test/src/lib.rs`），并把票 02 遗留的「kill 命令面随票 04 接入」文档行清掉。

**行为契约（新增 8 例：7 例本票 + 1 例承接票 03，逐条对测试）**

| 契约 | 来源 | 规则 | 测试 |
| --- | --- | --- | --- |
| C-101 | spec D6 | kill 过权限 + 属主后发起终止，属主收 reason=killed，句柄随摘除不可寻址 | `kill_terminates_handle_and_publishes_killed_event` |
| C-102 | spec D4 | 自然退出 → reason=stopped + 真实 exitCode（票 01 能力端到端） | `natural_exit_publishes_stopped_event_with_exit_code` |
| C-103 | spec D4 | `exitCode=0` 与「回收不到退出码」必须可区分（有值 vs 缺字段） | `zero_exit_code_is_reported_as_value_not_absent_field` |
| C-104 | spec D4/D7 | 事件只达 owner 作用域 topic；一条 PTY 恰好一条（不重放、不双发）；不牵连他人 | `exit_events_are_owner_scoped_and_exactly_once` |
| C-105 | spec D2 | 停用回收摘本人全部 + 逐条补发 killed + 集合式寻址 + 只碰本人 | `purge_for_plugin_retires_all_owned_handles_and_touches_nobody_else` |
| C-106 | spec D5 | spawn 失败零事件、零句柄（回归票 02 契约） | `failed_spawn_publishes_no_event_and_registers_nothing` |
| C-107 | D7/SDK | 宿主事件名与 topic 形状与 SDK `PTY_EXIT` / `pty_event_topic` 逐字一致（漂移锁） | `exit_event_name_matches_sdk_subscription_helper` |
| C-108 | 票 03 承接 | `is-running` 判据四格真值表（EOF 那一格必须 false，不得只信 `running`） | `is_running_verdict_covers_all_four_states` |

最高 seam e2e 新增 `test_pty_exit_event_and_purge_roundtrip`（A/B 双 runtime 双实例）：kill → guest `on_message` 真收到 `pty:exit.<A>`（topic/sender/reason 三断言）→ `pty-is-running` 回 `not found`；自然退出 `exit 3` → `exitCode==3` 经 WIT+bus 原样送达；`purge_for_plugin(A)` 恰 1 条且**恰好一次**、B 的句柄仍 running、B 的事件流为空。deactivate 的真实调用点没有 `PluginHost` 夹具可用，故以 `include_str!("host.rs")` 的接线锁 + 上述行为用例组合兜住（票 06 若上 PluginHost 级集成可替换）。

**实跑证据**

- `cargo test --offline --lib -- plugin::manager::wasm_runtime::host_impl::pty` → **28 passed / 0 failed**
- `cargo test --offline --lib -- ::test_pty` → **3 passed / 0 failed**（票 02/03/04 三条 e2e）
- `cargo test --offline --lib` → **988 passed / 1 failed**；唯一红项 `test_devices_plugin_artifact_lifecycle`（认证中心线 devices 插件在途用例，`wasm_runtime.rs:3377` 断言其 manifest 的 `permissions` 数组）。**归属证据**：同一条 lib 套测在 08:33 跑出 **987 passed / 0 failed**（本票 pty 用例当时已计入），而 `plugins/devices/plugin.json` 在 08:50:07 又被对侧改写（942 → 937 字节）后才翻红；本票全程未碰 devices 侧任何文件（AGENTS §11），登记待对侧收尾后复跑。
- `cargo test --offline --tests --no-fail-fast` → lib 987 + 8 集成目标绿（含 **`pty_session_chain` 业务链路绿**＝本票动 `pty_kill`/回收对业务线零影响的回归锁），唯一红 `broadcast_shutdown`＝认证线 `utils/auth/jwt.rs:43` 的 error 级日志（票 02 已登记同一项）
- `rustfmt --edition 2021 --check src/.../host_impl/pty.rs` → 无 diff（只格式化本票自有文件，未跑全仓 `cargo fmt`，避免重排对侧在途文件）；`cargo clippy --offline --tests` → `host_impl/pty.rs` 无告警（顺带清掉票 03 留下的一处 `repeat().take()`）
- 测试后进程核查：无 `sh -c read/echo/exit`、无 `sed`/`stty`/`cat` 残留、无 `pty-reaper` 线程、无监听端口占用（AGENTS §3）

**变异自检（实跑后全部还原）**

| 变异 | 结果 |
| --- | --- |
| M1 `exit_reason_of` 忽略 `killed` 位（恒报 stopped） | 宿主 `kill_terminates_*` + e2e 双双 FAILED（`left: "stopped" right: "killed"`）✅ 双杀 |
| M3 `purge_for_plugin` 去掉属主过滤（回收越界碰全员） | `purge_for_plugin_retires_*` FAILED（`left: 3 right: 2`）✅ 杀死 |
| M4 `publish_pty_exit` 丢 owner 后缀（发成全局 topic） | 4 例 FAILED（kill / natural_exit / purge / e2e 全部等不到投递）✅ 多杀 |
| M5 `running_verdict` 去掉 `!output_terminated`（只信 running 标志） | `is_running_verdict_covers_all_four_states` FAILED（27 passed / 1 failed，其余无串扰）✅ 杀死——票 03 的 C-② 判据锁已迁到确定性真值表 |
| M2 `subscribe_lifecycle()` 挪到 `start()` 之后 | **存活**——短命命令的终态在 µs 级窗口内仍赶得上订阅，竞态不可确定性触发 |

`M2 存活`的处置：该防御（订阅早于 start，广播不补发历史）保留构造顺序 + 原注释，**不**为其编造弱断言或睡眠放大；风险登记在下节。

**并发在途登记（不修对侧，AGENTS §11）**

1. `test_devices_plugin_artifact_lifecycle`（devices 线，`plugins/devices/` 未跟踪且 08:25 / 08:29 / 08:50 仍在被对侧写入）——本票全程只做 pty 相关追加，最后一次全量 lib 跑到 **988 绿 / 1 红（即该用例）**，同套测在 08:33 为 987 全绿；对侧收尾后需复跑。
2. `broadcast_shutdown` 的 error 级日志（`utils/auth/jwt.rs:43` secret-store 回退）——同票 02 登记项，仍未由对侧收口。

**未覆盖风险（本票遗留）**

- 「start 之前订阅」的时序防御无可杀变异（M2），若未来 `session.start()` 前出现阻塞/延后（例如改异步装配），短命 PTY 的事件可能真丢且句柄永久泄漏——彻底免疫需要「已终结即可立即摘除」的查询面（`PtyTerminationGate` 已有 `reader_closed`，缺 `emitted` 访问器），记进票 07 的抽取候选。
- deactivate 真实路径（`PluginHost::deactivate_plugin_inner`）未做行为级 e2e，当前由接线锁 + `purge_for_plugin` 行为用例 + e2e 投递证据组合覆盖。
- purge 补发的事件能否在停用窗口内送达 guest，取决于 dispatcher 与 `remove_all_subscriptions` 的先后（本票把 purge 放前，e2e 实证送达）；生产停用若引入提前注销订阅，事件会静默丢失（宿主侧无感）——票 06 的隔离矩阵宜补一条 deactivate 端到端。
- Windows/macOS 未验证：`kill` 的 `taskkill` 分支与 ConPTY 下的 EOF/回收时序按票 01 注记待发布前实机验证。

