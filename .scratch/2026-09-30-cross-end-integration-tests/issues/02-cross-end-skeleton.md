# 票 02 — cross-end-tests 骨架 + 两端互连编译验证（spike）

**状态**：resolved · 2026-09-30
**类型**：task

## 落点

`cross-end-tests/`（Cargo.toml + src/lib.rs + tests/common/{desktop_ctx,mobile_ctx}.rs
+ `harness_selfcheck.rs`）。依赖键 = lib name，两端各一行 `package = "..."` 重命名。

## spike 的四项风险结论（全部为「不成立」）

| 风险 | 结论 |
|---|---|
| 两个 tauri lib 同进程链接冲突 | **不成立**：两端同为 tauri 2.11.1，依赖图统一为一份；无 `#[no_mangle]` 冲突，`generate_context!` 两次展开各自作用域 |
| 移动端 crate 在 Linux 作依赖需额外 feature | **不成立**：`cargo check --all-targets` 直接过 |
| 桌面 `start_http_server` 无头可启停 | **不成立**：复刻 `pty_session_chain` 的 `app_handle(None)` 模式即可 |
| 移动端客户端无 AppHandle 可构造 | **不成立**：`AuthHttpClient` / `SessionHttpClient` 直接收 base_url；`TerminalLinkManager::subscribe` 收 `Arc<dyn TerminalEventSink>`（生产 impl 才需要 AppHandle） |

## 与 spec §3-A2 / §6-4 的偏差

spec 担心「认证中心 fixture 构建机制在桌面端 tests/ 内 cfg(test)，跨工程不可引用」。
**实查为伪命题**：认证中心 = `com.bedcode.terminal-session` **真实 wasm 应用**，
产物在 `bedcode-desktop/src-tauri/resources/plugins/desktop/`（随包目录，不入库），
跨工程按相对路径引用即可。spec 说的 fixture 是 `packages/plugin-sdk-fixtures`
那批**单元测试夹具**，与本场景无关。

## profile 落点（spec 未提，落地必需）

路径依赖的 profile 只认**本包根**。不写 `[profile.dev]` 会退回 cargo 默认
（全量调试信息），编译峰值内存显著更高、体积更大——本包依赖图含 wasmtime/actix，
必须与两端同款：`debug = "line-tables-only"` + `split-debuginfo = "packed"` +
`[profile.dev.package."*"] opt-level = 2` + wasmtime 关 debug-assertions。

## harness 自检（`harness_selfcheck.rs`）

场景红了第一个要排除的是**台子坏了**。四条强断言：目标设备回读一致 /
移动端真实客户端能从真实插件拿到配对码 / 已注册端点可升级而未注册端点 404 /
**停机后同一地址不得再应答**（证明应答者确是本进程服务器且无残留监听）。

## 验证

`cross-end-tests` 7 个测试二进制全部编译通过并运行。
