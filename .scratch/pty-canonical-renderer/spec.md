# PTY 显示尺寸规范（正统渲染端）— Spec

Status: ready-for-agent

> 本 spec 面向桌面端与移动端同时接入同一 PTY 输出的场景：PTY 只能有一个网格尺寸，输出格式必须匹配实际渲染的那个端。当前实现为「最后调整者生效」（last-writer-wins），无任何来源追踪，导致任一端的 resize 都会静默覆盖另一端，背压也只按会话计数而不区分来源。本 spec 引入「正统渲染端」（canonical renderer）概念，将尺寸所有权与背压记账绑定到单个权威端。

## Problem Statement

用户同时用桌面端和移动端查看同一个 PTY 会话时，存在两类实际问题：

1. **尺寸互相覆盖无提示**：桌面端 resize 走本地 Tauri 命令、移动端 resize 走 HTTP `/api/sessions/{id}/resize`，双方都最终调用同一个 `SessionManager::resize_session()`，PTY 按「最后调用者」重排。当桌面端正在渲染一个按桌面窗口尺寸排版的输出时，移动端改变其屏幕方向/旋转触发一次 HTTP resize，PTY 立即被改排成竖屏行列；桌面端显示立刻格式错乱，且桌面端用户毫不知情、也无法拒绝。

2. **背压记账与显示端解耦**：PTY 读取背压由渲染反馈环驱动——客户端渲染完成后回发 ack，服务端推进 `unacked_bytes` 记账释放字节。当前 ack 只按 `session_id` 计数，不区分是哪个端、哪台设备发出的。屏幕较小、吞吐慢的移动端若也发送 ack，它与桌面端的 ack 会混合推进同一本账，背压水位无法反映「真正在渲染的那个端」的实际消费速度；而格式错乱的非权威端回发的 ack 更不应当用于决定 PTY 读停顿。

3. **移动端无渲染背压**：桌面端前端已具备 `confirmWriteParsed()` 渲染背压（写入解析完成后按 64KB 阈值 + 250ms 空闲节流回发 TB v2 ACK 帧）；移动端读取同样的 TB v2 帧，但完全没有 ack 发送能力，移动端单独显示会话时也没有任何下游渲染背压反馈。

用户希望：PTY 输出维护一个「正统显示格式」——即当前匹配 PTY 网格尺寸的那个端。任何端在设置尺寸前先判断自己是否正统；若 PTY 当前尺寸属于另一端，则先弹窗确认（「当前{xx}端正在渲染输出，是否覆盖它的尺寸？覆盖后{xx}端显示格式将错乱」），确认后才实际设置。背压也只处理正统渲染端的 ack，非正统的 ack 直接丢弃。同时为移动端补齐桌面端同等的渲染背压。

## Solution

在服务端为每个 PTY 会话维护一个「正统渲染端」身份（`desktop` 或某个具体的移动端设备），并将其作为尺寸所有权与背压记账的唯一权威：

- **尺寸覆盖需确认**：resize 请求到达服务端时，若请求方已经是该会话的正统渲染端，则直接应用（自身上下文变化无需确认）；若请求方不是正统端（另一个端正在渲染），服务端**不立即应用**，而是返回「需要确认」信号 + 当前正统端身份。客户端在其 UI 上弹出确认对话框（提示语含当前正在渲染的端 + 覆盖后果），用户确认后客户端携带 `force` 标志重新发送 resize，服务端才应用并更新正统端归属。
- **背压按正统端门控**：服务端 ack 记账只接受当前正统渲染端发出的 ack；非正统端发出的 ack 被忽略（丢弃，不推进 `unacked_bytes`）。桌面端本地、各移动端设备的 client 身份在 WS ack 处理上下文可用，据此判定来源是否为正统端。
- **移动端补渲染背压**：移动端 `useTerminalSocket` 读取到输出帧、经 xterm 写入解析完成后，仿照桌面端 `confirmWriteParsed` 构建 TB v2 ACK 帧回发（复用 `FRAME_FLAG_ACK=0x02`）。移动端仅在确认自己是正统渲染端后才发送 ack（否则 ack 会被服务端丢弃，徒增流量）。

三个行为共享同一概念锚点——正统渲染端——由服务端 `SessionManager` 单一权威维护并广播给两端。

## User Stories

