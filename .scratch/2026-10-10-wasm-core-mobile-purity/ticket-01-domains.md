# 票 01 核验产出 · 移动形态 16 域分类与迁移面（2026-10-10）

> 用途：`spec.md` 票 04（移动独有域落移动宿主）与票 05（收窄/下沉）的输入。
> 证据行号基于 **2026-10-10 工作区快照**（在途改动前）；票 02 改名后需重扫一次行号。
> 范围：`packages/bedcode-wasm-core/src/manager/runtime/mobile/`（`host_impl.rs` 模块清单 + `component.rs` 接线 + `host_impl/*.rs`）。
> 分类：① 纯转发（值映射 + 权限门 + 端口调用）② 含域逻辑（状态机 / 句柄表 / 载荷构造 / 重拨 / 队列）③ 移动独有 interface；括号内为**正交属性**（④ 有共享 crate 对应物）。

## 0. 模块清单核对（三处与「17 interface 名清单」不符）

| 名义清单 | 实际落点 |
| --- | --- |
| `log` | **无 `host_impl/log.rs`**：`component.rs:39–61` 直调 `bedcode-host-api-core::log::log_{info,debug,warn,error}` ⇒ 非移动独有，票 05 无收窄对象 |
| `fs_mobile` | **无独立文件**：2 函数在 `host_impl/fs.rs:230–365`，接线 `component.rs:166–185` |
| `events`(5) | **无 host_impl 文件**：export 面在 `component.rs:788–896`（`call_on_bus_message` / `call_lifecycle_event`），SDK 实现 `bedcode-mobile/packages/plugin-sdk-mobile/rust/src/wasm.rs:240–320` |
| `event` | 文件名是 `event.rs`（单数）= `host-events` 的 `emit` |

**状态底座**：`WasmPluginState`（`mobile.rs:121–135`）= `plugin_id` / `host_ctx` / `runtime_handle` / `granted_permissions` / `on_message_binary`；`WasmHostContext`（`mobile_context.rs:20–44`）= `db` / `storage` / `app_handle` / `fs_auth` / `message_bus` / `status_reporter` / `plugin_dbs` / `ports`。

**聚合入口**（不在 16 文件内但同属迁移面）：`host_impl.rs:56–64` `purge_for_plugin` = `mdns::purge_for_plugin` + `ws::purge_for_plugin` + `terminal_stream_gateway().clear_all()`，唯一调用点 `bedcode-mobile/src-tauri/src/plugin/manager.rs:827`（`deactivate_inner`）。

## 1. 逐域表

