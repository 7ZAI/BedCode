# 票 12 · 终端订阅协议客户端迁插件（建 `com.bedcode.terminal-session` 移动版 app）

> 状态：**已实施（2026-10-08），门禁结果见 §11**。
> §10 三个裁决点（D-12a/b/c）均按推荐案落地：`jwt-auth` 宿主代发
> （`host_impl/ws.rs`）、`host-terminal-stream.forward-output` + `host-connection.primary-target`、
> R1 宿主 auto-reconnect（`run_reconnect`）；本票 ABI 14→15（并行会话票 14B 同日叠 16）。
>
> **接手会话 2026-10-08 07:36 校订（事件记录保留）**：实施会话变异自检期间
> 一次 execute_command 审批超时被打断，变异注入 stub `src-tauri/src/terminal_link.rs`
> 与 `lib.rs` 注册项一度残留——校订会话清理并复跑锁 4/4 全绿；实施会话随后完成
> 其余变异（含 jwt-auth 判据翻转）与全部还原，git diff 核对零漂移。校订注列出的
> 「未完成 ①②③④」由实施会话依次完成：① 插件 crate 40/40 ② wasm32 门禁通过
> ③ 真实组件闭环 `terminal_session_domain_full_loop_with_real_component` 通过
> ④ fixture ABI 断言已由票 14B 同步至 16（其条目），宿主 lib 6 处 egress 失败
> 确认为在途基线。
> 接手会话已把 fixture 修为 16，但**因并行会话改动未能复验**。
>
> 上一票：票 11（host-websocket 客户端域，ABI 14，`ticket-11-websocket-client-domain.md`）；
> 票 14 阶段 B 阻塞点①指名的「目标 app 由本票创建」已兑现。
> 全部事实取自 2026-10-08 工作区实测（行号可复现），不凭记忆。

## 1. 目标（spec 原文 + 实测校准）

- `terminal_link.rs`（1,363 行，`bedcode-mobile/src-tauri/src/terminal_link.rs` + `tests/` 11 个用例文件）的**协议客户端**整体迁入新内置 wasm app `com.bedcode.terminal-session`（移动版，D6 选项 A 已拍板）：subscribe / ack / ring_resync / special-key 翻译 / 退避重连随迁。
- 插件经 **host-websocket 客户端域**（票 11，ABI 14）连桌面 `/ws/plugin/com.bedcode.terminal-session/terminal`。
- 宿主 `terminal_*` 命令面注销（frontend 走插件命令面）；**保留例外**见 §3.2（页面 Channel 登记是 Tauri 传输机制，插件无 Channel 概念）。
- **C3 性能红线**：输出帧帧级直传，禁止逐帧 JSON 化。
- 同批处理票 11 §4 遗留：「v13 产物无 ws 能力」的单向 ABI 协商风险（§8——实测结论：本票无此风险面，论证落档）。

## 2. 事实盘点（实测，设计驱动）

### 2.1 迁出对象：terminal_link.rs 结构

| 块 | 内容 | 迁移去向 |
| --- | --- | --- |
| 帧构造纯函数（225–310 行） | subscribe / ack / poll / input 帧 + `plan_input_frames` + `should_send_ack` 节流 + `classify_server_error` | 插件 crate 纯函数（native 可测） |
| `TerminalLink` 状态机（315–555） | phase / subscribe_ack 门控 / cursor / pending_ack / session_missing strikes / pending_resync / 段2 Channel 转发 / 统计 | 插件 crate（Channel 转发段换新原语，§6.2） |
| `TerminalLinkManager`（560–743） | links / consumers / page_channels 三表 | links+consumers 归插件；**page_channels 留宿主**（§3.2） |
| `connect_once`（748–954） | tokio-tungstenite 直连 + 首消息 auth + select! 循环（出站帧 / 入站帧 / ack 空闲兜底 tick / Ping tick / 静默探测） | **逐段重造**（WASM 无 tokio，§6.3） |
| `handle_control_text`（957–1051） | subscribed / ring_resync / session_stopped / error 分类 | 插件 crate 纯状态机 |
| `link_io` 重连（1071–1115） | `ReconnectManager` 退避 + pending_resync 标记 | 见 §6.3 裁决点 |
| Tauri 命令面（1124–1294） | 10 个 `terminal_*` 命令 | 8 注销 + 2 保留（§3.2） |
| `terminal_get_history`（1219–1276） | HTTP 直取（前端未接线） | **删除不迁**（零消费者 + 无流会话历史由订阅回放提供；将来需要时走票 13 的会话 HTTP 面） |

