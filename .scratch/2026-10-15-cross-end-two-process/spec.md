# 跨端两进程夹具（cross-end L2）——阶段一：打通 terminal_ws_flow

> 日期：2026-10-15。前置：`.scratch/2026-09-30-cross-end-integration-tests/spec.md` §3.B
> （双进程轻量 E2E，A 落地后评估——单进程 rig 已证明装不下两端宿主域实现，转评估结论 = 实施）。
> 用户裁决：2026-10-15「A 继续」。

## 1. 问题与目标

### 1.1 单进程 rig 的硬边界（已实测钉死，见 terminal_ws_flow.rs:55 的 ignore 理由）

`inventory` 自报是**进程级**注册；两端宿主对端差异域（`host-http` / `host-websocket` /
`host-peer` / `host-mdns`）各有一份实现。`ModuleRegistry::install_all` 两遍装配的
`fallback` 让位语义（`packages/bedcode-host-kit/src/registry.rs`）让**移动侧实现**在
同进程双链下让位给桌面实现 ⇒ 移动真插件实际跑在桌面 host-http 上 ⇒ 无头桌面无
AppHandle → 网络授权一律拒 → 会话 HTTP 面不可用 ⇒ 5/10 场景 `#[ignore]`（含终端
全流程）。这是**结构性的**（单进程里只能有一套端差异域实现），不是装配 bug。

### 1.2 目标（阶段一）

两进程夹具：**桌面 headless 服务器进程**（只链桌面 lib + server-* crates）与
**移动真插件进程**（= 测试二进制，只链移动 lib + wasm-core test-support）经真实
TCP 互连，把 `terminal_ws_flow`（C-001..C-009）从 `#[ignore]` 摘成**激活场景**。

两端各在一个只含本端模块的进程里 ⇒ `fallback` 无竞争、移动宿主域实现照常安装 ⇒
移动插件的 HTTP / WS / 窄转发走的是**移动生产实现**——这正是单进程 rig 缺的那半。

### 1.3 非目标（后续阶段）

- 其余 4 个 ignore 场景（fail_closed / jwt_rotate / session_http / lifecycle）的移植
  ——逐个独立推进，本阶段只动 `terminal_ws_flow`。
- mDNS supervisor 启动路径（mdns_health 场景用，阶段二）。
- 链路加密非环回场景（http_proxy 用，阶段二）。
- peer-net 文件传输（README 诚实清单首项；需真实 discovery/peer 引擎，另立项）。
- 原 `tests/terminal_ws_flow.rs` 暂不删除（新场景证明自己后再退役，防回退窗口）。

## 2. 架构

```text
┌─ 测试进程（cross-end-tests 的 two_proc_* 测试二进制）───────────────┐
│ 依赖图 = 移动 lib + wasm-core(test-support)，**不含桌面 lib**         │
│ ・移动真插件产物（com.bedcode.terminal-session 移动侧 wasm）           │
│ ・移动宿主域实现：mobile-host-core / mobile-terminal-stream /        │
│   mobile-auth / mobile-connection / mobile-fs / mobile-notify …     │
│   （无桌面实现竞争 ⇒ 照常安装，fallback 无冲突）                      │
│ ・驱动：插件命令面 + 页面 Channel 观测（复用 common/mobile_*）         │
│ ・DesktopHarnessDriver：spawn 桌面进程 + stdin/stdout JSON-RPC      │
└───────────┬──────────────────────────────┬─────────────────────────┘
       真实 HTTP/WS (127.0.0.1:port)   JSON-RPC over stdio
┌───────────┴──────────────────────────────┴─────────────────────────┐
│ 桌面进程（cross-end-tests/desktop-harness bin）                     │
│ 依赖图 = 桌面 lib + server-* crates，**不含移动 lib**                 │
│ ・真 AppContext + 真认证中心插件（terminal-session 桌面侧）            │
│ ・真 Actix 服务器 + 真 PTY（装配顺序 = GUI bootstrap 同款）            │
│ ・stdout 首行 `READY <port>`；其后 stdin/stdout JSON-RPC 控制面       │
└──────────────────────────────────────────────────────────────────────┘
```