1. 作为桌面端用户，我在查看一个 PTY 会话，当移动端设备尝试改变该会话尺寸时，我不想我的显示格式被无声破坏——我希望在移动端改变其旋转/布局触发 resize 前，能看到确认提示或至少知道我的显示将受影响。
2. 作为桌面端用户，当移动端请求覆盖我的会话尺寸时，我希望看到明确的提示「当前桌面端正在渲染输出，覆盖后桌面端显示格式将错乱」，并能选择拒绝，使我的显示格式保持不变。
3. 作为桌面端用户，我自己调整桌面窗口大小（resize 我的会话），我不希望弹出任何确认框——因为当前正统渲染端就是我，我对自己尺寸的调整是权威的。
4. 作为移动端用户，我调整手机方向/屏幕尺寸（resize 会话），当当前正统渲染端是桌面端时，我希望弹窗询问我是否确认覆盖桌面端设置的尺寸，以免误触破坏桌面端显示。
5. 作为移动端用户，当我在手机上单独查看一个会话（自己是正统渲染端），我调整手机旋转/尺寸时，应当直接生效、无需确认。
6. 作为移动端用户，我在手机端看到确认弹窗，选择「覆盖」后，希望该会话的正统身份移交给我，后续我对它的尺寸调整不再需要确认。
7. 作为移动端用户，我在手机端看到确认弹窗，选择「取消」后，希望该会话尺寸保持不变，且本次被拒绝的 resize 不再重复打扰（不会因持续的 RO 事件反复弹窗）。
8. 作为任意端的用户，我不希望看到误报——即设备自身的旋转/布局微调（subpixel 抖动被 ±1 钳制后仍可能触发）在我是正统端时正确静默生效，不被误判为「覆盖他端」。
9. 作为桌面端用户，当移动端被确认为正统并在渲染时，我希望我（桌面端）发起的 resize 同样进入确认流程——制衡是对称的，正统身份不是某一端固有的。
10. 作为运行在桌面主机的服务端，我希望每次会话的 resize 后都知道「谁现在是正统渲染端」，并在会话生命周期内可靠维护这个归属，供背压门控查询。
11. 作为服务端，我希望背压 ack 只接受正统渲染端的 ack：非正统端（格式可能错乱、吞吐不代表权威消费速度）的 ack 被丢弃，不影响本会话的 `unacked_bytes` 记账。
12. 作为服务端，当正统渲染端离线/重连时，我希望正统身份的归属语义是明确的（例如回落为桌面端或保持直至重新 resize），不产生歧义或把背压门控锁死。
13. 作为移动端终端，当我是正统渲染端时，我希望我把输出写入解析完成后能回发 ack（渲染背压），让服务端按我的实际消费速度暂停/恢复 PTY 读取—而不是永远不反馈。
14. 作为移动端终端，当我不是正统渲染端时，我希望避免发送会被服务端丢弃的 ack 帧——不做无谓流量与计算。
15. 作为移动端用户，我希望移动端的渲染背压行为与桌面端一致（64KB 阈值批量回发 + 空闲兜底），避免高频逐帧 ack，也避免弱网时低吞吐导致的 PTY 读饥饿。
16. 作为开发者，我希望正统渲染端的判定逻辑与背压门控逻辑集中在服务端单一权威点，桌面命令路径、移动端 HTTP 路径、移动端 WS SessionControl 路径三个 resize 入口都统一走它。
17. 作为用户，我希望面向我的弹窗提示文案是本地化的（zh-CN 与 en），且用语稳定一致（「正在渲染输出」「覆盖后…格式将错乱」）。
18. 作为非正统端用户，在确认之前我看到的自己的显示内容保持现状（服务端不预先应用尺寸），确认后才按新尺寸重排——避免「看到一半被改」的诡异跳动。
19. 作为移动端用户，当我与桌面端同时查看同一会话且桌面端是正统端时，我的屏幕上的尺寸同步逻辑（fit/resize debounce）在未获确认前不应把最终尺寸发到服务端覆盖——确认门控在服务端是最后防线，但客户端应尽可能前置感知。
20. 作为维护 PTY 的进程，当正统端覆盖确认通过后，我希望第一次 resize 即稳定生效，不因后续该端自身的 RO 事件再次触发同尺寸重复确认。

## Implementation Decisions

### 正统渲染端（canonical renderer）模型

- 在 `SessionManager`（或紧邻其的会话级状态）上为每个 PTY 会话维护一个正统渲染端身份：

  ```
  CanonicalRenderer = Desktop | Mobile { device_name: String }   // serde
  ```

  桌面端本地路径（Tauri 命令）身份恒为 `Desktop`；移动端经 HTTP/WS 路径从 JWT claims 提取 `device_name`（`get_claims_from_request` 已可用）作为身份。