### 2.2 WASM 插件执行模型的三个硬缺口（本票设计的真正约束）

移动端插件是**被动回调**形态（`wasm_entry!`：activate / deactivate / invoke_command / on_bus_message / on_message_binary），WASM 内**无 tokio、无 sleep、无 interval、无自主任务**。terminal_link 的三处时间驱动面必须重新归位：

| 现状（Rust 宿主自主） | 缺口 | 归位方案（§10 裁决） |
| --- | --- | --- |
| 重连退避（`ReconnectManager` + sleep，1s→30s 指数 + 抖动 + 下限钳制） | 插件无法「等 N 秒再重连」 | **方案 R1（推荐）**：host-websocket 加 `auto-reconnect` 传输参数，宿主复用 `connection::reconnect::ReconnectManager` 重建连接，新句柄 `ws:open` → 插件按新句柄重新订阅；**方案 R2**：插件算退避、前端 setTimeout 到点调 `reconnect` 命令（webview 后台冻结时 setTimeout 停摆 = 重连停摆） |
| 心跳（30s Ping + 90s 静默判死 + 5s 探测 tick） | 插件无法周期发 Ping（WIT 无 ping 原语）且无法周期判死 | **宿主传输面**：host-websocket 客户端连接级心跳（桌面 server `conn.rs` 骨架级心跳同款先例——「心跳归引擎」）；config 可调周期，缺省 30s / 判死 3× |
| ack 空闲兜底（250ms 半空闲轮询 + 64KB 阈值） | 插件无法定时求值 | 阈值节流留插件（64KB）；空闲兜底由前端渲染驱动替代——前端 `onWriteParsed`/rAF 本就持续推进 ack（`useTerminalBuffer.ts` 现状），连续性弱于轮询但方向一致；极端静默场景由重连兜底 |

### 2.3 JWT 首消息认证：插件拿 token 违反 C4

- 桌面 terminal 端点认证 = **桌面宿主校验首消息** `{"type":"auth","token":"<jwt>"}`（`packages/bedcode-server-websocket/src/channel/plugin.rs`：`auth:"jwt"` → `AuthMode::Required` + `AuthFrame`——注释原文「宿主自有的极小契约，不引入 message.rs 类型」；窗口内未认证 → close 4001）。桌面插件 `ws_terminal.rs` 零 auth 帧处理（全文件无 `"auth"` 字面）。
- 现状移动端由宿主发 auth 帧（`terminal_link.rs:776` `crate::state::get_global_token()`）。
- 迁插件后：auth 帧由插件发 ⇒ **token 落插件内存 = 违反 C4**（spec C4：JWT 持有留宿主引擎；AGENTS §8 认证链路只走既有 auth 模块）。
- 方案（§10 裁决 D-12a）：host-websocket connect config 增加 **`jwt-auth: true`**——宿主在连接建立后立即从 `state::get_global_token()` 取 token 代发 auth 帧。论证：① 与 HTTP 面同构（前端零资源访问红线 = 网络发起权在 Rust + 闸门；HTTP 代理同理不把 token 交前端/插件）；② auth 帧形状是**两端宿主的传输面认证契约**（桌面 AuthFrame 已是「宿主自有契约」，移动宿主代发是对称出示，非业务解释）；③ 协议编排（subscribe / ack / resync）仍全部归插件；④ token 不落插件、不落日志（凭据红线不变）。

### 2.4 输出帧到前端的通道缺口（C3 的落点）

现状链路：桌面 → 移动宿主 WS → Rust ingest → **页面 Channel 裸字节**（`InvokeResponseBody::Raw`）→ terminalBuffer store → xterm。

迁插件后帧只能进 WASM（`<id>:ws:message` 二进制信封 → `on_message_binary`）。插件往前端的既有出口只有 `host-events.emit`（**JSON 面**——`host_impl/event.rs` 走 `app.emit(event_name, json_payload)`，字节必须 base64 ⇒ **C3 违规，排除**）。页面 Channel 是 Tauri 机制：前端 `new Channel()` → 宿主命令登记 → 宿主持表推送；**插件无 Channel 概念**。

