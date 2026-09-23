# 06: 移动端输出通道与历史改直读引擎游标环（M6/M7 收口）

**What to build:** 移动客户端连上一台桌面主机后，**对着插件会话也能看到实时输出、能拉一次历史**——
今天这两条是坏的（WS 终端通道报 `SESSION_NOT_FOUND`、HTTP 历史 404），因为它们在读一个
已经没有内容的内核输出环。本票把它们改成经 05 的只读映射直读同进程内的 PTY 引擎游标环，
零跨 WASM 边界。

**Blocked by:** 05（没有声明与映射就没有可读的东西）。

**Status:** done（2026-09-24）

## 验收标准

- [x] WS 终端通道：订阅/退订、游标续拉、缺口截断（`truncated` → 重同步）、终态收尾，
      对插件会话全部工作；帧形状与迁移前逐字一致（老客户端零改动）
- [x] HTTP 一次性历史：改读引擎游标环快照，返回形状与迁移前的字节语义一致；
      环已被淘汰时如实上报缺口而不是假装连续
- [x] 控制帧通道（鉴权/订阅确认/历史结束/重同步/模式/停止）不再依赖内核会话登记
- [x] 状态订阅与输出存在性判定脱离内核登记（这两处是 P1 窄转发层当时明确「不属于本层」
      留下的最后两个读旧真源的位置）
- [x] `pty_session_chain` 里被锁成「受损」的那条断言**翻正**为恢复断言（这是本票的红→绿目标），
      并且新增一条：插件会话在移动端面能看到输出且能拿到历史
- [x] 路线图受损清单 M6/M7 状态从「已触发」改为「已恢复（桌面侧待联调）」，并写明
      移动端仍需一次真机联调（本票不承诺移动端回归完成）
- [x] 门禁：宿主 `cargo test --lib` 全绿 + 集成 8 target 逐个串行全绿 + 桌面前端全绿 +
      根 `eslint .` 0 error；跑测试前重建插件产物并核 `[skip]` = 0

## 边界与不做

- 桌面终端窗口的输出路径不动（它已经在按游标拉引擎环）。
- 不做并发吞吐结论（07）。
- 不改移动端代码（M 系列授权口径不变）。

## Comments

（已填 2026-09-24）

### 实施记录

**宿主侧（票 06，P3 形态 B 直读引擎环）**：
- `terminal_ws/subscriber.rs`：新增 `spawn_engine_subscriber` + `engine_subscriber_loop`——
  经票 05 的 `broadcast_handle_for_session` 直读同进程 `PtyRing`（零跨 WASM 边界），
  帧语义与内核 `subscriber_loop` 逐字一致（subscribe_ok 三件套 / history_end / resync /
  TB v3 / ack 窗口 / 双速模式 / 僵尸回收）。引擎环无 watch 通道 → 自适应轮询
  （快档 50ms / 空闲 5 次后 250ms，镜像前端 `output.pull` 节奏，07 实测后可调）。
  终态 = `PtySession::subscribe_lifecycle()`：**宽限排空**（300ms，P0 已记：终态事件 ≠
  sink 已收尾帧）后发 `SessionStopped` 控制帧（尾帧先行、帧序保证）。新增
  `ForwardOutput::SessionStopped` 变体 → 桥接映射 `ServerFrame::SessionStopped`。
  订阅任务**持有 `PtySession` clone**：PTYS 注册表在终态时摘除条目并 drop 自己的
  session 引用，若订阅者不持 clone，lifecycle broadcast sender 先行关闭 → 尾帧排空
  与 SessionStopped 全被跳过（实测捕获的 bug，见下）。
- `subscription.rs::subscribe_output`：引擎优先（广播声明存在 → 引擎订阅者），内核环
  `GlobalOutputManager` 保留为旧内核会话/测试夹具兜底；`handle_ack_binary` 引擎句柄
  优先路由（`SubscriptionState.engine_subscribers` 表，key=`client:session`）；
  `cleanup_subscription_state` 增第 4 参（引擎句柄 retire+drain，测试同步更新）；
  `bump_generation` / `unsubscribe_output` 退休引擎句柄。
- `channel/terminal.rs`：auth 存在性 = 引擎优先 `broadcast_handle_for_session().is_some()`
  || 内核 `has_session` 兜底；`on_started` session_stopped watcher 对引擎会话不起内核
  watcher（避免双发——引擎订阅者已owns SessionStopped），内核会话保持原路径
  （ws_session_route 夹具依赖）。
- `session_gateway::history_snapshot`：引擎优先（`PtyRing` 水印 + fetch 全量驻留段，
  `from<min` 如实报缺口），内核兜底。

**测试（红→绿）**：`pty_session_chain` 场景 2 翻正——auth_ok → subscribe_ok（协议=3）
→ history_end → 写 echo marker → 收输出帧 → HTTP 历史含 marker → 场景 3c 断言
session_stopped 帧。新增 5 项引擎订阅者单测（历史回放+边界 / 空历史边界 / 截断 resync /
终态宽限排空+SessionStopped / 窗口驻留+ack 解除）与 2 项清理用例扩展（引擎句柄退休）。

**借道修复（对侧遗留，非本票引入）**：host-crypto 票 03 加了 `crypto:aead/asym/kdf`
权限位但漏了前端 `PERMISSION_META` 注册与 i18n 文案（`permissionMeta.test.ts` 红）——
补 `contributionKinds.ts` 三条目 + zh-CN/en `desktop.ts` 文案（均按「高危位才带 risk」
不变量，crypto 不进 HIGH_RISK_PERMISSIONS 故不带 risk）。

**门禁实测**：宿主 lib 1177/0；集成 8 target 逐个串行全绿（含 pty_session_chain 5.4s）；
SDK --lib 118/0；桌面前端 794/0；根 eslint 0 error（120 warning 不计）；插件产物重建
（terminal-session wasmHash 05166504…）；`[skip]`=0；跑测后无残留进程/端口。

**协作注意**：提交前对侧票 13（af75919b1）已 land；工作区其余 M（diagrams / sdk / fixture）
为对侧在途，未夹带。