- 会话最初创建/启动时，正统端为空或初始化为 `Desktop`（会话由桌面主机创建）。归属为「谁最后一次使 PTY 适配其尺寸」。

- 新增持久化的会话级字段与查询：`canonical_renderer(session_id)` 返回当前身份；`set_canonical_renderer(session_id, source)` 在确认覆盖后更新。归属只存内存（重连后语义见用户故事 12，先定：正统端若与其绑定会话/设备断开，归属保持最近值，直至被其他端或该端重新 resize 覆盖；不自动回落）。

### resize 覆盖确认协议（服务端权威）

- 改造统一入口 `SessionManager::resize_session`，除了 `cols/rows` 外接收来源身份与覆盖标志：

  ```
  resize_session(session_id, source: RendererSource, cols, rows, force: bool)
    -> ResizeOutcome
  ResizeOutcome = Applied { canonical: RendererSource }          // 已应用并确定正统
              | NeedConfirmation { current_canonical: RendererSource } // 未应用，需对方确认
  ```

  判定：若 `source == current_canonical`（或当前无正统/正好是请求方）→ `Applied`，直接 resize 并保持/确立正统；否则 → `NeedConfirmation{current_canonical}`，**不调用底层 `pty_registry.resize`**。

- 三个现有入口统一进此函数并透传来源/强制标志：
  - 桌面端 Tauri 命令 `resize_session`（本地，`source=Desktop`，按用户面对自身调整免确认；对移动端则为普通请求）。
  - 移动端 HTTP `POST /api/sessions/{id}/resize`（`source` 从 JWT claims 得 `device_name`；新增可选 `force` 请求字段）。
  - 移动端 WS `SessionControlAction::ResizeSession`（同样透传来源与 force）。

- 响应语义：HTTP 返回 `ResizeOutcome`（`applied` 或 `needs_confirmation` + `currentCanonical`）。桌面端本地命令可同步返回同样结构供前端弹窗判断。

### 客户端确认交互

- 客户端收到 `NeedConfirmation { current_canonical }` 时，在其最上层 UI 弹确认框提示：
  「当前{current_canonical}端正在渲染输出，是否覆盖它的尺寸？覆盖后{current_canonical}端显示格式将错乱」。
  提示文案经 i18n key（`{domain}.terminal.rendererOverride.confirmTitle` / `.confirmBody` / `.confirm` / `.cancel`），zh-CN 与 en 同步。
- 用户点「覆盖」→ 客户端以 `force=true` 重新发送 resize；点「取消」→ 不重发，本次尺寸调整在客户端侧保持本地网格并不再同步（避免 RO 事件反复弹窗，见下文抑制）。
- 客户端避免弹窗风暴：同一会话在某次「取消」后，对相同目标尺寸/方向的 resize 做短时间抑制（例如记住被拒绝的尺寸，去重），直到用户再次主动改变尺寸。

### 背压按正统端门控

- `GlobalOutputManager::ack(session_id, last_rendered_seq)` 当前不识别客户端来源（`terminal_ws.rs` `handle_ack_binary` 调用时不带 client 身份）。改造为接收来源身份 `RendererSource`：
  - 仅当 `source == 该会话的 canonical_renderer` 时推进 `unacked_bytes` / 弹出 FIFO；
  - 非正统端 ack 被丢弃（记 debug 日志，不推进记账），避免格式错乱端污染背压水位。
- `handle_ack_binary` 上下文可拿到该 WS actor 的 client/设备身份（`self.session` / 地址或登录 claims），用于判定来源。

### 移动端补渲染背压

- 移动端 `useTerminalSocket`（或视图层）在 xterm `onWriteParsed` 后调用 ack 发送函数，仿照桌面端 `confirmWriteParsed`：
  - 维护 `lastRenderedSeq`（帧末 seq）与 `ackedThroughSeq`；
  - 累加累计待 ack 字节，达阈值（64KB）批量回发；空闲超过 250ms 强制兜底回发；
  - 构建带 `FRAME_FLAG_ACK=0x02` 的 TB v2 帧（负载为 seq + session_id），与桌面端 wire 一致；
  - 仅当移动端是当前会话正统渲染端时才构造并发送 ack；非正统时不发（避免被服务端丢弃的浪费）。
- 移动端判定自己是否正统：在覆盖确认流程获知 `current_canonical == Mobile{自己}` 后置位；或订阅服务端正统端广播（见用户故事 12 的归属同步）。

### 归属同步与生命周期

