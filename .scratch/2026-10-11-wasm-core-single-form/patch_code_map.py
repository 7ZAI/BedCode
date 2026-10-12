#!/usr/bin/env python3
"""票 05 code-map 事实同步（移动端）：只改本轮改动使其失真的表述，整体改写留 T-06。"""
import pathlib
import sys

P = pathlib.Path("bedcode-mobile/docs/code-map.md")
s = P.read_text()

REPL = [
    # 1) 终端链路小节标题 + 网关落点
    (
        "### 终端链路（协议客户端与消费 UI 皆在插件 · 窄转发在宿主，票 12 + 票 15 阶段 A）— `wasm-apps/terminal-session/` + `src-tauri/src/terminal_stream_gateway.rs`",
        "### 终端链路（协议客户端与消费 UI 皆在插件 · 窄转发在宿主，票 12 + 票 15 阶段 A）— `wasm-apps/terminal-session/` + `src-tauri/src/plugin/terminal_stream.rs`",
    ),
    (
        """- **宿主侧（零解析窄转发，ADR 0022 四类薄壳④）**：`terminal_stream_gateway.rs` 页面
  Channel 表 + 转发（`terminal_page_subscribe/unsubscribe` 命令是 Tauri 传输机制登记面）；
  WIT `host-terminal-stream.forward-output`（`host_impl/terminal_stream.rs`，权限
  `terminal:output`）把插件交来的裸字节按 session-id 推给前端（`InvokeResponseBody::Raw`，
  全程零 JSON——C3）；`host_impl/connection.rs` 的 `host-connection.primary-target`
  （主连接目标事实，票 13 复用）""",
        """- **宿主侧（零解析窄转发，ADR 0022 四类薄壳④）**：`plugin/terminal_stream.rs` 同处两段——
  页面 Channel 表 + 转发（`terminal_page_subscribe/unsubscribe` 命令是 Tauri 传输机制登记面；
  该表票 05 前住 wasm-core `terminal_stream_gateway.rs`，随域迁入宿主）；WIT
  `host-terminal-stream.forward-output`（同文件，权限 `terminal:output`）把插件交来的裸字节
  按 session-id 推给前端（`InvokeResponseBody::Raw`，全程零 JSON——C3）；
  `plugin/connection.rs` 的 `host-connection.primary-target`（主连接目标事实，票 13 复用）""",
    ),
    # 2) host-websocket 薄适配器落点
    (
        """  `WsClientPorts` 注入）；移动端只剩薄适配器
  `packages/bedcode-wasm-core/src/manager/runtime/host_impl/ws.rs`（5 原语转发 +""",
        """  `WsClientPorts` 注入）；移动端只剩薄适配器
  `bedcode-mobile/src-tauri/src/plugin/host_domains/ws.rs`（票 05 绑定归位迁入；5 原语转发 +""",
    ),
    # 3) ws 全链路用例落点
    (
        """- 真实 WASM 组件全链路集成测试：`component.rs` tests 的 `ws_client_domain_full_loop_with_real_component`
  （component-test 插件 `ws-client` feature：connect → send-text → 对端回帧 → close 事件闭环）""",
        """- 真实 WASM 组件全链路集成测试：`src-tauri/tests/ws_client_e2e.rs` 的
  `ws_client_domain_full_loop_with_real_component`（票 05 自内核 `component.rs` tests 迁入：
  component-test 插件 `ws-client` feature：connect → send-text → 对端回帧 → close 事件闭环）""",
    ),
    # 4) wasm-core 依赖形态 + bare 面分层
    (
        """**插件机制整核（单一 wasm-core，仓库根 crate）— `packages/bedcode-wasm-core/`：**
双端共用单一 crate（ADR 0045 D1；移动 fork crate 已随票 06 批次 04 退役，目录删除）。
移动形态依赖形态（`src-tauri/Cargo.toml`）：
`bedcode-wasm-core-mobile = { package = "bedcode-wasm-core", path = "../../packages/bedcode-wasm-core",
default-features = false, features = ["bare-host"] }`——package rename 别名，源码
`bedcode_wasm_core_mobile::` 导入路径零改动。`bare-host` 面（2026-10-10
`wasm-core-mobile-purity` 票 02–05 收口后，ADR 0047）分四层：

- **机制核**（bus / config / db / monitor / permission / security / storage /
  `terminal_stream_gateway.rs`）：与 `common-host` 面**同一份源码**；
- **运行时与装配**（`manager/runtime/bare/**`）：bindgen 绑移动 WIT 生成物目录 `wit/`
  〔core.wit + cap-mobile.wit + `bedcode.wit` 分片拼装〕；`component.rs` 的 linker 装配 =
  双端共有机制域的**本端壳** + 移动独有域的 **cfg(test) 替身**（生产装配在宿主 adapter，
  见下节 `plugin/bindings.rs`）；
- **共有机制域壳**（`manager/runtime/bare/host_impl/`，11 域 + `support.rs`）：**纯转发 /
  端口接线**，域逻辑在共享 crate——peer → `bedcode-server-peer-net::plugin_binding`（票 03）、
  http → `bedcode-server-http::plugin_binding::egress`（票 05）、fs →
  `bedcode-host-api-core::fs`（票 05）、db·storage·bus·events·log → `bedcode-host-api-core`；
  Android 等平台差经端口注入（`FsHostPorts::platform_delete` 一类）；壳行数由纯净性锁封顶；
- **端口与适配**：`host_api/ports.rs`（`HostEnginePorts`，宿主引擎调用入口）+
  `host_api/sql_guard.rs`（SQL 表名前缀护栏）+ `host_api/bare_context.rs` +
  `test_support`（夹具构建器 + MockPorts，`any(test, feature = "test-support")` 门控）。

**防回接**：`src-tauri/tests/retired_mobile_wasm_core_fork_lock.rs`（fork 目录 / 包名 /
路径字形 / 别名指向四判据）+ `src-tauri/tests/mobile_wasm_core_purity_lock.rs`
（移动独有域不回流 / 壳不含域逻辑 / 共享核在场 / 行数上限 / 退役引擎不复活；见锁索引）；""",
        """**插件机制整核（单一 wasm-core，仓库根 crate）— `packages/bedcode-wasm-core/`：**
双端共用单一 crate（ADR 0045 D1；移动 fork crate 已随票 06 批次 04 退役，目录删除）。
移动形态依赖形态（`src-tauri/Cargo.toml`）：**直连**单 crate
`bedcode-wasm-core = { path = "../../packages/bedcode-wasm-core", default-features = false,
features = ["bare-host"] }`（票 05 收口：package rename 别名删除，源码统一 `bedcode_wasm_core::`）。
`bare-host` 面（2026-10-11/12 `wasm-core-single-form` 票 05 绑定归位后）分三层：

- **机制核**（bus / config / db / monitor / permission / security / storage /
  `manager::runtime::engine` 组件引擎唯一一份）：与 `common-host` 面**同一份源码**；
- **移动壳**（`manager/runtime/bare.rs` + `bare/component.rs`，合计 ~630 行、零 WIT 绑定）：
  Engine/Linker/Store 生命周期 + 导出调用骨架——linker = **宿主自报能力模块**
  （`inventory`；core 12 域 + 移动独有 5 域全在宿主，见下节）+ WASI p2/p3；
  `command` / `lifecycle` / `manifest` 走内核 core 交集绑定
  （`manager/runtime/core_binding.rs`，绑契约真源 `wit/core.wit`）、`abi.version` 走原生探测、
  `events.*` 走宿主注入的 `GuestEventDispatcher`。**12 个 core 域不再有内核实现**——
  移动权限门（`granted_permissions` 词表）与插件私有库布局（`plugins/<id>.db`）都与桌面不同源；
- **端口与测试基建**：`host_api/ports.rs`（`HostEnginePorts`，宿主引擎调用入口）+
  `test_support`（夹具构建器 + MockPorts + 组件实例转发面，`any(test, feature = "test-support")`
  门控）。`host_api/sql_guard.rs` 与 `terminal_stream_gateway.rs` 均已退役/迁出（票 05）。

**防回接**：`src-tauri/tests/retired_mobile_wasm_core_fork_lock.rs`（直连 / 禁别名键 /
路径字形 / 标识符四判据）+ `src-tauri/tests/mobile_wasm_core_purity_lock.rs`
（移动独有域不回流 / core 域绑定与实现不回流 / 共享核在场 / 壳行数上限 /
移动独有机制与退役垫片不回流 / 退役引擎不复活，6 例；见锁索引）；""",
    ),
    # 5) 宿主 plugin/ 段：新增 host_domains + guest_events；host_ports 描述；wasm_host 退役
    (
        """- **移动独有域 adapter**（票 04 落点，自带 `impl Host` + 域逻辑 + android 桥）：""",
        """- **core 12 域 adapter**（票 05 绑定归位落点 `host_domains/`，14 文件 /
  ~2.5k 行）：`mod.rs`（模块描述符 `mobile-host-core` + 12 interface 注册 + 停用回收聚合）
  + 每域「域逻辑 + `impl Host`」同处（`log` / `storage` / `db` / `event` / `http` / `fs` /
  `config` / `bus` / `peer` / `mdns` / `platform` / `ws` / `support`）。域逻辑是共享核的
  薄转发（`bedcode-host-api-core` / `bedcode-server-http` / `bedcode-server-peer-net` /
  `bedcode-ws-client-engine` / `bedcode-discovery-engine`）；**权限门读实例级
  `granted_permissions`**（桌面 `core_binding` 的 `PermissionManager` 词表不含移动词汇，
  不能共用那份实现）；插件私有库按移动布局 `plugins/<id 去点划>.db`（非内核
  `plugins/<plugin_id>/plugin.db`）
- **guest_events.rs**：移动 `events` 导出派发器（`on-bus-message` / `on-auth-success` /
  `on-disconnect` / `on-session-*`）——内核只留事件循环机制，调哪个导出由宿主绑定实现
  （ADR 0022「机制留内核、契约面上移」）；生产在 `PluginManager::init_wasm_runtime_with` 注入
- **移动独有域 adapter**（票 04 落点，自带 `impl Host` + 域逻辑 + android 桥）：""",
    ),
    (
        """- **端口接线**：`host_ports.rs`（`HostEnginePorts` 宿主引擎端口实现，含 egress_check /
  global_token 等）/ `peer_ports.rs`（`PeerPorts`，票 03）
- **wasm_host**：SQL 表名前缀校验与列类型转换（原宿主 `wasm_host.rs` 的 SQL 半部；
  HTTP 执行半部已随票 05 上移能力域 crate `bedcode-server-http`）""",
        """- **端口接线**：`host_ports.rs`（`HostEnginePorts` 宿主引擎端口实现，含 egress_check /
  global_token 等）/ `peer_ports.rs`（`PeerPorts`，票 03）
- **`wasm_host` 垫片已退役**（票 05）：它只 re-export 内核 `host_api::sql_guard`
  （单条 `column_to_json`）且两侧零消费者——域实现直连共享核。HTTP 执行半部早已上移
  能力域 crate `bedcode-server-http`""",
    ),
    (
        """- **message_bus / fs_auth / types / commands**：插件间消息总线、文件系统校验、类型定义、Tauri 命令""",
        """- **message_bus / types / commands**：插件间消息总线、类型定义、Tauri 命令
  （`fs_auth` 宿主自实现已随 2026-10-11 裁决退役：fs 授权统一走内核
  `security::fs_auth::FsAuthChecker` 判定链）""",
    ),
    # 6) 绑定层落点（生成物 path）
    (
        """移动 fork 的 bindgen（`packages/bedcode-wasm-core/src/manager/runtime/component.rs`）path 指
`../plugin-sdk-mobile/rust/wit` 目录（push_dir 加载合成 package）。""",
        """移动端 bindgen 在**宿主**：`bedcode-mobile/src-tauri/src/plugin/bindings.rs` 的 path 指
`../packages/plugin-sdk-mobile/rust/wit` 目录（push_dir 加载合成 package）——票 05 前该 bindgen
住内核 `manager/runtime/bare/component.rs`，随绑定归位迁出（内核零 WIT 绑定）。""",
    ),
    (
        """旧产物（v19）import `host-fs.write-media-downloads` 等在实例化期被拒，
`stale_artifact_rebuild_hint` 点名按 v20 SDK 重建。""",
        """旧产物（v19）import `host-fs.write-media-downloads` 等在实例化期被拒，
`stale_artifact_rebuild_hint` 点名按 v20 SDK 重建。票 05 起 `host-fs-mobile` 的
`impl Host` 在 `src-tauri/src/plugin/fs_mobile.rs`（宿主自报 `mobile-fs` 模块）。""",
    ),
    # 7) 典型任务入口
    (
        """**典型任务入口：** 新增宿主能力 → `wasm_runtime/host_impl/` + SDK `rust/src/host/` 对应域 trait；""",
        """**典型任务入口：** 新增宿主能力 → `src-tauri/src/plugin/host_domains/`（core 域）或
`src-tauri/src/plugin/<domain>.rs`（移动独有域）+ SDK `rust/src/host/` 对应域 trait；""",
    ),
    # 8) peer 域落点
    (
        """  共享根 / 浏览 / 拉取 / 节点启停 / active-transfers / collect-outgoing），宿主实现在
  wasm-core `bare-host` 面 `manager/runtime/bare/host_impl/peer.rs`（ADR 0022 v3）；""",
        """  共享根 / 浏览 / 拉取 / 节点启停 / active-transfers / collect-outgoing），宿主实现在
  `src-tauri/src/plugin/host_domains/peer.rs`（票 05 归位；薄转发到能力域 crate，ADR 0022 v3）；""",
    ),
    # 9) 锁索引条目
    (
        """- **wasm-core 纯净性防回接**（票 2026-10-10 `wasm-core-mobile-purity` 票 06-①；ADR 0047 D4/D5）：移动独有域不得回流 wasm-core（5 个域文件不得复现 + 生产装配段不得出现其 `Host` impl）/ 形态壳不得含域逻辑（http 执行本体、fs 系统调用、peer 时代句柄表与重拨符号）/ 三条共享实现核必须在场（`server-http::plugin_binding::egress` · `host-api-core::fs` · `server-peer-net::plugin_binding`）/ 壳文件与目录行数双重上限 / 已退役的 `host_api/http_engine.rs` 不得复活 → `src-tauri/tests/mobile_wasm_core_purity_lock.rs`（6 例；变异自检 **4/4**）。**改 wasm-core `runtime/bare/host_impl/*` 或移动 `src/plugin/{notify,auth,terminal_stream,connection,fs_mobile}.rs` 前先看它**""",
        """- **wasm-core 纯净性防回接**（票 2026-10-10 `wasm-core-mobile-purity` 票 06-①；**票 2026-10-11/12 `wasm-core-single-form` 票 05 按绑定归位终态重写**；ADR 0047 D4/D5）：① 移动独有域不得回流（5 域 `Host` impl / 域文件零命中，正向钉住宿主 adapter 在场）② core 12 域的绑定与实现不得回流（内核唯一合法落点 = `manager/runtime/core_binding.rs`；移动壳禁 `bindgen!` 与 `bedcode::plugin::` 路径，正向钉住宿主 `bindings.rs` / `guest_events.rs` / `host_domains/` 在场）③ 共享实现核必须在场（`server-http::plugin_binding::egress` · `host-api-core::{fs,storage,bus}`）且宿主 adapter 真的转发到它们 ④ 移动壳行数上限（单文件 600 / 合计 800）⑤ 移动独有机制与退役垫片不回流（终端网关住宿主 `plugin/terminal_stream.rs`；零消费者 `sql_guard` 垫片退役）⑥ 已退役的 `host_api/http_engine.rs` 不得复活 → `src-tauri/tests/mobile_wasm_core_purity_lock.rs`（6 例；变异自检 **4/4**）。**改 wasm-core `runtime/bare*` 或移动 `src/plugin/{host_domains/**,bindings,guest_events}.rs` 前先看它**""",
    ),
]

