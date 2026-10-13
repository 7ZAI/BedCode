# 契约归位 · 功能级盘点与裁决（票 01 产出）

> **来源**：`.scratch/2026-10-13-domain-owned-contracts/spec.md` §3–§5；票 `.scratch/2026-10-13-domain-owned-contracts/issues/01-feature-level-audit.md`。
> **基准**：2026-10-13 工作区（dev @ `4c5958a18` + 在途修改，未碰并行线文件）。**本票纯盘点，零代码改动。**
> **状态**：审计完成，**最小问题集（§D）待用户逐项确认**；确认记录将追加到 `issues/01-feature-level-audit.md` 的 Comments，之后本文件即 T-02/T-03/T-04 的冻结输入。
> **判据**：R1（core 三条件）/ R2（功能级交集）/ R3（端差异归宿主）/ R4（契约房=共享机制所在 crate）/ R5（变更形态）——引用自 spec §2 与 ADR 0052 D1–D5。

---

## 0. 基准事实（可回查）

| 事实 | 值 | 证据 |
| --- | --- | --- |
| `core.wit` interface 数 | **16** = 12 import + 4 export（`command` / `lifecycle` / `manifest` / `events-binary`） | `grep -c '^interface ' packages/bedcode-wasm-core/wit/core.wit` = 16 |
| `world core` 成员 | import 12（L347–358）+ export 3（L360–362；`events-binary` 不在 world 内，可选导出） | `core.wit` L344–363 |
| 内核实际实现域 | **8**：storage / plugin-database / log / config / events / fs / bus / platform | `packages/bedcode-wasm-core/src/manager/runtime/core_binding.rs` L68/94/132/156/174/188/219/253 共 8 个 `impl …::Host`；`add_to_linker` L338–357 注册 8 个 |
| `world core` 中内核**未**实现的 interface | `host-peer` / `host-mdns` / `host-websocket`（客户端 5）/ `host-http`（出站 1） | `core_binding.rs` L59–64 注释 + 逐个 `impl` 搜索（两文件无这 4 个 `impl`） |
| 桌面 world plugin 组成 | `core + cap-pty + cap-http + cap-ws + cap-desktop` | `bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit` |
| 移动 world plugin 组成 | `core + cap-mobile` | `bedcode-mobile/packages/plugin-sdk-mobile/rust/wit/bedcode.wit` |
| 桌面 compose caps | `pty`→`bedcode-pty-engine/wit/pty.wit`；`http`/`ws`→端 `wit-src/cap-{http,ws}.wit`；`desktop`→端 `wit-src/cap-desktop.wit` | `bedcode-desktop/packages/plugin-sdk-desktop/compose.json` |
| 移动 compose caps | 仅 `mobile`→端 `wit-src/cap-mobile.wit` | `bedcode-mobile/packages/plugin-sdk-mobile/compose.json` |
| 端组合实际消费的**域文件** | **只有 pty**（桌面 `caps.pty` 指向 pty-engine 分片）；mdns / peer / http / ws 的域文件目前只是 provider 侧 bindgen 副本，端生成物不消费 | 上面两条 + 桌面生成物目录仅 `cap-pty.wit` 来自域 crate |
| ABI | 桌面 38（form=1）/ 移动 21（无 form） | 双端 `compose.json` `abi.version`；ADR 0051 后整数 ABI 已退役，此处仅存 compose 元数据 |
| 包版本 | `bedcode:plugin@0.1.0` | `contracts/bedcode-plugin-package.rs` + 全部 WIT 源文件 package 行 |
| 「17 个」文档口径 | **陈旧**（ADR 0051 删 `interface abi` 后应为 16；收窄后 12） | §E.2 清单 |

**改造前双端 union world 基线（T-02 并集等价证据的对照面）**

- 桌面 `world plugin` import（26）：`host-storage` `host-events` `host-bus` `host-peer` `host-mdns` `host-platform` `host-websocket` `host-fs` `host-http` `host-log` `host-config` `host-plugin-database`（core 12）+ `host-pty`（cap-pty）+ `host-http-endpoint`（cap-http src）+ `host-websocket-server`（cap-ws src）+ `host-events-desktop` `host-fs-desktop` `host-platform-desktop` `host-api-call` `host-timer` `host-process` `host-app` `host-connection-registry` `host-auth-center` `host-task` `host-crypto`（cap-desktop 11）。
- 桌面 export：world 内必选 `command` `lifecycle` `manifest` `events` `abi-form`；独立 world 可选 `events-binary`（plugin-binary）/ `events-ws`（plugin-ws）/ `events-task`（plugin-task）/ `auth-policy`（plugin-auth-policy）/ `host-storage`（plugin-system，系统组件导出面）。
- 移动 `world plugin` import（17）：core 12 + `host-notify` `host-fs-mobile` `host-terminal-stream` `host-connection-target` `host-auth-client`。export：必选 `command` `lifecycle` `manifest` `events`；可选 `events-binary`。