- 覆盖确认成功后，正统身份变化可经现有同步事件/广播（`DesktopSyncEvent` 或订阅帧）通知两端，使桌面端与移动端 UI 都能显示「当前正交渲染端」状态（可选增强）。
- 会话销毁时清除其正统归属。

> 原型 / 现有可复用件：桌面端 `useTerminalOutputStream` 的 `confirmWriteParsed` 节流逻辑（`ACK_BYTES_THRESHOLD=64KB`、`ACK_MAX_IDLE_MS=250ms`）与 `control_frame.rs` 的 `TB_FRAME_FLAG_ACK` / `parse_ack_frame` 可直接作为移动端 ack 与帧编码的参考，无需发明新协议。

## Testing Decisions

- **只测外部行为**：尺寸覆盖是否需确认、确认后应用与否、非正统 ack 是否被丢弃、移动端是否回发 ack——不测内部实现重排细节。
- **服务端（Rust）**：
  - `resize_session` 的引擎判定：正统端自调直接应用且更新归属；他端请求返回 `NeedConfirmation` 且不改底层尺寸；`force=true` 他端请求应用并移交归属；非正统 ack 不推进 `unacked_bytes`（对照现有 `session_output.rs` 的背压测试风格）。
  - 覆盖 `should_pause` / `ack` 与正统端门控的组合：正统端 ack 释放字节、非正统端 ack 记账不变。
  - 参考先例：`session_manager.rs` / `session_output.rs` 内的 `#[cfg(test)]` 单元测试（背压 FIFO、`on_ack` 已具备类似记账测试）。
- **移动端前端（vitest）**：
  - `useTerminalSocket` ack：写入解析触发后按阈值/空闲回发 ACK 帧；非正统状态不发 ack。
  - 参考先例：桌面端 `useTerminalOutputStream` 相关测试（若有）与移动端既有 `__tests__/composables/` 测试。
- **端到端 / 集成**：桌面端 + 移动端同会话，移动端发起 heuristics 覆盖时返回 `needs_confirmation`，确认 force 后被应用——置于台式端集成测试套件，真机联调另列（沿用 `.scratch/pty-output-refactor` / `unified-pty-output` 的真机 checklist 惯例）。

## Out of Scope

- 会话创建/启动时的正统端初始化策略细化（本 spec 定为 `Desktop`/空，回退与多设备策略留待后续 ticket）。 
- 桌面端 UI「当前正统渲染端」常驻指示器（仅做覆盖确认弹窗；归属广播为可选增强，不阻塞主流程）。
- PTY 网格本身的其它渲染精度优化（DPR 拟合、±1 钳制、atlas 预热——已有独立 ticket，非本 spec）。
- 旧式 `/ws/terminal` 配对残留路径的清理（另有待评估项）。
- 移动端 HTTP resize 携带 `force` 外的鉴权强度变更。
- 多移动端设备同时各自查看同一会话时,正统归属的复杂仲裁(本 spec 只处理两端争用,多端沿用同一「最近确认者」语义)。

## Further Notes

- 本 spec 与已有渲染优化（resize 分层防抖、DPR 感知、±1 钳制、atlas 预热）正交：那些优化解决「自身 fit 的尺寸精度/闪烁」，本 spec 解决「多端争用同一个 PTY 网格时谁说了算」。
- resize 入口现状：桌面端本地 Tauri 命令、移动端 HTTP `/api/sessions/{id}/resize`、移动端 WS `SessionControlAction::ResizeSession` 三方最终都调 `SessionManager::resize_session()`——本 spec 将其收敛为唯一权威裁决点，覆盖全部入口，避免各端单独实现。
- 背压目前桌面端已在发 ack；本 spec 仅把服务端记账改为「按正统端门控」，并给移动端补齐同款 ack 发送。
- 确认弹窗的视觉实现遵循 `frontend-styles` skill 规范（token-bound、共享确认组件、移动端与桌面端一致），任何 UI 改动开工前加载该 skill。

## Seams 确认（2026-08-21 用户逐条确认）

1. **服务端 `resize_session` 为唯一裁决点**：确认符合期望——该函数是所有渠道终端打开都会调用的统一入口，将其收敛为尺寸/归属裁决点正确。
2. **移动端以 JWT claims 的 `device_name` 作为正统身份**：认可。
3. **正统端断连归属保持最近值、不自动回落桌面端**：确认——用户可能只是暂时关闭屏幕/后台，后续还会再打开；直到其他端显式调用 resize 才移交。spec 已按此写明（不改）。
4. **确认取消后的短时抑制（同尺寸去重）**：可接受。

> 四条 seam 全部确认，可进入实现。无其它调整项。