# Backlog — 移动端 pty 链路改造遗留事项（2026-09-12 晚）

> 本文件记录「改动全部就位、验收全绿，但尚未收口」的遗留事项与责任归属。
> 范围：`.scratch/mobile-ws-rust/` 方案实施（桌面 TB v3 + 移动端 WS 迁入 Rust）。
> 当前状态：ticket 01/02/03/04/05 resolved/交付；桌面端已提交（4 commit：desktop TB v3 /
> 双速 / HTTP 历史 / 文档）；**移动端死代码删除已执行完毕**（2026-09-12 06:10，见 §8）；
> 移动端提交同样由桌面 agent 统一执行（工作树仍有移动端改动未提交）。

## 1. Git 提交（🔴 最高优先，责任：桌面 agent）

工作树 `dev` 上有 38 项未提交改动（桌面 TB v3 全套 + 移动端全套 + 文档），含未跟踪的
`bedcode-mobile/src-tauri/src/terminal_link.rs`。建议按 ticket 拆提交（01/02 桌面、03/04 移动端、
05 文档），conventional commits（feat/fix/docs(desktop|mobile) …），禁止 Co-Authored-By。
注意 AGENTS.md §11：提交前过 pre-commit 钩子（eslint + 分支级文档跟踪）；`.pi/` 不入库。

## 2. 链路加密（ws-terminal 协商）— 后续 ticket（责任：移动端）

移动端 `terminal_link.rs` 本轮 JWT 认证 + 明文帧（与 v2 时代明文终端 WS 同安全位）。
spec §5 风险项采纳降级方案；后续接入 bedcode_link_crypto ws-terminal 协商（机制对齐
ws_event 通道，见 ws_client.rs）。记录于 spec §6 D-A5 与 ticket 03 Answer。

## 3. 移动端 v2 解析死代码清理（🔴 已定论可删，责任：移动端；与桌面端同步确认后）

`bedcode-mobile/src-tauri/src/terminal_link.rs::parse_tb_frames` 的 `TB_VERSION_V2` 分支
（`seq + count` 语义近似字节区间）为**不可达死代码**：桌面端 v3-only（forward.rs 只
`encode_output_frame_v3`，无任何 v2 帧路径；两端同步部署，AGENTS §9）。删除项：
- `TB_VERSION_V2` 常量 + `TB_V2_COUNT_SHIFT` 常量
- `parse_tb_frames` 的 `TB_VERSION_V2 =>` 匹配臂（保留 `_ => break` 守卫未知版本）
- 对应单测 `parse_tb_frames_v2_compat_count_semantics`
- v2 兼容注释（「过渡兼容」「正式版删除」字样的行内注解）

补充分析（详见本文件末尾「附录：v2 死代码分析」）：桌面端 `parse_ack_frame` 对 v2 ack 的
兼容是**服务端对旧客户端**的接受面，不构成移动端保留 v2 解析的理由（移动端只发 v3 ack，
`build_ack_frame` 恒 version=3）；旧移动端（useTerminalSocket 直连）已删除，无 v2 发送方。

## 4. R5 移动端重启游标丢失（🟡 可优化，责任：移动端）

移动端 App 重启后 Rust 会话级缓存清空（16MB LRU 在内存中）→ 重进终端页 from=0 全量重放，
肉眼重复（若会话存活且输出量大）。当前由截断语义兜底（min_offset > cursor → 清屏）。可选：
游标（lastRenderedOffset）持久化到本地存储，重进按游标增量拉取。review.md R5 同述。

## 5. HTTP 历史单次响应配额（🟢 可选防御，责任：桌面端）

`GET /api/sessions/{id}/history?from=` 无单次响应上限（上限即队列 max_total_bytes 50MB，
base64 约 67MB）。R3 关联；建议 chunk 级分页或单次上限（如 4MB）+ 移动端增量拉取。review.md R3 同述。

## 6. TerminalPreview.vue（桌面）oxlint 既有告警（⚪ 待裁决，责任：桌面 agent）

11 项告警（未用 import Select/Button/PluginTerminalToolbar、未用 computed statusColor/
themeSelectOptions/fontSizeSelectOptions/containerBgColor、未用函数 confirmRendererOverride/
cancelRendererOverride、`terminal != null` 反断言建议）。均为 **HEAD 既有**（非本方案引入），
桌面 eslint（CI 门禁）对该文件 0 errors；文件在桌面 agent 在途改动中（AGENTS §11 未覆盖）。
是否清理由桌面 agent 决定；若确需处理建议一次性 lint 修复提交。

## 7. 文档状态

- `docs/knowledge/pty-output-pipeline.md`：已重写为 TB v3 全链路（本会话完成）
- `bedcode-mobile/docs/code-map.md`：已增补终端链路模块（本会话完成）
- `spec.md`：§6 落地核对（D-A1~A6）已记录（本会话完成）
- `CHANGELOG.md`：**用户裁决不改**（2.1.0 为已发布日志，未发布变更不新增条目）
- 根 `docs/knowledge/logging.md` / 发布流程等未涉及本方案，无需改