---

## A. 双端 world 全量清单与实现落点

### A.1 `core.wit`（内核绑定面）

| interface | 函数（逐条） | 功能 | 内核实现 | 端实现落点（当前） |
| --- | --- | --- | --- | --- |
| `host-storage` | get / set / delete | 插件 KV | ✅ `core_binding.rs:68` | 内核一份，双端共用 |
| `host-plugin-database` | execute / query / execute-params / query-params / execute-batch | 插件私有 SQLite | ✅ `core_binding.rs:94` | 同上 |
| `host-log` | info / debug / warn / error / mark-plugin-error | 日志 + 状态上报 | ✅ `core_binding.rs:132` | 同上 |
| `host-config` | get | 配置读取 | ✅ `core_binding.rs:156` | 同上 |
| `host-events` | emit | 事件发布 | ✅ `core_binding.rs:174` | 同上（`notify` 已拆桌面扩展，见 A.2） |
| `host-fs` | read / write / copy / delete / exists / request-auth | 文件系统（交集 6） | ✅ `core_binding.rs:188`（`host_api/fs.rs`） | 同上 |
| `host-bus` | publish / publish-binary / subscribe / subscribe-binary / unsubscribe | 消息总线 | ✅ `core_binding.rs:219` | 同上 |
| `host-platform` | pick-files / pick-folder / platform | 平台对话框 + 平台标识 | ✅ `core_binding.rs:253`（`host_api/platform.rs`） | 同上 |
| `host-peer` | 19 条（dial-peer / close / respond-consent / list-trusted / revoke-trusted / send-files / respond-transfer / set-receive-policy / pause-transfer / resume-transfer / set-shared-roots / list-shared-roots / browse-directory / pull-files / set-download-dir / start-node / stop-node / active-transfers / collect-outgoing） | 对等网络 | ❌ | **按端**：桌面 `plugin/peer.rs` + `bedcode-server-peer-net::plugin_binding`（MODULE_INTERFACES L580）；移动 `plugin/host_domains/peer.rs` + 同 crate 端口 |
| `host-mdns` | browse / stop-browse / advertise / stop-advertise / is-advertising | mDNS 浏览/广播 | ❌ | **按端**：桌面 `plugin/mdns.rs` + `bedcode-discovery-engine`（L66）；移动 `plugin/host_domains/mdns.rs` + 同 crate |
| `host-websocket` | connect / send-text / send-binary / close / is-connected（客户端 5） | WS 出站 | ❌ | **按端**：桌面 `plugin/ws.rs` + `bedcode-server-websocket::plugin_binding`（L1227）；移动 `plugin/host_domains/ws.rs` + `bedcode-ws-client-engine` |
| `host-http` | fetch | HTTP 出站 | ❌ | **按端**：桌面 `plugin/http.rs` + `bedcode-server-http::plugin_binding`（L281）；移动 `plugin/host_domains/http.rs` + 同 crate |
| `command`（export） | invoke | 插件命令入口 | ✅ `core_binding.rs` 导出调用面（L280–296） | 内核绑定调用，双端共用 |
| `lifecycle`（export） | activate / deactivate / on-startup / on-shutdown | 生命周期 | ✅ 同上 | 同上 |
| `manifest`（export） | get | manifest 读取 | ✅ 同上 | 同上 |
| `events-binary`（export，可选） | on-message-binary | 二进制消息回调 | —（宿主 `TypedFunc` 探测） | 双端 `plugin-binary` world 由 SDK `wasm_entry!` 导出；不在 `world plugin` 内 |

### A.2 桌面 caps

