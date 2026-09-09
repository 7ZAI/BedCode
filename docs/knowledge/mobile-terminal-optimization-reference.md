# 移动端终端优化参考：桌面端输出管线优化全景 + 30ms 合并问题

> 目的：沉淀桌面端终端输出/渲染管线已落地的优化与踩坑，作为**移动端终端后续优化的参考**。
> 适用对象：bedcode-mobile 终端（`useTerminalSocket` / `useTerminalBuffer` / `writeCoalescer` → xterm）的后续优化。
> 协议细节见 [pty-output-pipeline.md](./pty-output-pipeline.md)，本文档只讲**优化措施、参数、与问题教训**。

---

## 1. 桌面端终端输出管线全景（当前状态，含未合并 worktree 改动）

```
PTY 进程输出
  ↓ (os pipe)
PtyReader（std::thread）→ 有序队列（mpsc 16384 + 单消费者顺序 on_output，根治乱序）
  ↓
SessionOutputManager（seq 真源）→ UnifiedOutputQueue（seq + 快照订阅）
  ↓ forward_loop（TB v2 帧；flush_interval + max_buffer_size 合并）
  ├→ WebSocket 路由（移动端/桌面端远程终端）：/ws/terminal/session/{id}，30ms/64KB 合并
  └→ Tauri Channel（桌面端本地终端）：终端流命令，ZERO 缓冲直通（无合并，天然无丢帧）
  ↓
前端写入管线：seq 校验/重订阅 → rAF 合并（单帧单次 write）→ 64KB 分块 → 256KB 让出主线程
  ↓
xterm.js 6.0（DOM/WebGL 渲染器）
```

**传输层双轨（桌面端）**：
- 桌面本地终端：Tauri Channel 原生 IPC（`commands/terminal_stream.rs` + `useTerminalOutputStreamChannel.ts`），经 `VITE_TERMINAL_TRANSPORT=channel` 启用。负载经 in-memory fetch 拉取，Rust 侧缓冲直到前端消费，**无 WS 缓冲溢出丢消息问题**。
- 桌面/移动远程终端：WebSocket（`terminal_ws.rs`），**移动端前端直连桌面端 `/ws/terminal/session/{id}`** —— 这条 WS 路径是移动端远程终端的传输层，不可删除。

**服务端关键参数（forward.rs）**：
- `flush_interval = 30ms`（WS 远程通道时间窗合并）、`max_buffer_size = 64KB`、TB v2 帧上限 128 事件。
- 时间窗 vs timeout 重计时的取舍（forward_loop 注释）：持续输出下 timeout 永不触发会退化成仅容量触发、慢速输出延迟 = 容量/速率（可达数百 ms）；**时间窗保证延迟恒 ≤ 30ms**，正确。
- 背压（session_output.rs）：`BACKPRESSURE_HIGH_BYTES=64KB` / `BACKPRESSURE_RESUME_BYTES=8KB` 带滞回三态（`paused: AtomicBool`）。原 1MB 单阈值是 VS Code 10 倍且前端 64KB ack 永远到不了水位 → PTY 从不暂停 → WebKitGTK WS 缓冲（~64-256KB）溢出丢消息。参考 VS Code `HighWatermarkChars=100000 / LowWatermarkChars=5000 / CharCountAckSize=5000`。

**前端写入管线关键参数（TerminalPreview.vue / writeCoalescer）**：
- rAF 合并：同帧所有事件合并为一次 `terminal.write()`（对齐 VS Code；xterm 内部再统一调度渲染）。**为什么不是 queueMicrotask**：Tauri/WS 事件每个是独立 macrotask，微任务会立即 flush 无法跨事件合并。
- 兜底定时器 `FALLBACK_FLUSH_MS=100ms`：窗口最小化/后台 rAF 暂停时保证队列最终清空。
- `MAX_WRITE_CHUNK=64KB`：单次 write 上限，超过拆块让 xterm parser 在块间让出主线程。
- `WRITE_YIELD_THRESHOLD=256KB`：单次 flush 累计达到即让出主线程一次（宏任务），风暴期间渲染/输入可插入，防 UI 冻结。
- `REPLAY_IDLE_MS=250ms` 回放静止补刷：订阅/重订阅后历史回放与渲染器冷启动竞态可能留中间态，回放完毕连续 250ms 无新数据 → 自动补一次全量重绘（等价用户点刷新）。
- seq 级连续性守护：seq gap → 自动重订阅（新快照）；历史截断（min_seq > last_rendered+1）→ 清屏全量重播。

