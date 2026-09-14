

I've completed the static review of the Rust integration tests and inline tests. Since my Bash is restricted to read-only commands per the reviewer constraint, and no write-capable tool is exposed, I'll return the complete report as my final response (the runtime will persist it to the configured path).

---

# Rust 集成测试 + 主要内联测试审核报告

### 1. 模块概览

**测试规模**
- 集成测试文件 4 个 / 用例 30 个 / 共 1548 行（含 `common/mod.rs` 250 行 mock 基建）
  - `ws_protocol_integration.rs` 11 用例 / 524 行
  - `http_auth_flow.rs` 11 用例 / 540 行
  - `http_proxy_flow.rs` 7 用例 / 452 行
  - `build_manifest_smoke.rs` 1 用例 / 11 行（Windows manifest 注入 smoke）
- 内联 `#[test]` 用例 ~240 个（32 个源文件）
- 覆盖清单：`model/message.rs`(38), `enums/special_key.rs`(32), `plugin/saf_io.rs`(16), `peer_transfer.rs`(12), `plugin/wasm_host.rs`(11), `terminal_link.rs`(11), `plugin/saf_path.rs`(10), `egress.rs`(10), `connection/codec.rs`(10), `connection/heartbeat.rs`(10), `plugin/wasm_runtime/component.rs`(9), `router/registry.rs`(8), `router/router.rs`(6), `auth/http.rs`(7), `plugin/approval.rs`(5), `peer_receive.rs`(5), `file_service/saf_tree.rs`(5), `commands/dev_logs.rs`(4), `commands/http_proxy.rs`(4), `peer_net.rs`(4), `plugin/fs_auth.rs`(4), `plugin/loader.rs`(1), `system/info.rs`(3), `plugin/wasm_runtime.rs`(3), `plugin/wasm_runtime/host_impl/mdns.rs`(3), `connection/request.rs`(2), `enums/session.rs`(2), `plugin/validation.rs`(3), `plugin/manager.rs`(4), `auth/manager.rs`(1)

---

### 2. 问题清单（按严重级别）

**Blocker**
- 无

**Major**

- `src/plugin/wasm_host.rs:497-500` | Major | `test_sanitize_plugin_id` 测试内直接用 `"com.example.my-plugin".replace('.', "_").replace('-', "_")` 重新实现逻辑，然后 assert 结果；源码里根本没有 `sanitize_plugin_id` 函数（内联在 `validate_sql_table_prefix` 内）——这个测试不引用被测代码，任何实现改动都测不到 | G4「复制实现逻辑作为预期」/ G6 无法杀死变异 | 删除该测试或改成真正调用 `validate_sql_table_prefix` 用不同 plugin_id 校验前缀

- `src/plugin/wasm_host.rs:559-562` | Major | `test_extract_table_names_quoted` 断言 `INSERT INTO \`my-table\`` 抽出 `"my"`。实际 `extract_table_names` 用 `(\w+)` 正则，`-` 不匹配，所以 `my-table` 被截成 `my`——测试锁死了实现 bug（正确断言应是 `"my-table"`） | G4「为绿改期望」/ G6 语义与契约不符 | 修 `extract_table_names` 用 `([^`\s"']+)` 匹配完整标识符，或改断言为完整名；否则此测试会阻止 bug 修复

- `tests/http_proxy_flow.rs:279` | Major | 遗留调试输出 `eprintln!("DEBUG jwt-1: status={} body={}", ...)` 在 CI 日志里刷脏 | G4 冗余代码 | 删除或降级为 `tracing::debug!`

- `src/system/info.rs:121-124` | Major | `test_local_ip_addresses` 只有 `let _ = local_ip_addresses();` 无任何 `assert`——纯「不 panic」测试，无法杀死任何变异 | G3 弱断言 / G4 无断言测试 | 至少断言返回 `Vec<SocketAddr>` 内元素协议族为 `Ipv4` 或 `Ipv6`，或对空列表场景显式验证

- `src/auth/manager.rs:694-725` | Major | 725 行 auth manager 只有 1 个测试（`verify_biometric_signature_roundtrip`）；`AuthManager::authenticate` / `refresh` / `biometric_sign` / `biometric_verify` 生产路径几乎无覆盖 | G1 行为契约缺口 | 至少补 `biometric_sign` 请求-响应、`refresh` token 过期分支、无效签名拒绝三条路径

- `src/plugin/manager.rs:1186-1421` | Major | 1421 行 PluginManager 只有 4 个测试，覆盖 degrade/retry/deactivate/dispatch 四态；`init_wasm_runtime` 失败路径、`activate_plugin` 权限闸门、`uninstall` 副作用、`message_bus` 分发错误处理全无 | G1/G2 | 补 init 失败、permission denied 拒绝、dispatch panic 恢复 3 条主路径

