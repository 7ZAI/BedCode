# 05: 消息总线 topic 命名空间与互调接收侧（P0-4）

**What to build:** 总线成为真正的强制边界：owner 作用域 topic（`<base>.<owner>`）**只有属主能订阅、只有宿主能发布**，普通插件无法向他人 topic 伪发布；插件互调的回复通道校验接收者身份。修完后 AGENTS §5「插件间只经互调 API 与消息总线通信」这句话才有安全含义——此前该通道是无锁的。

**Blocked by:** 无（可与 02/04 并行；但文档同步要与 06 一起提交）

**Status:** done（2026-09-22 实施完成，见「实施记录」；机制 B 定档、移动端跟演清单已登记）

## 现状（已复核）

- 订阅侧零校验：`host_impl/bus.rs:73-101`（`bus_subscribe` / `bus_subscribe_binary` / `bus_unsubscribe`）把任意 topic 串直投 `subscribe_wasm`；
- 发布侧只有 `bedcode.api.*` 目标门（`host_impl/bus.rs:9-34`），普通 topic 显式放行（其测试 `:228-232` 自证）；
- 宿主事件以 `sender = "host"` 发布（`ws.rs:968`、`pty.rs:317`），派发只跳过 sender 自身（`bus.rs:246`）；
- 三处文档承诺与现实相反：`bedcode.wit:262-265`、code-map:163、`.scratch/2026-09-19-pty-base-service/spec.md:129`（均称「非属主物理上订阅不到」）；
- 未逐行复核（本票内确认）：`host_impl/api.rs:39-50` ReplyHandler 不校验 `msg.sender`，配合上述订阅面可订阅他人 api topic 抢答。

## 验收

- [x] **先落红测**：改前红 12 项（bus 10 + api 2），其中抢答用例直接把伪造回复投给了调用方
      （`api_call_rejects_reply_from_non_owner` 改前失败信息就是 `Ok("{\"result\":{\"spoofed\":true}}")`，
      即互调结果可被任意已激活插件劫持的实证）；改后全绿
      （原句写的是「订阅 `pty:exit.<A>`」；采机制 B 后 A 的事件已搬到 `A::pty:exit`，
      故红测按新串表达「B 订阅/伪发布 A 的命名空间被拒」，legacy 旧串另有显式拒绝用例）
- [x] 机制选型落地：**B（私有 topic 命名空间 `<plugin-id>::<name>`）**，取舍见上节
- [x] 宿主定向投递（pty / ws 客户端域 + 服务端域 / mdns）改投属主私有 topic；
      跨属主订阅与伪发布在 Rust 端拒绝且错误回给 guest（门禁在 `handle.spawn` **之前**同步判定，
      不会退化成「返回 Ok 但没订阅上」）；task 域核实不经总线（`events-task` 直投），无需迁移
- [x] `bedcode.api.*` 回复通道：订阅面对 WASM 关闭 + `ReplyHandler` 校验 `msg.sender` == api 声明属主
      （`ApiRegistry::owner_of`，与门禁同表）；ADR 0017 层 2 补齐，层 1 现状保持
- [x] 向后兼容：四个内置插件逐一核过（见上节「附带核实」），改插件侧调用点 = file-transfer 的
      mdns 两个 topic（已改用 SDK `mdns_event_topic`）；session 只发公开道、ai-chatbox / agent-hub 不碰总线；
      宿主测试夹具（plugin-pty-test / plugin-ws-test）经 SDK 助手取串，随助手自动跟形
- [x] 文档同步：`bedcode.wit`（host-bus 新增命名空间节 + host-websocket / host-pty / host-mdns 三处
      「物理订阅不到」改为兑现口径 + abi 节登记「不 bump 的行为变更」）、code-map（bus 节 + ws 双通道节）、
      `.scratch/2026-09-19-pty-base-service/spec.md:129` 订正、AGENTS §7 双端偏离登记、CHANGELOG
- [x] 门禁：`cargo test --lib` **1125 passed / 0 failed**（含新隔离用例），
      SDK `cargo test --lib` **99 passed / 0 failed**；总线吞吐/背压既有断言
      （`test_queue_full_drops_with_monitor_count` 守恒律、格式双拒绝、恰好一次投递）全绿

## 实施记录（2026-09-22）