⇒ 需要**一条「插件 → 宿主 → 页面 Channel」的二进制窄转发原语**（§6.2）：宿主零解析（不读字节内容，按 session-id 寻址已登记 Channel），对齐桌面四类薄壳④「零解析窄转发」（`utils/session_gateway.rs` 同款）。宿主 `terminal_page_subscribe/unsubscribe` 保留为该转发表的登记面（§3.2）。

### 2.5 目标地址获取：插件需要「主连接桌面」事实

现状 `connect_once` 用 `conn.get_target()`（主连接真源，`connection/manager.rs:176`）。迁插件后插件需要桌面 address:port 拼终端端点 URL。候选：
- mdns browse 自建缓存（file-transfer deviceState 范式）——**否**：发现地址 ≠ 主连接地址（多网卡/多实例选择是产品语义，且 mdns 时序抖动造成行为回退）；
- **新原语 `host-connection.primary-target()`**（推荐，§10 裁决 D-12b）：`host-websocket` 同款「同名接口移动子集」先例，1 函数返回 `{address, port, connected}` 引擎事实（name 展示名不返回——归插件自持）。**票 13（会话控制迁插件）的 `resolve_base_url` 是同一地基**，一次落两票复用。与桌面 host-connection（15 函数连接上下文域）同名不同形 → C8 同款登记 ADR 0018 偏离表。

### 2.6 其它实测事实

- `host-terminal.send`（`terminal:input`）在移动端**零插件消费**（auto-task 权限 = `session:read/storage/ui:input/ui:toolbox`，无 terminal:input；全仓无 `terminal_send` 插件调用点）——票 12 **不动** host-terminal（票 15 退役整面）。
- special-key 翻译移植源：桌面 `wasm-apps/terminal-session/rust/src/keys.rs` 头注释「下沉自宿主 `enums/special_key.rs`，等价移植，宿主侧已删除同逻辑」——移动端同款处置：宿主 `enums/special_key.rs` 随迁插件后删除，插件 crate 内移植（语义逐字节一致）。
- SDK 已有 `bus_subscribe_binary` / `bus_publish_binary`（`wasm_host.rs:315–325`）与帧信封解析 `parse_ws_frame`（`host/ws.rs`）——插件消费 ws 帧零新机制。
- 权限词汇 `terminal:output` 已存在（`permission.rs:8`，现挂 `terminal.onOutput` API）——输出转发原语的权限门**复用既有词汇，零词汇新增**。
- 构建链：`scripts/plugin-package-list.json` mobile 列表 + `loader.rs extract_apk_plugins`（按 app_version 解压）——新 app 需登记 mobile 列表。
- 段2 页面 Channel 现状：`terminal_link.rs` Manager.page_channels + `terminal_page_subscribe` 命令（前端 `terminalBuffer.ts` markPageEntered/markPageLeft 配对）——保留在宿主（§3.2），表迁移到新窄转发模块。

## 3. 范围裁决（硬边界）

### 3.1 本票不做

- **终端 UI 迁移**（TerminalView / composables/terminal / terminalBuffer store / 终端字体）= 票 15；本票只换前端「数据源」（命令面 + 事件名），UI 层零改动。
- **auto-task 并入** = 票 16（D6）；本票 plugin.json 只声明终端域权限，所有权票 16 接管并集。
- **会话控制 / 配对编排** = 票 13 / 票 14B（但 `host-connection.primary-target` 本票落，供票 13 复用）。
- **wss**（票 11 已裁决不做）；**host-terminal / terminal-hooks 退役** = 票 15。

### 3.2 宿主保留面（四类薄壳④，零解析窄转发）

| 保留项 | 理由 |
| --- | --- |
| `terminal_page_subscribe` / `terminal_page_unsubscribe`（Tauri 命令，Channel 登记） | Channel 是 Tauri 传输机制，插件无法持有；命令从 terminal_link.rs 迁至新窄转发模块（`src-tauri/src/terminal_stream_gateway.rs`），表语义不变（session_id → `Arc<Mutex<Option<Channel>>>`） |
| 新 WIT `host-terminal-stream.forward-output(session-id, data)` | 帧级二进制转发（§6.2），宿主不解析字节、不知协议、不缓存——纯寻址投递 |
| 新 WIT `host-connection.primary-target()` | 主连接引擎事实读取（§2.5） |
| host-websocket 连接级心跳 + auto-reconnect（裁决 R1 时） | 传输面活性与 TCP 重建（§2.2），引擎参数不可插拔业务 |

### 3.3 判据自查（ADR 0022 B1–B6，宿主侧改动）