| cap | interface | 函数 | 实现落点（MODULE_INTERFACES / impl） |
| --- | --- | --- | --- |
| pty | `host-pty` | spawn / write / resize / kill / ring-fetch / is-running | `bedcode-pty-engine::plugin_binding`（L75 / impl L302）+ 桌面 `plugin/pty.rs` 端口 adapter |
| http | `host-http-endpoint` | register-endpoint / unregister-endpoint | `bedcode-server-http::plugin_binding`（L134 / impl L291）+ 桌面 `plugin/http.rs` 端口 adapter |
| ws | `host-websocket-server` | register-endpoint / send-text-to-client / send-binary-to-client / broadcast-text / broadcast-binary / close-client / unregister-endpoint / list-clients / list-endpoints / connection-context | `bedcode-server-websocket::plugin_binding`（L1078 / impl L1254）+ 桌面 `plugin/ws.rs` |
| desktop | `host-events-desktop` | notify | 桌面 `plugin/events.rs`（L26 / impl L67） |
| desktop | `host-fs-desktop` | read-dir / canonicalize / stat | 桌面 `plugin/fs.rs`（L27 / impl L69）；域实体在 `wasm-core host_api/fs.rs`（L354/384/400，与 `FsUnitExecutor` 共用） |
| desktop | `host-platform-desktop` | pick-folders / wsl-distros / local-ipv4-addresses / reveal-in-dir | 桌面 `plugin/platform.rs`（L33 / impl L75） |
| desktop | `host-api-call` | call | 桌面 `plugin/api_call.rs`（L30 / impl L81）；回复道编排留内核 `host_api/api.rs` |
| desktop | `host-timer` | register | 桌面 `plugin/timer.rs`（L29 / impl L76） |
| desktop | `host-process` | run / kill / run-sync | 桌面 `plugin/process.rs`（L46 / impl L418） |
| desktop | `host-app` | install-cli / uninstall-cli / plugin-resource-dir | 桌面 `plugin/app.rs`（L31 / impl L78） |
| desktop | `host-connection-registry` | connections-list | 桌面 `plugin/connection.rs`（L32 / impl L80） |
| desktop | `host-auth-center` | secret-get / secret-set / secret-delete / secret-keys / auth-setting-set / link-identity-parts / auth-center-register / auth-center-unregister / auth-methods-list / auth-method-invoke | 桌面 `plugin/auth.rs`（L59 / impl L105） |
| desktop | `host-task` | execute-batch / submit / status / cancel / list-jobs | 桌面 `plugin/task.rs`（L37 / impl L78）；单元执行器自报面（`fs.` / `process.` / `http.` …）在 wasm-core `host_api/unit_executor.rs` |
| desktop | `host-crypto` | aead-encrypt / aead-decrypt / aead-generate-key / aead-generate-nonce / kdf-derive / keyagreement-generate / keyagreement-shared | 桌面 `plugin/crypto.rs`（L47 / impl L65） |
| desktop（export） | `events`（并集 7） | on-message / on-process-done / on-bus-message / on-auth-success / on-disconnect / on-session-created / on-session-stopped | 桌面 `plugin/guest_events.rs` 绑定；并集理由见 cap-desktop.wit L518 头注 |
| desktop（export） | `abi-form` | form | 桌面宿主 `verify_abi` + SDK `wasm_entry!` |

### A.3 移动 caps

| interface | 函数 | 实现落点（MODULE_INTERFACES / impl） |
| --- | --- | --- |
| `host-notify` | notify / check-permission / request-permission / vibrate / play-sound | 移动 `plugin/notify.rs`（L25 / impl L95）；Android 桥经 `android_plugins/notifications.rs` |
| `host-fs-mobile` | write-media-downloads / save-to-document | 移动 `plugin/fs_mobile.rs`（L22 / impl L90）+ 内核 `host_api/ports.rs::SafIoPort`（L67）+ `plugin/saf_io.rs` |
| `host-terminal-stream` | forward-output | 移动 `plugin/terminal_stream.rs`（L37 / impl L182） |
| `host-connection-target` | primary-target | 移动 `plugin/connection.rs`（L23 / impl L59） |
| `host-auth-client` | request-pairing / verify-pairing-code / qr-connect / biometric-authenticate / has-credentials | 移动 `plugin/auth.rs`（L23 / impl L78）+ 内核 `host_api/ports.rs::AuthEnginePort`（L42） |
| `events`（export 5） | on-bus-message / on-auth-success / on-disconnect / on-session-created / on-session-stopped | 移动 `plugin/guest_events.rs` |

### A.4 可选导出面（不进 `world plugin`，宿主按名动态探测）