---

## 2. 30ms 合并造成的问题（重点教训）

**「30ms 合并」指服务端 `forward_loop` 的 30ms 时间窗合并**（WS 通道；Channel 本地通道用 ZERO 直通）。

### 2.1 问题清单

1. **固有延迟 ≤ 30ms**：输出排队恒 ≥0 且 ≤ 30ms（时间窗设计正确，见 §1 取舍）。
2. **批次截断 → 残影（移动端 TUI 滚动残影根因一，取证 2026-09-07）**：
   - TUI 应用（opencode 等）一次滚轮重绘被拆成多个 WS 消息，单次逻辑屏幕更新可达数百 KB，跨多个 30ms/64KB 帧。
   - 前端若**逐事件直写**（移动端旧行为：每个 WS 消息一次 `terminal.write()`）→ xterm 按 write 边界多次提交渲染 → 同一逻辑屏幕更新的多个批次在不同渲染帧提交 → 中间态残留固化在 canvas 位图：滚动停止后不消失、`refresh(0, rows-1)` 只能清主体剩边缘残字、transform 往返无效（排除合成器滞留）。
   - **修复**：前端 rAF 合并（保序合并同帧写入 → 单次渲染提交），残影 100% 消除。
3. **合并累积阈值过低会重蹈批次交错**：`MAX_COALESCED_BYTES` 256KB → **512KB**（移动端 writeCoalescer）：TUI 滚动重绘脉冲在 16ms 帧内可数百 KB，阈值过低会在逻辑更新中途截断合并 → 一次屏幕更新拆两次渲染提交 → 批次交错残影复发。512KB 足够容纳单帧内全部滚动重绘数据；瞬时内存峰 ≈ 1MB（pending + 合并缓冲），移动端可接受。
4. **合并加速缓冲积累 → 需要匹配的背压**：30ms 窗口内字节持续累积，WS 缓冲（WebKitGTK ~64-256KB）在 PTY 暂停前就可能溢出 → 丢消息。背压水位必须与合并/传输缓冲匹配（§1 64KB/8KB 滞回）。

### 2.2 结论 / 移动端注意事项

- **服务端 30ms 合并本身不是问题**（它减少 WS 消息数、延迟有界），**问题在前端消费方式**：必须 rAF 合并，禁止逐事件直写。
- 两级合并各司其职：服务端时间窗合并（减消息量） + 前端渲染帧合并（保渲染一致性）。移动端已有 `writeCoalescer`（rAF 合并 + 512KB + 100ms 兜底），与桌面端对齐。
- 若未来出现「后台写入延迟/黑屏」类回归，优先查 `FALLBACK_FLUSH_MS` 兜底路径；「渲染挂起/帧滞留」可用 `ENABLE_RAF_COALESCE=false` 回退直写做 A/B（writeCoalescer 文件头注释）。

---

## 3. 移动端现状与桌面端差距对照

> 现状同步（2026-09-10）：下表「移动端」列已按当前代码核实更新——渲染背压
> ack（spec 04-06）、history 缓存字节上限、WRITE_YIELD_THRESHOLD 让出机制均
> 已落地，与初版文档（写于 2026-09-08）不同。差距以最新为准。

| 能力 | 桌面端 | 移动端（当前） | 差距/参考 |
| --- | --- | --- | --- |
| 传输 | Channel 原生 IPC（本地）/ WS（远程） | WS 前端直连 `/ws/terminal/session/{id}` | 移动端只能 WS（跨设备），无 Channel 可选 |
| 写入合并 | rAF + 100ms 兜底 + 64KB 分块 + **256KB 让出** | rAF + 100ms 兜底 + 64KB 分块 + **512KB 阈值** + **128KB 让出** | 512KB 阈值是移动端特有（弱 CPU + 大脉冲）；让出阈值按弱 CPU 减半（桌面 256KB → 128KB），见 §3.1 |
| seq 守护 | gap → 重订阅；截断 → 清屏重播 | 同（useTerminalBuffer Store seq 状态机 + 历史缓存） | 已对齐 |
| 回放补刷 | 250ms 静止补刷 | replayIdleTimers（useTerminalBuffer） | 已对齐 |
| 背压 | 64KB/8KB 滞回（服务端）+ ack 反馈环 | **ack 已实现**（spec 04-06）：`ackRendered()` 64KB 节流 + 250ms 空闲兜底 → TB v2 ACK 帧 → 桌面端 `handle_ack_binary` → `GlobalOutputManager::ack` | 已对齐桌面端 useTerminalOutputStream 语义；两向接线完整 |
| 历史缓存 | — | `MAX_HISTORY_CACHE_BYTES` 字节上限 + shift 逐出 + 会话关闭 cleanup | 有界，无内存失控 |
| 渲染器 | DOM（Linux）/ WebGL，透明/背景图强制 DOM | WebGL 动态加载 + 上下文丢失回退 DOM + atlas 预热 | 移动端主题纯色（terminalThemes.ts 无透明度），route B 决策不适用；context loss 回退已覆盖 |
| 鼠标坐标 | 已修（去 CSS zoom，见 issue 06） | — | 移动端无全局 zoom，不适用；但若引入任何祖先 transform/zoom，xterm 鼠标 hit-test 坐标系会错位（issue 06 教训） |

