# P1 票完成记录：脱绑契约定案 + 样板域（bedcode-server-http）

> Date: 2026-10-08
> Status: **P1 完成（验证全绿）**。P2–P6 可按契约定案继续。

## 契约定案（写入 spec §3.1，样板域实测后定稿）

1. **feature 命名**：`desktop-host`（语义 = 桌面宿主 WIT 绑定层；移动端 / 无头宿主永不开启）。
2. **绑定层 cfg 包裹**：`plugin_binding.rs` 内按**段落** `#[cfg(feature = "desktop-host")]`，不是整文件——
   因 http 域机制函数（`http_register_endpoint` / `unregister_endpoint` / `purge_for_plugin`）与绑定层
   **同住 plugin_binding.rs**（wasm-core-lib-split 票 06 迁入形态）。绑定层段落 = `DESC` / `HttpModule` /
   `impl HostModule` / `HasSelf` / `MODULE` / `inventory::submit!` / `ports_for` / `bindgen!` / `impl Host`。
   **默认可用（无 WIT 依赖，不 cfg）**：机制函数、`ports` / `egress` 模块、`DOMAIN`（纯 &str 键，
   wasm-core adapter 在 desktop-host 装配路径引用）、`install` / `install_ports`（纯 Rust 装配）。
3. **optional 依赖**：`bedcode-host-kit` / `wasmtime` / `wit-bindgen` / `inventory` 全部 optional；
   `desktop-host = ["dep:…"]`。**`bedcode-plugin-api` 直接移除**（不进 optional）——常量自持后
   本域对桌面 SDK 零引用，比 spec 原案（收窄 optional）更干净。
4. **常量下沉**：`EndpointAuth`（wire 形状，含 parse 逻辑）+ `PERMISSION_NETWORK_HTTP`（权限位）
   沉到 `src/wire.rs`（自持副本）。**副本与 SDK 定义块逐字一致**（含注释），`wire/drift_lock.rs`
   （`#[cfg(test)]`）比对 SDK 源文件文本块，任一侧漂移即红。SDK 原常量不删（wasm-core host_api 等
   消费方仍在）。**换行符归一化**：SDK 侧 CRLF、本仓 LF，「逐字」按字符序列比对。
5. **crate 描述**：加了「默认 = 纯引擎 / `desktop-host` 装配 WIT 绑定层」。
6. **强制引用 gate**：wasm-core `component.rs` 的 `use bedcode_server_http as _;` 同步
   `#[cfg(feature = "desktop-host")]`（无该 feature 的宿主**不应**注册——无插件宿主机制）。

## wasm-core 接线（P1 样板必做，否则全量回归红）

- `[features] default = ["desktop-host"]; desktop-host = ["bedcode-server-http/desktop-host"]`
  （wasm-core 是宿主机制，桌面形态默认；无头宿主 `default-features = false` 拿纯引擎——已实测编译过）。
- 测试修正 1 处：`manager/runtime/tests/http_e2e.rs:54` 的 `bedcode_plugin_api::EndpointAuth::Jwt`
  → `bedcode_server_http::wire::EndpointAuth`（registry 现收 wire 副本）。**未动**：
  `register.rs` / `contributions_test.rs` 的 SDK `EndpointAuth` 是 **WS 面**（ws 域尚未脱绑，P3 处理）。

## 验证证据（全部实跑）

| 项 | 结果 |
| --- | --- |
| http 域 `cargo build`（无 feature，纯引擎） | ✅ 编译过；`cargo tree -e normal` 无 wasmtime/wit-bindgen/inventory/host-kit 直接依赖 |
| http 域 `cargo build --features desktop-host` | ✅ 编译过（bindgen!/impl Host 装配） |
| http 域 `cargo test`（无 feature，83） | ✅ 全绿（含 2 漂移锁） |
| http 域 `cargo test --features desktop-host` | ✅ 83 全绿 |
| wasm-core `cargo check --no-default-features`（无头态） | ✅ 编译过 |
| wasm-core `cargo test --lib`（677） | ✅ 676 绿 + 1 既有红（terminal_output_perf，scratchpad 已记录） |
| wasm-core `capability_registry_matches_whitelist` | ✅（inventory 注册 + 白名单比对） |
| wasm-core `http_e2e`（真实 WASM fixture） | ✅ |
| 桌面 src-tauri `cargo check` / `cargo test --lib`（73） | ✅ |
| 桌面治理锁 capability_crates_no_product_ids(6) / unit_tests_only(5) | ✅ |
| 桌面 server_integration | ✅ |
| 桌面 ws_e2e | ✅ 4 过 2 红（既有 fixture 缺失，scratchpad 记录） |
| 桌面 system_component_test | ❌ 6 红 = **既有** fixture 缺失（scratchpad 记录在案，非本票） |
| cargo fmt --check | 既有基线差异（business_endpoint_shapes / bus.rs / component.rs 等未动文件）+ wire.rs 逐字区与 SDK 格式对齐的**刻意豁免** |
| cargo clippy（desktop-host 态） | 3 警告全为既有（auth_gateway.rs:67 / registry.rs:97 / server-core link_crypto），不在改动行 |

## 已知项 / 偏差（移交 P5、对外记录）

1. **`bedcode-plugin-api` 仍经 `bedcode-server-base` 传递进 http 域编译图**（base 的
   `pub use …constants` + `BusMessage` 引用）——P5「server-base 常量下沉」清掉，此前
   「任何宿主零桌面 SDK」只对外到本域直接依赖层。spec §5 门禁 3 的树核对按「本域直接
   依赖」口径通过，传递面 P5 归零。
2. **wire.rs 逐字区豁免 rustfmt**：副本必须与 SDK 逐字（SDK 的 `Err(format!(…))` 多行格式
   与本地 rustfmt 压行规则冲突）——事实源格式优先，锁比对不含 rustfmt 归一化。
3. 桌面 src-tauri 的 http 域依赖**未显式加** `features = ["desktop-host"]`（spec §3.2 P5 接线）；
   feature 经 wasm-core 统一传播，桌面全量已验证带绑层（inventory 注册在）。
4. 磁盘清理（本票操作）：`target/host-kits/debug` 30G + `server-libs/debug` + 纯缓存
   （sccache/pnpm/chrome/opengrep）——构建产物已重编译回；并行会话移动端 target 未碰。

## 决策记录（本周票内定案，供后续票引用）

- 绑定层段落化 cfg（非整文件）：机制函数与绑层同文件的域，段落级切分最贴现状。
- `DOMAIN` / `install` / `install_ports` 默认可用：无 WIT 语句，wasm-core adapter 引用，
  cfg 掉反而扩大 wasm-core 改动面。
- `wasm-core` feature 转发 + default：桌面机制默认桌面形态，移动端复用（票 17）时关。
- 漂移锁放**能力域自身**（`#[cfg(test)]`，无 feature 也跑）+ **读 SDK 源文件文本块**比对：
  与 `capability_crates_no_product_ids` 同款手法，不依赖编译期依赖。