| 端 | interface / world | 函数 | 落点 |
| --- | --- | --- | --- |
| 桌面 | `events-binary` / `plugin-binary` | on-message-binary | SDK `wasm_binary.rs` + 宿主探测 |
| 桌面 | `events-ws` / `plugin-ws` | on-message / on-client-message | SDK `wasm_ws.rs` |
| 桌面 | `events-task` / `plugin-task` | on-task-event | SDK `wasm_task.rs` |
| 桌面 | `auth-policy` / `plugin-auth-policy` | verify-device-token | SDK `wasm_auth_policy.rs` |
| 桌面 | `plugin-system` | export command/lifecycle/events/manifest/abi-form + host-storage | 系统组件（`manifest.type=system`）绑定面 |
| 移动 | `events-binary` / `plugin-binary` | on-message-binary | SDK `wasm_binary.rs` |

---

## B. 逐 interface 判定（R1 / R2）

### B.1 core 保留（8 import + 4 export）——R1 三条件全满足

storage / plugin-database / log / config / events / bus / fs / platform + command / lifecycle / manifest / events-binary。

| interface | ① 内核实现 | ② 双端同源语义 | ③ 机制必需 / 共同需要 | 结论 |
| --- | --- | --- | --- | --- |
| host-storage | ✅ | ✅ 一份实现 | ✅ 全部插件共同需要 | 保留 |
| host-plugin-database | ✅ | ✅ | ✅ | 保留 |
| host-log | ✅ | ✅ | ✅ 全部插件 | 保留 |
| host-config | ✅ | ✅ | ✅ 端差异经端口注入（`HostFactsPort`） | 保留 |
| host-events（emit） | ✅ | ✅ | ✅ | 保留 |
| host-bus | ✅ | ✅ | ✅ 插件通信机制必需 | 保留 |
| host-fs（6） | ✅ | ✅ | ✅ 基础能力（用户点名 db/fs 类） | 保留 |
| host-platform（pick-files / pick-folder / platform） | ✅ | ✅ | ⚠️ 见 B.1.1 显式复核 | **保留（推荐）** |
| command / lifecycle / manifest | ✅ 内核导出调用面 | ✅ | ✅ 宿主装载契约 | 保留 |
| events-binary | — 可选导出（SDK 宏 + 宿主探测） | ✅ | ✅ 大载荷通信基础 | 保留 |

#### B.1.1 `host-platform` 的 `pick-*` 显式复核（R1 复核项）

- ① 内核实现：✅ 实体在 `wasm-core/src/host_api/platform.rs` L57/L71，经 `core_binding.rs:253` 绑定，双端同源一份。
- ② 双端同源语义：✅ 函数集一致；差异只在**选择器后端**（桌面原生对话框 / 移动 SAF 树）与结果形状（绝对路径 / 树 URI）——属实现面，不构成函数集差异。
- ③ 机制必需：系统对话框是**平台交互机制原语**（与 fs / platform 标识同层），非产品语义（不知道「在选什么文件、给谁用」）；且被多个第一方插件（file-transfer / agent-hub / terminal-session）与共享产物需要。
- **结论：保留在 core。** 注意：若 C2 统一为 `pick-folders`，则同一 interface 内函数级变更（minor bump），但 interface 本身仍按 R1 留 core（不进域 crate）。

### B.2 外移域共享契约（4 个）——契约与实现同归一处

| interface | 现居 core | 实现已按端落位（证据） | 目标契约房（R4） | T-02 动作 |
| --- | --- | --- | --- | --- |
| `host-peer` | `core.wit` L82 | 桌面 `plugin/peer.rs` + `server-peer-net`；移动 `host_domains/peer.rs` + 同 crate | `packages/bedcode-server-peer-net/wit/peer.wit`（已在盘） | core 删除；双端 caps 增 `peer`→域文件 |
| `host-mdns` | `core.wit` L178 | 桌面 + `discovery-engine`；移动 `host_domains/mdns.rs` + 同 crate | `packages/bedcode-discovery-engine/wit/mdns.wit`（已在盘） | core 删除；双端 caps 增 `mdns` |
| `host-http`（出站 1） | `core.wit` L287 | 桌面 `plugin/http.rs` + `server-http`；移动 `host_domains/http.rs` + 同 crate | `packages/bedcode-server-http/wit/http.wit`（去 `host-http-endpoint` 副本后） | core 删除；域文件去端扩展副本；双端 caps 增 `http` |
| `host-websocket`（客户端 5） | `core.wit` L246 | 桌面 `plugin/ws.rs` + `server-websocket`（客户端段）；移动 `host_domains/ws.rs` + `ws-client-engine`（**实现双轨**，ADR 0041 D1 已知项） | **待裁决**（§D Q4）：`bedcode-ws-client-engine/wit/ws.wit`（推荐） | core 删除；域文件去 `host-websocket-server` 副本；双端 caps 增 `ws` |

