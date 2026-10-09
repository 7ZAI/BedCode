# 更新日志

本文件记录本项目所有值得关注的变更。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

> 本文档为中文版本；英文版见 [`CHANGELOG.md`](./CHANGELOG.md)（GitHub Release 流程读取该文件）。

## [未发布]

#### 文件传输：双端共享业务核 `packages/bedcode-file-transfer-core`（ADR 0044）

- **新建双端共享业务核 crate**：文件传输业务实现（任务台账归约 / 重试 + 发送闸门 + 拉取意图判据 /
  共享根注册表 / 接收设置 / 会话表，约占两端各端代码 70%）收敛为**双端同一份**；每个端差异
  都表达为端口 trait（`ports.rs`，差异面唯一落点：SQL 表 vs KV 持久化、`path` vs `safTreeUri`
  wire 形状、节点电源、落点策略、平台选择、信任决策路径）——零 SDK / 零 WIT / 零平台依赖，
  **零产品身份字面量**（插件 id 经 `PluginIdentity` 运行期注入；核内 `boundary_lock.rs`）
- **双端退化为薄层**：各 wasm app 保留 `adapters.rs`（自家 SDK trait → 核端口的 1:1 委派，
  零额外判据）+ 签名不变的模块包装，`peer.rs` 与全部前端代码零改动（移动端页面按要求不动）；
  接线防漂移锁（`src/wiring_lock.rs`，5 例 + 变异自检 4/4）钉住核与端 app 的边界
- **桌面行为对齐（用户裁决 B）**：桌面 `peer.rs` 按移动端票 08 修正语义演进——引擎事件即
  `pull-started` 建行锚点、重试判据前置、发送闸门、排队批派发失败落终态行；旧快照通路
  （`merge_snapshot` / `prune_absent` / `reconcile_diff`）保留承担对账校正
- **门禁**：核 crate 72 全绿（含边界锁 + 接线锁）；函数级等价校验（核内 23 函数 vs 两端基线）PASS；
  桌面插件 crate 47 全绿；移动插件 crate 29 全绿 + 产物重建且 host import 集合与改动前同集合
  （未新增 import）；桌面产物重建被在途 `manifest-gen` 权限表阻塞 + 本机缺 wasip3 工具链，留 T6

#### 移动端：WS 出站连接引擎抽根为 `packages/bedcode-ws-client-engine` 能力 crate（ADR 0043）

- **新能力 crate `packages/bedcode-ws-client-engine`**：移动端 `host-websocket` 客户端域机制
  （约 1,350 行：句柄表 + 属主仲裁 / reader-writer 双任务 / 心跳与静默判死 / 退避自动重连 /
  帧信封 / 停用回收）自 fork crate（`bedcode-mobile/packages/bedcode-wasm-core/.../host_impl/ws.rs`）
  抽出为通用 crate，默认形态 = **纯引擎 + 端口抽象**——零 WIT / 零 SDK / 零平台（tauri）/
  零机制内核（host-kit）依赖。平台差异面经 7 方法端口 `WsClientPorts` 注入：权限门
  （`ws:client`，fail-closed）/ 总线 JSON 与二进制投递（topic 由引擎拼好）/ 宿主运行时任务
  派生（可取消 `WsTask`，运行期句柄不外泄）/ jwt 代发 token（凭据不落插件）/ 重连策略与
  钳制边界（全局退避单一事实源仍在宿主）
- **移动端收窄为薄适配器 + `MobileWsClientPorts`**：5 原语（connect / send-text / send-binary /
  close / is-connected）与 `purge_for_plugin` 签名及错误文案逐字保留；同步↔异步桥
  （`guarded_host_call` + `block_on_async`）留在宿主侧。**零 ABI / 零 WIT 变更**（本任务不触碰
  `bedcode.wit` 与 SDK `abi.rs`）**且行为零变化**：事件 topic、帧信封形状、权限位、fail-closed
  与停用回收语义逐字保留
- **wire 契约自持 + 漂移锁**：`src/wire.rs` 自持副本（事件名 / 属主私有 topic 拼法 / 帧 kind
  与头长 / `ws:client` 字面量），`wire::drift_lock` 与移动 SDK 源文件（`host/ws.rs` /
  `permission.rs`）文本级比对——任一侧漂移即测红；插件消费面（`parse_ws_frame` / `HostWs`）
  仍住移动 SDK
- **治理与防回接**：crate 内 `boundary_lock.rs`（生产源码零平台 / SDK / wasm-core / host-kit /
  WIT 绑定层针脚，生产清单零内部 crate，单测限 `src/` 内）；登记进桌面
  `capability_crates_no_product_ids` 扫描面；fork 边界锁共享锚点白名单 4 家 → 5 家
  （+ `bedcode-ws-client-engine`，与 ADR 0042 的 discovery-engine 同理由）；移动 ws 域边界锁
  修陈旧实现路径 + 扫描面扩到引擎 crate，并加目标文件存在性断言（陈旧路径会让锁静默空转）
- **门禁**：新 crate `cargo test` **20 绿**（引擎 13 + 漂移锁 3 + 边界锁 4）；fork crate
  `cargo test --features test-support` **285 + 4 + 4 绿**（含真实组件
  `ws_client_domain_full_loop_with_real_component` 全链路回归）；移动宿主 `cargo test`
  全量；变异自检 3/3（漂移锁 / 边界锁 / 适配器映射）