| 域（文件:行数） | 职责 | 分类 | 状态 / 端口 / support 依赖 | 现有锁命中（文件:行） | 迁移衔接点（host_ports / Android·SAF） |
| --- | --- | --- | --- | --- | --- |
| `auth.rs:127` | `host-auth` 5 原语 → 认证引擎（凭据零过境） | ③+① | `granted_permissions`(`auth`, :20–24)；`ports.auth_engine()`(:37)；`runtime_handle`+`block_on_async`(:54)；**不调 support** | `retired_mobile_auth_orchestration_command_face_lock.rs:64–65`（钉 `auth_verify_pairing_code(` / `auth_has_credentials(`） | `host_ports.rs:32–60` `AuthEnginePort`、`:172` `auth_engine()`；无 SAF |
| `bus.rs:245` | 总线 5 原语 → 共享核门禁链 | ①（④`host-api-core::bus`） | `granted_permissions` 经 `StateBusPorts`(:34–52)；`host_ctx.message_bus`(:93,106–110,127)；`support::guarded_host_call`(:89,104,123) | 无 | 无端口（总线本体在 crate `bus.rs`）；无 SAF |
| `config.rs:60` | `ConfigKey` 白名单取值（时间 / 下载目录） | ①（移动键集独有，**无共享核**） | **无权限门**；`host_ctx.app_handle`(:41)；`ports.resolve_downloads_dir`(:53)；`support`(:45) | 无（`bedcode-server-base/src/constants.rs:14` 仅注释） | `host_ports.rs:358` → `android_plugins::resolve_app_downloads_dir`（Android） |
| `connection.rs:109` | `primary-target` 引擎事实 → camelCase JSON | ③+① | **无权限门**；`ports.connection_engine()`(:18)；`block_on_async`(:20)；78 行单测 | `retired_mobile_terminal_link_lock.rs:103–104`；`retired_mobile_host_terminal_hooks_lock.rs:83–84` | `host_ports.rs:64–70` `ConnectionEnginePort`、`:182`；无 SAF |
| `db.rs:282` | 插件私有库 5 原语（属主分区） | ②（连接表句柄缓存）+④`core_db` | `granted_permissions`(`storage`,:16–25)；`host_ctx.plugin_dbs`(:34–64)；`app_handle`(:41)；`ports.app_data_dir`(:47) | 无 | `host_ports.rs:377` → `peer_net::app_data_dir`（Android 私有目录，间接） |
| `event.rs:29` | `host-events.emit` 严格解析后前端广播 | ①（④core events 解析） | **无权限门**；`host_ctx.app_handle`(:24)；无 ports / support | 无（`bedcode-host-api-core/src/events.rs:15` 注释钉两端各自保留） | 无端口方法（`app.emit` 前端通道 ⇒ 迁移即移动宿主 emit 面） |
| `fs.rs:365` | `host-fs` 交集 6 + `host-fs-mobile` 2 | ②（fs_auth 链 / 双平台分支 / 落点守卫） | `granted_permissions`(`fs:read`/`fs:write`)；`host_ctx.fs_auth.check/check_batch`；`ports.delete_file_android`(:213) / `is_within_app_downloads_dir`(:255,325) / `saf_io`(:265,335)；`support` | 无（锁只禁内核 `host_fs_mobile` 回流：`wasm-core/src/lib.rs:493`） | `host_ports.rs:364/368/372` → `android_plugins` + `saf_io::SafIoState`；**强 SAF / MediaStore** |
| `http.rs:210` | `fetch` + egress 闸门 + 流式分支 | ②（streamId / streamEvent / spawn + egress 前置） | `granted_permissions`(`network:http`,:21–26)；`ports.egress_check`(:112–118)；`app_handle`；`host_api::http_engine::execute_*`；`support`(:79) | 无直锁；同域真源 `retired_mobile_session_control_face_lock.rs:122–123`；`egress_tier_mapping_single_point_lock.rs:76–85` | `host_ports.rs:143` `egress_check`、`:166` `egress_redirect_policy`；无 SAF |
| `mdns.rs:302` | 5 原语薄转发 + 宿主登记 + purge（含 ~120 行测试） | ①（④`bedcode-discovery-engine`） | `granted_permissions`(:58–59)；`message_bus`(:52)；`ports.current_node_id`(:94)；`app_handle`(:56) | 间接 `peer_engine_consolidation_lock.rs:84` | 无端口方法（引擎在共享 crate）；`host_impl.rs:44` re-export 登记原语；无 SAF |
| `notify.rs:291` | 通知 / 震动 / 声音 5 原语（Android only） | ③+① | `granted_permissions`(`notify`,:24–29)；`ports.notify_*`；`support`（android 分支）；域内钳制(:20–21,141) | 无 | `host_ports.rs:387–507` → `android_plugins::notification_plugin_handle`（Kotlin TaskNotificationPlugin） |
| `peer.rs:510` | 19 原语 + 进程级 session 表 + 自动重拨 | ②（`PEER_HANDLES` / `with_auto_redial`）+④`bedcode-server-peer-net` | `granted_permissions`(`peer`,:19–28)；21 个 `peer_*` 端口；`app_handle`；`support`+`block_on_async` | `peer_engine_consolidation_lock.rs`；`retired_mobile_send_orchestration_lock.rs:14`（钉拒 `concurrency`）；`retired_mobile_peer_transfer_command_face_lock.rs:29–35` | `host_ports.rs:208–346`；SAF 树 URI 入口 `:246–257` |
| `platform.rs:48` | `pick-files` / `pick-folder` | ①（pick-folder 为 SAF 树 URI 的③形态） | **无权限门**；`ports.platform_pick_files`(:13) / `platform_pick_shared_directory`(:26)；`app_handle`(:30–37)；`support`+`block_on_async` | 无 | `host_ports.rs:350` → `peer_transfer::peer_pick_files`；`:354` → `android_plugins::pick_shared_directory_android` |
| `storage.rs:181` | KV 3 原语 adapter | ①（④`host-api-core::storage`） | `granted_permissions`(`storage`)；`host_ctx.storage`(:37,50,67)；`support`；`runtime_handle` | 无（`plugin_storage_db_backed_lock.rs` 锁宿主侧落盘，非本文件） | 无端口（存储本体在 crate） |
| `terminal_stream.rs:122` | `forward-output` 裸字节窄转发 | ③+①（网关在 crate 内） | `granted_permissions`(`terminal:output`,:30–35)；仅 `plugin_id`；`crate::terminal_stream_gateway()`(:39)；无 support | `retired_mobile_terminal_link_lock.rs:83–84`；`retired_mobile_host_terminal_hooks_lock.rs:99–101` | 网关 = `wasm-core/src/terminal_stream_gateway.rs`（lib.rs:92 迁入）；Tauri 命令 / Channel 面在移动 `lib.rs` ⇒ **迁移须两处同步** |
| `ws.rs:333` | 客户端 5 原语薄转发 + 端口适配 | ①（④`bedcode-ws-client-engine`） | `granted_permissions`(`ws:client`,:66–68)；`message_bus`(:63,146,151)；`ports.global_token`/`reconnect_bounds`/`reconnect_policy`(:119,124,133)；`support`+`block_on_async`+`spawn_with_error_boundary_on`(:156–162) | `mobile_host_websocket_client_domain_lock.rs:90`（禁 `ws:server`）；`retired_mobile_terminal_link_lock.rs:88–89` | `host_ports.rs:176` `global_token`、`:188` `reconnect_policy`、`:195` `reconnect_bounds`；无 SAF |
| `support.rs:33` | 共享 panic 守卫 `guarded_host_call`(:15–33) | 基础设施（非域） | 入参 `plugin_id`+`host_fn`；**被 auth 以外 10 域调用** | 无 | 跨域共用 ⇒ **迁移原子件**（不能漏迁） |