> 说明：这 4 个 interface 当前是「内核绑定的 `world core` 声明着它并不实现的业务能力」（ADR 0052 背景 1 的原状），契约位置与实现位置本就分居——外移后契约与实现同归一处，同时消除「core.wit ↔ 域文件两处逐字副本必漂移」的第 3 条背景。

### B.3 端 cap 保留（R2：只有一端提供的功能）

| 端 | interface | 功能 | 判定 |
| --- | --- | --- | --- |
| 桌面 | `host-events-desktop`（notify） | 系统通知（Tauri toast） | **待 C1 裁决**：功能两端都有 → 按 R2 应统一进域共享契约（推荐）；统一后本 interface 删除 |
| 桌面 | `host-fs-desktop`（read-dir / canonicalize / stat） | 目录浏览 POSIX 元数据 | 留端扩展（C3 推荐）——移动 SAF 无 POSIX canonicalize 语义 |
| 桌面 | `host-platform-desktop`（pick-folders / wsl-distros / local-ipv4-addresses / reveal-in-dir） | 多选目录 / WSL / 本机 IP / 定位 | `pick-folders` 待 C2 裁决（统一进 core → 删除）；其余 3 条留端扩展 |
| 桌面 | `host-api-call` / `host-timer` / `host-process` / `host-app` / `host-connection-registry` / `host-auth-center` / `host-task` / `host-crypto` | 互调 / 定时器 / 进程 / CLI 生命周期 / 连接清单 / 认证中心 / 并发任务 / 加密引擎 | 留端扩展（桌面独有，ADR 0018 双端偏离） |
| 桌面 | `abi-form`（export） | component 形态声明 | 留端扩展（桌面 verify_abi 消费；移动无 core 形态共存） |
| 移动 | `host-notify`（notify 之外的 4 条） | 通知权限 / 震动 / 提示音 | 留移动扩展（C1 裁决：rename 为 `host-notify-mobile`）；`notify` 本体统一进域共享契约 |
| 移动 | `host-fs-mobile`（write-media-downloads / save-to-document） | MediaStore 下载 / 系统保存框 | 留端扩展（C4：桌面无等价，本轮不新增） |
| 移动 | `host-terminal-stream`（forward-output） | 终端输出→前端页面通道 | 留端扩展（C5：桌面插件面无等价物，桌面输出走插件自身 WS 协议 + `ring-fetch`） |
| 移动 | `host-connection-target`（primary-target） | 主连接事实 | 留端扩展（桌面同名角色是 `host-connection-registry`，函数集零交集） |
| 移动 | `host-auth-client`（5 条） | 设备入场客户端 | 留端扩展（桌面侧是 `host-auth-center`，角色不同） |
| 移动 | `events`（export 5） | 移动生命周期回调 | 导出面不进 R2（C6），保留；共享产物场景由桌面并集承载（ADR 0050） |

---

## C. 合并提案（R2 后半句：功能一致 → 统一为一个函数）

### C1 · 系统通知（统一）

**现状**：桌面 `host-events-desktop.notify(title, body)`（无选项；Tauri `plugin:notify` 事件，无权限门，无头上下文显性报错——`plugin/events.rs` L67–L95）；移动 `host-notify.notify(title, body, options-json)`（options=`{vibrate?, sound?}` 缺省均 true；权限位 `notify` fail-closed；`plugin/notify.rs` L95–L130）。

**统一签名草案**（破坏面 = minor bump，ADR 0051 D4）：

```wit
interface host-notify {
    /// 发送系统通知。options-json（camelCase，可省字段）：`{ vibrate?: bool, sound?: bool }`，
    /// 缺省均为 true（与移动现行为一致）。**平台不支持的选项静默忽略**（桌面无震动/提示音通道），
    /// 非法 JSON / 字段类型错误 → Err（严格解析，禁静默降级）。
    notify: func(title: string, body: string, options-json: string) -> result<_, string>;
}
```

**落点**：新建轻量域 crate `packages/bedcode-notify-engine`（`wit/notify.wit`，`world cap-notify { import host-notify; }`，R4 允许的「无共享引擎的轻量契约房」）+ 双端 caps 增 `notify`。桌面 `host-events-desktop` 删除（唯一函数已迁出），桌面实现落 `plugin/notify.rs`（或重命名 `events.rs`），移动 `plugin/notify.rs` 保留（删去已迁出的 `notify`）。

