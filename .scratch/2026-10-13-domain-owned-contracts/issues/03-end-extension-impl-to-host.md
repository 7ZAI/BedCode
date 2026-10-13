# 票 03 · 端扩展实现归位（变体 B）

**来源**：`.scratch/2026-10-13-domain-owned-contracts/spec.md` §4.3。

**Blocked by**：02（契约搬家与去副本完成后，端扩展 impl 才可整体外迁）。

**Status**：**resolved**（2026-10-13；实施记录与逐项盘点见文末 Comments）。

## What to build

按 R3（端差异声明与实现都在端宿主）把两处端扩展的**实现**搬到桌面宿主：

1. **`host-http-endpoint`**：删除 `packages/bedcode-server-http/src/plugin_binding.rs` 中该 interface 的 `impl Host` 与 `MODULE_INTERFACES` 条目；桌面专属引擎面（端点注册表 / 路由匹配 / 冲突仲裁 / 停用回收相关部分，按代码实际盘点）迁 `bedcode-desktop/src-tauri/src/plugin/http.rs`（或同目录新文件），以路径 B 模块形态自报（`HostModule` + `expected_host_modules` 白名单 + 强制引用行同处）。
2. **`host-websocket-server`**：同上。`bedcode-server-websocket` crate 按 01 裁决处置：
   - 若客户端面已由 `bedcode-ws-client-engine` 承载 → 该 crate 整体视为桌面专属，服务端 impl + 引擎面迁桌面宿主后，crate 去留（拆/留做桌面本地库）按 01 结论执行；
   - 若客户端面仍留 crate → crate 只保留客户端共享部分，服务端面外迁。
3. **边界行为零变化**：权限位（`network:http` / `ws:server`）、属主隔离、冲突仲裁（同名 fail-visible）、停用回收（`purge_for_plugin` / `DomainHooksRegistry`）、事件形状与日志字段全部逐字保留；只改落点。
4. **装配面收口**：白名单双向校验（组合面 interface ↔ 装配面 MODULE_INTERFACES）更新后全绿；能力域 crate 的 `common-host` 绑定不含端扩展 interface（「能力域端中立」可由锁断言，见 05 票）。

## 验收标准

- [ ] `server-http` / `server-websocket` 无端扩展 WIT 定义、无端扩展 `impl Host`、无端专属引擎面（逐项核对）
- [ ] 桌面 `cargo test`：`http_e2e` / `ws_e2e` / 白名单锁 / 回收相关用例全绿
- [ ] 移动端 `cargo check` + `cargo test`（受影响面）绿（本票理论不动移动）
- [ ] 迁移后的权限门与属主隔离行为有针对性用例（不是只靠 e2e 顺带覆盖）

## 门禁

- 桌面 `cargo test` 过滤 http / ws / bindings / owner 面；`cargo fmt` 自查
- 不做「顺手重构」：只搬不改语义；发现必须改语义的点停下问用户

## 注意

- 迁移是「桌面专属」，禁止把端扩展实现塞进任何共享 crate 或内核（防回接锁覆盖，05 票登记）。
- 与并行线（file-transfer / runtime-single-form 等）的文件集求交，避免同文件并发写。

## Comments

### 2026-10-13 · 实施完成（用户裁决：Q4 = A；ws 客户端实现暂留 server-websocket）

**迁移内容**
- `host-http-endpoint`（桌面服务端域 2 函数）：
  - `bedcode-server-http`：`MODULE_INTERFACES` 收为 `[host-http]`；删除该 interface 的 `impl Host`；`bindgen!` 世界随 T-02 只剩出站面。
  - 桌面 `src/plugin/http.rs`：新增路径 B 模块 `HttpEndpointModule`（`MODULE_NAME = "http-endpoint"` / `interfaces = [host-http-endpoint]` / `permissions = [network:http]` / `abi_min = 29`）+ `impl host_http_endpoint::Host` 薄转发到 crate 引擎 API（`http_register_endpoint` / `http_unregister_endpoint`，权限门与注册表仲裁逐字不变）；`expect_host_module!` + `inventory::submit!` 同处。