### 2.1 为什么 desktop-harness 是独立 leaf crate 而不是同包 bin

同包 bin 的依赖图 = 包的依赖图（两端 lib 都在）。rustc 惰性链接理论上让「只用桌面
lib 的 bin」不链接移动 lib——但这是**脆弱事实**：一旦有人给 bin 加一行移动 `use`，
冲突静默回归且极难定位。独立 leaf crate 的依赖图由 cargo **编译期保证**只含桌面侧，
无歧义。移动侧同理：two_proc 测试二进制只引用移动侧代码。

### 2.2 装配守卫（双保险，fail-visible）

| 位置 | 守卫 |
|---|---|
| harness bin 启动 | `ModuleRegistry::collected()` 断言：含桌面独有模块（fs-desktop / process / task / platform-desktop / api_call / crypto / ws / http），**不含**移动独有模块（mobile-*）。违反 = 启动 panic（进程不起来，READY 永不出现）。 |
| two_proc 测试自检 | 同上反向断言（含 mobile-*、不含桌面独有）。惰性链接语义被破坏时测试立即红。 |

守卫清单以实测 `collected()` 输出为准校准（写代码后跑一次两进程自检对表）。

### 2.3 控制面协议（JSON-RPC 2.0 over stdin/stdout）

- 请求 `{"jsonrpc":"2.0","id":n,"method":m,"params":{...}}`，一行一个 JSON。
- 响应 `{"jsonrpc":"2.0","id":n,"result":...}` / `{"error":{...}}`。
- 日志一律走 stderr（bin 内 tracing subscriber 写 stderr；测试进程把子进程 stderr
  攒进缓冲，失败时随断言现场 dump）。
- `READY <port>` 是 stdout 第一行（就绪 + 端口报告；端口由桌面进程自选，测试
  不猜端口 → 无 pick-free-port 竞态）。

方法集（阶段一最小集；后续场景按需扩展）：

| method | params | result |
|---|---|---|
| `activate` | `{}` | null（幂等；默认启动已激活，为 fail_closed 场景留口） |
| `seed-shell-config` | `{name}` | `{id}` |
| `plugin-command` | `{pluginId, command, args}` | 命令结果 JSON |
| `plugin-api-call` | `{api, params}` | 互调结果 JSON |
| `ws-endpoint-clients` | `{pluginId, path}` | 连接摘要数组（含 `authenticated` / `clientId`） |
| `disconnect-ws-endpoint` | `{pluginId, path, code, reason}` | `{count}` |
| `enable-link-crypto-http` | `{}` | null |
| `set-link-crypto-enabled` | `{enabled}` | null |
| `link-crypto-counters` | `{}` | `[encrypted, decryptFailures, keyMiss]` |
| `shutdown` | `{}` | null（随后优雅停服务器 + 清理临时目录 + 退出） |

## 3. 文件落点（最终形态；§5.1 实测偏离 1 后调整）

```text
cross-end-tests/                        # workspace 根（desktop-harness 为成员，共享 target 桶）
├── Cargo.toml                          # [workspace] members + optional desktop-lib + [features] + [[test]]
├── desktop-harness/                    # 桌面进程（独立包；依赖图只含桌面侧）
│   ├── Cargo.toml                       # 依赖：桌面 lib + server-* + host-kit + tokio…
│   ├── src/lib.rs                       # 装配面（desktop_ctx.rs 同构搬运）+ rpc_dispatch + 守卫
│   └── src/main.rs                      # READY + JSON-RPC 循环
└── tests/                              # 两种构建模式的场景同住一处
    ├── common/                         # desktop_ctx / mobile_ctx / mobile_plugin（单一真源）
    ├── two_proc_common/                # 两进程：desktop 驱动；mobile_* 以 #[path] 复用 common/
    ├── two_proc_selfcheck.rs           # 装配守卫 + 驱动自检（two-proc 模式）
    ├── two_proc_terminal_ws.rs         # terminal_ws_flow 两进程版（C-001..C-009，two-proc 模式）
    └── *其他 10 个单进程场景*            # desktop 模式（required-features = ["desktop"]）
```