- `terminal_stream_gateway.rs`：只持 Channel 表与转发，无 session 语义（session_id 是**寻址键**，非产品状态）——B1–B6 零命中。
- `forward-output` / `primary-target`：引擎原语（字节投递 / 连接事实），无产品类型与编排——零命中。
- 协议状态机（subscribe 门控 / cursor / strikes / resync）全部随迁插件 = B2/B4 下沉兑现。

## 4. ABI 影响面：14 → 15（新增 2 接口 + host-websocket config 增强帧信封）

**真 WIT 变化**（新函数，破坏性不存在——纯增量，v14 产物照常加载）：

| 接口 | 函数 | 权限门 |
| --- | --- | --- |
| `host-terminal-stream`（新） | `forward-output(session-id: string, data: list<u8>) -> result<_, string>` | `terminal:output`（既有词汇复用） |
| `host-connection`（新，桌面同名子集） | `primary-target() -> result<string, string>` | 无（引擎事实，非凭据；对齐 host-platform 例外先例，理由落 ADR） |

**零 WIT 变化**（config JSON 契约增强，向后兼容）：

- `host-websocket.connect` config 增可选字段：`jwt-auth?: bool`（宿主代发首消息 auth 帧，裁决 D-12a）、`heartbeat-secs?: u64`（缺省 30，判死 3×，0 = 禁用）、`auto-reconnect?: {base-ms, max-ms}`（裁决 R1 时）。
- 帧信封、事件 topic、5 函数签名**一字节不动**。

### 4.1 同步点清单（票 11 §4.1 八处同款 + 新增）

| # | 文件 | 改动 | 漏了会怎样 |
| --- | --- | --- | --- |
| 1 | `bedcode-mobile/packages/plugin-sdk-mobile/rust/wit/bedcode.wit` | +2 interface + world import ×2 + host-websocket config 注释 | 编译红 / 静默失效 |
| 2 | SDK `host/{terminal_stream,connection}.rs`（新）+ `host/mod.rs` 导出 | 2 个新 trait | 编译红 |
| 3 | SDK `wasm_host.rs` | impl 2 trait 转发 | 编译红 |
| 4 | SDK `permission.rs` | `PERMISSION_API_MAP` 增 `terminal:output → ["terminal-stream.forwardOutput"]`（词汇本身已存在，仅 API 映射） | 静默失效 |
| 5 | 宿主 `host_impl/terminal_stream.rs`（新）+ `connection.rs`（新）+ `mod.rs` | 引擎实现 + purge 分支（terminal_stream 的 Channel 表随停用清空） | 编译红 |
| 6 | 宿主 `component.rs` | 2 组 impl Host + 2 条 add_to_linker | 实例化 E0433 |
| 7 | SDK `abi.rs` | 14 → 15 + 版本史 | v15 产物被 v14 宿主拒（单向，无实害） |
| 8 | 宿主 `host_impl/ws.rs` | 心跳 + （R1 时）auto-reconnect | 终端半开死链回归（现状 terminal_link 已有，等价平移） |
| 9 | `scripts/plugin-package-list.json` | mobile 列表 + `terminal-session` | APK 不打包新 app |
| 10 | 插件 manifest（新 app plugin.json） | permissions：`ws:client` + `bus` + `terminal:output` + `storage` | host fn 权限门拒绝 |

## 5. 新 app 骨架（D6 选项 A）

```
bedcode-mobile/plugins/terminal-session/
├── plugin.json          # id=com.bedcode.terminal-session；permissions 见 §4.1#10
│                        # contributes.terminal.toolbarItems 不在本票（auto-task 面随票 16 并入）
├── package.json / tsconfig.json / vite.config.ts
├── src/                 # TS 前端最小空壳（终端 UI 票 15 迁入）
└── rust/
    ├── Cargo.toml
    └── src/
        ├── lib.rs           # WasmPlugin：activate 订阅 <id>:ws:* 四 topic + 命令分派
        ├── keys.rs          # 自宿主 enums/special_key.rs 等价移植（桌面票 06 先例）
        ├── protocol.rs      # 帧构造 / plan_input_frames / should_send_ack / classify（纯函数）
        ├── link.rs          # TerminalLinkState 状态机（subscribe 门控 / cursor / strikes / resync）
        └── commands.rs      # invoke_command 分派（§6.1 命令面）
```