---

## 附录：移动端 v2 解析死代码分析（2026-09-12，为条目 3 提供依据）

### 现状

`terminal_link.rs::parse_tb_frames`（TB 二进制消息 → 帧列表）：

```rust
const TB_VERSION_V2: u8 = 2;
const TB_V2_COUNT_SHIFT: u8 = 1;

let (start, end) = match version {
    TB_VERSION_V3 => { let start = raw; (start, start + len4 as u64) }
    TB_VERSION_V2 => {
        // 过渡兼容：seq + count 语义近似字节区间（移除以官方版本为准）
        let count = ((flags >> TB_V2_COUNT_SHIFT) as u64) + 1;
        (raw, raw + count - 1)
    }
    _ => break,
};
```

v2 分支把「事件序号 seq + 事件数 count」近似映射为字节区间——对 v2 帧的 `end` 只能给出
「事件序号跨度」而非真实字节长度，语义上只是粗粒度占位，并非真实字节连续。

### 为什么不可达（三层证据）

1. **发送方只有桌面端 forward.rs，v3-only**：`encode_output_frame_v3` 编码 v3 帧
   （version=3）；全文件无任何 version=2 输出路径。桌面侧已无 v2 帧生成器。
2. **老客户端职责迁移**：v2 语义的旧消费方 = 移动端 `useTerminalSocket.ts` 直连前端
   （已删除，本方案迁入 Rust）；无第二个 v2 发送/接收对。
3. **两端同步部署（AGENTS §9）**：协议改动两端同步上线，不存在「新移动端连旧桌面端」
   的过渡窗口；即便出现（回滚场景），移动端对未知版本 `_ => break` 会安全停止解析该
   消息，不会误判数据——v2 分支的「近似区间」在那种场景下反而会产出错误 end 值。

### 删除影响面

| 项 | 位置 | 影响 |
| --- | --- | --- |
| `TB_VERSION_V2` 常量 | 常量区 | 引用者仅 v2 匹配臂 + 单测，删 |
| `TB_V2_COUNT_SHIFT` 常量 | 常量区 | 引用者仅 v2 匹配臂，删 |
| v2 匹配臂 | parse_tb_frames | 删除后 `_ => break` 仍守卫未知版本 |
| 单测 `parse_tb_frames_v2_compat_count_semantics` | 同文件 tests | 删（测试的是死代码） |
| 行内注释 | 帧解析区 | 「过渡兼容」「正式版删除」字样清理 |

风险评估：**低**。删除后移动端对任何非 v3 帧一律 `break`（与桌面端行为一致）；`build_ack_frame`
恒发 v3（不动）。桌面端对 v2 ack 的兼容接受（parse_ack_frame）是给「旧客户端」的回退面，
保留（移动端侧无对应发送方，不影响死代码判定）。

### 残留其它 v2 痕迹核查（已确认干净）

- `terminalBuffer.ts` / `useTerminalBuffer.ts` / `useMobileConnection.ts`：游标已全部
  v3 化（lastRenderedOffset/endOffset），无 seq/lastSeq 残留
- `terminal-flow.test.ts`：本会话重写，无 v2 帧 helper
- `useTerminalSocket.ts`：已删除（git D 状态）
---

## 附录 B：移动端全部遗留死代码调查（2026-09-12，scipq 精确引用 + grep 交叉验证）

> 本次重建 SCIP 索引后系统排查。分三层：**已确认可删（A）** / **可删但需桌面端确认（B）** / **纯注释残留（C）**。

### A. terminal_link.rs 内 v2 兼容分支 —— 已确认死代码（桌面端 v3-only）

SCIP 引用面：`TB_VERSION_V2` / `TB_V2_COUNT_SHIFT` 仅 terminal_link.rs 内部引用（定义 + parse 臂 + 测试）；桌面端 forward.rs 只 `encode_output_frame_v3`，无任何 v2 输出路径，控制帧的 v2 仅是对旧客户端 ack 的**接受面**（parse_ack_frame，非发送）。两端同步部署（AGENTS §9）→ 移动端 v2 解析分支不可达。

删除项（同 backlog §3 附录 A）：
- `TB_VERSION_V2` / `TB_V2_COUNT_SHIFT` 常量
- `parse_tb_frames` 的 `TB_VERSION_V2` 匹配臂（保留 `_ => break` 守卫未知版本）
- 单测 `parse_tb_frames_v2_compat_count_semantics`
- 「过渡兼容」「正式版删除」行内注释

### B. 旧 ws_event 通道的终端残留 —— 移动端无发送方，可删；需桌面端确认不再走旧通道

**发现（scipq + grep 实证）**：Rust 侧有一条**旧前端直连通道的 Rust 镜像**——`handler/terminal.rs`（TerminalHandler）仍注册在 `connection/manager.rs:59` 的旧 ws_event 路由；其处理的 `TerminalAction::SubscribeResponse`（min_seq/max_seq/history_count）是 v2 帧语义。但移动端已**无任何发送点**构造这些消息：
- `JoinSession`/`SessionSubscribe`/`Message::input*` 无生产调用（仅 router/registry、router、model 的**测试**在用）
- 前端 `useMobileCommands.ts` 的 `wsSendInputAsync` 仅定义无调用（useTuiCompat.ts 只有注释引用）
- `Message::output` 仅在 registry/router 测试里构造