- `src/peer_net.rs:1787-1915` | Major | 1915 行 peer 网络模块只有 4 个测试（`share_landing_tests` 3 个 + `dial_payload_tests` 1 个），`connect/disconnect`、握手、超时、消息路由、鉴权等主链路 0 覆盖 | G1 高风险未覆盖 | 补握手超时、非法 peer id 拒绝、发送队列溢出 3 条基础路径

**Minor**

- `src/system/info.rs:162-173` | Minor | `test_collect_non_android` 只断言 `!os_name.is_empty()`、`!app_version.is_empty()`、`!device_name.is_empty()`，是 G3 弱断言（仅非空）；仅 `app_version == env!("CARGO_PKG_VERSION")` 是强断言 | G3 | 补 `os_name` 白名单 / `device_name` 格式断言

- `src/auth/http.rs:359-362` | Minor | `client_is_constructible` 只测 `Arc::strong_count == 1`，与被测 API 行为无关 | G3/G4 | 删除或替换为 `client.base_url()` 语义验证

- `src/connection/heartbeat.rs:245-262` | Minor | `test_connection_lost_true_after_timeout_elapses` 用真实 `sleep(30ms)` 触发 `Instant::elapsed`；无时钟注入缝，跨机器耗时波动可能让断言失败 | G4 真实时间依赖 | 加 100ms 容差或将 `Instant` 替换为可注入 `TimeProvider`

- `src/connection/heartbeat.rs:226-230` | Minor | `test_connection_lost_false_before_any_pong` 只断言 `!is_connection_lost()` 默认状态，未构造业务场景 | G3 | 与 `test_pong_received_resets_timeouts_and_emits_event` 合并或补业务上下文

- `src/plugin/fs_auth.rs:470-479` | Minor | `prefix_matches_real_path_uses_canonical_prefix` 断言 `checker.prefix_matches("/", &canonical_of_/)` 恒真，无信息量 | G3 | 用 `/tmp/foo` 前缀匹配 `/tmp/foo/bar`，验证多级前缀

- `src/plugin/loader.rs:322-326` | Minor | `mod tests` 内 3 行纯空白+尾随空格 | G4 冗余代码 | 删除空行

- `tests/ws_protocol_integration.rs:298-311` | Minor | `supervisor_recovers_after_disconnect` 内含 `sleep(200ms)` 与 1s 后 fallback `event_tx.send` 注入——非确定性；依赖时序收敛可能 flaky | G4 真实时间 | 拆出两个明确用例（回声触发 vs 兜底注入）

- `tests/ws_protocol_integration.rs:122-129` | Minor | `auth_full_flow` 中 `events` 在 `manager.disconnect()` 后接收无超时保护——若断连过程产生 Error 事件可能提前命中 `recv()` 造成误判 | G4 顺序耦合 | 加 `wait_event` 谓词或 drop 订阅

- `src/plugin/saf_io.rs:516-707` | Minor | `FakeSafIo` 的每个方法内嵌 `assert_eq!` 校验参数——断言分散在被测对象外（fake），若未来改用记录调用模式会失去语义；目前可通过但可读性差 | G4 反模式边界 | 用 `Vec<CapturedCall>` 显式记录调用后统一断言，与 `CapturedRequest`（`http_proxy_flow.rs`）模式对齐

**Nit**

- `tests/build_manifest_smoke.rs:11-12` | Nit | `assert!(true)` 恒真断言；文档注释明确说明「测试存在即验证」，是合理例外但需保持注释 | G4（例外） | 保持，注释完整即可

- `src/model/message.rs:1662` | Nit | 单测注释 `// b"hello" 的 Base64 手算值` 与 `test_output_base64_encoding` 的 `// "hello world" → aGVsbG8gd29ybGQ=` 一致——建议把 Base64 真源集中到一处常量 | G4 可读性 | 抽 `const B64_HELLO: &str = "aGVsbG8="`

- `tests/http_auth_flow.rs:333-336` | Nit | `resolve_base_url_happy_path_with_target` 断言 `base == format!("http://...")`；对协议固定 http 隐式硬编码，未验证 IPv6 场景 | G2 边界 | 补 `"[::1]"` 主机格式断言

- `src/plugin/approval.rs:262-279` | Nit | `test_approve_roundtrip_and_revoke` 用 `content_hash="abc123"` 而非真实哈希；若 `verify_approval` 校验 hash 一致性，此测试与真实流程脱节 | G6 | 用 `compute_dir_hash` 生成 hash

- `src/plugin/wasm_host.rs:441-462` | Nit | `spawn_mock_server` 手写 HTTP 响应头，未处理 `Content-Length` 与实际 body 长度不一致、也未做 keep-alive 测试 | G4 可读性 | 用 `http-body-util::Full` 或直接引 `warp`/`actix`（项目已有 actix-web）

