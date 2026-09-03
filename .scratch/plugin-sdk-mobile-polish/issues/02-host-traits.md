# 02 — host/ trait 体系（架构借鉴，功能自定）

Type: task
Status: resolved
Blocked by: —

## 问题

移动端 `WasmHost` 是具体类型 + 直接方法调用，插件无法用 trait 抽象依赖宿主能力，无法 mock 测试；
宿主 `wasm_runtime.rs` 的手工 host functions 与 SDK 侧 `extern "C"` 声明之间无编译期约束，签名漂移风险高。

## 任务

**架构层面借鉴桌面端**：`HostError` + 按功能域拆分子 trait + `HostApi` 聚合（blanket impl）的模式。

**功能层面按移动端现状自定**，只覆盖移动端实际暴露的能力，不照抄桌面端功能集：

| 子 trait | 移动端能力（按 wasm_host.rs 现状） |
|---|---|
| `HostStorage` | `storage_get` / `storage_set` / `storage_delete` |
| `HostDatabase` | `db_execute` / `db_query`（无参数绑定版，移动端宿主仅注册这两个） |
| `HostTerminal` | `terminal_send` |
| `HostSession` | `session_list` / `session_get`（移动端宿主目前是空操作 noop，保留 trait 定义即可） |
| `HostEvents` | `emit_event` / `notify`（无 broadcast_sync） |
| `HostHttp` | `http_fetch` |
| `HostFs` | `fs_read` / `fs_write` / `fs_copy`（无 fs_delete） |
| `HostLog` | `log_info` / `log_debug` / `log_warn` / `log_error` / `mark_plugin_error`（T6） |
| `HostBus` | `bus_publish` / `bus_subscribe` / `bus_unsubscribe` |

不移植：`HostConfig`、`HostPluginDatabase`（移动端无插件独立库）、`session_config_list`、参数绑定 SQL。

## Answer

## Answer

已新建 `rust/src/host/` 模块（9 个子 trait + HostError + HostApi 聚合），仅覆盖移动端现有能力：
HostStorage / HostDatabase / HostTerminal / HostSession / HostEvents / HostHttp / HostFs /
HostLog（含 mark_plugin_error）/ HostBus。未移植桌面端 HostConfig、HostPluginDatabase、
session_config_list、参数绑定 SQL（移动端无对应能力）。lib.rs re-export 全部子 trait。
cargo test 通过。

## 验收

- SDK `cargo test` 通过。
- 每个子 trait 的方法与 `WasmHost` 现有方法一一对应。