## 2. 逐域要点（影响迁移方案的部分）

- **auth**：唯一「锁钉死文件路径 + 符号名」的域之一；`block_on_async` 走 `mobile.rs:324`（非 support）⇒ 该 helper 与其重入守卫必须随域走，否则 host fn panic 面失控。
- **bus**：`authorize_api_call` 恒 `true`（移动无 host-api-call，:40–43）；订阅簿 / 队列留 crate `bus.rs`。
- **config**：无权限门、`ConfigKey` 仅 2 变体（移动独有）⇒ 桌面 `host_api/config.rs` 不构成共享实现。
- **connection**：78/109 行是单测；无权限门是 ADR 0022 明示例外（`cap-mobile.wit:123–126` 注释）。
- **db**：连接表生命周期靠 `host_ctx` drop 自动回收（:29–32 注释）⇒ 若 `WasmHostContext` 不随迁，回收链会断。
- **event**：3 行实质逻辑；移动侧「非法 JSON 降级投递」已对齐为拒绝（:15–21）。
- **fs**：唯一同时依赖 `fs_auth` **和** SAF 桥的域；`delete` 的 android 分支在 `#[cfg(target_os = "android")]` 内直接调端口（:199–218）。
- **http**：egress 无 `app_handle` 即 fail-closed（:104–111）；流式分支 `tokio::spawn` 挂在宿主 handle 上（:54）。
- **mdns**：本文件零 `ServiceDaemon::new()`（防双守护红线，:13–17）；5 个能力路由恒 `None`（移动无 core-plugin-manager）。
- **notify**：非 Android 恒 `Err("only supported on Android")`，且权限门在平台分支**之前**（单测 :238–249 钉死序）。
- **peer**：`PEER_HANDLES` 是**进程级 static**（:90–91）⇒ 迁移动宿主可继承，但两形态同进程并存会串表；`with_auto_redial` 以记忆 endpoint 重拨（:140–158）。
- **platform**：48 行里 30 行是 `require_app` / `run` 模板；`pick_folder` 必须 SAF 树 URI（真实路径会被引擎整批拒，:18–23）。
- **storage**：`set` 的 JSON 解析被前置到权限门**之前**（:12–14 显式记账的边缘行为）。
- **terminal_stream**：唯一「零 ports、零 `host_ctx` 字段（除 `plugin_id`）」的域；表在 crate 根、命令面在宿主 ⇒ 迁移需两处同步。
- **ws**：`MobileReconnectPolicy` 只做 trait 类型转换、不复制退避算法（:84–104）——退避真源在移动宿主 `connection::reconnect`。
- **support**：`catch_unwind` 是 wasmtime `extern "C"` 边界唯一防线，10 域共用；漏迁 = 10 域 panic 直穿 Store。

## 3. C 类移动独有 interface 函数清单