---

### 3. 评分卡（0-100）

| 维度 | 分数 | 依据 |
|---|---|---|
| 需求/行为契约追溯性（G1） | **72** | 多数测试注释解释了「验证什么契约」，尤其 `model/message.rs`、`egress.rs`、`saf_path.rs` 优秀；但 `auth/manager.rs`、`plugin/manager.rs`、`peer_net.rs` 三个 700+ 行模块各 1–4 测试，契约覆盖不足 |
| 正反例覆盖（G2） | **78** | `special_key.rs`(32 用例正/反/边界全覆盖)、`egress.rs`(L1/L2/L3 三层策略正/反)、`saf_path.rs`(路径逃逸拒绝)、`approval.rs`(gating) 都做了正反例；`heartbeat.rs`、`heartbeat.rs::connection_lost_false_before_any_pong`、`client_is_constructible` 只有正例 |
| 边界+异常覆盖 | **76** | `codec.rs`(非法 JSON/UTF-8/Close 帧)、`saf_io.rs`(base64 损坏/EOF/权限不可用)、`terminal_link.rs`(截断帧/未知版本/魔数错) 边界强；`heartbeat.rs` 用真实 sleep 触发超时不够严谨；`http_auth_flow.rs` 无 4xx/5xx 传输错误分支 |
| 断言强度（G3） | **72** | `message.rs` 逐字段精确 JSON 断言、`special_key.rs` 精确 PTY 字节断言、`egress.rs` `matches!(Allow(L2Builtin))` 精细匹配；扣分项：`system/info.rs::test_local_ip_addresses` 无断言、`test_collect_non_android` 全 `is_empty` 弱断言、`client_is_constructible` 空断言 |
| 独立性+确定性 | **82** | `http_proxy_flow.rs` 用 `SERIAL: Mutex` 显式串行化全局态；每个集成测试用 `MockDesktopServer::start()` 独立端口；`tempfile::tempdir` 隔离；扣分项：`supervisor_recovers_after_disconnect` 依赖真实时序、`heartbeat.rs` sleep 30ms、`build_auto_task_component` 触发子进程 `cargo build` |
| 可读性+可维护性 | **88** | 每个测试都有中文注释解释验收点，mock 基建 `common/mod.rs` 职责清晰，`FakeSafIo`/`RecordingHandler` 模式统一；扣分项：`test_sanitize_plugin_id` 假测试误导读者、`http_proxy_flow.rs:279` 遗留 eprintln |

**加权平均（G1 20%, G2 15%, 边界 15%, G3 20%, 独立 15%, 可读 15%）= 76.7**

**⚠ 总分 76.7 < 80，需局部重写**。核心问题集中在 4 个 700+ 行模块测试稀疏 + `test_sanitize_plugin_id` / `test_extract_table_names_quoted` 两处「为绿改期望」+ `system/info.rs` 无断言测试。若补齐 `plugin/manager` 与 `auth/manager` 主路径用例并清理 3 处反模式，可拉到 82+。

---

### 4. 高风险未覆盖清单

**该模块有测试但明显缺失的关键场景**

- `src/model/message.rs`：无并发访问测试（`send_and_wait` 请求-响应匹配在集成层有，但 `Message::with_request_id` 的 `&mut` 语义在单元层无 `Send`/`Sync` 检查）
- `src/plugin/wasm_host.rs`：`validate_sql_table_prefix` 只测 INSERT/FROM/CREATE 关键字；缺 UPDATE/DELETE/ALTER/DROP 独立覆盖，且 `test_extract_table_names_quoted` 锁死 bug（见 Major 项）
- `src/egress.rs`：无 `X-Forwarded-For` / 307 相对跳转 / 空 body redirect 场景
- `src/connection/heartbeat.rs`：无「收到 Pong 但超时后仍判定断连」的时钟漂移测试
- `src/plugin/approval.rs`：无「approved_permissions 空 + version mismatch」组合拒绝
- `tests/ws_protocol_integration.rs`：无「多客户端并发连接同一 server」测试，虽 `common/mod.rs` 支持多连接

**该模块完全没有测试但应该有的关键路径**

