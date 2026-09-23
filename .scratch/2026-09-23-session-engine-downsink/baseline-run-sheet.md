# 票 01 · 人工基线复跑单（run sheet）

> 本文件是 [`issues/01-desktop-parity-human-baseline.md`](issues/01-desktop-parity-human-baseline.md)
> 的**可执行形态**。票面验收标准是「勾选项」，这里把每条展开成
> 「怎么操作 → 什么算对 → 去哪儿取证 → 结果槽」，供人与 agent 分工填写。
> **结果只往本文件与票末 Comments 记，本票不修任何东西。**

## 0. 分工与前提（2026-09-24 用户定）

| 角色 | 负责 |
| --- | --- |
| **人** | 每一条目测：键盘回显、多行粘贴、中文 IME 组合窗、滚动拽底、「回到底部」按钮出现时机、通知点击、弹窗文案与计数 |
| **agent** | 起环境（`pnpm run tauri:dev`）、盯 `runtime.<date>.log` 出 `warn`/`error` 取证、判差异归属、记账落票末 |

**为什么必须人跑**：本机无截图工具（`grim`/`spectacle`/`import` 都不在，只有 `xdotool` +
`fcitx5`），无头测试也覆盖不到 xterm 真实渲染、合成器输入路与 IME 组合窗口。

**起跑前置**：先跑 `bash .scratch/2026-09-23-session-engine-downsink/baseline-preflight.sh --compile`，
它的输出直接粘进本文件「## 1 起跑记录」。

## 1. 起跑记录（每次复跑填一次）

### 本轮：2026-09-24 06:2x–06:4x（首轮实质观测）

```
日期 / 时间      : 06:22 首启 → 06:41 起稳定实例（本地 CST）
HEAD 提交        : 0f7567477（票 06 landed，在本专项票 13/14 三笔之上）
工作区污染       : docs/diagrams/*（文档线，不影响运行态）
                   + 并发批次此刻在改 enums/auth.rs、server/core/link_crypto.rs、
                     websocket/channel/{event,terminal}.rs → 见下「重启抖动」
插件产物预演     : ai-chatbox / terminal-session / file-transfer 三条全 FRESH（起跑不补建）
宿主 lib 可编译  : cargo check --lib 通过
插件激活态       : terminal-session Activated、agent-hub Activated、file-transfer Activated
日志实路径       : …/logs/runtime.2026-09-23.log（按 UTC 命名，见 §1c）
稳定实例健康度   : 红线 0 · 凭证拒绝 0 · ERROR 仅 2 条（重复 id 拒绝，§3.10 判非回归）
```

**一条重要更正（我第一版把原因判错了）**：首轮 `pnpm run tauri:dev` 十分钟内**宿主重启 11 次**。
我先归因给「plugin-watch 复制 → `resources/**` 变更 → tauri 重启」，但它只贡献 3 次
（三条 `resources/.../index.js changed`）；**主因是并发批次在保存 `src-tauri/src/**` 宿主源文件**，
而 `tauri dev` watch 的正是 `src-tauri/` 整个目录——对侧每存一次盘，基线 app 重启一次、
日志被 `[logging] dev reset` 清零一次。

⇒ **判据（补进每次起跑）**：走清单之前先确认「没人在编辑宿主源」。两种观测任选：
`grep -cE "Running DevCommand" <dev日志>` 隔 100 秒取差（本轮 4→4，增量 0 才算稳）；
或 `find bedcode-desktop/src-tauri/src -newermt '-3 minutes' -name '*.rs'` 为空。
**不稳定时不要开始走清单**——半程重启会把已观测条目连同证据一起作废（本轮真实代价：
首实例的 3 条凭证拒绝证据就散在被重置的日志里，只能靠 §3.11 的时序记录留档）。

**本轮已顺带验通、不需人测的一条**：06:30 那轮插件激活后建了真实会话——
`host-pty: 插件私有 PTY 已创建` → `[plugin:…] session created via host-pty (session_id=806fad27-…)`
→ `SyncEventHandler SessionCreated` → `/terminal-window/<id>` 独立窗 page-load。
即「插件自持 PTY 建会话 + 事件面自足 + 桌面开终端窗」这条 P1-b 主链**真机可用**。