**破坏面分类**：接口删除（`host-events-desktop`）+ 新 interface（`host-notify` 进双端 world）+ 移动 `host-notify` 函数集收缩（4 条移入 `host-notify-mobile`）→ **minor**。

**两端实现可行性**：
- 桌面：Tauri emit 路径不变，仅需接受/校验 options（忽略取值）；无新平台依赖。✅
- 移动：实现不变（把 `notify` 挂到新 interface 名下）；4 条附加函数改名落 `host-notify-mobile`。✅
- 共享产物影响：file-transfer 单产物移动分支将来可直接用统一 notify（当前无 notify 调用点，但契约统一后两端一致）。✅

**推荐**：统一 + `options` 不支持项静默忽略（文档声明）+ 移动附加 4 条改名 `host-notify-mobile` + 新建轻量域 crate。（备选：若用户不愿新建 crate，可暂以 `bedcode-wasm-core` 之外的既有 crate 挂载——但无合适落点，故推荐新建。）

### C2 · 目录选择（统一）

**现状**：core `host-platform.pick-folder()` → 单目录字符串（空串=取消；内核实现，双端同源，`core.wit` L228）；桌面扩展 `host-platform-desktop.pick-folders()` → `string[] JSON`（空数组=取消；`plugin/platform.rs` L75–L90）。

**统一签名草案**（minor）：

```wit
interface host-platform {
    pick-files: func() -> result<string, string>;   // 不动
    /// 系统文件夹选择器（多选）→ string[] JSON，空数组 = 用户取消。
    /// 权限 `fs:pick` + 选择后授权校验（口径同 pick-files；授权落账到所选目录本身）。
    /// 移动端 SAF 目录树选择器为单选语义，返回 0/1 个元素（树 URI）。
    pick-folders: func() -> result<string, string>;
    platform: func() -> string;                     // 不动
}
```

**T-04 动作**：core host-platform 删 `pick-folder`、增 `pick-folders`（实体由内核新增 `platform_pick_folders`，复用现有 `pick_folders(app)` 对话框路径）；桌面 `host-platform-desktop` 删 `pick-folders`（剩 3 条扩展）；双端 SDK `host/platform.rs` 与 `wasm_host.rs` 跟进；移动端 manifest 需声明 `fs:pick`（见 §E.1 现状疑点）。

**破坏面分类**：core 内函数删除 + 添加（同一 interface）→ **minor**。

**两端实现可行性**：
- 桌面：内核已有 `pick_folders` 对话框实现（`plugin/platform.rs` 的私有 `pick_folders`），上移/复用即可。✅
- 移动：Tauri dialog 的 `pick_folders` 在 SAF 下返回 0/1 项；调用点从「取首元素」改为「数组透传/取首元素」。✅（`fs:pick` 门现状需一并核对，见 §E.1）
- 共享产物：file-transfer 移动分支 `adapters_mobile.rs` 的 `platform_pick_folder` → 改 `platform_pick_folders` 取首元素；桌面分支删掉一条 extensions 依赖。属 T-04 同步面。

**推荐**：统一（spec §5 推荐）。`host-platform` 仍留 core（B.1.1）。

### C3 · 目录读取 / 元数据（保留桌面扩展）

**现状**：桌面 `host-fs-desktop.read-dir / canonicalize / stat`（POSIX 语义；实体与内核 `FsUnitExecutor` 共用 `host_api/fs.rs`）；移动无插件面等价物（移动 SAF 目录遍历在宿主内部 `file_service/saf_tree.rs`，契约形状是「树 URI + documentId 下钻」，与 POSIX 三函数不同形）。

**核查结论**：移动若统一需新增「SAF 列目录」函数——**形状不同**（寻址是树 URI + 文档 id，`canonicalize` 对 `doc:<id>` 类 provider 无 POSIX 语义）且超出两端现状（ADR 0052 Out of scope：不为统一而新增能力）。

**推荐**：保留桌面扩展（不统一）。理由记录入 ADR/票。

### C4 · 保存到系统位置（保留移动扩展）

**现状**：移动 `host-fs-mobile.write-media-downloads / save-to-document`（`plugin/fs_mobile.rs` + `SafIoPort`）；桌面无任何等价插件面原语（桌面 `reveal-in-dir` 是「定位」不是「保存」）。

**推荐**：保留移动扩展（不统一）；桌面「系统保存框」是新增能力，不在本轮。

