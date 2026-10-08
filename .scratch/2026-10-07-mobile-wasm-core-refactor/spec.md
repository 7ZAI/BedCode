# 移动端整体架构重构：Rust 层无业务化 + 业务下沉 wasm-apps + 复用 wasm-core 与桌面看齐

Status: **in-progress**（阶段 0 / 1 / 2 已实施：票 01–10；**票 11 已实施**（2026-10-08，阶段 3 首票：host-websocket 客户端域，mobile ABI 13→14，落 ADR 0041；引擎形态 A / 不做 wss / timeout 5s 三点经用户裁决）；**票 12 已实施**（2026-10-08，终端订阅协议客户端迁插件，mobile ABI 14→15，落 ADR 0018 偏离表条目；§10 三个裁决点 D-12a/b/c 均按推荐案落地）；**票 14 已实施阶段 A + 阶段 B**（2026-10-08，宿主本地配对码编排面退役 + 认证/配对编排下沉，mobile ABI 15→16）；**票 16 已实施**（2026-10-08，auto-task 并入 `com.bedcode.terminal-session`，阶段 3 收口；零 ABI 变更（mobile 停 v16），详见 `ticket-16-auto-task-merge.md`）；**下一票 = 票 15（终端 UI 域下沉 + host-terminal/terminal-hooks 退役）**。开放裁决点见 §4，其中 D1/D2/D3/D5 已在票 01/02/20 前置拍板并落 ADR；每票文档 `ticket-*.md` 是实施与门禁的真源。
> **2026-10-08 07:36 状态校订（接手会话补记）**：票 12 与票 14 阶段 B 的代码已落盘（`plugins/terminal-session/` 新建、`terminal_stream_gateway.rs` 窄转发、`host_impl/{auth,terminal_stream,ws,connection}.rs`、`abi.rs` = v16），但**独立端到端验证未完成**：`plugins/terminal-session` crate 当前编译红（3 处 `HostLog` trait 未在作用域，与并行会话在途改动相关），wasm32 门禁（`node scripts/plugin-build.js --rust-only`）与 cross-end-tests 未跑；宿主 `cargo test --lib` 另有 6 处 `egress.rs` 失败（见票 12 §8.4 判定的在途基线）。**故上列「已实施」依据是代码与退役锁，不等价于票 12 §8.4 / 票 14 门禁全部通过**——收口见各票文档。
> **2026-10-08 08:45 门禁收口（票 14 实施会话）**：上列缺口已逐项补齐——① 插件 crate 编译红已修（`auth.rs` 补 `HostLog` import），native `cargo test` **40/40 绿**；② **wasm32 真门禁通过**（SDK CLI `pnpm run build`，组件化 490 KB）；③ 阶段 B 新锁 4/4 + 变异自检 3/3（阶段 A 锁回归 4/4）；④ 前端全量 vitest **65 文件 / 680 测试全绿**（pairing-flow / connection-flow mock 面已换插件命令形状）+ 改动文件 eslint 0 error；⑤ 宿主全量全部集成目标绿（lib 6 处 `egress.rs` 失败仍为并行会话在途基线，同数同款）。cross-end-tests 未跑（无跨端 wire 变更）、真机三流往返留票 21——明细见票 14 文档 §9.3。
**2026-10-08 新增需求（用户指令）**：**auto-task 与终端合并进同一个 wasm app** —— 落为裁决点 **§4 D6** 与阶段 3 收口票 **票 16**（原阶段 4/5 票号顺延 +1）。
**D6 已于 2026-10-08 由用户拍板 = 选项 A**：移动端 app id 沿用 **`com.bedcode.terminal-session`**（与桌面同名同构；B/C 否决）。
Date: 2026-10-07
用户方向指令: 「移动端开始实施 rust 层无业务化，业务下沉到 wasm-app，复用 wasm-core 作为核心包进行重构，与桌面端架构看齐」+「整个重构写成 spec 文档、划分 ticket 进行」
前置文档: `docs/knowledge/businessless-kernel-vision.md`（终态愿景）· `docs/knowledge/plugin-kernel-roadmap.md`（渐进路线）· `docs/adr/0022`（边界裁决单一事实源）· `docs/adr/0037`（wasm-core 整核抽出）· `docs/adr/0018`（移动契约独立）· `docs/adr/0019`（双端锁版）· `.scratch/2026-10-07-capability-crates-to-root-packages/`（8 个能力域上提根 packages）

---

## 0. 一句话目标

把移动端宿主（`bedcode-mobile/src-tauri/src/`）从「自持业务 App + 自带插件机制」收敛为与桌面端同构的**薄壳**：宿主只留引擎原语与安全闸门，产品业务（对等传输编排、设备/连接编排、会话控制、终端消费、认证配对编排）下沉到 wasm 插件，插件机制复用 `bedcode-wasm-core` 的机制面，最终移动端宿主命令面/业务代码按 ADR 0022 六条判据（B1–B6）归零。

---

## 1. 现状盘点（全部取自 2026-10-07 工作区实测，勿凭记忆）

### 1.1 Rust 宿主业务清单（142 文件 / 34,656 行）