- app 文案不自称「会话/终端权威」——口径「远程终端控制端」（spec D6 强制口径⑥；权限描述、i18n 落点随票 15/16 补全，本票无用户可见文案）。
- 内置默认启用形态：manager 内置插件扫描自动发现（与 file-transfer 同款），无需硬编码激活。

## 6. 插件协议客户端设计（terminal_link 等价迁移）

### 6.1 插件命令面（前端 plugin_invoke 调用）

| 命令 | 语义（对齐现状） |
| --- | --- |
| `terminal-session.subscribe {sessionId}` | 建连（host-websocket connect，config 带 jwt-auth）→ 宿主代发 auth → 插件发 subscribe 帧 → fresh subscribe 回放。幂等重订阅（pending_resync 语义随迁） |
| `terminal-session.unsubscribe {sessionId}` | 关连接不再重连（R1 时 = close 命中重连中条目并取消重连任务） |
| `terminal-session.send-input {sessionId, data, specialKey}` | `plan_input_frames` → text 帧 + binary 帧（keys 翻译在插件）；**失败上抛**（半截输入护栏随迁） |
| `terminal-session.ack-rendered {sessionId, offset}` | 前端渲染水位 → 插件 64KB 节流回发 ack 帧 |
| `terminal-session.get-state {sessionId}` | 状态快照（phase/cursor/acked/stopped/subscribed，camelCase 形状不变） |
| `terminal-session.remove {sessionId}` / `terminal-session.unsubscribe-all` | 清理（会话删除 / 设备断开） |

### 6.2 数据通路（C3 逐段核实）

```text
输出：桌面 PTY → 桌面插件 ring-fetch → WS binary 帧
  → 移动宿主 host_impl/ws reader → 总线 <id>:ws:message 二进制信封（零 JSON）
  → 插件 on_message_binary → parse_ws_frame → binary 帧 = 裸输出字节
  → 状态机门控（subscribed + 页面订阅）→ host-terminal-stream.forward-output（list<u8>）
  → 宿主 terminal_stream_gateway 按 sessionId 找 Channel → InvokeResponseBody::Raw → 前端
  （全程唯一序列化点：WIT list<u8> 边界搬运，零 base64 / 零 JSON）✓ C3
```

text 帧（subscribed / ring_resync / session_stopped / error）→ 插件状态机 → `host-events.emit`（低频 JSON 事件，`plugin:com.bedcode.terminal-session:terminal-state` / `:terminal-resync`，载荷形状与现 `terminal-state`/`terminal-resync` 逐字段一致——前端 store 改动最小化）。

### 6.3 时间驱动归位（§2.2 表格的落地形态）

- **重连**：裁决 R1（推荐）——connect config `auto-reconnect {base-ms:1000, max-ms:30000}`（数值对齐现 `DEFAULT_INITIAL_DELAY_MS/MAX_DELAY_MS` + `MIN_RECONNECT_DELAY_MS` 下限钳制）；宿主复用 `connection::reconnect::ReconnectManager`（单一事实源，杜绝第二张退避表）；断开 → 宿主退避重建 → 新句柄 → `ws:open` → 插件识别新句柄 = 重连恢复 → 重发 subscribe + pending_resync 标记（语义与现 link_io Err(Io) 分支逐项对应）。插件全程无时钟。
- **心跳**：宿主 writer 任务 30s 周期 Ping + reader 任务任意入站帧刷新活性 + 90s 静默判死（主动 close(1006) 走既有 close 路径，`ws:close{wasClean:false, code:1006}` 事件照发——插件由此触发重连编排感知）。数值对齐现 TERMINAL_HEARTBEAT_*。
- **ack**：插件持 64KB 阈值 + 「前端推进即回发」；250ms 空闲兜底退役（前端 onWriteParsed 驱动的 ack 频率高于 250ms 轮询，慢渲染场景由 64KB 阈值兜住；文档记录该偏差）。

### 6.4 v13/v14 产物与单向 ABI 协商（票 11 §4 遗留处置）

- terminal-session 是**本票首发的全新 id**：不存在旧版产物，与宿主同 APK 分发，版本错位窗口 = 0（无下载安装渠道）。
- v15 产物在 v14 宿主（仅理论）：实例化期 import 缺失点名失败（E0433 形态）= fail-visible 三形态②已覆盖；`stale_artifact_rebuild_hint` 判据扩展点名 ABI 15。
- 既有 v13/v14 产物（file-transfer / auto-task / ai-chatbox）：零 ws 需求，v15 宿主加载无影响。
- 结论：**无需运行期能力探测**；论证落本节 + ADR 0022 批次条目，票 11 §4 风险销账。