- `host-websocket-server`（桌面服务端域 10 函数）：
  - `bedcode-server-websocket`：`MODULE_INTERFACES` 收为 `[host-websocket]`、`MODULE_PERMISSIONS` 收为 `[ws:client]`；删除该 interface 的 `impl Host` 与模块注册行；**删除 crate 自有 `wit/ws.wit`**，provider 绑定改指契约真源 `../bedcode-ws-client-engine/wit/ws.wit`（Q4=A；桌面客户端实现暂留本 crate 的双轨已按裁决保留）；`ports_for` 提 `pub` 供宿主 impl 取端口。
  - 桌面 `src/plugin/ws.rs`：新增路径 B 模块 `WsServerModule`（`"ws-server"` / `[host-websocket-server]` / `[ws:server]` / `abi_min = 14`）+ `impl host_websocket_server::Host` 10 条薄转发；`expect_host_module!` + `inventory::submit!` 同处。
- `bedcode-ws-client-engine`：新增 `wit/ws.wit` 契约分片；`boundary_lock.rs` 的 B-4「零 WIT」判据按 ADR 0052 D2 修订为「WIT **绑定层**零承载，`wit/` 只允许 `.wit` 契约分片」（禁词 B-1 / 依赖 B-3 判据不变）。
- 锁与清单：桌面 `bindings.rs` 的域分片 bindgen 锁为 server-websocket 登记特例（绑定源 = ws-client-engine 契约文件，非端 SDK）；`wit_package_version_lock.rs` 源文件清单同步。

**端扩展引擎面逐项盘点（「按代码实际盘点」结论）**

| 项 | 处置 | 理由 |
| --- | --- | --- |
| WIT 声明（两份端扩展） | 归桌面 wit-src（T-02） | ADR 0052 D4：端独有声明住端 cap |
| `impl Host`（WIT 绑定层） | 归桌面宿主路径 B 模块 | 同上；孤儿规则要求装配方自带绑定 |
| 模块元数据（MODULE_INTERFACES / permissions / HostModule / inventory） | 归桌面宿主模块 | 白名单由宿主自报 |
| 端点注册表 / 路由匹配 / 冲突仲裁 / 停用回收（`server-http::registry`、`server-websocket::{endpoint,channel,registry}`） | **留在能力域 crate**（服务器引擎设施） | 它们被服务器自身直接消费（`routes.rs`、`middleware/auth_gateway.rs`、`conn`、`websocket_manager`），迁移会造成服务器 → 宿主的反向依赖（违反票面「只搬不改语义」）；且 WIT-无关（默认形态可编译）。域函数（`http_register_endpoint` / `ws_*`）是服务器引擎的插件面 API，宿主 impl 与 crate 单测/宿主 e2e 共同消费——保持原位 |
| ws 客户端契约真源 | `ws-client-engine/wit/ws.wit`；桌面客户端实现暂留 `server-websocket` | Q4=A 裁决；双轨收敛另行立项 |

**验证证据**
- 能力域 crate 测试：server-http **85/0**（含 inbound 权限门 / 属主仲裁 / 回收只碰本人用例）、server-websocket **51/0**、ws-client-engine **20/0**（含边界锁）。
- 桌面：`cargo check` 0 error；`--lib` **157/0**；`http_e2e` 1/1；`ws_e2e` **6/6**；`system_component_test` 10/10；`wit_package_version_lock` 5/5；`capabilities_lock` 9/9；`pty_wiring` 6/6；`owner_e2e` 5/5。
- wasm-core `--lib` 576/0；移动 `cargo check` 0 error + 三把锁（3/3、7/7、4/4）。
- 白名单双向校验（组合面 `compose.json` caps ↔ 装配面 `MODULE_INTERFACES` 并集）在 `plugin/bindings.rs` cfg(test) 全绿。

**附带修复（测试接线，非语义变更）**
- `tests/ws_e2e.rs`：`setup_wasm_runtime` 原先经显式列表从内核 `test_support` 导入（不注入 guest 事件派发器 ⇒ `on-message` 投递走 `UnwiredGuestEvents`，`test_ws_client_outbound_roundtrip` 恒红）；改为由 `support::*` 提供宿主包装（注入 `DesktopGuestEvents` + `DesktopHostFacts`），并留注释防止回退。该红是既有基线（HEAD 同构），随本票门禁一并转绿。
- 遗留：`tests/session_e2e.rs` 有同样的显式导入形态（并行线在途文件，未触碰），建议在并行线收口时核对该文件顶部导入。
