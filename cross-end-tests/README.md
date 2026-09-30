# cross-end-tests — 跨端真实互连集成测试

同一测试进程内，让**桌面端真实服务器**与**移动端真实客户端代码**互连。零 mock、
零 adb、零 WebView、零模拟器。

```bash
# 前置：桌面随包 wasm 产物（认证中心）须已构建
cd bedcode-desktop && pnpm run plugins:build

cd cross-end-tests && cargo test
```

产物缺失时测试**显性失败**（不是 `[skip]`）——本工程的唯一驱动方式就是真实产物，
静默跳过会把「未验证」伪装成「通过」。

## 为什么需要它

两端各自的集成测试都用**协议级 mock 对端**：

| 端 | 真实侧 | mock 侧 |
|---|---|---|
| 桌面 | Actix 服务器（`src-tauri/tests/pty_session_chain.rs` 等） | 通用 reqwest / tokio-tungstenite 客户端 |
| 移动 | AuthHttpClient / SessionHttpClient / TerminalLinkManager | 假桌面服务器（`tests/common/mod.rs`） |

两套 mock 各自自洽。若文档或任一端实现偏离，**两端测试全绿但真实互连必坏**。
本工程消灭这个盲区：请求字节由移动端真实客户端生成，应答字节由桌面真实插件生成。

## 结构

```text
cross-end-tests/
├── Cargo.toml              # 独立包；同时依赖两端 lib（bedcode-desktop-lib / bedcode-mobile-lib）
├── src/lib.rs              # 空 lib（crate 级说明）
└── tests/
    ├── common/
    │   ├── desktop_ctx.rs  # 桌面无头装配：AppContext + 真实认证中心产物 + Actix 服务器
    │   └── mobile_ctx.rs   # 移动端客户端装配：目标设备 / 全局 token / 事件与输出记录替身
    ├── harness_selfcheck.rs    # 台子自检（场景红了先排除台子坏了）
    ├── pairing_auth_flow.rs    # 配对 / QR / 重认证（正例 + 4 类反例）
    ├── jwt_rotate_reconnect.rs # 密钥环轮换宽限期（ADR 0033：旧 token 轮换后仍可用）
    ├── session_http_flow.rs    # 会话 HTTP 面 + 1002 错误信封 + remove 幂等
    ├── terminal_ws_flow.rs     # 终端流闭环（真实 bash PTY 输出到达移动端）
    ├── fail_closed_flow.rs     # fail-closed（无中心在册 / 伪造凭证）
    └── lifecycle_flow.rs       # 桌面停用 / 激活对移动端连接的联动
```

**每个场景 = 一个独立测试二进制**：`AppContext` 是进程级 `OnceLock` 单例，场景之间
无法重装，只能靠进程隔离。

## 为什么依赖键等于 lib 名

L0（2026-09-30）把两端 lib 从同名 `bedcode_lib` 改为 `bedcode_desktop_lib` /
`bedcode_mobile_lib` 之前，这两行只能二选一——同名 crate 无法同时出现在一个依赖图里。
路径依赖的依赖键默认取 package 名，故 Cargo.toml 里要显式写 `package = "..."`。

## 记录替身口径

`EventRecorder`（`TerminalEventSink`）/ `OutputRecorder`（`Channel<InvokeResponseBody>`）
**只记录已真实发生的数据**（链路状态事件、PTY 输出字节），不伪造任何协议应答。
生产形态是 Tauri WebView，测试里没有 WebView，替身只被「读」用于断言。

## 覆盖不到的边界（诚实清单）

- `deny_kind` 三态（`no_center` / `unavailable` / `policy`）是宿主**日志字段**不是
  wire 字段——客户端一律看到 401（这是有意的，不泄露部署信息）。三态分类的覆盖在
  宿主 `utils/auth/auth_center` 单测。
- 生物认证正向路径：移动端私钥在 Android Keystore，无头进程构造不出真设备密钥。
- QR 的「桌面扫码确认」UI 步骤：直接驱动插件 `qr-code-generate` 互调（即桌面 UI 的
  同一入口）生成 token。

## 产物落点与磁盘

本包有**自己的** `target/`（依赖图与两端都不同，合并会互相驱逐缓存）。
首次全量构建分钟级、峰值十几 GB——构建前按 AGENTS §3 检查磁盘。

spec：`.scratch/2026-09-30-cross-end-integration-tests/spec.md`