**构建模式（`[features]`）**：`default = ["desktop"]`（单进程 rig；desktop-lib = optional 依赖
开启 ⇒ common-host 随之）vs `two-proc = []`（**desktop-lib 整个出图** ⇒ common-host 无人开启 ⇒
移动 whitelist 干净）。单进程场景 `[[test]] required-features = ["desktop"]`、两进程场景
`required-features = ["two-proc"]`——cargo 按构建模式决定编译集，两种模式互不编译对方的测试。
两模式互斥无法被 cargo 强制（混用会复现污染，被 inventory 守卫显性拦下）。

`desktop-harness/src/lib.rs` 与 `cross-end-tests/tests/common/desktop_ctx.rs` 是**同源同构**
（同一套 GUI bootstrap 装配顺序）。两处头注互相指认；改装配顺序必须同步两处 + GUI
`src-tauri/src/lib.rs` setup（server-lib-split 票 07 的教训）。夹具无副本（two_proc 直接
复用 `common/mobile_*`，`#[path]` 编入同一源文件）。

## 4. 验收（阶段一 + 迁移；已完成项打勾）

- [x] `cd cross-end-tests && cargo build -p bedcode-crossend-desktop-harness` 出 bin（共享桶）
- [x] `two_proc_selfcheck` 绿：装配守卫（模块清单对表）、spawn → READY → shutdown 全流程
- [x] `two_proc_terminal_ws` 绿：C-001..C-009 逐条过（判据函数与单进程旧场景一致；连跑稳定）
- [x] 两进程测试迁回 cross-end-tests（optional dep + feature 门控，夹具单一真源；移动包副本删除）
- [ ] 旧 4 个激活场景（pairing_auth / event_channel / http_proxy / mdns_health）不回归
- [ ] CI：test.yml cross-end job 增 `desktop-harness` 构建步骤 + two_proc 显式运行步骤
- [ ] README（cross-end-tests）补两进程段：形态 / 前置 / 守卫 / 与单进程 rig 的关系

## 5. 风险与踩坑预案

1. **惰性链接不成立**（two_proc 测试进程里收集到桌面模块）：自检会红——备选方案是把
   two_proc 测试拆成独立 leaf crate（依赖图只含移动侧，编译期保证）。先跑自检定夺。
2. **harness 进程 panic 后 READY 不出现**：Driver 的 READY 等待带 30s 超时 + stderr dump，
   失败现场含 panic 全文。
3. **端口竞态**：端口由桌面进程自选并 REPORT，测试零猜测。
4. **CI 组播 / 非环回**：本阶段场景全走 127.0.0.1 + 轻量 `start_http_server`（不经
   supervisor），不引入组播依赖。
5. **json-rpc 顺序性**：Driver 的 rpc 用互斥锁串行化（读 stdout 与写 stdin 不交错）；
   桌面进程顺序处理 stdin 行，无并发。

## 5.1 实测结论（2026-10-15 实施后记录，含两处方案偏离）

**偏离 1：两进程测试必须住在移动包，不能在 cross-end-tests 包内。**

