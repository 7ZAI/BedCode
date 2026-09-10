# 任务图进程边界可靠性协议（进程握手 / 两阶段结果提升 / 代际 token / 结果索引）

---
status: accepted
---

auto-task 任务图（DAG）编排把每个图节点定义为独立非交互进程、以 `exit code` 为权威终态（spec 决策 D1）。但**「exit code 权威」在三个场景下会失真**：① 进程 spawn 成功 ≠ agent 真正启动（配置错、模型 key 无效会秒退，节点长时间滞留 running）；② 下游读取上游输出文件时可能读到**半写的文件**（多图并发 + 断点续跑时尤其危险）；③ 重试/取消/迟到的旧退出回调可能**被误判为新一轮的完成证据**（run-id 不唯一）。本 ADR 借鉴 `pi-subagents` 扩展（`~/.pi/agent/git/github.com/nicobailon/pi-subagents`）的四项进程级可靠性模式，为进程节点补齐「在崩溃、重试、并发下依然权威」的承诺。宿主零新增能力（遵守 ADR-0022），四项全部在插件层以「文件 + 原子 rename + token」实现。

## Considered Options

### 1. 启动握手：给 `ready → running` 加一个可验证的前置条件

- **维持现状（不做握手）**：`process_run` 返回 run-id 即置 running，启动失败（agent 二进制缺失、凭据错误、参数非法）只能等 `on_process_done` 才暴露。问题：失败被当成"运行中"，用户看到的是长时间 pending/running 假象；且图编排无法区分「启动即崩」与「运行中崩溃」，`fail-fast` 语义不可靠。否决。
- **插件层启动握手**：节点进程启动后写 `{nodeId}.startup.json { state: "ready", token }`，插件确认后才把 `ready → running` 推进；超时（默认 30s，与 pi-subagents `RUNNER_STARTUP_TIMEOUT_MS=10s` 同级量级、按 agent 冷启动放宽）未确认则标 failed 并 `process_kill`。采纳。这是 D4 状态机的一个**前置守卫**，不改变状态机拓扑。

### 2. 两阶段结果提升：下游永远读不到半写的输出

- **维持现状**：`process_run` 输出落盘完成后 `on_process_done` 才来——但「落盘完成」与「文件内容完整」是两回事：agent 进程可能写了输出后**未退出**（缓冲未刷）、或输出文件被**并发续写**（断点续跑）。下游按 done 事件去读可能拿到截断 JSON。否决。
- **done 哨兵 + 原子 rename**：节点写完正式输出后写 `<node>.output.done` 哨兵文件（内容 = 输出文件路径 + 写完全时间 + 本轮 token），**下游与模板解析只在哨兵存在后读取**；输出文件本身可放 `<node>.output.pending` 再 `fs::rename` 成 `<node>.output.json`（rename 原子，POSIX/Rust `std::fs` 直接支持）。采纳。两阶段（哨兵 = 完成承诺，rename = 原子发布）互补：哨兵先于发布，rename 保证读侧要么看到旧完整、要么看到新完整。

### 3. 代际 token（generation token）：把「这是这一次运行的退出码」钉死

- **维持现状**：运行实例表存宿主 run-id，`on_process_done` 按 run-id 收敛。问题：节点超时被杀（进程组未死透）→ 用户重试（新 run-id）→ **迟到的旧进程退出回调**仍携带 run-id（若宿主复用时序）或与新一轮 run-id 同前缀（前缀匹配）时，会把旧退出码当成新结果，节点被错误标记 succeeded。否决。
- **每次启动生成 launch_token**：每次 spawn（含每次重试）生成随机 token（UUID），写入运行实例行与节点启动配置；`on_process_done` 必须携带 `launch_token`，状态机只在 token 匹配当前 attempt 时才收敛；不匹配的回调记日志丢弃。采纳。这使「退出码权威」升级为「**带代际证明的退出码权威**」——退出码要先证明「我是这一轮跑出来的」，才配当终态。

