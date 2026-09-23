# 01: 桌面功能等价人工基线（P1-b 后目测复跑）

**What to build:** 把「桌面端功能等价」这条验收红线从「测试绿」变成**一份可复跑的人工基线记录**——
在真机上把终端窗口的每一条交互走一遍并把结果记在票末，后续每次内核会话线被削（本专项的 03/08/09/11）
都按同一清单复跑一次。无头测试覆盖不到 xterm 渲染、滚动缓冲、IME 组合窗口与通知点击这些
真实浏览器/合成器行为，所以这份清单是**唯一**能证明「下沉没改用户看到的东西」的证据。

**Blocked by:** 无（可以立即开始；P1-b 已经 landed，基线必须以当前 dev 为准）。

**Status:** ready-for-human（agent 侧前置与配套已就绪 2026-09-24 05:2x：
起跑预演绿、`../baseline-preflight.sh` + `../baseline-run-sheet.md` 已交付；
唯一待定是「等票 06 落地再跑」还是「即刻跑半成品树」，见票末 Comments 最新一节）

**配套**：下面 10 项的**可执行展开**（操作 / 预期 / 取证点 / 结果槽 / 归属三问）在
[`../baseline-run-sheet.md`](../baseline-run-sheet.md)；起跑前先跑
[`../baseline-preflight.sh`](../baseline-preflight.sh)。

## 验收标准

- [ ] 跑 `cd bedcode-desktop && pnpm run tauri:dev`，逐条完成下面清单并在「## Comments」记录
      实际现象（正常 / 异常 + 复现步骤），一条不落
- [ ] 键盘输入：普通字符、回车提交、退格编辑、方向键历史、Tab 补全 —— 终端有回显且 PTY 收到
- [ ] 粘贴：多行文本粘贴不重复执行（括号粘贴块内换行不当提交），且**不**在任务历史里长出多余行
- [ ] 中文 IME（Linux WebKitGTK）：组合窗口内不双发（本项历史上出过缺陷，是重点）
- [ ] 滚动：上滚查看历史不被新一轮输出拽底；「回到底部」按钮出现与消失正确
- [ ] 特殊键：Ctrl-C 中断前台程序并显示 `^C`；工具栏「停止」「复制」按预期生效
- [ ] 多窗口：同一会话开两个终端窗口，尺寸争用弹「覆盖确认」，确认后归属移交
- [ ] 通知与种子化：会话创建/停止通知文案与计数正确；从会话列表/通知打开终端窗口时内容回放正确
- [ ] 任务域：在受支持的 agent 会话里敲一行提交，任务历史出现该记录；队列下发（自动任务）能把
      prompt 投递进会话并进入 executing
- [ ] 关窗守卫：有存活会话时关主窗弹确认，列表与计数与实况一致；无会话时直接关
- [ ] 发现的每一处差异：要么当场立一张新票（`issues/13-…` 起编号）并在本票 Comments 里指向它，
      要么写明「与迁移前一致，非回归」的判据

## 边界与不做

- 移动端不在本票范围（M1–M9 已知受损，见路线图清单）。
- 本票不修任何东西：只出基线。改动归后续票或新立的差异票。
- 性能不在本票（多端并发拉取实测是 07）。

## Comments

### 2026-09-24 05:2x · 准备工作完成（agent 侧），本轮仍未开跑——但挡路的原因换了

**先说清状态**：这一轮做的是**前置与配套**，不是基线本身；10 项清单依旧零条观测，
但「为什么没跑成」和上一轮已经不是同一件事了。

**上一轮的两个挡路点，现状：**

1. **构建链红（票 13）→ 已消除。** 票 13 以「防漂移按声明方/消费方分判」落地（`af75919b1`）。
   预演 `ensurePluginWasm()` 的判据（比 resources 产物 vs「插件 rust ∪ SDK rust」最新 mtime，
   只看 `PLUGIN_WATCH_CMDS` 那三条）：三条**全 FRESH，起跑不触发补建**。
   四插件产物已按含票 13/14 的 SDK 重建（file-transfer `b599056d…` / terminal-session
   `05166504…` / ai-chatbox `f1d55683…`），即起跑不会再被 fail-fast 挡住。
   - 顺带钉死一个我自己在预演时踩的坑：**staleness 比的是 `resources/` 里的产物**，
     不是 `plugins/<id>/rust/target/…` 那个源树产物。拿源树路径判会得出「四插件全需补建」的
     假结论（ai-chatbox 源树那个 wasip2 文件确实是 09-15 的陈迹，但早已不是构建目标）。