| 模块 | 行数 | 内容 | 判据（ADR 0022 B1–B6） | 归位 |
| --- | --- | --- | --- | --- |
| `peer_net.rs` | 1,941 | 节点身份 + 发现守护装配 + **设备列表派生视图（发现缓存快照指纹 `peer-devices-changed` 全量推送）** + 共享目录注册表 CRUD + 可信列表命令面 + `dial_peer`/`disconnect_peer` | B4（设备列表 = 派生视图）、B2（发现缓存比对编排） | 引擎部分留（node/daemon 装配、consent 闸门、信任存储），**投影下沉插件** |
| `peer_transfer.rs` | 1,870 | `SendTask` 状态机 + **发送并发闸门/队列泵**（`current_concurrency`/`pick_pending_to_start`/`pump_send_queue`）+ 历史持久化（`HistoryFile`）+ retry / resume-all 编排 + 取消原因码映射 | B2（任务状态机、并发封顶、重试编排）、B5（默认并发策略） | **整体下沉 file-transfer**（对齐桌面 v30/v31） |
| `peer_receive.rs` | 934 | 接收策略（ask/always_accept/always_deny）+ 询问超时 + 接收批登记 + 历史 | B2（接收编排） | **下沉 file-transfer**（策略闸门参数留宿主原语） |
| `peer_remote.rs` | 469 | 远端浏览/拉取编排 | B2 | **下沉 file-transfer**（线协议动词留 host-peer） |
| `session/http.rs` | 472 | 会话控制 HTTP 客户端（list/start/stop/remove/input） | B4/B2（会话控制编排） | **下沉移动端内置插件** |
| `auth/` | ~1,600 | 配对编排（manager/pairing）+ 设备身份持久化 + 凭据存储 + HTTP 认证客户端 | B2（配对流程编排）；JWT/身份属安全边界 | **编排下沉插件，身份/凭据/认证客户端留宿主引擎（安全边界）** |
| `terminal_link.rs` | 1,363 | 终端 WS 订阅协议客户端（subscribe/ack/ring_resync/special key 翻译/退避重连） | B2/B4（终端消费是产品语义）；帧级传输是引擎 | **整体迁入终端插件**（WS 帧通道留引擎原语） |
| `egress.rs` | 833 | 外网授权策略（三层：桌面目标/静态声明/弹窗授权，fail-closed，授权记忆持久化） | **安全闸门（薄壳②）**，对照 ADR 0022「授权策略=安全闸门」节 | **留宿主**，形态与桌面授权策略对齐后落锁 |
| `connection/`（manager/ws_client/reconnect/event_ws/heartbeat） | ~3,000 | WS 客户端传输 + 心跳重连 + 事件通道 | 引擎（传输面） | 留内核 |
| `mdns/`（discovery + advertiser） | ~700 | **双实例**（`MdnsDiscovery` + `MdnsAdvertiser` 各自独立守护，未收敛为桌面单 `ServiceDaemon` 形态） | 引擎；双守护生命周期分叉是桌面已消灭的历史病灶（桌面「双 daemon 同绑 5353 互抢」）的同构风险 | 收敛为单守护，复用 `bedcode-discovery-engine` |
| `handler/` + `router/` | ~1,100 | WS 消息路由 + `MobileEvent` 分发 | 传输面引擎 | 留内核（业务变体解释权随业务下沉迁移） |
| `model/message.rs` | 1,060 | wire 形状 | 传输面契约（桌面同类已收编 SDK `wire`） | 留（形状真源待对齐决策） |
| `plugin/` | **11,640** | 移动端自持插件机制：manager/loader/registry/storage/wasm_runtime/component（`bindgen!` 绑移动 WIT）/message_bus/fs_auth/approval/validation/downloader/saf_io/saf_path/commands + host_impl 13 域（2,309 行） | 机制，但与桌面 wasm-core **大量同构、双份持有** | **复用 wasm-core 机制面**（§4 裁决） |
| `file_service/saf_tree.rs` | 516 | SAF 目录树 | 平台能力（Android SAF） | 留宿主（平台原语），经 host-platform/host-fs 暴露 |

### 1.2 WIT 差异面（移动 ABI 11 vs 桌面 ABI 31，`packages/plugin-sdk-mobile/rust/wit/bedcode.wit`）

| 接口 | 移动端现状 | 桌面端终态 | 差异处置 |
| --- | --- | --- | --- |
| `host-peer` | 15 函数（**含 `resume-all-transfers`**，无 `set-download-dir` / `start-node` / `stop-node` / `active-transfers` / `collect-outgoing`） | 19 函数（v31 已删 `resume-all-transfers`） | 对齐 19：增 5 删 1；业务面（`list-*`/`retry`/`clear-*`/`set-transfer-encryption`/`set-transfer-concurrency`）已不在 WIT |
| `host-terminal` | 1 函数（`send`） | **v27 整 interface 删除** | 对齐：随终端下沉退役 |
| `terminal-hooks` | 2 函数（on-terminal-input/output） | **v27 整 interface 删除** | 对齐：退役 |
| `host-mdns` | 5 函数（browse/stop-browse/advertise/stop-advertise/is-advertising） | 同形（v2 基础能力服务形态） | 语义对齐（属主定向事件 `<owner>::mdns:found|lost`） |
| `host-database` | 2 函数（execute/query） | database/plugin-database/storage 13 原语 | 开放裁决（§4 D4）：薄库 vs 统一 |
| `host-storage` | 3 函数 | 同形 | 对齐 |
| `host-websocket` | **无** | 15 函数（客户端 5 + 服务端 9 + connection-context） | **新增客户端域 5 函数**（移动端是拉取方，终端/事件通道下沉必需） |
| `host-pty` / `host-auth` / `host-crypto` / `host-connection` / `host-process` / `host-app` / `host-task` / `host-session`（已退役） | 无 | 桌面独有 | 不跟演（ADR 0018 双端偏离，移动端不需要主机侧引擎） |

### 1.3 插件机制重复面（「复用 wasm-core」的量化基础）

- 桌面 `bedcode-wasm-core` = 49,093 行 / 136 文件：机制（manager/bus/security/permission/monitor/config/runtime_util/intercall/storage/host_context_registry）+ 引擎面（db/enums/system）+ 宿主胶水（utils/auth、session_gateway）+ host_api 21 域。
- 移动端 `plugin/` = 11,640 行：与 wasm-core 机制层**同构但独立演进**（component.rs 各自 `bindgen!`、manager 生命周期各自写、message_bus vs bus、fs_auth vs security::fs_auth、approval/validation 各有桌面镜像）。
- 双端机制双份 = 每次机制修复/ABI 演进两处同步（桌面已吃过的五处同步税，移动端照付）。
- 根 `packages/bedcode-host-kit`（1,110 行）是**唯一已共享**的机制锚点（module/registry/state/ports/limits/metrics）。

### 1.4 移动端插件现状 + 桌面「auto-task 已并入终端 app」先例（2026-10-08 实测，合并需求的事实底座）

| 插件 / 位置 | 形态 | 业务归属 | 本专项处置 |
| --- | --- | --- | --- |
| `plugins/auto-task`（`com.bedcode.auto-task`） | TS 面板 + 工具箱页 + 极简 rust 壳（`manifest()` 读 `plugin.json`，`invoke_command` **显式全部拒绝**）；业务全走宿主 HTTP 通道访问桌面 `/api/plugin/com.bedcode.auto-task/*`（`src/api.ts:52`）；权限 `session:read` / `storage` / `ui:input` / `ui:toolbox` | B2/B4（任务队列编排 + 桌面任务域的只读投影视图；ADR 0012「手机看、桌面管」） | **并入终端 wasm app**（2026-10-08 新增需求 → §4 D6 / 票 16） |
| `plugins/ai-chatbox` | TS + rust | 独立产品域 | 不动（非本专项对象） |
| `plugins/file-transfer` | 全栈 rust + TS | 对等传输 | 阶段 2 已下沉（票 06–10） |