### C5 · 终端输出通道（保留移动扩展）

**现状**：移动 `host-terminal-stream.forward-output`（插件 → 宿主 → 前端页面 Channel 的裸字节窄转发）；桌面插件面**不存在**等价物——桌面 terminal-session 插件把输出编码进自身 WS 协议给远端/前端，前端展示走插件命令面拉取（`lib.rs` L771–775 注释：旧 Channel 传输命令已摘除），宿主侧 `utils/session_gateway.rs` 是**互调 api 转发层**，不是插件可调原语。

**推荐**：保留移动扩展（不统一）。

### C6 · 导出面（events 等，保留现状）

`events` 桌面并集 7 函数 / 移动子集 5；`events-ws` / `events-task` / `auth-policy` / `events-binary` 均可选导出（不进 world），宿主按名探测——**不参与 R2 的 import 交集判定**（无 linker 闭合要求，名字对不上只影响能力可用性，实例化不受影响）。共享产物的并集导出由 ADR 0050 承载。

**推荐**：保留现状 + 理由记录（本条）。

---

## D. 最小问题集（待用户逐项拍板；每问附推荐）

> 回答后逐条记入 `issues/01-feature-level-audit.md` 的 Comments，并冻结 T-02/T-03/T-04 输入。

**Q1 · 通知统一（C1）**
(a) 是否统一为共享 `host-notify.notify(title, body, options-json)`？**推荐：统一**（R2 功能级交集；AD:-0052 D3）。
(b) 桌面不支持 `vibrate` / `sound` 的处置：**推荐：静默忽略 + WIT 文档声明**（报错会让只想发通知的桌面调用点无谓失败；静默忽略符合「平台不支持的选项是装饰」语义）。
(c) 移动附加 4 函数（check/request-permission、vibrate、play-sound）：**推荐：改名为移动扩展 `host-notify-mobile`**（cap-mobile），不进共享契约（桌面无权限/震动概念，强行提升会造出恒过死门）。
(d) notify 契约房：**推荐：新建轻量域 crate `packages/bedcode-notify-engine`（`wit/notify.wit`，只持契约 + 可选端口词汇）**；备选：挂到既有 crate（无自然落点，不推荐）。

**Q2 · 目录选择统一（C2）**
是否统一为 core `host-platform.pick-folders()`（`string[] JSON`，空数组=取消），删除 `pick-folder` 与桌面 `host-platform-desktop.pick-folders`，移动返回 0/1 项？**推荐：统一**（spec §5 C2）。附带确认：移动端 `fs:pick` 权限门现状核验（§E.1）由 T-04 一并处理。

**Q3 · C3 / C4 / C5 是否统一**
**推荐：均保留端扩展（不统一）**——C3 移动无同形 SAF 等价（新增能力超出本轮）、C4 桌面无等价、C5 桌面插件面无等价。确认后写入 ADR/CHANGELOG 理由。

**Q4 · ws 客户端契约房（§4.4 待裁）**
选项 A：`packages/bedcode-ws-client-engine/wit/ws.wit`（**推荐**；R4「契约房=共享机制所在 crate」，移动实现在此 crate；桌面客户端实现暂留 `server-websocket` 的 common-host 绑定并以该文件为契约源）。
选项 B：`packages/bedcode-server-websocket/wit/ws.wit` 保留客户端面（改动最小，但与「桌面 actix 服务端 crate 端中立」冲突，长期仍是「端 crate 持共享契约」）。
请一并裁决桌面 ws **客户端**实现去留：留 `server-websocket` common-host（T-03 只搬服务端）vs 一并迁桌面宿主（工作量更大、语义不变）。

**Q5 · core 复核项（B.1.1）**
`host-platform` 的 `pick-*` / `platform` 留 core：**推荐：留**（内核实现 + 双端同源 + 平台机制原语，满足 R1 三条件）。确认或要求外移。

---

## E. 现状疑点与文档漂移（T-02/T-05 输入，本票只记录不改）

### E.1 移动端 `fs:pick` 与 pick 权限门（需 T-04 前核实）

- 内核 `host_api/platform.rs` L63/L77 对 `pick-*` 统一要求 `fs:pick`（双端同一份实现）。
- 桌面 `cap-desktop.wit` L123–132（v31 版本说明段）声称「移动端维持『选择即授权、不设门』」——**与内核单形态后的实际实现矛盾**（移动 file-transfer `plugin.json` 未声明 `fs:pick`，`grep -c 'fs:pick'` = 0）。
- 影响：若移动确需该位，则移动 file-transfer 的 `pick-folder` 现状即被拒（或文档口径错误）。**T-04 开始前必须先核实并收口（补 manifest 或修正文档）**，否则统一后的 `pick-folders` 会带同一问题。

