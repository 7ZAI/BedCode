# mDNS 广播器迁出宿主（advertiser 归入 bedcode-discovery-engine）

> 指令（用户，2026-10-06）：「bedcode-desktop/src-tauri/src/mdns 应该迁移出宿主 到 discover 中」
> 前置：bedcode-discovery-engine 已在 wasm-core-lib-split 票 03/04 承接 host-mdns 能力域
> （浏览/广播双句柄表 + 共享守护），宿主只余「自播面」一个目录未迁。

## 0. 现状事实（2026-10-06 实测）

| 事实 | 落点 |
| --- | --- |
| 宿主残留 `src-tauri/src/mdns/{advertiser.rs,types.rs}`（CRLF，10 个单测 + `service_type_constant_locked` 跨端契约锁） | 作用：ServerSupervisor 启动时把桌面 `_bedcode._tcp.local.` 服务广播到局域网供移动端发现（`bedcode-server-core/supervisor.rs` → `MdnsAdvertiserPort` → 宿主壳 `HostMdnsAdvertiserPort`） |
| `bedcode-discovery-engine` 已有 `engine.rs`（浏览/广播原语 + 共享守护）+ `ports.rs` + `lib.rs`（host-mdns WIT 接线），测试 18 绿 | 基线：`cargo test --lib` 18 passed |
| peer-net 节点身份广播走 `_bedcode-peer._tcp.local.`（`packages/peer-net/discovery.rs:55`）+ 共享守护，与自播面互不相干 | |
| 消费方：`lib.rs`（构造）、`system/app_context.rs`（类型）、`server/ports_impl.rs`（`AdvertiseConfig`）、`src-tauri/tests/` 4 个集成测试（import）、`cross-end-tests/tests/common/desktop_ctx.rs`（import + 构造） | |
| 宿主 `src-tauri/Cargo.toml` 的 `flume` dev-dep 仅 mdns 测试使用；`mdns-sd` 仍被 `HostMdnsPort`（`MdnsPort::shared_daemon` 返回类型）使用 | |

## 1. 决策

- **D1 忠实搬迁、零行为变更**：`advertiser.rs` + `types.rs` 原样迁入 `bedcode-discovery-engine`（CRLF→LF），API / 校验语义 / 错误文案逐字保留；不借机改造（含不顺手并共享守护）。
  - **偏离记录（2026-10-06 16:44，并行会话的后续改造，D1 的「不顺手并共享守护」被推翻）**：自播面登记/停播改走 `engine::advertise_inner` / `stop_advertise_inner`（owner=host），与peer-net 节点身份、插件 advertise **同共享守护**（消灭「双`ServiceDaemon` 同绑 5353」）；连带语义变化：`unregister` 尽力而为（失败仅 warn，靠缓存 TTL 收敛）、不再有 `shutdown` 路径、`is_advertising` 以句柄表 id 为准、注入面由 `MdnsDaemon` 工厂改为 `AdvertiseTarget`（句柄表是进程级单例，无法注入）、`flume` dev-dep 不再需要（D4 的最后一句作废）、附带修掉「旧实现只登记一次、45s 后对端缓存 TTL 过期」病灶。校验语义与错误文案仍逐字保留（D1 的前半段成立）。逐条见 `CHANGELOG*` 的「mDNS 自播面并入共享守护」条目。
- **D2 错误类型自持**：`crate::AppError`（宿主类型）改 `AdvertiserError{InvalidInput, Internal}` + Display——本 crate 生产依赖锁只允许 `["bedcode-host-kit"]`（`crate_boundary_lock`），不能新引 `bedcode-server-base`；文案不变（`服务名不能为空` 等）。
- **D3 调用点全部改显式路径**（不留宿主转发垫片）：`bedcode_discovery_engine::advertiser::MdnsAdvertiser` / `...::types::{AdvertiseConfig, SERVICE_TYPE}`；`cross-end-tests` 新增该 path 依赖。
- **D4 暴露面**：crate 顶层加 `pub mod advertiser; pub mod types;`，lib.rs 分层注释同步；tokio 主依赖补 `sync` feature（`RwLock`），dev-dep 补 `flume = "0.12"`（与 src-tauri / mdns-sd 同源）。
- **D5 跨端契约零改动**：`_bedcode._tcp.local.` 常量、TXT keys（platform / device_name / version）与端口语义不动；移动端 `bedcode-mobile/.../mdns/` 不随动。

## 2. 迁移清单