### 每次复跑要填的空格

```
日期 / 时间      :
HEAD 提交        :
工作区污染       : （preflight [2] 段原样粘贴）
插件产物预演     : （preflight [3] 段：FRESH / 需补建）
宿主 lib 可编译  : （preflight [4] 段）
插件激活态       : ← 关键前置，见 §1b
重启抖动         : ← 上节判据，增量 0 才继续
日志文件实路径    : ← 用 `ls -t …/logs/runtime.*.log | head -1` 取，禁止按本地日期拼名
日志起跑前行数   : （用于只读增量：tail -n +<行数+1>）
```

### 1a. 日志锚点的**实测纠正**（照原锚点 grep 会两头错）

2026-09-24 06:30 真机一轮：插件激活后建了一条真实会话（真 PTY），但三个「转发层」锚点
**全部 0 命中**——

```
"session created via plugin"           0
"session stop requested via plugin"    0
"session removed via plugin"           0
```

原因：`utils/session_gateway.rs` 那组 info 只在**宿主命令面**发起的路径上打；
桌面前端的建会话/输入走**插件贡献的命令通道**（不经转发层），所以生产路径上它们不出现。
拿它们当判据会得出两种错结论：grep 不到 → 以为「会话没建起来」（假红），
或以为「这条路径没被走到」而跳过核对（假绿）。

**该用的实测锚点**（真机验证过命中）：

| 用途 | 锚点 |
| --- | --- |
| 引擎侧确实起了 PTY | `host-pty: 插件私有 PTY 已创建 plugin_id=com.bedcode.terminal-session` |
| 插件登记域收了会话 | `[plugin:com.bedcode.terminal-session] session created via host-pty` |
| 配额声明生效 | `host-pty: 配额已登记 plugin_id=…` |
| 插件域就绪（激活成功） | `Terminal Session Center plugin activated` / `session registry store ready` / `session lifecycle listener registered` / `session input listener registered` |
| 事件面被宿主收到 | `[SyncEventHandler] Processing event: SessionCreated { session_id: … }` |
| **红线（必须 0）** | `session plugin not active` / `failed via plugin` |
| **通道凭证被拒** | `[PluginChannel] 缺少有效通道凭证，插件面命令被拒绝`（fail-closed，见 §3.11） |

> 顺带一条给票 08 的输入：票 08 注销宿主会话命令面时，那组 `via plugin` info 会随之消失——
> 本清单的锚点表届时要再核一次，别留成「查不到就等于坏」。

### 1b. 起跑后第一件事：确认 `com.bedcode.terminal-session` 已激活

P1-b 起会话真源在插件，宿主无降级轨（AGENTS §8）。该插件未激活时**清单 10 项一条都观测不了**
（终端窗口打不开 / 会话面显性报错），而这看起来像「基线挂了」而不是「环境没就绪」。
判据在 dev stdout 的这两行：

```
[PluginHost] Initialization complete: 4 plugin(s) total, 4 wasm, N activated, 0 degraded, 0 error
[PluginHost]   - com.bedcode.terminal-session (state=Activated, type=RustTs)   ← 必须是 Activated
```

`state=Loaded` 就是没激活（2026-09-24 首轮即撞上：只有 file-transfer 从持久化状态自动激活）。
**处置**：在插件管理界面启用「会话中心」后重跑，不要改持久化状态文件去绕。

### 1c. 日志路径与格式的三个坑（2026-09-24 实测，都是会让人误判「无异常」的坑）

1. **文件名按 UTC，不按本地日期**。本地 `2026-09-24 06:2x CST` 起的那轮，日志落在
   `runtime.2026-09-23.log`（UTC 仍是 09-23T22:2x）。
   ⇒ 票面写的 `runtime.$(date +%F).log` 会指向一个**不存在的文件**，
   grep 空文件 = 假绿「无异常」。一律用：
   `LOG="$(ls -t "$HOME/.local/share/com.bedcode.app/logs/"runtime.*.log | head -1)"`