1. **原语落点选了 SDK，不是宿主**：`owned_topic` / `topic_owner` / `is_reply_topic` /
   `is_legacy_owner_suffix` / `TOPIC_NS_SEP` + `API_TOPIC_PREFIX` / `REPLY_TOPIC_PREFIX`
   全部定义在桌面 SDK `host::bus`（native 可见，宿主直接 import；`api_call.rs` 在 `wasm` feature 下
   读不到，故两个前缀常量搬到这里再被它 re-export）。宿主 `host_impl/{pty,ws,mdns}.rs` 与
   `server/ws/channel/plugin.rs` 的定向 topic 改用它组合 → **宿主发布形状与插件订阅形状变成同一个函数**，
   原先「逐字节一致」漂移锁（`exit_event_name_matches_sdk_subscription_helper`）失去漂移面，
   改为锁「事件名常量 + SDK 域助手确实由 `owned_topic` 组合」，宿主是否真用了它由投递断言行为性兜住。
2. **门禁分三点，规则不同**（各自有理由，不做统一开关）：命名空间属主 → 订阅/发布/退订三点都查；
   回复道关闭 → 只查订阅（响应者必须能发布进 caller 的回复道）；legacy 形态拒绝 → 只查订阅
   （退订是清理动作，必须幂等可用，不能被新规则噎住旧产物）。
3. **`::` 的 fail-closed 取舍**：`topic_owner` 不校验属主段是不是合法插件 id——
   `::x`（空属主）、`task::foo`（非插件段）一律判为「有属主的命名空间」，
   于是任何插件都订不到（属主永远不等于它），只有宿主能投。宁可死掉一个 topic，
   也不留一个「解析不出属主就按公开道放行」的分支。有对应用例锁住。
4. **顺带修的泄漏**：`api_call` 旧顺序「先注册回复订阅 → 后过门禁」，门禁拒绝时 `?` 直接返回，
   订阅条目与其消费任务永久残留（req-id 单调递增，每次未声明调用漏一条）。现改为
   先解析属主（门禁前移）再注册，发布失败路径补 cleanup；两条用例（拒绝路径 / 成功路径）
   用新加的 `MessageBus::subscriber_count` 断言零残留。
5. **夹具的两难与处置**：pty/ws 隔离用例把**同一产物**以第二个属主 id
   （`com.bedcode.pty-test.peer`、`TERM_PERF_PLUGIN`）实例化，而 guest 只能用编译期 `Self::ID`
   拼自己的命名空间 → 票 05 之后这种跨属主订阅被拒（正是要它拒的行为），夹具 activate 由
   Err 降级为 log+continue。**属主本体没有变松**：`subscribe_allows_own_namespace` 单测 +
   pty e2e「A 必须收到 `<A>::pty:exit` 投递」两处仍要求真订阅成功，
   门禁若误伤自身命名空间会立刻红。（这是**测试装置**的既有用法，不是产品形态：
   生产路径 instantiate id == manifest id == 目录名，见 `validate_dir_binding`。）
6. **变异自检**（新增断言必须证明承重，全部转红后还原）：
   旁路 `check_namespace` → 6 红（订阅/发布/二进制发布/退订/端到端）；旁路订阅面两条规则 → 7 红
   （含回复道两条 + legacy 一条）；旁路 `ReplyHandler` 的 sender 校验 → 1 红（抢答用例）。
7. **未做 / 遗留**：
   - 移动端跟演清单（见上节）；
   - `bedcode-desktop/src-tauri/src/plugin/bus.rs` 的 `subscribe_static` 仍是「宿主侧无门禁」入口
     （本票刻意保留：`peer_net` 的刷新订阅、api 回复订阅都靠它），
     若将来引入插件可触达的静态订阅路径，必须另设门禁。
8. **仓库既有红（非本票造成，未动）**：`cargo check --lib --tests` 仍有 **4 个集成 target** 编译不过
   （`ws_session_route` / `ws_auth_rules` / `http_auth_biometric` / `broadcast_shutdown`，报的是已退役符号）。
   交接文档记的 5 个里 `pty_session_chain` 现已能编译（对侧改名批次带出来的变化，非本票）。