桌面先例（合并口径的单一参照，逐项对照）：

- 桌面 `com.bedcode.terminal-session` = **配对/信任 + 会话配置与生命周期 + Agent 任务域（队列状态机 / 定时任务 / agent hook 安装）+ 终端** 四域合一：`wasm-apps/terminal-session/plugin.json`（`api[]` 含 `session.task.*` 全族 + `wsEndpoints` 含 `terminal`）、`rust/src/task/`（queue/scheduled/preset/agent/hooks）、`src/components/TaskQueueModal.vue`、`scripts/{auto_task_hook.py,codex_task_hook.py,pi_task_hook.ts,opencode_task_hook.ts}`。
- 2026-09-20 桌面 `com.bedcode.auto-task` 与 `com.bedcode.devices` 已退役并入该 app（CHANGELOG 双语条目 + ADR 0022 v8 批次条目 + `plugin-kernel-roadmap.md`）。
- 旧 HTTP 前缀仍由宿主别名表兜底：`packages/bedcode-server-http/src/controllers/plugin_controller.rs:167` `("com.bedcode.auto-task", "com.bedcode.terminal-session")`。
- roadmap「移动端受影响清单」**M1**：桌面**若**切断旧前缀，移动端任务面板全部 HTTP 404；预案 = 改移动端 api 基址常量一处 + 插件重打包，**同批**删宿主别名表与 `resolve_http_owner`。
- 移动端对 `com.bedcode.auto-task` 的**硬引用五处**（合并时必须同步）：`src-tauri/src/lib.rs:203`（fs_auth 可信插件白名单）、`scripts/plugin-package-list.json`（mobile 列表）、`scripts/dev-run.js:177`（dev 注册 id）、`packages/plugin-sdk-mobile/dev-shell/src/mock/mobile-api.ts:129`（mock 基址）、`plugin-dev-mobile.md`（构建示例）。

---

## 2. 目标架构（与桌面看齐）

```text
移动端 wasm 应用层（业务事实面：file-transfer + 移动端内置 `com.bedcode.terminal-session` = 连接/会话/终端/auto-task 四域合一）
        ↓ 下行只经四闸门：能力 · 身份 · 隔离 · 生命周期
        ↓ 插件上行只经四通道：WIT host-* · bus · events · 互调
移动端宿主内核层（引擎 + 安全闸门，复用 wasm-core 机制）
```

> **同名 id 声明（D6 选项 A 的对冲）**：移动端 `com.bedcode.terminal-session` 与桌面端**同名但职责不同**——桌面端是会话/任务/终端的**权威**（持有 PTY、WS 服务端、认证中心桥接），移动端是**远程终端控制端**（只做订阅消费、会话控制调用、任务面板的只读投影，ADR 0012「手机看、桌面管」）。两端契约各自独立（ADR 0018），WIT/ABI 不因同名而互相约束；该声明须同步进 ADR 0018 双端偏离表与双端 `docs/code-map.md`（票 16 落地 + 票 21 文档收口）。

- 宿主 Rust 层保留：WS 客户端传输（connection/）、单守护 mDNS（复用 discovery-engine）、host-peer 原语面（引擎句柄表 + 闸门）、host-storage/database 机制、host-http（代理/出站闸门）、host-fs（SAF 平台）、egress 授权策略（安全闸门）、host-platform、Android 原生桥（Kotlin 插件注册）、传输面路由（handler/router/model）。
- 宿主 Rust 层**不再持有**：设备列表派生视图、传输任务状态机、历史/策略编排、会话控制编排、配对流程编排、终端订阅/输入/special-key 翻译、`MobileEvent` 业务变体解释。
- 插件机制：移动端以 `bedcode-wasm-core` 的机制面为单一事实源（§4 D1 定形态），WIT 绑定与 host_api 移动域自持（ADR 0018 契约独立）。

---

## 3. 约束与 ADR 张力（不可自行放松，冲突按 §0 裁决优先级上报）

| # | 约束 | 出处 | 对本专项的影响 |
| --- | --- | --- | --- |
| C1 | 移动端契约独立，不跟演桌面破坏性变更；同名词义对齐 | ADR 0018 | 「复用 wasm-core」= 复用**机制**，不是采纳桌面 WIT；host-terminal/terminal-hooks 退役与 host-websocket 新增是**移动端自身演进**（ABI bump 走 ADR 0019 双端各自演进口径） |
| C2 | wasm-core 落 `bedcode-desktop/packages/`，理由=依赖桌面基础层+桌面 WIT，移动端拉不动 | ADR 0037 D1 | **2026-10-07 已部分消解**：8 个能力域 crate（含 bedcode-server-base/core/http/ws/peer-net/discovery/pty）上提根 `packages/`，根/桌面 packages 依赖边归零。剩余阻塞 = ① WIT 绑定（bindgen 桌面 bedcode.wit + bedcode-plugin-api 桌面类型如 PluginKind/WsiPreopenDir，ADR 0032 明确 PluginKind 桌面独有）② host_api 桌面域。**本 spec 提议修订 ADR 0037 D1**（见票 01） |
| C3 | 输出字节禁止经 JSON 命令通道搬运；经 WIT 二进制原语可进 WASM | ADR 0022 v23 性能红线 | 终端下沉到插件：输出消费须帧级/二进制直传（host-websocket 客户端帧 or 二进制原语），禁止逐帧 JSON 化 |
| C4 | 安全边界（JWT/密钥/信任/授权闸门）永远留内核 | businessless-kernel-vision §Out of Scope | auth 编排可下沉，但设备身份/JWT 持有/认证客户端/egress 裁决留宿主 |
| C5 | 会话/终端/设备连接在桌面已下沉；移动端曾「暂停推进，随移动端需要另行立项」 | roadmap 阶段 2/3、businessless-kernel-vision Out of Scope | **用户本次指令即该立项**；移动端形态与桌面不同（移动端是消费端：无 PTY/认证中心/WS 服务端），下沉对象是「远程终端控制客户端」 |
| C6 | 双端共有接口改 WIT 必须双端同步评估；桌面独有接口不要求移动端跟演 | ADR 0019 / ADR 0022 双端偏离 | host-mdns 语义对齐、host-peer 对齐 19 属双端共有接口演变，须双端同步评估（桌面已到终态，移动端向它对齐是单向往还，风险低） |
| C7 | 移动端是自持业务 App | ADR 0018 / ADR 0022 | 移动端业务插件化是**渐进**的：内置插件默认启用保核心体验（对齐桌面「内置插件默认启用」形态） |
| C8 | 双端 app id 同名 ≠ 契约同一（D6 选项 A 引入） | ADR 0018 双端偏离 | 移动端 `com.bedcode.terminal-session` 与桌面同名但只做「远程终端控制端」，**禁止**因同名推断两端共享 WIT/ABI/权限集/存储 schema；差异须登记 ADR 0018 偏离表 + 双端 code-map（票 16 / 票 21） |