2. **落盘日志时间戳是 UTC ISO**（`2026-09-23T22:25:36Z`），与 dev stdout 的
   `2026-09-24 06:22:33.334` **不同形**。⇒ 按本地时分 grep 恒为 0 命中，别拿它当「没发生」。
3. **每次 dev 启动会重写当日日志**（stdout 可见 `[logging] dev reset: replaced today's log …`）。
   ⇒ 基线跑到一半重启 = **前半程证据清零**。要么一轮跑完，要么每轮结束立刻把增量另存
   `/tmp/baseline-run-<hhmm>.log` 再记账。

> **污染必须记**：本仓库常有并发批次在同一 worktree 写盘，且 `src-tauri/target` 已 20G /
> 磁盘仅剩 ~19G，**无法另开 worktree 跑干净树**（此路已在票末排除）。所以基线一律
> 「照当前工作区跑 + 记账标明污染」，差异归属走上节三问。

## 2. 归属三问（每处差异必答，答完才允许立票）

1. **是不是并发票的 WIP 带出来的？** 看 preflight [2] 的归属列：差异面若落在
   票 06（`server/websocket/*` / `session_gateway.rs` / `terminal_ws/*` / `pty_session_chain`）
   或 host-crypto 线在途文件上 → 标「对侧 WIP」，不算本专项回归，也**不代改**。
2. **是不是 P1-b 会话真源下沉的回归？** 判据：日志出现
   `session plugin not active` / `refused: session plugin not active` / `failed via plugin`，
   或界面上出现「会话列表空 / 终端无回显 / 输入丢键 / 任务状态假中断」
   —— 这类是**真源换了地方却被静默吞掉**（AGENTS §8 红线），必立票。
3. **是不是与迁移前一致？** 拿 2026-09-22 之前的行为做对照（终端窗口域下沉那批的票面记录
   在 `.scratch/2026-09-22-pty-business-downsink/` 与路线图 M 清单）；一致则写
   「非回归 + 判据一句话」，**不得留空**。

新立差异票编号：**从 `issues/16-…` 起**（票面上写的「`issues/13-` 起」已被 13/14/15 占用）。

## 3. 清单条目

### 3.1 键盘输入

- **操作**：开一个终端窗口（bash 会话），依次敲：普通字符若干 → 回车提交 → 退格删两个字符再改 →
  ↑ 调历史命令 → Tab 补全一个路径
- **预期**：每步终端都有回显，且**确实进了 PTY**（回显 + 命令真执行）；退格不留残字符；
  ↑ 出历史；Tab 补全出候选或直接补全
- **反例特征**（历史上真出过）：按键丢了 / 双发 / 只回显不执行 → 指向「宿主回查内核拿会话」
  的静默降级（§8 红线），日志应有 `refused: session plugin not active`
- **结果**：☐ 正常 ☐ 异常 → 记 `issues/__`
- **备注**：

### 3.2 多行粘贴

- **操作**：复制一段 3 行以上、每行都是可执行命令的文本，粘贴进终端
- **预期**：括号粘贴块生效——**不**被当成连按回车逐行立刻执行；粘贴后停在最后一行等回车
- **另一面**：任务历史里**不应**长出多余行（每行被记成一条提交行 = 括号粘贴没透传到写入管线）
- **结果**：☐ 正常 ☐ 异常 → 记 `issues/__`
- **备注**：

### 3.3 中文 IME（重点，历史出过缺陷）

- **操作**：`fcitx5` 切中文，在终端里组合一个词（如「测试」）→ 空格上屏 → 回车
- **预期**：组合窗口内**不双发**（候选串不进 PTY，只有上屏结果进）；上屏后回显正确一次
- **取证**：日志无输入相关 error；必要时对照 `echo` 输出行数
- **结果**：☐ 正常 ☐ 异常 → 记 `issues/__`
- **备注**：

### 3.4 滚动