#### 桌面+移动：双端共享 mDNS 引擎（ADR 0042，M1–M4）

- **共享引擎** `packages/bedcode-discovery-engine` 成为双端唯一的 mDNS 引擎（引擎机制 + 双句柄表
  + 属主仲裁；`desktop-host` feature 门控 WIT 绑定层，默认形态零 WIT）。新增 `set_daemon_init_hook`
  平台钩子 / `pub daemon_if_initialized`；`register_host_service` 去掉端口参数（NullTask 占位——
  桌面 `MdnsPort` 实现同步简化）
- **移动端** fork crate `host_impl/mdns.rs`（821→~230 行）重写为薄转发层 + `MobileDiscoveryPorts`
  （权限门 = manifest / 总线发布 / 节点 ID 自播回显过滤 / 宿主运行时任务派生，替换历史
  std-thread 阻塞 recv 循环）；宿主 `mdns/engine.rs` **删除**——守护单例真源移入共享引擎（Android
  多播锁经宿主 setup 装配的 init 钩子获取）；`HostEnginePorts::{mdns_daemon, mdns_daemon_if_initialized,
  mdns_reannounce_interval}` 退役；mdns 事件 topic wire 统一为 `<owner>::mdns:found|lost`
  （ABI 不变；file-transfer 插件 Rust 订阅字面量与 SDK/WIT 注释同步迁移）
- **peer-net**：`spawn_peer_mdns_advertiser` + `DiscoveryAdvertiser` 删除（零生产消费者；广告面由
  宿主自播 / 插件 advertise 在共享守护上覆盖）
- 门禁：移动端 fork crate `cargo test --features test-support --lib` **290 全绿**（清偿上一条票 18
  的「暂挂」注记）、移动端宿主 245 绿、桌面 discovery-engine 31 绿、peer-net 101 绿；移动端
  `ServiceDaemon::new()` 代码层归零；旧 `mdns:found.<owner>` 字面量扫描归零

#### 桌面+移动：host_api 共享实现核批次 2——bus 域语义（票 18）

- **bus 语义抽取**至 `packages/bedcode-host-api-core::bus`：topic 形态机制（`owned_topic` /
  `topic_owner` / 回复道 / legacy 形态识别——宿主侧单点，guest 侧拷贝留在桌面 SDK；票 19 Part B
  对照锁将钉住两份一致）+ 审计票 05 三道门禁（命名空间 / 订阅面 / 互调门）+ 发布判定链
  （权限位 → 严格 JSON → 命名空间 → 互调门 → 投递）。队列与订阅簿留各端
- **两端策略分叉由端口承载**：桌面无权限位（topic 形态即 ACL）→ `Option<&PermissionGate>` 传
  `None`；移动查 `PERMISSION_BUS`；桌面互调门走 core-security 授权框架，移动恒放行（WIT v17
  无 host-api-call）
- **移动行为对齐**（此前移动总线无任何门禁——双份漂移税实例）：发布/订阅过命名空间门、回复道
  + legacy 形态订阅显性拒绝、退订仅命名空间门（清理幂等）、非法 JSON 发布由「降级原始串」改为
  显性拒绝（既有移动插件经 SDK `serde_json::Value` 走公开道——实测 file-transfer 零回归）
- **桌面** adapter 重写，绑定层与测试套件逐字保真（已对 HEAD 核验）；门禁：`cargo test` 全量
  677 绿（+1 既有 perf 红基线）、无头编译过、ABI / WIT / world 零字节变动
- **移动**：fork crate lib 编译通过；全量测试门禁待并行「双端共享 lib M3」会话收口后补跑
  （其在途夹具面当前使 crate 测试构建编译红）

#### 桌面+移动：host_api 共享实现核批次 1——storage 域（票 18）

- **新建 crate** `packages/bedcode-host-api-core`（ADR 0040 第二步）：无 WIT 依赖域的 host_api
  机制实现层，「实现层 + 各端 adapter」两层化；仅机制级依赖（serde_json / tracing——禁 SDK /
  tauri / tokio），由新建 crate 边界锁强制（变异自检 2/2）
- **storage 域抽取**：权限门（权限词汇经参数传入）→ 系统空间纵深守卫 → 能力路由（桌面独有，
  端口默认 `None`）→ 键值原语（serde_json 规范形）；`SYSTEM_PLUGIN_ID` 真源随实现层上移，
  双端经 re-export 保既有路径
- **桌面** `bedcode-wasm-core`：`host_api/storage.rs` 变薄 adapter（`SqlitePorts` → 共享核端口），
  域函数签名与 guest 可见错误文本逐字保留。门禁：`cargo test` 全量 677 绿（+1 既有 perf 红基线）、
  `--no-default-features` 无头编译通过、ABI / WIT / world 零字节变动
- **移动** fork crate：`host_impl/storage.rs` 同形 adapter；**行为对齐**——系统空间纵深守卫自本批
  起对移动端生效（此前移动缺该守卫，双份漂移税实例）；`set()` 的 JSON 解析移到权限门之前
  （仅边缘入参错误文本变化，授权路径零变化）。移动全量门禁暂挂：并行在途「双端共享 lib M3」
  （mdns → discovery-engine）使 fork crate 处中间态；storage 侧 3 文件在该基线下名字解析零报错

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