### E.2 「17 个」陈旧口径（T-05 统一收口）

`AGENTS.md` L150/L170、`bedcode-desktop/docs/code-map.md` L232/L270、`docs/adr/0049` L29、`docs/adr/0045` L33（历史正文，加注记不改）、`CHANGELOG_zh.md` L159/L331、`packages/bedcode-wasm-core/src/lib.rs` L37、`manager/runtime/core_binding.rs` L20、`manager/runtime/guest_events.rs` L14、`mobile host_domains/mod.rs` L5——均需改为新口径（收窄后 12）。

### E.3 `wit_package_version_lock.rs` 注释漂移（T-05）

`bedcode-desktop/src-tauri/tests/wit_package_version_lock.rs` 注释「pty / http / ws 会被双端 compose 拷贝」——实际移动只组合 `cap-mobile`（移动 caps 无 pty/http/ws）。锁的 `WIT_SOURCES` 清单本身无功能错误，但注释与事实不符。

### E.4 其他

- `core_binding.rs` L20 注释「cap 域…不属 core.wit 的 17 个双端全等 interface」——同上计数漂移。
- `mobile host_domains/mod.rs` L5「17 个双端全等 interface 里 8 域内核实现」——事实是 16 中 8，且其中 4 个（mdns/peer/http/ws）将随 T-02 出 core。
- 若 Q4 选 A 且 T-03 不改桌面 ws 客户端实现落点：`server-websocket` 将同时存在「桌面服务端实现」与「共享客户端契约的既有实现」，需在 T-05 锁里明确「能力域端中立锁」对它的判据（该 crate 是否算端中立）。

---

## F. 冻结输入（用户 2026-10-13 全部按推荐确认）

| 项 | 冻结值 | 裁决来源 |
| --- | --- | --- |
| core 保留面 | 8 import（storage / plugin-database / log / config / events / bus / fs / platform）+ 4 export（command / lifecycle / manifest / events-binary）；`host-platform` 含 `pick-*` / `platform` 留 core | Q5 |
| 外移域清单与目标文件 | `host-peer`→`server-peer-net/wit/peer.wit`（world cap-peer）；`host-mdns`→`discovery-engine/wit/mdns.wit`（cap-mdns）；`host-http` 出站→`server-http/wit/http.wit`（去 endpoint 副本后，cap-http）；`host-websocket` 客户端→**新建** `ws-client-engine/wit/ws.wit`（cap-ws） | Q4 + B.2 |
| 端扩展 world 改名 | 桌面 wit-src `cap-http`→`cap-http-endpoint`；`cap-ws`→`cap-ws-server`（interface 定义不动） | spec §4.3 |
| C1 统一签名 + 契约房 | 共享 `host-notify.notify(title, body, options-json)`（options `{vibrate?,sound?}` 缺省 true；不支持项静默忽略）；契约房 = 新建 `packages/bedcode-notify-engine`（`wit/notify.wit`，cap-notify）；移动附加 4 函数 → `host-notify-mobile`（cap-mobile）；桌面 `host-events-desktop` 删除 | Q1 |
| C2 统一签名 | core `host-platform.pick-folders() -> result<string,string>`（`string[] JSON`，空数组=取消）；删 `pick-folder` + 桌面扩展 `pick-folders`；移动返回 0/1 项；`fs:pick` 门统一 | Q2 |
| C3 / C4 / C5 | 保留端扩展，不统一（`host-fs-desktop` 三函数 / `host-fs-mobile` 两函数 / `host-terminal-stream`） | Q3 |
| C6 导出面 | 保留现状（桌面 `events` 并集 7 / 移动子集 5；可选导出按名探测；共享产物并集由 ADR 0050 承载） | C6 |
| 桌面 ws 客户端实现落点 | 暂留 `bedcode-server-websocket` 的 common-host 绑定，契约源改指 `ws-client-engine/wit/ws.wit`；服务端 impl 迁桌面宿主 | Q4 附问 |
| T-02 union 等价基线 | §0 基线名单 | 本文件 |
| T-04 破坏面 | 一次 minor bump `0.1.0 → 0.2.0`（C1 + C2 的 interface 增删/函数变），双端同步 + 产物重建 | ADR 0051 D3/D4 |