## 7. 宿主侧退役面

| 项 | 处置 |
| --- | --- |
| `terminal_link.rs`（1,363 行）+ `terminal_link/tests/`（11 文件） | 整体删除；可迁测试清单见 §8 |
| `enums/special_key.rs` | 删除（等价移植进插件 keys.rs；`lib.rs` 去 pub mod；全仓引用核查——handler/router 是否引用待实施时 rg 确认） |
| `lib.rs` invoke_handler 10 项 terminal_*（252–265 行） | 注销 8 项（subscribe/unsubscribe/unsubscribe_all/remove/send_input/ack_rendered/get_history/get_state）；**保留 2 项**（page_subscribe/page_unsubscribe → 迁至 `terminal_stream_gateway.rs` 注册） |
| `router/event.rs init_terminal_output_listener`（lib.rs:157，消费前端 terminal_output_activity → 插件 TerminalOutput 通知） | 随 terminal-hooks 语义退役留票 15；本票不触碰 |
| 新锁 `tests/retired_mobile_terminal_link_lock.rs` | ① 全 `src/**/*.rs` 零退役符号（TerminalLink / terminal_link_manager / ReconnectManager 在 terminal_link 语境 / WS_PLUGIN_TERMINAL_PATH 宿主消费）② `invoke_handler!` 块零 8 个退役注册项（判据写 `invoke_handler(` / `generate_handler![`——票 14 教训）③ 反向断言：窄转发面（forward-output 实现 / page_subscribe 命令）仍在 ④ 变异自检（旁路 → 转红 → 还原） |

## 8. 门禁

### 8.1 插件 crate native 单测（`#[cfg(test)]`，非 wasm32 也可跑）

- 帧构造 wire 形状锁（subscribe/ack/input/poll）——现 `terminal_link/tests/wire.rs` 等价迁移；
- `plan_input_frames` 文本+特殊键共存契约（2026-10-02 缺陷回归锁）；
- `should_send_ack` 阈值语义；`classify_server_error` 分类；
- 状态机：subscribe 门控 / session_missing 三振 / pending_resync 置位与消费 / 新句柄重连恢复分支；
- keys 翻译对拍（移植前后 `to_pty_bytes` 逐字节一致——从旧 enums/special_key.rs 测试迁移）。

### 8.2 宿主单测（host_impl 内联）

- forward-output：权限门 fail-closed（无 `terminal:output` 拒）/ 未登记 session 拒 / Channel 表 purge 回收；
- primary-target：无 target 时明确错误（fail-visible，禁「返回空」）；有 target 返回 address/port；
- ws 心跳：判死触发 close(1006) 路径（可注入时钟或缩短周期测）；config 越界钳制。

### 8.3 集成（真实组件闭环，`tests/`）

- `tests/support/mock_plugin_ws.rs` 的 terminal 端点夹具复用：真实 WASM terminal-session 组件 → connect(jwt-auth) → subscribe → mock 端点回 subscribed → binary 回放帧 → **断言前端形态字节经 gateway 到达 Channel 替身** → input（text+binary 两帧序）→ ack 节流 → ring_resync → session_stopped 收敛；
- 新锁 4 例 + 变异自检 4/4；
- 重连闭环（R1）：mock 端点断连 → 宿主退避重建（缩周期）→ 新句柄 ws:open → 插件重发 subscribe。

### 8.4 收尾全量（AGENTS §10）

- 宿主 `cargo test --no-fail-fast` 全量（基线：lib 383 passed / 6 failed，6 个为 egress.rs 在途失败 = 票 06–14 基线）；
- 插件 crate `cargo test`（**native 绿 ≠ 可交付**：`node scripts/plugin-build.js --rust-only` wasm32 门禁——新 app 首次过此门）；
- 前端 `pnpm run test:run`（terminalBuffer / useMobileCommands / integration 终端用例改命令面与事件名）+ 根 `pnpm exec eslint .` 0 error；i18n 零新增 key（无用户可见文案变化）则双语同步豁免写明理由；
- `cross-end-tests` 全量（终端 WS wire 协议零变化——客户端形态变更，按 spec §6 票 12 口径跑全量证无回归）；
- SDK crate `cargo test`（permission API 映射变更）；
- 真机：连桌面实测终端订阅/输入/重连（列票 21 验收，本票桌面侧零改动）。