- **操作**：制造持续输出（如 `seq 1 5000 | awk '{print $1}'` 或 `yes | head -20000`），
  输出过程中向上滚动看历史，再让新输出一阵
- **预期**：上滚后**不被新一轮输出拽底**；离开底部时「回到底部」按钮出现，回到底部后消失
- **结果**：☐ 正常 ☐ 异常 → 记 `issues/__`
- **备注**：

### 3.5 特殊键与工具栏

- **操作**：跑一个前台程序（`sleep 300`）→ Ctrl-C；工具栏「停止」「复制」各点一次
- **预期**：Ctrl-C 中断并显示 `^C`；「停止」终止会话且状态转 `Stopped`；「复制」把选区送进剪贴板
- **取证**（锚点见 §1a，原写的 `session stop requested via plugin` 实测 0 命中、已废弃）：
  停会话看 `[plugin:com.bedcode.terminal-session]` 侧终态行与
  `[SyncEventHandler] Processing event: SessionStopped`，且**只**一次
  （双发 = kill 与 `pty:exit` 各广播一次的旧缺陷复发；单点广播是 P1-b 定死的）
- **结果**：☐ 正常 ☐ 异常 → 记 `issues/__`
- **备注**：

### 3.6 多窗口与尺寸争用

- **操作**：同一会话开两个终端窗口，第二个窗口改尺寸（或反向）
- **预期**：弹出「覆盖确认」；确认后归属移交，此后原窗口改尺寸再次触发确认
- **对照**：裁决逻辑在插件登记域（`resize` 的 `force` 位），宿主只转发（`session_gateway::resize`）
- **结果**：☐ 正常 ☐ 异常 → 记 `issues/__`
- **备注**：

### 3.7 通知与种子化

- **操作**：新建会话 → 停止会话，看通知文案与计数；然后**从会话列表**和**从通知**分别打开终端窗口
- **预期**：文案与实际动作一致（名称/来源设备正确）；计数不重复不吞并；
  两种入口打开后**内容回放正确**（不是空白、不是半截错位）
- **注意**：回放路径可能经过票 06 在途的 `session_gateway::history_snapshot`（引擎环优先）——
  若这里出差异，先走归属三问第 1 问
- **结果**：☐ 正常 ☐ 异常 → 记 `issues/__`
- **备注**：

### 3.8 任务域

- **操作**：在受支持的 agent 会话里敲一行提交 → 看任务历史；再从「自动任务」队列下发一条 prompt
- **预期**：任务历史出现该记录；队列投递后进入 `executing`
- **反例特征**：队列被批量标中断 = 任务队列假中断（曾与真源降级同族出现）
- **结果**：☐ 正常 ☐ 异常 → 记 `issues/__`
- **备注**：

### 3.9 关窗守卫

- **操作**：有存活会话时关主窗；再把会话全关后重开、再关主窗
- **预期**：有会话时弹确认，且列表与计数**与实况一致**（不多列已停的、不漏列活着的）；
  无会话时直接关，不弹
- **注意**：守卫自 v25 起只引用 PTY 引擎事实（存活句柄计数 / 全局回收）——
  计数对不上就是引擎事实与插件登记不同步，属本专项红线
- **结果**：☐ 正常 ☐ 异常 → 记 `issues/__`
- **备注**：

### 3.10 全程日志体检（agent 跑）

```bash
# 路径按 mtime 取，不要按本地日期拼（见 1b 坑 1）
LOG="$(ls -t "$HOME/.local/share/com.bedcode.app/logs/"runtime.*.log | head -1)"; echo "$LOG"
# 只看本次增量（行数从「## 1 起跑记录」取）
tail -n +<N> "$LOG" > /tmp/baseline-run.log
grep -cE 'ERROR|WARN' /tmp/baseline-run.log
grep -E 'session plugin not active|failed via plugin|\[plugin:' /tmp/baseline-run.log | tail -40
```

- **红线判据（必须零条）**：`session plugin not active` / `refused: session plugin not active`
  / `failed via plugin`