### 4. 结果索引：断点续跑与审计的查询面

- **维持现状**：运行实例表（plugin_db）已可支撑断点续跑（按图查非终态节点）。问题：**下游找上游输出**（「按依赖找产物」）与**按时间线审计**（「某次触发各节点何时开始/结束」）没有统一查询面；且跨实例复用（同图多次触发、对比历史）依赖逐行扫表。否决。
- **运行实例表补索引列**：新增 `trigger_id`（每次图触发一个 id）、`node_id`、`launch_token`、`output_file`、`output_done_file`、`started_at / finished_at / attempts` 的索引；下游解析模板时按 `(trigger_id, 上游 node_id)` 定位输出文件。采纳（这本质是 pi-subagents `result-index/{sessions,runs,tool-calls}` 的 DB 等价形态，DB 比文件索引更可靠——事务 + 原子性由宿主 plugin_db 提供）。

### 5. 明确排除：不借鉴 pi-subagents 的 Supervisor 双向通道与同进程子会话

- **Supervisor 通道**（`contact_supervisor` → parent 回复）：pi-subagents 让 child 运行中阻塞问 parent。与 spec 用户故事 #20「无人值守执行时不需要任何人守在终端前点击确认」直接冲突；图编排的失败重试 / fail-fast / continue 已经替代"运行中问人"。排除。
- **同进程子会话**（前台 child 在 parent 进程内 `createAgentSession`）：与 D1「节点 = 独立非交互进程」矛盾——同进程会重新引入"状态靠推断"。排除。

## Consequences

- **D4 状态机微调**：`ready → running` 前增加「启动握手确认」前置条件（超时标 failed）；`running → succeeded/failed` 的收敛增加「launch_token 匹配」守卫。状态机迁移图语义不变，仅加两个守卫。
- **文件契约新增**：每个节点至多三个文件——`<node>.output.pending`（半写，临时）、`<node>.output.json`（正式，原子发布）、`<node>.output.done`（哨兵：输出路径 + 写全时间 + launch_token）。下游**只认哨兵**。
- **重试语义收紧**：每次重试 = 新 launch_token + 新输出文件（覆盖式原子发布），attempts 递增复用同一运行实例行（沿用 spec D6）。
- **断点续跑**：插件重启后按「运行实例表非终态节点 + 其 launch_token」恢复；若输出已发布（哨兵存在）而节点状态未终态（崩溃在收敛前），先收敛再续跑——不会重跑已完成节点。
- **对失败传播的影响**：`fail-fast` 因「启动握手」而更早触发（启动即崩立即判 failed，不再等 on_process_done）；`continue` 因「两阶段提升」而更安全（下游绝不会消费半写产物）。
- **性能**：握手 / 哨兵 / token 均为一次小文件写 + 一次 rename，在进程级（秒级）粒度下可忽略；不触发热路径。
- **宿主 ABI**：零新增（沿用 D2）。`on_process_done` 需在**插件侧**与 launch_token 关联——若宿主回调未来带 run_id，插件以「本行当前 launch_token 的 run-id 与回调 run-id 一致性」兜底；当前阶段插件自管映射（launch_token → run-id）即可。
- **术语**：CONTEXT.md「自动任务」节新增术语——**启动握手 (Startup Handshake)**、**两阶段结果提升 (Two-Phase Result Promotion)**、**代际 token (Generation Token)**、**结果哨兵 (Result Sentinel)**；避免与现有「上下文清理 / 任务记录 / 执行状态」混淆。

## 测试先例

沿用 spec Testing Decisions 的接缝（fake 执行器注入）：fake 执行器按「启动握手文件 / 输出文件 + 哨兵 / launch_token」断言**启动顺序、token 传递、done 回调匹配**；纯函数单测覆盖「令牌匹配收敛」「哨兵未现不读」「rename 原子性语义」（Rust 侧对 `std::fs::rename` 的路径级断言）。