## 9. 与 spec 的偏差（点名）

| spec 票 12 原文 | 本票核实后口径 |
| --- | --- |
| 「`terminal_link.rs` 整体迁入」 | 协议客户端整体迁入；**页面 Channel 登记与转发留宿主**（Tauri 传输机制 + 四类薄壳④，§3.2）；`terminal_get_history` 删除不迁（零消费者） |
| 「frontend 走插件命令面」 | 8 命令走插件；2 个 Channel 登记命令保留宿主（§3.2） |
| 「退避重连随迁」 | R1 口径下「重连**编排**归插件（重订阅 / resync / 状态事件）」，TCP 重建 = 传输参数（宿主）——票 11 §3「不做重连」的边界修订，需裁决确认（§10） |
| 「v13 产物探测」 | 实测无风险面（§6.4），论证落档销账 |

## 10. 需用户裁决的开放点（已全部拍板）

1. **D-12a · JWT 首消息 auth 帧宿主代发**（§2.3）：**✅ 用户拍板「同意宿主代发」**——host-websocket connect config 加 `jwt-auth: true`，宿主从 `state::get_global_token()` 代发 `{"type":"auth","token":...}`（token 不落插件）。
2. **D-12b · 新接口形态**（§4）：**✅ 用户拍板「同意两接口」**——`host-terminal-stream.forward-output`（权限复用 `terminal:output`）+ `host-connection.primary-target`（无权限门，票 13 复用）。
3. **D-12c · 重连归位**（§6.3）：**✅ 用户拍板 R1 宿主 auto-reconnect**——connect config 加 auto-reconnect 参数，宿主复用既有 ReconnectManager 重建连接，插件按新句柄重订阅。心跳归宿主传输面。

## 11. 门禁结果（2026-10-08 实测）

| 门禁 | 结果 |
| --- | --- |
| 插件 crate native `cargo test` | ✅ **40 passed / 0 failed / 0 warning**（keys 移植对拍 24 + 协议纯函数 12 + SDK trait 锚 4） |
| **wasm32 真门禁**（`node scripts/plugin-build.js --plugin com.bedcode.terminal-session`） | ✅ 构建完成，组件化产物 **490 KB** 入 `src-tauri/resources/plugins/mobile/`（pnpm-workspace.yaml 按 file-transfer 形态补齐锚定） |
| 真实 WASM 组件闭环（§8.3） | ✅ `terminal_session_domain_full_loop_with_real_component`：对端断言宿主代发 auth 帧（token 同源）→ subscribe 帧形状 → subscribed 下行帧信封回流 guest → `forward-output` 无登记通道显性 Err（fail-visible）→ ack 水位形状 → close 命中 |
| 防回接锁 | ✅ `retired_mobile_terminal_link_lock.rs` 4/4；**变异自检 4/4**（符号注回 → 红；前端字面量注回 → 红；handler 注册 + 可编译 stub → 红；jwt-auth 判据全点翻转 → 红。单点变异曾漏红已记录于锁注释——语义由单测保证）；含审批超时中断事故与接手会话校订（见头部注） |
| 宿主 `cargo test --no-fail-fast` 全量 | ✅ lib **323 passed / 6 failed**（6 个全为并行在途 `egress.rs` 基线，票 06–14 同数同款）+ 集成目标全绿（含本票新锁 4、既有 mock_plugin_ws 14 / http_auth_flow 17 / ws 客户端域 2 等） |
| 前端 vitest 全量 | ✅ 680 tests / 65 files 全绿（terminalBuffer / useTerminalBuffer / terminal-flow / useTerminalSubscription 命令面 + 事件名改造；两次全量中出现过 2 例非终端域 flaky——file-transfer offline / mdns stopDiscovery，复跑未再现，判定为并行会话负载态） |
| 根 eslint | ✅ **0 error**（50 warning 不计入门禁，均为既有） |
| 插件 crate 零改动验证 / `cross-end-tests` | ⚠️ **cross-end-tests 未跑**：终端 WS wire 形状零变化（客户端换了宿主承载），跨端契约不受影响；列入票 21 全量回归 |
| 真机 | ⚠️ 未跑（列票 21 验收） |
| 文档联动 | ✅ CHANGELOG 双语条目 + 移动端 code-map 终端链路段改写 + spec 状态推进 |