- **先扣除「本机环境噪音」再统计 ERROR/WARN**（2026-09-24 06:2x 实测基线，全部来自
  `~/.local/share/com.bedcode.app/plugins/` 里跨批次留下的目录，**与 P1-b 无关**）：

  | 条数 | 级别 | 内容 | 判 |
  | --- | --- | --- | --- |
  | 4 | WARN | `Skipping dir without plugin.json (orphan residue)` → `com.bedcode.scheduler`（已退役）/ `.session`（改名前旧 id）/ `.terminal-session`、`.file-transfer`（空壳） | 非回归；本机残留 |
  | 2 | ERROR | `Rejecting duplicate plugin id "com.bedcode.ai-chatbox" / "com.bedcode.agent-hub" … already loaded from another directory` | 去重**行为正确**（内置胜出，用户目录副本被拒），但级别用错：可恢复的过滤拒绝按 AGENTS §8 应为 `warn!` |

  ⇒ 本机的「干净起跑」不是 0 ERROR，而是 **0 未解释 ERROR**；报数时写「2 条重复 id 拒绝（已判非回归）」。
  级别误用这条若要对齐 §8，另立新票，不在本票修。
- **先排除另一类刷屏**：`peer mDNS search started …` 这类 DEBUG 每 4–8 秒一条
  （实测 5 分钟 250+ 行），`grep -c 'DEBUG'` 的大小不代表健康度——按级别统计只数 ERROR/WARN。
- **结果**：☐ 无红线 ☐ 有红线 → 记 `issues/__`
- **备注**：

### 3.11 插件面命令的通道凭证（2026-09-24 真机首轮撞到的新面）

- **背景**：`frontend_channel.rs::authorize` 是 **fail-closed**——凭证解析不出来就拒，
  绝不回退到「按参数 plugin_id 放行」（`7825359bd` 审计票 06 / P0-5 的裁决）。
- **首轮实测时序**（UTC 22:30 一轮，会话建成功之后）：
  ```
  22:30:26  host-pty: 插件私有 PTY 已创建 …            ← 建会话 OK
  22:30:28  [PluginChannel] 页面加载，重置前端通道会话  url=…/terminal-window/806fad27-…
            前端通道会话已重置 reason=page-load revoked_tokens=3
  22:30:29  loader 会话密钥已签发 / 四插件通道令牌已签发
  22:30:40  WARN 缺少有效通道凭证，插件面命令被拒绝 plugin_id=com.bedcode.terminal-session
  22:30:42  WARN 同上
  22:30:48  WARN 同上                                    ← 三次，均在签发之后
  ```
- **要人回答的问题**（agent 只能看到拒绝，看不到界面上是什么）：
  1. 这三次对应你**点了什么**？（终端窗口里的操作 / 侧栏 / 设置页）
  2. 界面有没有可见错误提示？还是静默无反应？——**静默无反应要单判**，
     fail-closed 拒了但没告诉用户 = §8「用户可见错误走 i18n」那条的另一半
  3. 同一操作再点一次还失败吗？（区分「终端窗口这个独立 webview 从来没拿到令牌」
     与「page-load 重置把已签发令牌作废了」两种根因）
- **归属提示**：终端窗口是 `/terminal-window/<id>` 这个**独立 webview**，
  令牌签发发生在主窗 loader 路径上——若确认是它没凭证，属「P1-b 终端域下沉 + P0-5 凭证绑定」
  两条线的**交界处**，不是任一方的单独回归；立票时两条都要点名。
- **结果**：☐ 未复现 ☐ 可复现 → 记 `issues/__`
- **备注**：

## 4. 结论模板（跑完填，同步到票末 Comments）

```
本轮基线：N/10 条正常，M 条异常，K 条判「与迁移前一致」
HEAD：   <hash>
污染：   <preflight [2] 摘要>
新立票： issues/16-…, issues/17-…
阻塞下一轮：无 / 有（写明）
```

**票 01 只有在全表有结论（正常 / 异常已立票 / 非回归已写判据）后才算 done**；
「一条都没观测」不等于「一条都正常」——这是 2026-09-24 那轮空跑留下的教训。