1. `src-tauri/src/mdns/types.rs` → `packages/bedcode-discovery-engine/src/types.rs`
2. `src-tauri/src/mdns/advertiser.rs` → `packages/bedcode-discovery-engine/src/advertiser.rs`（错误类型替换）
3. crate `lib.rs` / `Cargo.toml` 增量
4. 宿主 `lib.rs`（删 `pub mod mdns;` + 构造点改路径）、`app_context.rs`、`ports_impl.rs`
5. 删 `src-tauri/src/mdns/`；`src-tauri/Cargo.toml` 删 `flume` dev-dep
6. `src-tauri/tests/{pty_session_chain,ws_auth_rules,http_auth_biometric,broadcast_shutdown}.rs` import 改显式路径
7. `cross-end-tests/Cargo.toml` 加依赖 + `tests/common/desktop_ctx.rs` 改路径
8. 文档：`code-map.md`（mdns/ 条目删除、discovery 描述补自播面）、`CHANGELOG.md` / `CHANGELOG_zh.md`

## 3. 验收

> 注：下列计数是**搬迁当时**的基线（27 = 18 引擎 + 9 广告器）；并入共享守护后为 **28**（18 引擎 + 10 广告器），见偏离记录。

- [x] discovery-engine `cargo test --lib` 全绿（27 = 18 引擎 + 9 广告器用例）
- [x] `src-tauri cargo check --lib` 通过（4 个既有警告皆并行会话在途，与本改动无关）
- [x] 4 个集成测试 import 迁移编译通过（`cargo check --test pty_session_chain/ws_auth_rules/http_auth_biometric/broadcast_shutdown`）
- [x] `cargo fmt --check`：discovery-engine crate 零漂移（新文件已按默认 rustfmt 格式化；crate 原文件自身 clean）
- [x] 宿主 `src-tauri/src/` 无 `crate::mdns` / `mod mdns` / `mdns::advertiser` 残留
- [x] code-map / CHANGELOG 双语 / mobile-desktop-auth.md 同步
- [x] `cross-end-tests` 编译验证：`cargo check --tests` 通过（6m17s；锁已登记 discovery-engine 依赖）；`mdns_health_flow.rs` 一条 `let _ =` 警告为既有代码，非本票
- 已知非本改动红：`src-tauri` lib 单测 68/69 —— `server_lib_manifests_have_no_lateral_or_upward_edges` 报 pty-engine 依赖 host-kit 缺登记，系并行 pty 会话在途（锁文件我开工前已 M），不属本票职责

## 4. follow-up：自播面并入共享守护（2026-10-06 执行）

**指令**（用户）：问题「共享守护能否实现不同业务隔离使用 mDNS，还是需要独立服务进程」→ 答：能且已实现（句柄表 owner 仲裁 + 定向事件投递 + 权限门，peer-net 为样板）；不需要独立进程（无「进程死广播仍在」需求；多守护/多进程在 5353 协议层互抢）。→ 用户「按建议执行」。

**D6 自播面收编 engine 共享守护**（behavior change 三处，均在交付说明点名）：
- `MdnsAdvertiser` 不再自建 `ServiceDaemon`：`start` → `engine::advertise_inner(owner="host")`（共享守护注册 + **新增 45s re-announce 续期**，原实现注册后不续期，TTL 后对端缓存会消失——顺带修复）；`stop` → `engine::stop_advertise_inner`（表条目 + 续期取消，unregister 尽力，失败仅 warn）；停止不 shutdown 任何 daemon
- **停播失败语义变更**：原「unregister 失败 → 保留广播状态供重试」→ 新「表条目移除即停播成功」（与插件 advertise 停播语义全局统一）；`stop_failure_keeps_state_for_retry` 保留为新状态机防御测试（生产路径 engine 恒 Ok）
- **注入面换端口**：`DaemonFactory`/`FakeDaemon`/flume dev-dep 删除 → `AdvertiseTarget` 登记层端口（生产 `SharedDaemonTarget` 包 engine + 全局 ports；测试 fake 记调用契约，验证 owner="host" / SERVICE_TYPE / 转义 fullname）；`is_advertising` 真源 = 本地登记 id（论证：owner=host 条目只随本面 stop 消失，purge 不碰，id ≙ 表条目；engine `table_contains` 曾尝试又撤销——测试注入下真表不可观察）
- **装配时序已核实**：`PluginHost::new` → `install_capability_domain_ports` → `host_api::mdns::install` → discovery PORTS；生产 bootstrap 与 cross-end rig 皆在 supervisor 广播之前装配 ✓（peer-net 同约束）

**验收**：crate `cargo test --lib` 28 passed（18 engine + 10 advertiser）；`cargo fmt --check` 零漂移；src-tauri `cargo check --lib` 通过；host 调用点零改动（`MdnsAdvertiser::{new,start,stop,is_advertising}` 签名不变）