---

## 4. 开放裁决点（需用户拍板；每项给出推荐与备选）

### D1 · wasm-core 复用形态（本专项最大件，票 01）
- **选项 A（推荐）对称复用**：把 wasm-core 中**与 WIT 无关**的机制模块（manager 生命周期/loader/registry/storage/downloader/approval/validation、permission、bus、security/fs_auth、monitor、config、runtime_util、intercall、host_context_registry、db 机制端口）抽为**共享机制核**（并入 `bedcode-host-kit` 或新建 `bedcode-plugin-mechanism`）；双端各自保留「WIT 绑定 + host_api 域 + 引擎」，`bedcode-wasm-core` 收窄为桌面薄壳（facade 与锁不动）。移动端新 crate（如 `bedcode-mobile-wasm-core`）= 机制核 + 移动 WIT 绑定 + 移动 host_api（12 域）。
  - 收益：机制单一事实源；桌面不再背移动端不需要的域；双端机制修复一次生效。
  - 成本：host_api 实现从 `impl bedcode::plugin::host_X::Host`（bindgen trait）解耦为「面向机制层端口 + 各端 adapter」是大手术，须保桌面零回归（ABI/WIT 零变动）。
- **选项 B 整份 fork**：复制 wasm-core → `bedcode-mobile/packages/bedcode-wasm-core`，替换 bindgen 路径 + 删桌面域 + 适配移动 WIT。成本最低、立即对齐；代价是**双份漂移**（须同步落防回接锁/对照锁，如 SDK 已有的 `mobile_parallel_copy_shape_lock` 先例）。
- **选项 C 两步走（推荐组合）**：票 17 先 fork 对齐（B，兑现「移动端拥有 wasm-core 级机制」），票 18/19 再把无 WIT 依赖机制抽回共享核（A），fork 面逐步收缩到「WIT 绑定 + host_api」。**推荐 C**：一步到位 A 风险集中在一次大手术；C 每步可验证可回退，且共享核抽取方向与桌面既有的 host-kit 演进同轨。

### D2 · 移动端 ABI 规划（票 02）
- 新增 `host-websocket` 客户端域（5 函数：connect/send-text/send-binary/close/is-connected）+ 可选导出 `events-ws`：ABI 11 → **12**。
- `host-peer` 对齐桌面 19：删 `resume-all-transfers`（破坏性，随传输下沉同批），增 `set-download-dir` / `start-node` / `stop-node` / `active-transfers` / `collect-outgoing`（纯增量可先加）。
- `host-terminal`（1 函数）+ `terminal-hooks`（2 导出）退役：ABI 12 → **13**（破坏性，随终端下沉同批，旧产物实例化期点名重建——fail-visible 三形态②）。
- 原则：每批破坏性变更必须带 `stale_artifact_rebuild_hint` 判据扩展 + 权限位/命令字眼加载即抛（三形态③）。