9. **同 worktree 交叠（证据）**：对侧 commit `5b008eb5c`（其「票 06 插件 id 改名全链」）
   把本票当时在途的 5 个文件一起提交了 —— `host_impl/{mdns,pty,ws}.rs`、`host_impl/tests/pty.rs`、
   `server/ws/channel/plugin.rs`（`git log -S owned_topic` 可查）。本票剩余改动单独成 commit，
   历史里「票 05 的宿主迁移」因此分在两个 commit 内；另外对侧的改名清扫把本票新写的
   `api_registry.rs` 测试字符串 `com.bedcode.session.*` 一并换成 `com.bedcode.terminal-session.*`
   （仅测试内自洽字面量，无行为影响）。AGENTS.md / CHANGELOG.md / code-map.md 三处文档
   本票只提交自己的 hunk（`git apply --cached` 局部暂存），对侧未提交的行留在工作区。

   **实际收尾与本条第 9 点的差别（如实记）**：对侧随后又落了 `36a142027`（票 07）与
   `c284f92ae`（其票 08 文档），后者把本票写在 AGENTS.md / CHANGELOG.md / code-map.md 的
   文档行连同其未提交的行一起提交了 → 局部暂存未及执行，本票文档散在
   `5b008eb5c` / `c284f92ae` / `d3eecff73` 三个 commit 里。收尾另加一个纯注释/文档
   commit 清掉残余旧串引用（`activation.rs`、`code-map` pty 行、SDK `abi.rs` 历史条目、
   WIT v13/v14 历史条目与两处函数注释、pty/ws 夹具模块头）。

## 门禁实跑数字（2026-09-22）

- `cargo test --lib`（bedcode-desktop/src-tauri）：**1125 passed / 0 failed / 0 ignored**，27.92s；
  重出插件产物后跑（file-transfer + terminal-session 各一次），日志内无 `[skip]` 标记
  （16 处 "skip" 全是测试名子串）
- `cargo test --lib`（packages/plugin-sdk-desktop/rust）：**99 passed / 0 failed**
- 新用例：`host_impl/bus.rs` +13（12→25）、`host_impl/api.rs` +4（5→9）、
  `plugin/security/api_registry.rs` +2、SDK `host/bus.rs` +5、SDK `host/mdns.rs` +1；
  改形 2 条（SDK pty/ws 形状锁）+ 宿主/ e2e 侧 topic 构造迁移（pty_e2e、ws_e2e、host_impl/tests/pty、mdns 内联）
- `cargo check --lib --tests`：`src/` 零错误；仅上条第 8 点的既有 4 个集成 target 红
- 前端零改动（总线没有 TS 侧订阅 API，已核实），故未跑 vitest / eslint
- rustfmt：改动的 4 个文件 HEAD 基线 clean → 单文件 rustfmt；
  `host_impl/mdns.rs`、`tests/terminal_output_perf.rs` HEAD 基线本就 dirty → 只手工对齐我新增的行，
  未整文件格式化（`api.rs` 曾被 rustfmt 由 CRLF 改成 LF，已按 `git diff --ignore-cr-at-eol` 核回 194/8）

## Comments

- 2026-09-21 立项：来源 spec §4-P0-4。

### 机制裁决（2026-09-22，采票面推荐 B：私有 topic 命名空间）

**规则**：topic 里出现分隔符 `::` → 第一个 `::` 之前的整段是**属主插件 id**，其后是事件名：
`<plugin-id>::<name>`。插件只能订阅/发布**自己命名空间内**的 topic；宿主不受限（静态订阅与定向投递）。
公开 topic（不含 `::`）语义不变，仍是插件间广播道（`task:*`、`peer:*`、`session:mode-changed`、
`bedcode.api.*`）。分隔符选 `::` 而非 `.` 的依据：`validate_plugin_id` 只允许 `[a-z0-9-]` 分段
（`plugin/manager/validation.rs:24`），`:` 不可能出现在插件 id 里 → 命名空间解析无歧义、不需要任何注册表。

**为什么不选 A（总线内建 owner 段解析）**：A 必须在 topic 串里猜「哪一段是属主」，而插件 id 本身含 `.`
（`com.bedcode.session`），要么维护「哪些 base 属定向事件」的清单（每个新事件域都要登记，漏登记即静默破口），
要么拿插件 id 集合做后缀匹配——后者在**属主当时未激活/尚未安装**时判不出来：B 抢先订阅 `pty:exit.com.victim`
（此刻 victim 不在表里 → 按公开 topic 放行）→ victim 激活后事件照投 B。定向投递的边界不能依赖时序，
故 A 否决。B 把「这是谁的收件箱」写进了串本身，判定与激活态、安装态、时序全部无关。