删除项（需桌面端确认 ws_event 通道不再用于终端订阅/输入后）：
- `handler/terminal.rs` 整个文件 + `handler.rs:8,14` 的 mod/use
- `connection/manager.rs:59` 的 `.route("Terminal", Arc::new(TerminalHandler))`
- `enums/control.rs` 的 `SubscribeMode` 枚举（仅被上述 handler 引用）
- `model/message.rs` 的 `output_from_base64` / `subscribe_response`（产出方，零生产引用）
- router/registry.rs + router.rs 中引用 `Message::output` 的测试夹具

**风险点**：桌面端 `server/ws/message.rs` 旧路由是否还会经 ws_event 通道向移动端推 Terminal 帧——若桌面 agent 清理过（桌面侧 ticket 01 已删 JoinSession/LeaveSession 旧链路），则移动端 handler 等随之删除安全；建议与桌面 agent 同步时复核。

### C. 前端/文档纯注释残留（低优先）

- `stores/terminalBuffer.ts:613` `startGlobalListener` no-op 兼容垫片 + 两个调用点（useMobileConnection.ts:194/236）——已确认 no-op，可删（或保留注释说明）
- 注释残留：`views/TerminalView.vue:917`（ws_output）、`useTerminalBuffer.ts:6/277`（lastRenderedSeq）、`useMobileConnection.ts:341`（ws_output）、`services/linkCrypto.ts:15`（useTerminalSocket）——仅注释文案过时，可顺手清理

### 删除优先级建议

1. **A 组**：可立即删（单文件、零外部引用、不涉两端协议）——随移动端提交一起做
2. **B 组**：桌面 agent 确认旧 ws_event 通道终端路径已清后删
3. **C 组**：随任意前端改动顺手清理

---

## 8. 移动端死代码删除执行记录（2026-09-12 06:10 完成）

按附录 A/B/C 执行完毕，scipq 索引已重建（desktop 195 docs / mobile 136 docs，双 fresh）后逐项核实：

| 项 | 位置 | 状态 |
| --- | --- | --- |
| `TB_VERSION_V2` / `TB_V2_COUNT_SHIFT` 常量 | terminal_link.rs | ✅ 已删 |
| `parse_tb_frames` v2 匹配臂（保留 `_ => break`） | terminal_link.rs | ✅ 已删 |
| 单测 `parse_tb_frames_v2_compat_count_semantics` | terminal_link.rs | ✅ 已删 |
| v2 兼容注释 | terminal_link.rs | ✅ 已清理（含 unused flags 变量修复） |
| `handler/terminal.rs`（TerminalHandler） | 整个文件 | ✅ 已删 |
| `handler.rs` mod/use | handler.rs | ✅ 已删 |
| `.route("Terminal", …)` 注册 | connection/manager.rs | ✅ 已删（含 router.rs re-export） |
| `output_from_base64` / `subscribe_response(_with_request_id)` 构造器 | model/message.rs | ✅ 已删 |
| 对应单测 4 个（passthrough / response×2 / roundtrip 夹具改字面量） | model/message.rs | ✅ 已删/已改 |
| `startGlobalListener` no-op 垫片 + 2 调用点 | terminalBuffer.ts / useMobileConnection.ts | ✅ 已删 |
| 过时注释（ws_output / lastRenderedSeq / useTerminalSocket） | 前端 3 文件 | ✅ 已清理 |
| `wsGetStatus` import | useMobileConnection.ts | ✅ 已删（useMobileCommands 封装保留——对应真实 Rust 命令） |
| `catch (_)` ×3 → `catch {` | useMobileConnection.ts | ✅ 语义等价修法（lint no-unused-vars） |
| `sessionName` 未用参数 | useMobileConnection.ts / DevicesView.vue | ✅ 已删（httpStartSession 本就不收 name） |

**保留（wire 契约面，不删）**：`SubscribeMode` 枚举 + `TerminalAction::SubscribeResponse` 变体（被
`SubscribeResponse.mode` 字段引用，旧 ws_event 通道反序列化兼容面；scipq 证伪了 backlog 中「仅被
handler 引用」的初判）；`Message::output` 构造器（router/registry 路由机制测试夹具仍在用，非死代码）；
`unsubscribe_response` 构造器（unsubscribe 请求/响应 wire 仍在）。

**验证**：`cargo test --lib` **278 全绿**（净减 4 测试；egress/heartbeat 两用例偶发 flaky 为既有负载
时序问题，孤立与重跑均过）；`vitest` 372 全绿；`vue-tsc` 0；eslint 移动端 0 error；无测试残留进程。

**归属**：删除随移动端工作树改动，由桌面 agent 一并提交（用户裁决）——已确认桌面侧 4 commit 完成，
移动端改动仍在工作树。
