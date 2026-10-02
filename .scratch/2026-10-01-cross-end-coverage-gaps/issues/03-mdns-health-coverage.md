# 票 03 — mDNS 发现/广播 + /api/health 探测跨端覆盖

**状态**：resolved · 2026-10-01
**类型**：task
**依赖**：spec.md §5.3（前置工程 cross-end-tests 已交付）

## 落点

新增 `cross-end-tests/tests/mdns_health_flow.rs` + `common/desktop_ctx.rs` 三个装配增量
（`start_server_via_supervisor` / `stop_server_via_supervisor` / `system_info` /
`advertised_app_version`）+ 移动端一处**最小生产改动**（见下）。

## 契约落表

| 契约 | 行为 | 场景 |
|---|---|---|
| M-001 | 桌面 `supervisor::start_mdns_advertisement` 广播 → 移动端真实浏览 `_bedcode._tcp.local.` → 发现该服务；逐字断言 `port`（= 真实服务器端口）、`platform=desktop`、`device_name` = `BedCode-<device>`、`version` TXT、`host_name` 以 `.local.` 结尾、`address` 非空 | 正例 |
| M-002 | 照抄前端 `httpProbe` 的请求构造（先 `egress_declare_desktop_target` → `kind="desktop"` 的 `GET /api/health`）→ 断言 `{status:"ok", port:<真实端口>, uptime_secs:u64}` | 正例 |
| M-003 | `MdnsDiscovery::stop` 后发现列表清空（不留陈旧条目） | 正例 |

## 关键发现（改变了实现形态）

1. **必须经 supervisor 启动服务器**：服务器生命周期的**主人**是
   `bedcode_server_core::supervisor::ServerSupervisor`，它（a）把端口写进自己的状态
   ——`/api/health` 的 `port` 取自这里，（b）启动 mDNS 广播（`supervisor.rs:231`），
   （c）重置指标。GUI bootstrap 走的正是 `init_config` → `ws_manager.init()` →
   `supervisor.start(port)`。而 rig 常用的 `composition::start_http_server` → `app::serve`
   **绕过 supervisor**：既不广播（票面观察到的现象），`/api/health` 还会报默认端口
   **8765** 而不是真实端口（实测第一版就撞上这个）。
2. **环回豁免**：`link_crypto::is_exempt` 对环回对端显式豁免（票 02 已处理）。
3. **`version` TXT 的真源不是 `SystemInfo::collect().app_version`**：`collect()` 在
   `bedcode-server-base` 里编译，取的是**该包**的版本（0.1.0）；广播走
   `SystemInfoPort::app_version()`（宿主壳 = 桌面 crate 的 `CARGO_PKG_VERSION` = 2.1.1）。
   断言必须对着端口（`desktop_ctx::advertised_app_version`），否则会拿 0.1.0 去比 2.1.1。
   ——**这是既有代码里的一个命名陷阱**（`SystemInfo.app_version` 名不副实），记录在案，
   本票不改（最小改动原则，且它不在本票范围）。
4. **唯一一处生产代码改动**（移动端 `mdns/discovery.rs`）：`start(app_handle: AppHandle)`
   的 `AppHandle` 运行时类型是 `Wry`，无头进程构造不出来。拆成
   `start` / `start_headless` / `start_inner(Option<AppHandle>)` —— 浏览、解析、缓存逻辑
   **完全共用**，只是 `app_handle=None` 时跳过前端 emit（`emit` 原本就是 `let _ =`）。
   理由：发现链路的真实可观测面是服务缓存 `get_services()`，不值得为测试改生产行为。
   （移动端仍是自持业务 App，不受 §5.1 宿主红线约束。）

## 验证

- `cargo test --test mdns_health_flow` 绿（~1.0s，**连跑 3 次稳定**）
- cross-end 全量 **11 个二进制**两次连跑全绿
- 变异自检 1 项：`platform` 断言值改错 → 精确打红
- mDNS 在本机**真实工作**（wlo1 接口组播，发现日志
  `Resolved: BedCode-binblink-PC at 192.168.1.5:<port> (platform=desktop)`）

## 附带修掉的一个测试基建陷阱

**失败路径从 120s 压到 1.0s**：断言 panic 后 unwind 会直接 drop 仍在运行的
`mdns_sd::ServiceDaemon`（join 其收包线程约 2 分钟），且 `WebSocketManager` 的 actix
runtime 跑在**非 daemon OS 线程**上、不 stop supervisor 就不退出。修法：发现器由外层
持有 + `catch_unwind` 后再收尾、`resume_unwind` 重抛（失败信息照常打印，测试仍判失败）。

## 诚实边界

- 依赖**本机组播**：无组播环境（部分 CI 容器）下 15s 内发现不到即 **panic**，
  不静默 skip（与「缺产物显性失败」同口径）。
- `ServiceEvent` 的 `ServiceRemoved` 分支未在本文件触发（需要另一台真实广播端下线），
  只验了「stop 后缓存清空」。