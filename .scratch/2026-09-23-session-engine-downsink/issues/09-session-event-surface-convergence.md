# 09: 事件面收口 —— 会话事件 100% 由插件自携带，删宿主回查兜底

**What to build:** 「会话发生了什么」这件事在宿主里**不再有第二个事实来源**：
广播处理器不再回查内核登记，前端那条靠内核状态订阅通道驱动的事件也一并裁定归属。
用户看到的效果：会话创建/状态变化/停止/移除的推送与今天完全一致，但内核会话目录
在事件路径上已经无人可读——这是 11 能整目录删除的最后一个前提。

**Blocked by:** 01（先有基线，才知道前端事件通道退役会不会碰到看不见的消费方）、08（命令面先干净，事件面的消费方才好定归属）。

**Status:** done（2026-09-24 落地；人工核验项待 01 基线复跑，见票末）

## 现状取证（2026-09-24）

- P1-b 已让会话事件载荷自足（概要/会话名/来源设备随事件携带），处理器的「回查内核」分支
  只对内核路径生效 → 生产上恒为空，是**假兜底**
- 还有一条内核状态订阅通道驱动前端事件：前端注册名与宿主发布名历史上就不一致，
  且前端**零生产消费方**（只有测试）——本票给出裁定：删，或换成插件贡献的事件

## 验收标准

- [ ] 广播处理器的内核回查分支删除；载荷缺失时**显性留痕**（`warn` + 不广播）而不是回查空值
- [ ] 内核状态订阅到前端事件的转接通道退役（或改由插件事件驱动），并留下一条锁：
      宿主事件模块不得再 import 内核会话登记
- [ ] 事件载荷形状逐字不变：WS 同步事件的会话四类，字段与迁移前对照表全等
      （移动端零改动的口径不被破坏）
- [ ] 广播排除语义（来源设备不重复收）与幂等移除广播（未知会话仍广播移除）两条
      P1-b 行为锁继续绿
- [ ] 桌面通知/侧栏刷新等行为按 01 清单复跑无差异
- [ ] 门禁：宿主 lib + 集成 8 target 逐个串行全绿、桌面前端全绿、`eslint .` 0 error

## 边界与不做

- 不动 WIT / ABI（interface 删除统一在 10）。
- 不改移动端。
- 不引入新的事件总线拓扑（既有 `host-events` / 消息总线够用）。
- **不碰 WS 动作词表与同步载荷词表的声明式化**：那是并发批次
  `.scratch/2026-09-24-host-crypto-business-downsink/` 票 09a/09b/09c 的范围
  （宿主硬编码业务词表 switch 的 expand–contract）。本票只删「会话事件的宿主回查兜底 +
  内核状态订阅转接」，若发现某条词表 switch 与待删转接同处一文件，**只加注释登记、不搬不删**，
  把那一处留给 09c，避免两条线在同一 switch 上互相覆盖。

## Comments

### 2026-09-24 · 落地：内核回查兜底删除 + 状态订阅转接通道退役

**裁定：事件载荷必须自足；缺字段即 `warn` + 不广播。**

P1-b 之后宿主里那条「回查 `SessionManager` 补字段」的路径已经是**假兜底**——内核登记里
没有插件会话，回查恒空，补出来的只是空字符串 / 空概要。绿的是代码路径（分支被走到、
测试覆盖到），不是行为。票面给的处置是「显性留痕 + 不广播」，本票逐条落：

| 事件 | 原回查字段 | 现行为 |
| --- | --- | --- |
| `SessionCreated` | `session`（整份 `SessionSummary`） | 缺失 → `warn` + 不广播 |
| `SessionStatusChanged` | `session_name` | 缺失 → `warn` + 不广播 |
| `SessionStopped` | `session_name` | 缺失 → `warn` + 不广播 |
| `SessionRemoved` | **无回查**（历史上就允许空名） | 保持空名照广播 |

`SessionRemoved` 为什么不一刀切：它是四类里唯一没有内核回查的分支，且 P1-b 的
**「未知会话仍广播移除」行为锁**要求它必须发（多客户端刷新依赖幂等删除广播）。
按「缺名即不广播」会把一条真事件吞掉——那是拿稳定性换一致性，不划算。已在实现处
写清这条不对称。