2. **工作区不干净 → 现在仍然不干净，但性质变了。** 本轮脏的是**并发批次票 06 的在途实现**
   （`server/websocket/{subscription,channel/terminal,terminal_ws/*}.rs` +
   `utils/session_gateway.rs` + `tests/pty_session_chain.rs`，写盘时间就在 05:15–05:19）：
   把 `pty_session_chain` 里「受损形态断言」（`auth → error(SESSION_NOT_FOUND)`）
   改成恢复断言（`auth_ok → subscribe_ok → 输出帧 → HTTP 历史）。
   - 05:0x 时这棵树**连 lib 都编不过**（`session_gateway.rs::history_snapshot` 调一个还不存在的
     `broadcast_handle_for_session`）；05:23 复跑 `cargo check --lib` **已通过**。
   - **也就是说：现在技术上能起跑，但跑的是「票 06 半成品」**——而票 06 改的正是终端输出与
     历史回放路径，恰与本票第 3.4/3.7 条（滚动、种子化回放）重叠。据「归属三问」，
     这一轮跑出来的回放类差异会同时有 P1-b 回归 / 票 06 WIP 两个候选解释，**分不清**。
     建议：等票 06 落地后再跑基线；若用户要求即刻跑，则回放类条目按「对侧 WIP 待判」出记录。

**交付的两个可复跑配套（本票的「配套方式」那一节落地）：**

- [`../baseline-preflight.sh`](../baseline-preflight.sh) —— 起跑前置自检，五段输出：
  基线锚点（HEAD + 分支）/ 未提交改动**逐文件标归属**（票 06、文档线、未归属）/
  插件产物补建预演（与 dev-run.js 同形，含上面那个路径坑）/ `--compile` 宿主 lib 可编译性 /
  当日日志路径与 grep 锚点。`--save` 可把报告落成 `preflight-<date>-<hhmm>.txt`。
  后续每次内核会话线被削（03/08/09/11）复跑同一清单时，第一件事就是跑它。
- [`../baseline-run-sheet.md`](../baseline-run-sheet.md) —— 10 项验收展开成
  「操作 → 预期 → 取证点 → 结果槽」，含**归属三问**、agent/人分工表、日志体检命令，
  以及结论模板。新立差异票编号纠正：票面写的「`issues/13-` 起」已被 13/14/15 占用，
  **实际从 `issues/16-` 起**。

**观测锚点已核到源码**（不是凭印象写的）：`session created via plugin`（info，带
`config_id`/`session_id`）、`session stop requested via plugin`、`session removed via plugin`、
`refused: session plugin not active`（warn）、`failed via plugin`（error），
前五条出自 `utils/session_gateway.rs`；插件侧统一 `[plugin:` 前缀（AGENTS §7 日志条）。
本机工具实测：`xdotool` + `fcitx5` 在，`grim`/`spectacle`/`import`/`wl-copy` **全无**
—— 印证「目测项必须人眼」，agent 只能替到日志一层。

### 2026-09-24 01:10 · 本轮未开跑（起跑即被挡），零条清单结论


**基线没跑成，不是跑了没问题。** 按票面第一条 `cd bedcode-desktop && pnpm run tauri:dev`
就停住了，宿主进程根本没起来，所以上面 10 项清单**一条都没有观测**（不是「一条都正常」）。
两个挡路的理由都记在这里，避免下一轮误判：

1. **构建链红：见 [`issues/13-plugin-api-drift-trait-stale.md`](13-plugin-api-drift-trait-stale.md)。**
   `dev-run.js` 的 `ensurePluginWasm()` 在补建 `file-transfer` WASM 时编译失败——
   `#[plugin_api(manifest = "../../terminal-session/plugin.json")]` 的跨插件防漂移比对
   发现 manifest 比 `SessionCenterApi` trait 多四条
   （`session-list` / `session-get` / `session-close` / `session-input`，由 P1-b `c7b632397`
   加入），fail-fast 直接退出。**这条要先修，否则票 01 与所有依赖它的票（03/09）都开不了跑。**
   注意它是**延迟暴露**：资源目录里 file-transfer 的旧产物是 09-23 07:23 的，
   后续批次 touch SDK rust 目录才把补建触发出来——红属 P1-b landed 的账，不属触发它的那批。
2. **工作区不是干净 dev**：76 个未提交文件属同日并发的
   `.scratch/2026-09-24-host-crypto-business-downsink/`（其中真实语义改动约 980 增 / 920 删，
   其余 5200 行是行尾抖动）。用户 2026-09-24 裁定：**当前工作区照跑，票里记账标明污染**；
   因此下一轮基线若发现差异，必须逐条判归属（P1-b 回归 / 对侧批次引入 / 与迁移前一致），
   不得默认算本专项的账。
   - 另记一条已被排除的路径：「另开 worktree 检出 HEAD 跑干净基线」不可行——
     `src-tauri/target` 已 20G、磁盘剩 19G，全量重编会写爆，且会撞对侧构建缓存。

### 下一轮开跑的前置与配合方式（用户 2026-09-24 定）

- 前置：票 13 落地（`plugins:build` 与 `tauri:dev` 起跑绿）。
- 配合：**人跑清单，agent 起环境 + 盯日志 + 记账**。清单里键盘回显、多行粘贴、
  中文 IME 组合窗口、通知点击、滚动拽底、「回到底部」按钮出现时机——
  本机无截图工具（无 `grim`/`spectacle`/`import`，只有 `xdotool` + `fcitx5`），
  agent 无法替人目测，**这几项必须人手**；agent 侧负责 `runtime.<date>.log` 的
  `warn`/`error` 取证与归属判定。
- 观测点建议（供下一轮直接抄）：日志
  `~/.local/share/com.bedcode.app/logs/runtime.$(date +%F).log`，
  结构化字段按 AGENTS §8 是 `session_id = %… / plugin_id = %…`，
  会话面异常优先 grep `session_gateway` / `terminal-session` / `pty`。