### 3.1 让出阈值减半决策（2026-09-10 落地）

- 桌面端 `WRITE_YIELD_THRESHOLD = 256KB`；移动端取 **128KB**（一半）。
- 理由：移动端 CPU 更弱，256KB 连续 parse 在低端机可致数百 ms 冻结（掉触摸/卡滚动）；
  128KB 在风暴下约每 2 个 64KB 块让出一次，渲染/输入可插入。
- 只控制写节奏不限制总量：flush 内 while 轮次持续消费新入队数据（重入守卫 `flushing`
  防双写与残留 timer），数据无滞留。
- 实现：`writeCoalescer.ts` —— `flushPendingSync()`（合并/直写零 await，保持小数据
  同步语义）+ `writeInChunks()`（每累积 128KB `setTimeout(0)` 让出一次宏任务）。

**移动端后续优化候选**（按优先级，已去重已落地项）：
1. seq gap / 丢帧观测埋点：store 的 gap 检测处加计数 + frontendLogger 采样日志（防刷屏），
   风暴时期直接回答「是否真在丢、丢多少」——ack 背压已有，但缺观测闭环。
2. 输入侧跟手性评估：软键盘弹收（handleVisualViewportChange）fit 防抖已有；WS 往返 +
   PTY echo 在弱 WiFi 下的键盘延迟未评估——不建议 localEcho（会破坏 TUI 应用）。
3. 若未来移动端主题引入透明度/背景图，需补桌面 route B 决策（透明 → 强制 DOM）。

---

## 4. 桌面端旧 WS 本地传输实现删除评估（待 worktree 合并后执行）

**背景**：桌面端本地终端原走 WS 环回（`/ws/terminal/local` + `useTerminalOutputStream.ts`），worktree 新增 Channel 传输后经 `TERMINAL_TRANSPORT` 三元选择（默认 ws、dev 用 channel）。

**评估结论**：
- **后端 WS 绝不能删**：`/ws/terminal/session/{id}` 是移动端远程终端的前端直连路由（`useTerminalSocket` 依赖），`terminal_ws.rs` / `forward.rs` 保留。
- **可删（合并后）**：桌面端前端 WS 消费实现 `useTerminalOutputStream.ts` + TerminalPreview 的 `TERMINAL_TRANSPORT` 三元选择 + `env.d.ts` 的 `VITE_TERMINAL_TRANSPORT` 声明 + 对应测试（`useTerminalOutputStream.test.ts` 等）。理由：桌面本地终端唯一消费路径已切 Channel，双实现 ~400 行重复逻辑（seq 校验/重订阅/快照/ack）存在漂移风险。
- **保留理由（删除前需权衡）**：
  1. vite 浏览器调试场景（非 Tauri 运行时 Channel invoke 不可用）可回退 WS。
  2. Channel 大负载传输依赖 in-memory fetch 拉取，远程/隧道场景不可用（本地环回才用）。
  3. 双实现互为回归对照（A/B）。
- **建议**：worktree（残影 + Channel）合并到 dev 后，评估删除前端 WS 消费实现；至少将 `useTerminalOutputStream.ts` 标注 legacy 不再维护。删除动作在合并后进行，避免扩大合并冲突面。

---

## 5. 关联文档

- 协议全链路：`docs/knowledge/pty-output-pipeline.md`
- 移动端残影根因取证：`.scratch/terminal-scroll-ghosting/root-cause-handover.md`（写入批次交错 → rAF 合并修复）
- 服务端输出重构：`.scratch/pty-output-refactor/`（30ms/64KB 合并策略由来）
- 桌面残影收敛 + 鼠标坐标（zoom）根因：`.scratch/xterm-ghosting-closure/`（issue 05 背压、issue 06 zoom）