真源：`bedcode-mobile/packages/plugin-sdk-mobile/rust/wit-src/cap-mobile.wit`（生成物同名文件行号一致）。

| interface（定义行） | 函数 | 形状 |
| --- | --- | --- |
| `host-notify`(68–88) | `notify(title, body, options-json)` | 3×string → `result<_, string>`；options `{vibrate?, sound?}` 缺省 true |
| | `check-permission()` / `request-permission()` | → `result<bool, string>` |
| | `vibrate(duration-ms)` | u32 → `result<_, string>`（钳制 [1, 5000]） |
| | `play-sound()` | → `result<_, string>` |
| `host-fs-mobile`(89–95) | `write-media-downloads(src-path, display-name, mime-type)` | 3×string → `result<_, string>`（MediaStore 落点） |
| | `save-to-document(src-path, display-name, mime-type)` | 3×string → `result<_, string>`（系统保存对话框） |
| `host-terminal-stream`(113–118) | `forward-output(session-id, data)` | string + `list<u8>` → `result<_, string>`（零 JSON / base64） |
| `host-connection`(127–133) | `primary-target()` | → `result<string, string>`（camelCase `{address, port, connected}`） |
| `host-auth`(153–172) | `request-pairing()` | → `result<_, string>` |
| | `verify-pairing-code(code)` | string → `result<bool, string>` |
| | `qr-connect(token)` | string → `result<bool, string>` |
| | `biometric-authenticate()` | → `result<bool, string>` |
| | `has-credentials()` | → `result<bool, string>` |
| `events`（**export** 174–180） | `on-bus-message(topic, payload-json)` | 2×string → `result<_, string>` |
| | `on-auth-success()` / `on-disconnect(reason)` / `on-session-created(session-id)` / `on-session-stopped(session-id)` | → `result<_, string>`；宿主调点 `component.rs:788–896`，实现在 SDK `wasm.rs:240–320` |

其余 import 接口（host-log / host-storage / host-plugin-database / host-events / host-fs / host-http / host-config / host-bus / host-peer / host-mdns / host-platform / host-websocket 客户端）定义在共享核心 `packages/bedcode-wasm-core/wit/core.wit`。

## 4. 分类纠正（对 spec §1.4 原 A/B/C 的修订）

1. `log` **不是**移动独有、也无 host_impl 文件（`component.rs:39–61` 直调 `bedcode-host-api-core::log`）。
2. `connection` 归 **C** 而非 B：桌面同名是 15 函数域，移动只有 `primary-target` 1 函数（引擎事实投影）。
3. `events`(5) 归 C 成立，但**落点不在 `host_impl/`**（`component.rs` export 面 + 移动 SDK + 组件测试插件三处）。
4. **B 类的「同一份机制」真源是独立 crate `bedcode-host-api-core`**；wasm-core 内 `host_api/{storage,database,bus,events,config,fs,platform}.rs` 全是 `cfg(desktop-host)` 的**桌面 adapter**（`host_api.rs:58–80`），移动**不共享**它们。
5. `config` 是 B 类反例：移动键集独有 + 走 Android 桥 + 无共享核 ⇒ ④不成立。
6. `host-fs-mobile` 无独立文件（`fs.rs` 内 2 函数）。

**结论（票 04 / 05 落点）**：
- 纯转发（票 05 只需收窄适配面）：`mdns` / `ws` / `storage` / `bus` / `config` / `platform` / `event` / `auth` / `connection` / `terminal_stream`。
- 含域逻辑（需先下沉或随迁）：`peer`（票 03 覆盖）/ `fs` / `db` / `http`。
- 移动独有（票 04 全迁）：`notify`(5) / `terminal_stream`(1) / `host-fs-mobile`(2) / `connection.primary-target`(1) / `auth`(5) / `events`(5，export 面)。

> **票 05 实际落点（2026-10-10，见 `ticket-05-domain-sinking.md`）**：`http` 下沉能力域
> crate `bedcode-server-http`（`http_engine` 退役）；`fs` 上移 `bedcode-host-api-core::fs`
> （新共享核 + `FsHostPorts`）；`db` 判定已在目标形态（机制语义全在 `core_db`，本端只留
> 私有库句柄解析）。表中 `http.rs` / `fs.rs` 的行号与「`host_api::http_engine::execute_*`」
> 依赖均为**改动前快照**，改动后不再成立（历史证据，不改写）。