首个实现把 two_proc 测试放在 cross-end-tests 包（依赖桌面 lib）——`inventory` 守卫
（`ModuleRegistry::collected()`）通过了，但**移动插件世界创建时 whitelist 直接红**：
`unlisted=["discovery", "http", "peer-net", "websocket"]`。根因 = **cargo feature
统一**：cross-end-tests 依赖桌面 lib，桌面 lib 对 server crates 开 `common-host`，feature
统一后这些绑定模块在本包所有测试二进制里都被编译并 submit——它们实现的正是
`host-http/host-websocket/host-mdns/host-peer`（与 `mobile-host-core` 同名 interface，
且非 fallback 先装）。cargo 没有 per-target feature 隔离，同一包内无解。

**结论（迁移后终态，2026-10-15 用户裁决「做吧」）**：用 **optional 依赖 + feature 门控**解除，
two_proc 测试迁回 cross-end-tests：`bedcode-desktop-lib` 改成 `optional = true`，
`default = ["desktop"]` / `two-proc = []` 两个 feature——`two-proc` 模式下桌面 lib **整个出图**
（不是「链接了但 cfg 掉」：cargo 对未启用的 optional 依赖不 resolve，feature 统一只作用在
解析进图的 crate 上），common-host 无人开启 ⇒ 绑定模块不提交 ⇒ whitelist 干净。测试目标按
`[[test]] required-features` 分集：单进程场景 `["desktop"]`、两进程场景 `["two-proc"]`，两种
模式互不编译对方的测试。中间态（搬到移动包）撤销：夹具不再有两份副本，`\#[path]` 直接复用
`common/mobile_*`。`desktop-harness` 保持独立包/workspace 成员（桌面侧，无污染问题，共享
target 桶省磁盘——独立桶实测把开发盘推到 92% 满）；两进程模式 `-p cross-end-tests` 单包构建，
避免与桌面侧成员一起构建时 feature 统一复现污染。

⚠ 两 feature 互斥无法被 cargo 强制：`--features "desktop two-proc"` 会复现污染，但
`two_proc_selfcheck` 的 inventory 装配守卫运行时直接红（显性失败不静默）。

**偏离 2：会话生命周期（start/stop）改走桌面命令面，移动插件 HTTP 面无头不可测。**

移动插件 `start-session/stop-session` 经 host-http → 移动 egress 闸门。该闸门对无头
上下文**有意 fail-closed**（`plugin/host_domains/http.rs` 的 `headless_context_denied_by_egress`
用例钉着：弹窗事务无 AppHandle 载体，收紧为判定失败）。headless 造不出 `tauri::AppHandle`
（Wry；tauri `test` feature 只出 MockRuntime，`AppHandle<R>` 泛型不匹配）。这是结构性
边界。因此场景 C-001/C-007 改走**桌面命令面** `session.ws.control`（与生产 WS 会话控制
通道共用 `ws_control` 域实现），被测跨端面（移动插件 WS 链路 + 窄转发）全真。移动插件
host-http 的 wire 契约由激活场景 `event_channel_flow.rs`（reqwest）+ 移动单端
`session_http_flow.rs`（就地端口 impl）承接。

> 待用户裁决（安全相关，不得单方面改）：移动 egress 的「L1 桌面端目标」层在无头 CI
> 下是否放行（decide() 对主目标本就免弹窗 Allow；卡点是 adapter 的 None-app_handle 短路）。

**实测发现 3：`seq_lines` 行首判据被 ANSI 序列粘住。**

bash 执行命令前复位 bracketed paste（`\x1b[?2004l`）后 `\r` 回列首再输出首行 →
`\x1b[?2004l\rSEQ_1` 在 `str::lines()`（只按 `\n`）下粘成一行，`starts_with` 落空 →
首行静默丢失（C-005 红、C-009 同病）。修法：按终端语义 `\r`/`\n` 都分行（两进程版与
单进程版 `terminal_ws_flow.rs` 同实现）。

**验收现状（2026-10-15）**：`two_proc_terminal_ws`（C-001..C-009）+ `two_proc_selfcheck`
（装配守卫 + 驱动自检）全绿 ×2（3.88s / 0.27s）。旧激活场景待回归（§6）。