**为什么不选 A'（保留 `<base>.<owner>` 串、只加 ACL）**：同上的未安装窗口，且 `filesrv.com.bedcode.session`
这类合法的公开广播名会被误判成定向道。命名空间形态没有这两类误判。

**代价（已接受）**：宿主定向事件的 topic 串变了（`pty:exit.<o>` → `<o>::pty:exit`，ws 五事件、mdns found/lost 同），
SDK 助手与 file-transfer 的 mdns 订阅点随迁；按 ADR 0022 登记双端偏离（移动端 `host-mdns` 仍用
`mdns:found.<owner>`，见下方偏离清单）。WIT **函数签名零变化**，故不 bump ABI——理由是破坏面在串而不在签名，
bump 也拦不住旧产物（兼容规则只拒「插件 ABI > 宿主」），真正的兜底是下面的 legacy 显式拒绝。

### legacy 形态处置（防旧产物静默断流）

旧 SDK 产出的串（无 `::` 且以 `.<自身插件 id>` 结尾）在**订阅侧显式拒绝**并回带新形态文案，
不再退化成「订阅得到、永远收不到」。依据：票面验收要求「拒绝且错误 fail-visible（不静默丢弃）」，
而旧产物里唯一会产出该形态的路径就是被本票迁移掉的定向事件（SDK 助手是唯一入口，插件手拼只有 file-transfer 两处）。
发布侧的旧形态无需处理：定向事件已改投命名空间 topic，旧串上不存在合法订阅者，伪发布打不到人。

### 互调回复道（P1-4）规则

- 回复 topic **不改串**（仍 `bedcode.api.reply.<caller>.<req-id>`）：响应者是**他人**，必须能发布到 caller 的
  回复道，改走 `<caller>::…` 会让「非属主可发布」变常态，命名空间的收件箱语义就废了。
- 隐私缺口另堵：`bedcode.api.reply.*` 对 WASM 订阅面**一律拒绝**（回复订阅本就是宿主侧 `subscribe_static`
  注册的，插件从不订阅自己被投的路径）。此前该道可被窃听，因为 SDK 的 correlation id 是
  **进程内单调计数器**（`req-1`、`req-2`…，`api_call.rs:40`），完全可猜。
- 抢答缺口：ReplyHandler 校验 `msg.sender` == api 注册表里该 api 的**声明属主**。`sender` 由宿主按
  Caller state 落章（guest 无法自报），所以校验成立；非属主回复丢弃 + `warn`，调用方按超时收敛。
- 顺带修：`api_call` 原顺序是「先注册回复订阅 → 后过门禁发布」，门禁拒绝时 `?` 直接返回、订阅与消费任务泄漏。
  改为先解析属主（门禁前移）再注册，发布失败路径补 cleanup。

### 双端偏离清单（移动端本轮不动，AGENTS §7）

- `bedcode-mobile/src-tauri/src/plugin/wasm_runtime/host_impl/mdns.rs:467` 仍发 `mdns:found.<owner>`；
- `bedcode-mobile/packages/plugin-sdk-mobile` 无 `owned_topic` / 无命名空间门禁；
- `bedcode-mobile/plugins/file-transfer/rust/src/lib.rs:39,41` 仍手拼旧形态订阅。
⇒ 移动端要跟演时需补：mobile SDK 命名空间原语 + mobile 总线同款 ACL + mobile file-transfer 迁移，
  三处一起动才不会自相矛盾。桌面本轮结果不构成移动端的正确性依据。

### 附带核实（票面「未逐行复核」项）

- 宿主定向事件域清点：`pty:exit`（host_impl/pty.rs）、`ws:open|error|close`（host_impl/ws.rs）、
  `ws:client-connect|client-disconnect`（server/ws/channel/plugin.rs）、`mdns:found|lost`（host_impl/mdns.rs）
  ——**task 域不经总线**（`events-task` 可选导出直投，见 host_impl/task.rs），票面「pty/ws/mdns/task」
  中的 task 无需迁移，登记以免下轮重复查证。
- 4 个内置插件 topic 用法逐一核过：session 只**发布**公开道（`task:*`、`session:mode-changed`，当前零订阅者，
  含宿主前端在内——总线无 TS 侧订阅 API）；file-transfer 订阅定向 mdns 道 + 公开 `peer:*` 道 +
  发布 `peer:discovery-refresh`（宿主静态订阅）；ai-chatbox / agent-hub 完全不碰总线。
  ⇒ 需要改插件侧调用点的只有 file-transfer 两处。
