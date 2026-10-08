# 更新日志

本文件记录本项目所有值得关注的变更。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

> 本文档为中文版本；英文版见 [`CHANGELOG.md`](./CHANGELOG.md)（GitHub Release 流程读取该文件）。

## [未发布]

#### 移动端：egress 三档访问策略对齐 + 闸门锁（票 20）

- **收口**：egress 安全闸门（`src-tauri/src/egress.rs`，留宿主裁决 ADR 0022 D5——三档只回答
  「遇到授权记录未覆盖的目标时要不要问」，B1–B6 零命中）：档位→动作映射保持单点
  （`StrategyStep::of`）；写入面保持 `parse_wire`（未知值显性报错，不猜档位）；deny 记录优先于
  一切放行路径（含 always_allow 档）；弹窗超时 fail-closed；`always_allow` 必落审计记录
- **修复**：① `decide` 此前 `strip_prefix("plugin:")` 去前缀，而 `record_grant` / `set_plugin_strategy`
  存带前缀 key → 记录与档位永远匹配不上（即长期以「在途基线」挂账的 6 例失败根因）；带前缀来源
  现保留完整前缀，无前缀来源归一为 `host`。② `EgressSettingsView` 读 `path_prefix` 而 `AuthRecord`
  serde camelCase → 路径粒度恒显示「全部路径」；interface 与读取点统一 camelCase。
  ③ 共享全局 `policy()` 的测试并行互踩 → 加 `POLICY_LOCK` 串行锁确定性（18/18 绿）
- **新防回接锁**：`egress_tier_mapping_single_point_lock.rs`（3 例 + 变异自检 3/3）——映射单点旁路 /
  写入面读面解析 / 安全义务符号（`must_land_auto_allow` / `CONSENT_TIMEOUT` / deny 记录消费）在场
- **前端测试**：`EgressSettingsView.test.ts`（12 例：档位切换 / 记录管理 / 空态加载态 / 异常 / 多来源隔离）
- **门禁**：移动端宿主 `cargo test` 全量绿（320 lib + 全部集成目标）；前端 `pnpm run test:run` 732/732；
  根 eslint 0 error；零 ABI / WIT / wire 变更（宿主内部收口，cross-end-tests 不适用）

#### 移动端：wasm-core fork crate 迁入移动运行时与 16 域 host 原语（票 17 批次 1b）

- **落地**：`bedcode-mobile/packages/bedcode-wasm-core`（`bedcode-wasm-core-mobile`，fork 自桌面整核）
  迁入移动运行时与绑定层——`manager/runtime{,/component.rs,/host_impl/}`：wasmtime Engine/Store/AOT
  缓存、bindgen 换绑移动 WIT v17（16 import / 5 export + 可选 events-binary）、16 域 host 原语
  （auth/bus/config/connection/db/event/fs/http/mdns/notify/peer/platform/storage/terminal_stream/ws/support）。
  宿主引擎调用（auth / egress / peer 四模块 / mdns 守护 / android 平台桥）经新增
  `host_api/ports.rs` 的 `HostEnginePorts` 端口注入（30 方法 + 五个子 trait + `UnimplementedPorts`
  无头占位）——auth 凭据（C4）、egress 安全闸门（D5）、peer 引擎、mDNS 守护单例、重连状态机真源留宿主
- **拆分迁入**：宿主 `wasm_host.rs` 拆为 `host_api/{http_engine,sql_guard}`（HTTP 执行引擎 + SQL
  表名前缀护栏，egress/token 经端口）；`terminal_stream_gateway.rs` 窄转发表迁 crate（Tauri 命令
  薄壳留宿主）；`test_support` 测试支持面（夹具构建器 + mock WS server + `MockPorts` 端口替身，
  `any(test, feature = "test-support")` 门控）。fs_auth 形状漂移裁决：宿主保持自持，经 `FsAuthGate`
  端口（check/check_batch）注入，白名单/弹窗真源不动
- **门禁**：fork crate `cargo test` lib 295 用例 + fork_boundary_lock 3 用例全绿（批次 1 基线 230
  + 新增 65）；桌面 crate 零改动；移动宿主零改动（未接线）。宿主切换（垫片替换）为批次 2b，
  前置裁决三项见票文档 §6.1

#### 移动端：宿主机制面切换至 fork crate（票 17 批次 2b）

- **落地**：宿主 `plugin/` 变转发垫片（`pub use bedcode_wasm_core_mobile::…`）——76+ 处
  `crate::plugin::` 引用路径零改动；`wasm_host` 符号面逐字保真（glob re-export http_engine/sql_guard）。
  宿主侧端口装配 `plugin/host_ports.rs` 注入真引擎（auth C4 / egress D5 / peer 四模块 / mDNS 共享
  守护 / android 桥 / `FsAuthGate`）；`lib.rs` 插件库连接所有权移交 crate `Database` wrapper
  （schema 真源留宿主 `db_schema.rs`）
- **退役（宿主侧）**：`plugin/{wasm_runtime,wasm_host,validation,storage,message_bus}.rs` 与
  `terminal_stream_gateway.rs`（窄转发表迁 crate 根）
- **锁/测试收口**：4 把保留面锁（terminal_link / host_terminal_hooks / auth_orchestration /
  session_control）改钉 fork crate 新真源；`session_http_flow` 换 `http_engine` 端口签名
  （真 `HostPorts`，全局 token JWT 代注语义不变）
- **门禁**：fork crate 295 lib + 3 锁；宿主 245 lib + 全部集成目标（含 4 锁 + session_http_flow）；
  前端 `pnpm run test:run` 732/732；根 eslint 0 error

#### 移动端：业务应用源码目录 `plugins/` → `wasm-apps/`（对齐桌面）

- **重命名**：`bedcode-mobile/plugins/`（ai-chatbox / file-transfer / terminal-session）→
  `bedcode-mobile/wasm-apps/`，与桌面 `wasm-apps/<app-id>/` 同构。「插件」机制词保留：SDK 包、
  WIT / 权限位 / bus·events 话题、运行时 `app_data_dir/plugins`、`resources/plugins/mobile`、
  `src/plugin/`（前端机制）与 `src-tauri/src/plugin/`（Rust 机制）全部不动
- **触点已同步**：CI 插件安装循环（`test.yml` / `release.yml`，working-directory 相对与带前缀两种形态）、
  `scripts/{dev-run.js,plugin-build.js}` 扫描路径、`vitest.config.ts` include、`vite.config.ts`
  chunk 前缀判断、`tailwind.config.js` 内容扫描、4 把防回接锁源码路径字面量、`test_support.rs:52`
  夹具构建路径、8 个 file-transfer 测试相对 import、文档（`AGENTS.md`、`code-map.md`、
  `plugin-dev-mobile.md`、`commands.md`、`wasip3-toolchain.md`）；审计记录
  `.scratch/2026-10-08-mobile-wasm-app-rename/audit.md`（含审查补漏触点 #18-#23、release.yml
  #292-297、code-map #299 修正）