- `src/auth/manager.rs`（725 行）：`authenticate` / `refresh` / `biometric_sign` / `biometric_verify` / credential 存储 5 条主路径全部无测试；仅 `verify_biometric_signature` 被测
- `src/plugin/manager.rs`（1421 行）：`init_wasm_runtime` 失败降级、`activate_plugin` 权限闸门、`uninstall` 副作用、plugin DB schema migration 全无
- `src/peer_net.rs`（1915 行）：`connect/disconnect`、握手机制、消息路由、peer 鉴权、超时重连全部无测试
- `src/commands/http_proxy.rs`（491 行）：仅测 4 个纯工具函数；`execute_proxy` / `http_cancel` 集成层已覆盖但 error path（超时/连接拒绝/401/500）无单元测试
- `src/plugin/wasm_runtime.rs`（458 行）：`WasmRuntime::new` 无 AOT cache 路径、燃料看门狗配置错误处理、`instantiate_component` 失败恢复全无
- `src/router/registry.rs`（271 行）：`build_default_registry` 缺「所有 11 个变体都被注册」的穷举断言（现只测 `RecordingHandler` 收到）
- `src/plugin/loader.rs`（390 行）：只有 1 个测试且依赖 `cargo build` 子进程；`manifest 解析失败`、`组件编译失败`、`权限校验失败` 三条错误路径无覆盖
- `src/commands/dev_logs.rs`：无环形 buffer 溢出 / 多订阅者 / level 过滤测试

---

### 5. 改进优先级建议

**P0（必须修）**

- `src/plugin/wasm_host.rs:497-500` — 删除或重写 `test_sanitize_plugin_id`（G4 假测试）
- `src/plugin/wasm_host.rs:559-562` — 修 `extract_table_names` 正则或用完整名断言（G4 锁 bug）
- `src/system/info.rs:121-124` — 给 `test_local_ip_addresses` 加实质断言
- `tests/http_proxy_flow.rs:279` — 删除遗留 `eprintln!`
- `src/plugin/manager.rs` — 至少补 3 条 activate/permission error path 主路径
- `src/auth/manager.rs` — 至少补 `authenticate` + `refresh` + `biometric_sign` 三条核心路径

**P1（建议修）**

- `src/system/info.rs:162-173` — `test_collect_non_android` 加白名单/格式断言
- `src/auth/http.rs:359-362` — 删 `client_is_constructible` 或替换为有意义测试
- `src/connection/heartbeat.rs:245-262` — 引入可注入时钟或加容差
- `src/plugin/fs_auth.rs:470-479` — 用非平凡前缀替换 `/` vs `/` 平凡断言
- `tests/ws_protocol_integration.rs:298-311` — 拆分 fallback 注入与回声触发两个用例
- `src/plugin/loader.rs:322-326` — 清理尾随空格空行
- `src/plugin/wasm_host.rs:441-462` — mock server 换用 actix-web（项目已有）
- `src/commands/http_proxy.rs` — 补 `execute_proxy` 超时 / 4xx / 5xx 单元测试

**P2（可选）**

- `src/plugin/saf_io.rs:516-707` — 将 `FakeSafIo` 内嵌 `assert_eq!` 改为 `CapturedCall` 记录 + 统一断言，与 `http_proxy_flow.rs::CapturedRequest` 模式对齐
- `src/plugin/approval.rs:262-279` — `content_hash` 用 `compute_dir_hash` 生成
- `src/egress.rs` — 补 307 相对跳转 / `X-Forwarded-For` 注入场景
- `src/router/registry.rs` — 补「11 变体全注册」穷举断言
- `tests/ws_protocol_integration.rs` — 补多客户端并发连接测试（`common/mod.rs` 基建已支持）
- `src/model/message.rs` — 抽 `const B64_HELLO: &str` 集中 Base64 真源
- 引入 `#[tokio::test(start_paused = true)]` 到所有使用 `tokio::time::sleep` 的测试，用虚拟时钟替代真实 sleep
- 补 `cargo fuzz` 或 property-based tests 给 `extract_table_names`、`resolve_saf_path` 两个安全敏感函数

---

## Standards Compliance（对照 BedCode 规范）

| 规范项 | 状态 |
|---|---|
| 错误类型用 `AppError`（无裸字符串） | ✅ 集成测试断言 `AppError::Auth/Internal/Parse` 变体一致 |
| `unsafe impl Send/Sync` | ✅ 未发现 |
| 关键路径 `let _ =` 静默忽略 | ⚠ `wasm_host.rs` 内 `emit` 用 `let _ =` 是合理（前端订阅丢失不应阻塞），非关键路径 |
| `tokio::spawn` 用 `spawn_with_error_boundary` | ⚠ `http_proxy_flow.rs::concurrent_request_ids` 用裸 `tokio::spawn` 且 `.expect("join")`——集成测试场景可接受，但生产代码应核对 |
| panic hook 中误用 `tracing::error!` | ✅ 未发现 |
| 日志级别 | ✅ 集成测试无日志污染（除 `http_proxy_flow.rs:279` eprintln） |
| 中文硬编码（前端 i18n） | N/A（本审计为 Rust 侧） |
| JWT/token 处理 | ✅ `MOCK_SESSION_TOKEN`、`MOCK_REAUTH_TOKEN` 常量隔离，未泄漏到生产；`clear_global_token()` 每测试清理 |