### 状态订阅转接通道退役

删 `events/forwarder.rs`（`EventForwarder`）与 `system/constants.rs::SESSION_STATUS_CHANGED`：
它把内核 `status_tx`（`subscribe_status()`）的事件转成 Tauri 前端事件
`session-status-changed`。三个删除理由，任一都足够：

1. **订阅源对插件会话无流量**：`status_tx` 的唯一发送点在内核会话执行端
   （`start_lifecycle_handler` / `kill_session_with_source`），生产路径自 P1-b 起不再产生；
2. **两侧注册名本就不一致**：宿主发 `session-status-changed`，前端注册
   `session:statusChange` → **从来没触发过**（前端零生产消费方，只有测试）；
3. 会话状态的前端可见性由插件自己经 `host-events` / `host-bus` 发布（本插件已是
   `SessionCreated` / `SessionStopped` 的发布方），宿主不必替它转接。

连带前端 `context.session.onStatusChange` 一并退役（`SessionAPI` 只剩四个窗口原语），
SDK 权限映射去掉 `session.onStatusChange` 并重出生成物。

### 为「11 能删目录」上锁

`sync_handler::tests::events_module_does_not_depend_on_kernel_session_registry`：扫
`src/events/**` 全部 Rust 源的非注释行，出现 `crate::session` 或 `SessionManager` 即红。
这条锁是**票 11 的前置判据**——事件面若还读内核登记，删 `src/session/` 就会把它带塌。

**变异自检 2 处**（均转红后还原）：
① 在 `events/app_event.rs` 临时加 `fn __mutation_probe(_m: &crate::session::SessionManager)`
→ 结构锁转红并点名 `app_event.rs:13`；
② 把 `handle_session_stopped` 的「缺名即拒播」改回 `unwrap_or_default()`
→ 行为锁转红，失败信息直接显示被广播出的空名载荷
（`SessionStopped { session_id: "s-t", session_name: "" }`）。

### 测试改写（判据从「走通回查」改为「载荷自足」）

- `session_created_event_broadcasts_session_created` / `session_stopped_…` /
  `session_status_changed_…`：改为携带载荷；
- 新增 `incomplete_session_payloads_are_not_broadcast`：三类缺载荷 → 广播数必须为 0
  （**行为锁**，不是「代码里没有回查」这种结构判据）；
- `session_created_for_unknown_session_skips_broadcast` 删除（它测的是「内核查不到」，
  该原因已不存在；等价行为由上面那条覆盖）；
- 两条任务字段用例（票 12 的 M2 降级 / 取值口径）改为**从载荷取值**构造——
  它们守的是 wire 形状（键不出现 / 键出现），与取值来源无关；
- `tests/broadcast_shutdown.rs` 的处理器构造去掉会话管理器参数。

**顺带抓到一处门禁假绿**：集成 8 target 的那轮循环首跑时 `broadcast_shutdown` 因
`SyncEventHandler::new` 签名变化**编译失败**，但循环里的 grep 没捕到
（`cargo test` 的编译错误行与 `test result` 不在同一处），表格里那格是空的——
是本轮**单独复跑**才暴露。教训：脚本化收口必须把「无结果行」也判为失败，
不能只看有没有 `test result`。

### 门禁实测（本票）

- 宿主 lib：**1148 passed / 0 failed**（+2 新用例 −1 删除）
- 集成 8 target **逐个串行全绿**（含单独复跑 `broadcast_shutdown`）
- 插件 native：**298 passed / 0 failed**
- 桌面前端全量：见下方提交前复跑（受影响文件已单独绿：22/22）
- 权限词汇：`gen:permissions` 重出双生成物；前端锁 L1 基线下界 19 → **18**

### 记账

- 票面验收第 5 条「桌面通知/侧栏刷新按 01 清单复跑」**未跑**（01 基线仍零观测）。
- **不碰 WS 动作词表与同步载荷词表的声明式化**：那是并发批次的 09a/09b/09c 范围；
  本票只删「回查兜底 + 状态订阅转接」，`sync_handler.rs` 里同文件的词表 switch 未动。