### D3 · 移动端业务插件组织
- 传输类：下沉既有 `com.bedcode.file-transfer`（已全栈，peer.rs/device_bridge/transfer_store 已就位，模式与桌面 file-transfer 同构）。
- 连接/会话/终端/**自动任务**类：**新建内置 wasm app `com.bedcode.terminal-session`（移动版，D6 选项 A 已定案）**，承载：连接编排视图（配对流程/设备列表/状态合并）、会话控制（list/start/stop/remove/input）、终端消费（订阅/输入/special-key/渲染 UI）、**自动任务**（任务队列面板 + 工具箱「自动任务」页：任务记录 / 定时任务 + i18n 表 + 桌面端 HTTP 客户端）。对齐桌面 `com.bedcode.terminal-session`「配对—会话—终端—Agent 任务四域一元」的合并口径（桌面已于 2026-09-20 把 `auto-task` 并入该 app，见 §1.4）。
  - 备选：终端独立 `com.bedcode.terminal`（移动版）、auto-task 维持独立插件。**均否决**：故障半径靠**分域 Result 边界**吸收即可（桌面先例已证），拆 app 反而制造双端粒度分裂与两次 ABI 窗。
- **auto-task（2026-10-08 用户指令口径，取代原「维持插件化、不回归宿主」）**：auto-task 与终端进**同一个 wasm app**。理由：① 与桌面四域合一形态同构，避免「同一插件在两个端是两种粒度」的心智分裂；② auto-task 主入口是**会话级终端工具栏按钮**（`plugin.json contributes.terminal.toolbarItems`）与终端消费天然同生命周期、同会话上下文；③ 数据面本是桌面任务域的只读投影（ADR 0012），无独立真源，合并不增加故障半径；④ 桌面先例证明合并后 i18n / 视图 / hook 脚本按域重组可行（非逐文件平移）。**不回归宿主**的底线不变。

### D4 · host-database 形态
- 移动端 host-database 仅 2 函数（execute/query 裸 SQL），桌面 13 原语带权限门/表名前缀纵深/属主分区。
- 推荐：**随 D1 的机制核抽取，把移动端 db 机制对齐桌面 13 原语语义**（权限门 + 表名前缀），迁移成本小（移动端插件生态薄：ai-chatbox/auto-task/file-transfer 三家用库），换来双端插件行为同构。
- 备选：保持薄库，仅共享执行器（SQL 超时/上限，D1 审计项）。

### D5 · egress 授权策略
- **默认裁决：留宿主**（安全闸门，薄壳②，对照 ADR 0022 2026-09-28 节三档策略判据——它只决定「问不问」，不解释产品语义）。票 20 对齐桌面三档形态（总是询问/默认/始终允许 + 授权记录）后落防回接锁。

### D6 · auto-task 与终端合并进同一个 wasm app（2026-10-08 用户指令新增 · **已拍板：选项 A**）
- **需求**：移动端 `com.bedcode.auto-task` **不再作为独立插件存在**——其任务队列面板 / 工具箱「自动任务」页（任务记录 + 定时任务）/ `api.ts` HTTP 客户端 / i18n 表 / 样式 / 生命周期订阅，整体并入阶段 3 新建的终端 wasm app，与终端消费同属**一个 app、一个 manifest、一个 ABI 窗、一份权限集**（权限位与贡献点取并集）。桌面 `com.bedcode.terminal-session` 是同构先例（§1.4）。
- **✅ 选项 A（2026-10-08 用户拍板）沿用桌面 id `com.bedcode.terminal-session`**：与桌面同名同构，「手机看、桌面管」的端点映射零心智；顺带把移动端 api 基址从 legacy `com.bedcode.auto-task` 切到 `com.bedcode.terminal-session`，一次性消解 roadmap M1 风险。
  - **已知代价与对冲**：移动端是消费端（无 PTY / 无会话权威 / 无 WS 服务端），同名 id 必须在文档显式声明「**两端同 id、职责不同**」——落点为 ADR 0018 双端偏离表 + 双端 `docs/code-map.md` + 本 spec §2 目标架构图注（票 21 文档联动，票 16 落地时同步）。
- ~~**选项 B** 沿用本 spec 原口径 `com.bedcode.session`~~：**已否决**（双端 id 与粒度再次分叉，ADR 0018「同名词义对齐」更难解释）。
- ~~**选项 C** 新建 `com.bedcode.mobile-terminal`~~：**已否决**（第三套命名，跨端映射全靠特例表）。
- **合并的强制口径（不可自行放松）**：① manifest 权限位与贡献点取并集（`session:read` / `storage` / `ui:input` / `ui:toolbox` + 终端域所需），未用到的贡献点随迁即删，不许「先留着」；② 按**域重组**而非逐文件平移（桌面先例的明面经验），i18n key 双语同步并带 app 前缀；③ 桌面端旧 HTTP 前缀切断**不在本专项**（§8 Out of scope 含桌面改动）——本专项只改移动端基址，`LEGACY_HTTP_PLUGIN_ALIASES` 保留，切断列双端同批待立项；④ 旧 app id `com.bedcode.auto-task` 退役走 fail-visible 三形态（旧读路径删除或显性报错、旧产物实例化期点名重建、退役 id / 视图 id 加载即抛）+ 防回接锁 + 变异自检；⑤ 五处硬引用同批同步（§1.4 清单），其中 fs_auth 可信白名单条目由 `com.bedcode.auto-task` 换为 `com.bedcode.terminal-session`；⑥ 移动端 app 的 `plugin.json` 与前端文案不得自称「会话/终端权威」，wing 文案口径 = 「远程终端控制端」（避免与桌面同名 app 的权威语义混淆）。

---

## 5. 阶段与票（渐进：每阶段一件事、可验证、可回退；扩-收排程）

> 票号按执行序；每票自带门禁（§6）。括号内为依赖。

### 阶段 0 —— 基线裁决（不落代码）
- **票 01 · wasm-core 复用形态定案（D1）**：三选项对比落地 ADR（修订 ADR 0037 D1 + 新增「移动端 wasm-core 复用形态」ADR）。产出：机制核抽取边界清单（WIT 无关模块白名单）+ 移动端 wasm-core crate 骨架规划。门禁：ADR 落档 + 桌面 wasm-core 零改动编译通过。
- **票 02 · 移动端 ABI 规划落档（D2）**：WIT 差异表逐接口定案（对齐/新增/退役三列各带版本号与破坏性标记）；权限位增删表（对照桌面五同步点：SDK 常量/打包 CLI/前端合法集合/宿主能力清单/权限门）。门禁：`ABI_VERSION` 规划表 + 双端 WIT 对照锁草案。

### 阶段 1 —— 共享底座对齐（机制层，零业务语义）
- **票 03 · mDNS 单守护收敛**：`mdns/advertiser.rs` + `discovery.rs` 双实例 → 复用根 `packages/bedcode-discovery-engine` 单 `ServiceDaemon`（消灭双 daemon 同绑 5353 病灶）；`host_impl/mdns.rs`（815 行）对齐桌面属主定向事件 `<owner>::mdns:found|lost`（payload 增量 serviceType/browserId）。门禁：双端 mDNS 行为等价（对等网络集成测试）；host-mdns 权限门 + 属主仲裁测试。
- **票 04 · host-peer 对齐 19**：删 `resume-all-transfers`（先随票 06 迁移再删，破坏性）、增 5 函数（`set-download-dir` / `start-node` / `stop-node` / `active-transfers` / `collect-outgoing`，纯增量先加）；宿主命令面 `dial_peer` 不再替插件从发现缓存解析 endpoint（WIT 已是 `dial-peer(endpoint-json)`，命令层同步显式化，对齐桌面「引擎不内藏 node-id→地址解析表」）；命令面收窄为薄转发（consent/trust/闸门应答留）。门禁：插件调用点零漂移用例 + 五同步点锁。
- **票 05 · host-database/host-storage 机制对齐（依 D1/D4）**：若 D4 选统一 → 移动端 db 域对齐 13 原语语义（权限门/表名前缀/护栏/属主分区），`bedcode_plugins.db` 迁移；插件三方（ai-chatbox/auto-task/file-transfer）调用点同批迁。门禁：插件存储隔离测试（桌面 host-api tests 移植）。

### 阶段 2 —— 对等传输业务下沉 file-transfer（对齐桌面 v30/v31）
- **票 06 · 发送侧事件桥 + send-files 语义收窄**：宿主 `peer_transfer.rs` 收敛为「batch_id → 发送会话句柄表 + 引擎事件桥」（`peer:transfer-event`，150ms 进度节流 + OfferPending oneshot 回执登记）；`send-files` = 一次调用即发一会话，并发闸门/队列泵删除（插件自控 `PENDING_SENDS`）；`current_concurrency`/`pump_*` 退役。门禁：桌面 v31 测试模式移植（`pty_session_chain` 式集成：真实插件闭环）。
- **票 07 · 接收侧事件桥 + 策略闸门**：`peer_receive.rs` 收敛为「询问回执表 + 接收事件桥（`peer:receive-event`）+ 策略闸门」（policy/timeout/download_dir 留原语参数）；ask 逐批放行经 `respond-transfer`（安全闸门留宿主）。
- **票 08 · 插件事件归约状态机**：file-transfer 插件以事件归约为唯一任务真源（建行/推进/终态/原因码映射/封顶/重试回放单点，真源在插件私有库 `transfer_store` 扩展）；历史/设置/注册表读面迁移。门禁：断点续传（redial）集成 + 终态历史持久化测试。
- **票 09 · 发现/设备列表投影下沉**：宿主 `peer-devices-changed` 快照指纹链路退役；插件 device_bridge 自建缓存（订阅 `<owner>::mdns:found|lost` + last-seen 持久化缓解首屏空窗）；共享目录注册表 CRUD → 插件 `host-plugin-database`；`list_discovered_peers`/`list_trusted_peers` 收窄（引擎事实原语 or 插件派生，按 host-peer 终态表）。
- **票 10 · 宿主命令面收口 + 防回接锁（done 2026-10-08）**：宿主命令面（`send_files_to_peer`/`list_peer_transfers`/`retry_*`/`resume_all_peer_transfers`/`clear_*`/`get_peer_receive_settings`/`set_peer_transfer_encryption`/`set_peer_transfer_concurrency` 等）注销/转薄转发；源码扫描锁 `retired_mobile_peer_transfer_orchestration_is_not_reintroduced`（移动版）+ 变异自检；`stale_artifact_rebuild_hint` 判据扩展点名新 ABI。**实际落地**：spec 点名的发送 / 列表 / 重试 / 清历史 / 批量恢复命令在票 06–08 已退役，本票摘掉的是最后11 个（传输调度4 + 接收设置 4 + 远端浏览 3）——票 09 + 10 之后移动端 `peer_net`/`peer_transfer`/`peer_receive`/`peer_remote` 四模块**前端命令面为零**，引擎原语保留并收 `pub(crate)`；`stale_artifact_rebuild_hint` 本票零 ABI 变更故无需扩展（票 06 的 `concurrency` 载荷 fail-visible 仍是唯一退役载荷判据）。详见 `ticket-10-command-face-retirement.md`。

### 阶段 3 —— 连接/会话/终端/自动任务业务下沉（移动端内置 app `com.bedcode.terminal-session`，D6 选项 A）
- **票 11 · host-websocket 客户端域（ABI 11→12，方案已出待实施 · 实为13→14；done 2026-10-08）**：WIT 新增 + SDK 绑定 + host_impl（客户端域：connect/send-text/send-binary/close/is-connected + 属主私有 topic `<owner>::ws:open|error|close`）+ 可选导出 events-ws（宿主动态探测）。门禁：桌面 host-websocket 客户端域测试移植（`ws_e2e` 式）。**实际落地**：5 函数真子集（服务端域不做，边界锁 `mobile_host_websocket_client_domain_lock` 变异自检 2/2）；下行帧改走既有 `events-binary` 导出 + 二进制属主 topic `<owner>:ws:message` 帧信封（与 spec 的 events-ws 偏差，零新导出，见 ADR 0041 D4）；消费须同时持 `bus` 权限位（集成测试实证）；引擎自建 `host_impl/ws.rs`（形态 A 用户裁决）、不做 wss、connect-timeout 上限 5s。门禁换用真实 WASM 组件全链路（component-test `ws-client` feature：connect → send-text → 对端回帧 → close 事件闭环）+ 宿主内联单测八类。migration §3.2 联动跳过（文档已不存在，差异事实以 WIT 注释 + ADR 0041 为真源）。
- **票 12 · 终端订阅协议客户端迁插件**：`terminal_link.rs`（1,363 行）整体迁入 `com.bedcode.terminal-session`（移动版 app，D6 选项 A；subscribe/ack/ring_resync/special-key 翻译/退避重连随迁）；插件经 host-websocket 客户端域连桌面 `/ws/plugin/com.bedcode.terminal-session/terminal`；宿主 `terminal_*` 命令面注销（frontend 走插件命令面）。**性能红线（C3）**：输出帧帧级直传，禁止逐帧 JSON 化（桌面 P3 探针口径复用）。
  - **2026-10-08 已实施（代码落盘）**：mobile ABI 14→15；新建 `plugins/terminal-session/`（`plugin.json` 7 命令 + 权限 `bus` / `terminal:output` / `ws:client`，`rust/src/{protocol,link,keys,commands,lib}.rs`）；宿主保留窄转发 `terminal_stream_gateway.rs`（页面 Channel 登记 + `forward_output` 零解析，ADR 0022 薄壳④）+ `host_impl/terminal_stream.rs`（WIT `forward-output` 权限门）；`terminal_link.rs` 整体退役；新锁 `retired_mobile_terminal_link_lock.rs`（4 例，**接手会话实测 4/4 通过**——原工作区残留一处变异注入 stub 使该锁转红，已清理，见下方状态校订）；`terminal_get_history` 删除不迁（零消费者）。§10 三个裁决点按推荐案落地：D-12a `jwt-auth` 宿主代发（`host_impl/ws.rs`，token 不落插件）/ D-12b `forward-output` + `connection.primary-target` / D-12c R1 宿主 auto-reconnect（`run_reconnect`）。**未完成的门禁**：插件 crate `cargo test` 当前编译红、wasm32 门禁与 cross-end-tests 未跑（理由见文首状态校订）
- **票 13 · 会话控制客户端迁插件**：`session/http.rs` + `commands/session.rs` 迁入插件（list/start/stop/remove/input 经插件自有 HTTP 面，对齐桌面 `sessions_http` 模式）；JWT 注入语义保持（认证链路只走既有 auth 模块）。
- **票 14 · 认证/配对编排下沉**：配对流程编排（QR/配对码/生物挑战 UI 流）迁插件；**设备身份文件 / JWT 持有 / AuthHttpClient / 生物凭证绑定留宿主引擎（安全边界 C4）**；`ws_request_pairing`/`ws_verify_pairing_code`/`ws_authenticate_with_qr` 等命令面随编排收窄。门禁：认证链路 fail-closed 不变（无宿主代签路径，对齐桌面 v33 后形态）。
  - **2026-10-08 阶段 A 已实施**（`ticket-14-auth-pairing-downsink.md`）：**宿主本地配对码编排面退役**——`connection/pairing_service.rs`（`PairingService`）+ `auth/pairing.rs`（`PairingCode` / `PendingDevice` / `PAIRING_CODE_TTL_SECS`）+ `system::commands` 配对码 4 命令 + `PAIRING_CODE_DIGITS` 整体删除（零消费者；B1/B2/B5 命中，且与桌面端颁发面无同步通道 = 双真源）；新锁 `retired_mobile_local_pairing_code_face_lock.rs`（4 例，变异自检 4/4）+ 反向断言钉住 C4 引擎面。零 ABI / WIT / 协议 / 前端 / 插件改动。
  - **阶段 B 阻塞已解除，2026-10-08 已实施且门禁全绿**（`ticket-14-auth-pairing-downsink.md` §9）：三条阻塞的解法——① 目标 app 由票 12 先行创建（`plugins/terminal-session/`），本票在其上追加 auth 域；② 新增 `host-auth` 认证引擎面 5 函数 + 权限位 `auth`（占 **ABI 15→16** 窗口，非原估的 14→15——v15 已被票 12 占用），**凭据零过境**（JWT 留宿主 global token + 凭据表，`host_impl/auth.rs` 权限门 fail-closed；前端持久化镜像经新增窄读命令 `ws_get_auth_credentials` 读引擎，不经插件）；③ 命令面随插件就位同批收窄（注销 5 编排命令 + 3 事件 helper，新增 1 窄读；`ws_authenticate` 与生物凭证绑定面保留）。流程事件名与载荷逐字一致 → 前端监听零改动。门禁：新锁 4/4 + 变异 3/3、阶段 A 锁回归 4/4、插件 native 40/40、**wasm32 SDK CLI 构建通过**、前端全量 680/680、宿主全量集成目标全绿（lib 6 失败 = egress 在途基线）。cross-end-tests 未跑（无跨端 wire 变更）、真机三流往返留票 21。
  - （原文保留存档）**阶段 B（活跃编排命令迁插件）曾阻塞**，三条硬阻塞见票文档 §5：① 目标 app `com.bedcode.terminal-session`（移动版）由票 12 创建，工作区实测未开工，本票自建会与票 12/16 争 `plugin.json` 所有权；② 插件触达 `/api/auth/*` 需新增 `host-auth`（占 ABI 14→15 窗口，与票 15 同窗）或复用 `host-http`（JWT 落插件，违反 C4 与 AGENTS §8「认证链路禁止旁路」）；③ 四命令有活跃前端消费者，插件未就位即注销 = 功能回退。**待用户裁决**：① 先跑票 12（推荐）② 本票接管 app 骨架 ③ 阶段 B 与票 15 合并进同一 ABI 窗口。
- **票 15 · 终端 UI 域下沉 + host-terminal/terminal-hooks 退役（ABI 12→13）**：`TerminalView` + `composables/terminal/*` + `terminalBuffer` store + 终端字体/主题随插件前端迁移；host-terminal（`send`）+ terminal-hooks 整面退役（旧产物实例化期点名重建）；`MobileEvent` 业务变体解释权随插件事件面（host-bus/host-events）迁移，handler/router 收窄为传输转发。
- **票 16 · auto-task 并入终端 wasm app（D6 已定案选项 A，阶段 3 收口票 · 2026-10-08 新增需求）**：`plugins/auto-task/` 整体并入 **`com.bedcode.terminal-session`**（移动版 app，与桌面同名）。
  - **随迁面**：`src/`（`AutoTaskToolboxView` / `AutoTaskPanelHost` / `ScheduledJobsTab` / `TaskHistoryTab` / `composables/{useScheduledJobs,useTaskHistory}` / `api.ts` / `state.ts` / `i18n.ts` / `panel.css` / `toolbox.css`）、`plugin.json`（权限位与贡献点取并集：`session:read`/`storage`/`ui:input`/`ui:toolbox`）、`rust/src/lib.rs`（极简壳，随合并退役）；按**域重组**目录，不逐文件平移。
  - **硬引用同批同步五处**：`src-tauri/src/lib.rs:203` fs_auth 可信白名单、`scripts/plugin-package-list.json`（mobile 列表删 `auto-task`）、`scripts/dev-run.js:177` dev 注册 id、`packages/plugin-sdk-mobile/dev-shell/src/mock/mobile-api.ts:129` mock 基址、`plugin-dev-mobile.md` 构建示例。
  - **HTTP 基址切换**：`api.ts` `/api/plugin/com.bedcode.auto-task` → `/api/plugin/com.bedcode.terminal-session`（消解 roadmap M1）；桌面 `LEGACY_HTTP_PLUGIN_ALIASES` **保留不动**（桌面改动 Out of scope），切断列双端同批待立项。
  - **退役与锁**：旧 id `com.bedcode.auto-task` 与视图 id `auto-task.toolbox` / 命令 id `auto-task.*` 退役，走 fail-visible 三形态 + 源码扫描防回接锁 + 变异自检（旁路 → 转红 → 还原）。
  - **门禁**：合并后 app `node scripts/plugin-build.js --rust-only`（wasm32 真门禁，native 绿 ≠ 可交付）+ 宿主 `cargo test` 全量 + 前端 `pnpm run test:run` 全量（i18n key 双语同步、前缀门禁）+ 根 eslint 0 error；真机验收：终端工具栏「自动任务」入口 + 工具箱「任务记录 / 定时任务」两页签 + 队列面板可用，ADR 0012 的 WS 刷新链路（`ws_sync_task_*` 三事件）不回归。
  - **2026-10-08 已实施（代码落盘）**：任务域按域重组进 `plugins/terminal-session/src/task/`（activate / api / i18n / devMock / components / composables），入口 `src/index.ts` 改为「域组合」形态；极简 rust 壳 + 4 条 `auto-task.*` 命令 + `contributes.lifecycle` 随迁即删（D6 强制①）；HTTP 基址切 `/api/plugin/com.bedcode.terminal-session`；旧插件目录与打包资源目录删除；五处硬引用同批（fs_auth 白名单 / 根插件清单 / dev 注册表 / dev-shell mock 基址 / 开发文档）+ CI 插件安装列表补 terminal-session；新锁 `retired_mobile_auto_task_plugin_lock.rs`（4 例）。宿主测试样本从 auto-task 换为合并后的 terminal-session（`build_terminal_session_component` + 生产权限集）。门禁明细见票文档 §5。

### 阶段 4 —— 机制复用 wasm-core（用户点名的核心件）
- **票 17 · 移动端 wasm-core 落地（依 D1 形态）**：fork 对齐（选项 B）→ `bedcode-mobile/packages/bedcode-wasm-core`（bindgen 换移动 WIT、删桌面域、host_api 12 域自持）；或机制核抽取（选项 A）一步到位；移动端 `plugin/` 11,640 行替换为 crate 引用 + 移动绑定层。门禁：移动端 cargo test 全量 + 插件三方案例回归（load/activate/deactivate/权限门）。
- **票 18 · host_api 实现层共享**：无 WIT 依赖域（storage/db/events/http/fs/config/log/bus 的实现层，若其不依赖桌面 bindgen 类型）抽共享；每域「实现层 + 各端 adapter」两层化。
- **票 19 · 防回接与漂移锁**：`mobile_parallel_copy_shape_lock` 扩展为全接口逐函数对照锁（SDK 层已有先例：`mobile_parallel_copy_shape_lock`）；wasm-core 双端对称结构锁（移动端 wasm-core 不得回接桌面域）；`crate_boundary_lock` 移动端登记。

### 阶段 5 —— 冻结与收口
- **票 20 · egress 授权策略对齐（D5）**：三档形态 + 授权记录（对齐桌面 2026-09-28 节），宿主内收口，落防回接锁（档位→动作映射单点）。
- **票 21 · 全量验收 + 文档**：跨端协议回归（`cross-end-tests`，双端真实互连）；AGENTS.md §5.4 双端差异节改写（移动端不再「自持业务 App」而「与桌面同构薄壳」）；双端 code-map、`docs/knowledge/mobile-desktop-auth.md`、CHANGELOG 双语；移动端 `docs/implementation-plans/mobile-wasmtime-component-migration.md` §3.2 差异表更新。

---

## 6. 验证门禁（每票通用，AGENTS §10 两段式）

- 开发中：只跑针对性单测自验（移动端 `cargo test` 过滤、vitest 端目录执行），红了立即修。
- 收尾：移动端 `cargo test` 全量 + `pnpm run test:run` 全量 + 根 `pnpm exec eslint .` 0 error + `cargo fmt`/`clippy` 自查；wasm 插件在各自 crate 根 `cargo test`（**禁拿宿主 cargo test 当插件验证**）。
- 跨端协议改动（票 04/11/12/14/16 —— 后者切换桌面任务域 HTTP 前缀）：`cross-end-tests` 全量；两端 mock 各自自洽 + 真实互连为契约门禁。
- 每批破坏性 ABI 变更：`stale_artifact_rebuild_hint` 判据扩展 + 旧产物实例化期点名（fail-visible 三形态②）+ 退役词汇/权限位加载即抛（③）。
- 真源搬迁：旧读路径删除或显性报错（①），禁止「查不到就返回空」。
- 移动端 wasm 插件 Rust 改动：**native cargo test 绿 ≠ 可交付**（`#[cfg(target_arch="wasm32")]` 代码 native 不覆盖）——必须 `node scripts/build.js --rust-only` 验证（2026-10-06 教训）。

## 7. 风险与回退

| 风险 | 吸收 |
| --- | --- |
| 阶段 3 是最高风险段：终端消费进 WASM 的每帧开销（C3） | 先做票 11 host-websocket 客户端域 + 只读探针（复用桌面 P1/P2/P3 方法论：真实插件路径 ~40µs/op 量级核算），超限回退为「终端保留宿主稳定面、只开放扩展点」（businessless-kernel-vision Further Notes 同款回退） |
| 移动端业务插件化后「开箱即用」依赖插件激活 | 内置插件默认启用 + 宿主命令面未激活显性报错（对齐桌面「插件未激活时前端命令面显性报错」）；每批 expand–contract 排程可单步回退 |
| 机制核抽取（选项 A）破坏桌面零回归 | 桌面 ABI/WIT/world 一个字节不动（同 ADR 0037 口径）；抽取面 = 纯搬移 + 保 facade，crate 边界锁先行 |
| 双份漂移（选项 B fork 面） | 票 19 对照锁与 fork 同步落地；共享核抽取（票 18）逐步收缩 fork 面 |
| **票 16 合并风险**：桌面切断 `com.bedcode.auto-task` 旧 HTTP 前缀 ⇒ 移动端任务面全 404（roadmap M1） | 本专项只切移动端基址、`LEGACY_HTTP_PLUGIN_ALIASES` **保留**（§8 Out of scope）；切断列双端同批待立项，票 21 文档点名；真机验收含队列/历史/定时三面 |
| **票 16 合并风险**：app 体积与权限面膨胀（权限位取并集） | manifest 权限位逐项给理由、未用到的贡献点随迁即删；权限四同步点同批；合并后 app 仍是「内置默认启用」单一实例，不新增运行期开销 |
| 移动端插件生态薄（3 家），迁移牵动面小但要求同批 | 每票列出受影响插件调用点清单，迁移放宽不放门禁（桌面 v31 教训） |
| 认证编排下沉误伤安全边界 | 硬约束 C4：JWT/身份/凭据/认证客户端/egress 留宿主；下沉只动「UI 编排与派生视图」，fail-closed 语义逐字保留，有防回接锁 |

## 8. Out of scope（本专项不做）

- 桌面端任何改动（含桌面独有接口、认证中心、PTY/WS 服务端引擎）——本专项只动移动端 + 根 packages 共享机制的抽取动作（后者不动桌面行为）。
- 移动端「完整」第三方插件生态建设（能力面按需开，不预设）。
- wasm-core / 机制核发布为 crates.io 包（仓库内 path 依赖形态，发布是产品决策，ADR 0037 Out of scope 延续）。
- wasmtime / SDK 版本升级（双端已 48，ADR 0019；本专项不升级）。
- 移动端 Kotlin 原生层改造（Android 原生插件桥维持现状，只随 host-platform/host-fs 暴露面走）。
- **桌面侧切断 `com.bedcode.auto-task` 旧 HTTP 前缀 / 删 `LEGACY_HTTP_PLUGIN_ALIASES`** —— 票 16 只改移动端基址并保留别名表；切断须双端同批（ADR 0019 双端同步评估口径），另立项。

## 9. 文档联动（每批落地同步，不许滞后）

- `docs/adr/`：票 01 新增「移动端 wasm-core 复用形态」ADR + 修订 ADR 0037 D1 / ADR 0035 D6 关联表述；每批下沉按 ADR 0022 追加批次条目（移动端小节）；**票 16 追加**：ADR 0018 双端偏离表登记「`com.bedcode.terminal-session` 两端同名不同职责」（C8）+ ADR 0022 移动端批次条目补「auto-task 并入终端 app」（B1–B6 零命中）；ADR 0012 补「移动端视图已随 app 合并改 id」。
- `docs/knowledge/`：businessless-kernel-vision / plugin-kernel-roadmap 补「移动端实施」状态段；mobile-desktop-auth.md 随票 12/14 更新；**票 16 需更新**：`plugin-kernel-roadmap.md`「移动端受影响清单」M1 状态（基址已切 / 别名表待双端同批切断）、`docs/diagrams/plugin-auto-task-mobile.html`（桌面插件节点标注 `/api/plugin/com.bedcode.auto-task/…` → 新前缀）、ADR 0012 补「移动端 auto-task 已并入终端 app，视图/命令 id 变更」条目。
- 双端 `docs/code-map.md`、AGENTS.md §5.4、CHANGELOG 双语（移动端与桌面同步维护版本号规则不变，但移动端 APK 是否随版本重发按发布流程另议）。