missing = []
for a, b in REPL:
    if a not in s:
        missing.append(a.splitlines()[0][:80])
    else:
        s = s.replace(a, b, 1)

if missing:
    print("MISSING ANCHORS:")
    for m in missing:
        print("  -", m)
    sys.exit(1)

# 10) 交互面表格行（终端链路）
s = s.replace(
    "| 终端链路（协议客户端在插件 `com.bedcode.terminal-session`；宿主窄转发面 `terminal_stream_gateway.rs`） | `wasm-apps/terminal-session/` + `src-tauri/src/terminal_stream_gateway.rs` |",
    "| 终端链路（协议客户端在插件 `com.bedcode.terminal-session`；宿主窄转发面 `plugin/terminal_stream.rs`） | `wasm-apps/terminal-session/` + `src-tauri/src/plugin/terminal_stream.rs` |",
)
# 11) 锁描述里的 host_impl 引用
s = s.replace(
    "  `auth_engine_projection_and_plugin_domain_stays` 反向钉住：引擎方法 + `host_impl/auth.rs` 投影 +",
    "  `auth_engine_projection_and_plugin_domain_stays` 反向钉住：引擎方法 + `plugin/auth.rs` 投影 +",
)
s = s.replace(
    "  `terminal:output` 权限位 + `terminal_stream_gateway` 窄转发 + `primary-target` 在场）",
    "  `terminal:output` 权限位 + `plugin/terminal_stream.rs` 窄转发 + `primary-target` 在场）",
)
s = s.replace(
    "路径字形 / 别名指向四判据）+ `src-tauri/tests/mobile_wasm_core_purity_lock.rs`\n（移动独有域不回流 / 壳不含域逻辑 / 共享核在场 / 行数上限 / 退役引擎不复活；见锁索引）；",
    "直连 / 禁别名键 / 标识符四判据）+ `src-tauri/tests/mobile_wasm_core_purity_lock.rs`\n（六判据见锁索引）；",
)
P.write_text(s)
print("code-map 已同步；剩余旧路径命中：")
for i, l in enumerate(s.splitlines(), 1):
    if "host_impl" in l or "terminal_stream_gateway.rs" in l:
        print(f"{i}: {l}")
