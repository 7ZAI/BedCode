# wasm-core 纯净性收口：引擎面（pty / auth / session 桥接）迁出 bedcode-wasm-core

> Status: ready-for-agent（用户 2026-10-06 拍板方向，终裁已定：pty → 能力 crate；auth/session 桥接 → 回宿主 lib）
> Date: 2026-10-06
> 分支：dev（**仅桌面端**；移动端零改动，ADR 0018 契约独立）
> 用户诉求：`bedcode-wasm-core` 应只含 **wasm 相关机制**；pty / authcenter 等其他功能的
> 代码不应留在其中——保证 crate 单一性 / 可复用性 / 通用性。
> 前作：`.scratch/2026-10-06-wasm-core-whole-crate/`（整核抽出，ADR 0037，已实施）；
> 能力域先例：`.scratch/2026-10-04-wasm-core-lib-split/`（ADR 0035，mdns/ws/peer/http 四域
> 迁出为能力 crate，55 原语）。
> 边界单一事实源：ADR 0022 / ADR 0035 / ADR 0036 / ADR 0037；本 spec 是 0037 的**后继细化**
> （0037 spec §8 Out of scope 第 3 项「L2 interface 出 core」在此兑现第一步）。

---

## 1. Problem Statement

### 1.1 现状：wasm-core 里混着三类「非机制」代码

`bedcode-wasm-core`（149 rs 文件，≈ 3,454 顶层行 + host_api 9,475 + pty 2,469 + …）当前按
ADR 0037「引擎归内核」原则把引擎面一并搬入。用户指出：**crate 应只装 wasm 机制**——引擎面
不是 wasm 机制，留着会污染 crate 的单一职责，且让第三方宿主想复用插件机制时被迫背上
portable_pty / 认证中心桥接等无关引擎。

实测三类「非机制」代码：

| 组 | 模块 | 行数 | 性质 |
| --- | --- | --- | --- |
| **A. pty 引擎面** | `src/pty/`（pty_process / pty_ring / pty_reader / output_sink / lifecycle / wsl） | 2,469 | portable_pty 封装，**零 wasm 依赖**（只收 argv 的纯引擎 + WSL 发行版列举） |
| | `host_api/pty.rs` + `host_api/pty_output.rs` | 599 + 337 | **host-pty WIT 绑定面**（是机制，但实现与引擎同侧） |
| | `enums/pty_status.rs`（PtySessionStatus） | 33 | pty 引擎词汇 |
| **B. 认证中心桥接** | `utils/auth/auth_center.rs` | 216 | 认证中心宿主桥接门（问中心一次 + deny_kind 三态） |
| | `utils/auth/test_tokens.rs` | 130 | 测试夹具 |
| | `host_api/auth_center.rs` | 299 | **单中心注册表**（ADR 0031）——安全闸门（ADR 0022 四类薄壳②） |
| | `host_api/auth.rs` | 763 | **host-auth WIT 绑定面**（secret-store 密钥托管 / 生物验签链路原语）——机制面，真源在主库 |
| **C. 会话窄转发** | `utils/session_gateway.rs` | 254 | 零解析窄转发（ADR 0022 四类薄壳④） |

### 1.2 与用户直觉不同的点（必须呈报）

用户点名「pty、authcenter」。深挖后**两组性质不同**：

- **pty（A 组）**：确属「引擎面」，几乎零 wasm 依赖（只碰 enums + system/config 2 个读点 +
  `create_command`）。迁出形态干净，与 ADR 0035 四域先例完全同构 → **本期迁出**。
- **auth / session（B/C 组）**：**不是业务功能，而是宿主安全薄壳**（ADR 0022 明示宿主可留
  四类薄壳：② 安全闸门 ④ 零解析窄转发）。`auth_center` 是「认证中心注册表 + fail-closed
  裁决」，`host_api/auth.rs` 的 secret-store 机制面**真源在主库**（ADR 0036「机制与真源
  同侧」）。把它们挪出 wasm-core 需要：(a) 先端口化 `WasmHostContext` 访问（否则新 crate
  反向依赖 wasm-core，Cargo 环）；(b) 触碰 ADR 0036 的「真源同侧」裁决；(c) 与
  `manager/host` 激活链 20+ 处引用纠缠。**成本与风险不成比例，且会不会让 wasm-core
  更「机制纯净」存疑**——auth 本来就是机制（安全闸门）。

**→ Q1（终裁点）**：本期范围 = 只拆 A 组（pty），还是 A + B/C 全拆，还是 A + B/C 中再细拆
（如只拆 `utils/auth/auth_center.rs` 的桥接、留注册表与 WIT 面）？

### 1.3 为什么「能力域模式」可行（先例已铺路）

`component.rs` 的两段式装配注释明示：「**core 本地表**……域迁出时，其那一行与对应实现
一起搬进能力 crate，从此由第 2 段（`host_module_registry` 自动收集）接管」。这行注释就是
为 pty/auth 这类域预留的出口。四个能力域（ws/peer/http/mdns）已完整走通：新 crate 自带
provider 侧 `bindgen!` + `HostModule` 自报（`inventory::submit!`），wasm-core 只留薄
adapter（`host_api/ws.rs` 161 行先例）+ 开机一次装配。

### 1.4 既定基线（不回归）

- 不推翻 ADR 0035/0036/0037 的裁决；不改 WIT / 不触发 ABI bump（world 22 import 一个不动）。
- 不碰移动端（ADR 0018）。
- db 保留（ADR 0036「机制与真源同侧」——用户已确认不动 db）。
- lib 垫片纪律、crate_boundary_lock、hot_path_logging_lock、capabilities_lock 同步更新。

---

## 1.5 用户终裁（2026-10-06 两轮指令）

1. **pty → 独立能力 crate** `bedcode-pty-engine`（上一轮「按你的建议来做」＝接受档一）。
2. **auth / session 桥接 → 回宿主 lib**（本轮明确：「应该拆到宿主层面，不应该留在 wasm core 中」）。

   回到 lib 的不是「能力 crate」而是**宿主薄壳原位**（ADR 0022 四类薄壳②安全闸门④窄转发
   本来就是宿主允许自留的）。依赖方向干净：lib → crate 单向（沿用 0037 垫片方向），
   桥接在 lib 里引用 `bedcode_wasm_core::*` 不构成环。

### 1.6 外部审查发现（P0-1 ~ P0-3，与本票同源，一并纳入）

另一会话审查发现三处「业务代码落在机制 crate 里」的实例，与本票目标一致，纳入范围：

| # | 位置 | 问题 | 修复方案 |
| --- | --- | --- | --- |
| **P0-1** | `utils/session_gateway.rs`（254 行） | 硬编码 8 个产品互调 api 名（`com.bedcode.terminal-session.session-*`）→ B1；具名产品参数（`start`/`resize`/`special_key`…）→ B4；`running_views` 发 `{"filter":"running"}` 问产品判据 → 擦 B6。**已不是零解析窄转发**（实测本文件注释自认层接口不持业务类型，但 api 名与参数形状是产品 wire） | **随票 05 回 lib**（与 auth 桥接同机；回 lib 后是宿主自己的产品窄转发，不再污染机制 crate） |
| **P0-2** | `security/fs_auth.rs:116-147` `FIRST_PARTY_TRUSTED_DIRS` | 安全豁免表硬编码两个产品插件 id（agent-hub / terminal-session）及产品目录约定 → B1 + B5；「免弹窗放行 fs」的授权键是产品语义 | **豁免表从 const 改为宿主注入**：判定逻辑（`first_party_dir_matches_with_home`）留 crate（机制），产品清单由 lib 装配时传入 `FsAuthChecker`（构造参数 / 装配链）；`auth_policy::overview` 读模型经 checker 取表。**新票 08** |
| **P0-3** | `system/config.rs` `AppConfig.ui`（UiConfig，~30 行 schema + 默认值） | 可复用插件机制 crate 里装着产品外观 schema（theme/terminal_font/language/bg_image/animations）→ B1；wasm-core 内**零消费**（host-config 原语只读 `network.port`，实测） | **UiConfig 从 AppConfig 拆出 → lib**：`AppConfig` 本体留 crate（host-config WIT 原语需要），`ui` 段迁 lib 侧产品配置面（`save_app_settings` 等命令改读 lib 配置）；engine 段（`network`/`channels`/`terminal`/`log`）留。**新票 09** |

> 票序：P0-1 并入票 05；P0-2 为票 08；P0-3 为票 09。三票都在「wasm-core 只含机制」的同一裁决下，
> 顺序执行不互相阻塞（P0-3 的 UiConfig 零消费，不依赖任何搬迁结果）。

**范围落定**：

| 拆出 | 去向 | 形态 |
| --- | --- | --- |
| `src/pty/`（引擎 2,469 行）+ `host_api/pty.rs` + `pty_output.rs`（WIT 面）+ `enums/pty_status.rs` | `bedcode-pty-engine`（bedcode-desktop/packages/） | 能力域 crate（provider bindgen + HostModule 自报，ADR 0035 形态） |
| `utils/auth/auth_center.rs` 的 `enforce_connection_policy` / `session_active`（裁决面/桥接门） | 宿主 `src-tauri/src/utils/auth/` | 实现回 lib（不再是垫片） |
| `utils/session_gateway.rs`（窄转发 254 行） | 宿主 `src-tauri/src/utils/` | 实现回 lib |
| `utils/auth/test_tokens.rs`（测试夹具 130 行） | 宿主 `src-tauri/src/utils/auth/` | 实现回 lib（`#[cfg(test)]` 可恢复） |
| `utils/auth/auth_center.rs::invoke_auth_method`（WIT 原语实现） | **留 wasm-core**，并入 `host_api/auth_center.rs` | 机制（host-auth `auth-method-invoke` 实现链，`host_api/auth.rs:329` 生产调用） |

**留在 wasm-core 的边界**（不可回 lib：WIT 绑定面，component.rs `impl Host` 在
wasm-core 内直接调用；回 lib = wasm-core → lib 反向依赖 = Cargo 环）：

| 模块 | 理由 |
| --- | --- |
| `host_api/auth.rs`（763 行） | host-auth WIT 绑定面（secret-store 密钥托管），真源在主库（ADR 0036「机制与真源同侧」） |
| `host_api/auth_center.rs`（299 行） | host-auth 注册面（auth-center-register 等 4 个 WIT 函数分派 + 单中心注册表）；被 `host_api/auth.rs` 与 `manager/host/{activation,boot}.rs` 消费 |
| `utils/auth/auth_center.rs` 的 `invoke_auth_method` | **host-auth WIT 原语 `auth-method-invoke` 的实现链**（`host_api/auth.rs:329` 生产调用）——机制非薄壳，并入 `host_api/auth_center.rs` |

> **auth 切分线（终版）**：`invoke_auth_method`（WIT 原语实现）留 wasm-core 并入
> `host_api/auth_center.rs`；`enforce_connection_policy`（server 连接面裁决，消费方
> `server/ports_impl.rs:74`）与 `session_active`（窄转发激活门，消费方 `session_gateway`）
> 是**宿主薄壳**，回 lib。回 lib 后它们仍经 `bedcode_wasm_core::host_api::auth_center`
> （注册表）与 `bedcode_wasm_core::intercall`（互调客户端）取用——lib 单向依赖成立，无需端口。

## 2. Solution（终裁版）

### 2.1 pty 域整组迁出（能力域 crate）

**目标形态**：新建能力域 crate `bedcode-pty-engine`（`bedcode-desktop/packages/`），含：

```
bedcode-pty-engine/
├── Cargo.toml          # portable_pty + tokio + encoding_rs + bedcode-host-kit + wit-bindgen + wasmtime 48 + bedcode-plugin-api + bedcode-server-base
└── src/
    ├── plugin_binding.rs   # provider 侧 bindgen!（host-pty）+ impl Host for WasmPluginState + HostModule 自报（抄 ws/peer 先例）
    ├── pty_process.rs / pty_ring.rs / pty_reader.rs / output_sink.rs / lifecycle.rs  # 引擎本体（零 wasm 依赖）
    ├── wsl.rs              # WSL 发行版列举（host-platform 原生语的引擎面）——⚠ 被 host_api/platform.rs 消费，需处理（见 §3 边界 E3）
    └── pty_status.rs       # PtySessionStatus 词汇随引擎
```

**wasm-core 侧**：
- 删 `src/pty/`（引擎 + wsl）、`host_api/pty.rs`、`host_api/pty_output.rs`、`enums/pty_status.rs`；
- `component.rs` core 本地表删 host_pty 行 → 第二段 `payload` 自动注册接管；
- `host_api.rs` 增 one-line `pty::install(...)`（如需要端口注入，抄 ws 先例）；
- `HOST_MODULES` 白名单 + 强制引用行更新（`use bedcode_pty_engine as _;`）；
- `crate_boundary_lock.rs` SPLIT_CRATES 登记；
- lib 垫片 `lib.rs` / `pty.rs` 改指 `bedcode_pty_engine`（或经 wasm-core `pub use` 保路径，二选一见 §4 决策 D2）。

### 2.2 auth / session 桥接回宿主 lib

`utils/auth/auth_center.rs` 的裁决面/桥接门 + `utils/session_gateway.rs` + `test_tokens.rs`
从 wasm-core 实现回迁到宿主 `src-tauri/src/`（去掉垫片化），wasm-core 删除对应模块面；
`invoke_auth_method` 并入 `host_api/auth_center.rs`（机制侧）；lib 的 `server/ports_impl.rs` /
`lib.rs` 调用点改直引用。`host_api/auth*.rs` 留 wasm-core（WIT 绑定面）。

### 2.3 不拆的

- `db/`（ADR 0036）；`manager/` `security/` `permission/` `bus/` `intercall/` `runtime_util/`
  `monitor/` `storage/` `host_context_registry/` `config/` `crate_boundary_lock/` = wasm 机制本体；
- `host_api/` 其余域（fs/platform/process/connection/crypto…）——WIT 绑定面 = 机制（若将来
  再收口另立 spec，本期不碰）。

---

## 3. 边界详查（档一 pty 迁移的依赖清单，实测）

| # | 依赖 | 处理 |
| --- | --- | --- |
| E1 | pty 引擎读 `AppConfig::global()` 2 处（pty_process.rs:150 `lifecycle_capacity`、pty_reader.rs:45 `read_buffer_size`） | 迁出后 AppConfig 在 wasm-core——新 crate 不能依赖 wasm-core（Cargo 环）。**改构造参数注入**（`PtyEngineConfig { lifecycle_capacity, read_buffer_size }`），wasm-core 装配时从 AppConfig 取值传入 |
| E2 | pty 引擎用 `system::process::create_command`（pty_process.rs 及 Windows 路径） | `create_command`（22 行，Windows CREATE_NO_WINDOW）迁入新 crate 或下沉 `bedcode-server-base`（它还被子 `wsl_fs.rs` 消费——见 E3 判定） |
| E3 | `host_api/platform.rs` 用 `crate::pty::{list_distributions, WslDistro}`（host-platform 的 `wsl-distros` 原语） | ⚠ **最纠缠点**：platform 域（留 wasm-core）依赖 pty 引擎的 wsl 面。三条路：① wsl 面留在 wasm-core（挪到 `host_api/` 或 `system/`，平台域自持引擎面——合理：wsl_distros 是平台事实非 pty 能力）；② 新 crate 暴露 wsl 模块，wasm-core 依赖 pty-engine crate（**wasm-core 反向依赖引擎 crate**，可接受——引擎不依赖 wasm-core 即可）；③ 端口化。**推荐 ①**（wsl 只有 150 行、依赖 create_command + encoding_rs，与 pty 引擎正交） |
| E4 | `bus::MessageBus`（pty_output.rs 投递输出事件）+ `bedcode_plugin_api::host::bus::owned_topic` | 新 crate 引 bedcode-server-base 的 BusPort 端口（ws 先例）或直接依赖 wasm-core 的 bus？——wasm-core 依赖方向：**引擎 crate 只能向下依赖 base/kit/plugin-api，不得依赖 wasm-core**。输出投递改经端口（BusPort 已在 base） |
| E5 | 测试：`manager/runtime/tests/{pty_e2e,terminal_output_perf}.rs` + `host_harness` | 迁入新 crate 的测试面；宿主侧集成测试（pty_session_chain）改指新符号 |
| E6 | `lib.rs` / `enums.rs` 再导出（`PtySessionStatus` 经 `crate::pty` 与 `crate::enums` 两处） | 垫片决策见 D2 |

---

## 4. 关键决策（待 Q1/Q2 终裁）

| # | 决策 | 默认倾向 |
| --- | --- | --- |
| D1 | 范围：A / A+B / A+B+C | **A（pty）起步**；B/C 单独立项评估（auth 属安全薄壳，拆了要端口化 WasmHostContext，成本高） |
| D2 | lib 垫片路径：wasm-core 兜底 `pub use bedcode_pty_engine`（lib 零改动） vs lib 直接改指新 crate | 前者（lib 集成测试/cross-end 零改动，符合 0037 垫片惯例） |
| D3 | wsl 面 ①②③ | ①（wsl 留 wasm-core，platform 域自持） |
| D4 | create_command 去向 | 下沉 bedcode-server-base（wsl_fs 与 pty 引擎都要用） |
| D5 | AppConfig 2 读点 | 构造参数注入 `PtyEngineConfig`（引擎保持零业务、零全局） |
| D6 | 引擎 crate 名 | `bedcode-pty-engine`（对齐 discovery-engine / crypto-engine） |
| D7 | 新 crate 的 host-pty impl 引 `wasmtime` 与 `wit-bindgen` | 是（ADT 0035 D5 先例：能力 crate 自带 provider 侧 bindgen） |

---

## 5. 分期（串行票，每票可编译可测）

### 5.0 用户终裁补充（2026-10-06 第三轮：测试资产重构）

**裁定**：「session e2e 应该迁移到宿主侧，wasm-core 内不应该包含任何跨 crate 集成测试」。
范围确认：**全部**加载真实产品插件产物（`resources/plugins/desktop/*.wasm`）的
wasm-core 测试都迁宿主侧 `src-tauri/tests/`（与 pty_session_chain 并列，独立测试二进制）；
只测机制本体、用本地 SDK fixture（`plugin-sdk-fixtures`）的测试留 wasm-core。

| 文件 | 行数 | 产物 | 迁出/留下 |
| --- | --- | --- | --- |
| `manager/runtime/tests/session_e2e.rs` | 3,720 | terminal-session + consent-consumer + file-transfer | **迁出**（用户点名） |
| `manager/runtime/tests/ws_e2e.rs` | 1,953 | terminal-session + ws-test | **迁出** |
| `manager/host/tests/system_component_test.rs` | 957 | terminal-session + system-test | **迁出** |
| `manager/runtime/tests/task_e2e.rs` | 404 | ai-chatbox（其中 1 用例） | **迁出**（ai-chatbox 用例；其余 SDK fixture 用例留） |
| `manager/runtime/tests/auth_center_perf.rs` | 167 | terminal-session（#[ignore] 探针） | **迁出** |
| `manager/runtime/tests/ws_output_perf.rs` | 392 | terminal-session | **迁出** |
| `manager/runtime/tests/a03_probe.rs` | 591 | terminal-session + wasip3 fixture | **迁出**（fixture 部分留 wasm-core，产物闭环用例随迁） |
| pty_e2e / sdk_e2e / wasi_e2e / component_e2e / engine_limits / task_e2e（其余） | — | SDK fixture | **留 wasm-core**（测机制本体） |

**执行策略（用户确认）**：先迁 `session_e2e`（最大最典型，验证迁移模式）→ 验证 `cargo test`
两目标全绿 → 再批量迁其余。**迁移模式**：
- 依赖 wasm-core 内部脚手架（`scaffold.rs` 的 setup_host / `fixture_build` / `use super::*` 继承）
  的部分重构为 lib 公开面（`PluginHost::new` + lib 侧测试 helper）；
- 对已回 lib 的桥接（`session_gateway` / `auth_center` / `test_tokens`）直接用 lib 真源
  （不再重建 wasm-core 内等价物）；
- 产品 api 名 / 插件 id 常量随测试归宿主，wasm-core 生产面与测试面都不再持产品 wire
  （P0-1 彻底闭环）；
- l2_gating_test 锁的扫描根与白名单随测试迁出同步收缩。

| 票 | 内容 | 验收 |
| --- | --- | --- |
| 01 | **量测复核 + 认领在途改动**（gate）：本 spec 行数/路径按工作区实测；确认无并行会话在改 wasm-core | spec 数据与工作区一致 |
| 02 | 🟢 `bedcode-pty-engine` 骨架 crate + 引擎机械搬迁：pty_process/pty_ring/pty_reader/output_sink/lifecycle + PtySessionStatus 迁入，E1/D5 配置参数化 | crate 根 `cargo check` 绿；wasm-core 临时 `pub use` 垫片保编译 |
| 03 | boundary 收口：E3 wsl 安置 + E4 输出投递端口化 + E2/D4 create_command 下沉 | 两 crate 各自 `cargo check` 绿，互不循环依赖 |
| 04 | **WIT 绑定面迁出**：host_api/pty.rs + pty_output.rs → 新 crate plugin_binding（provider bindgen + HostModule 自报），component.rs 本地表删行 + 白名单 + 强制引用行 | 宿主与引擎两 crate 全量测试绿（含 pty_e2e 迁入） |
| 05 | **auth/session 桥接回宿主 lib**：`utils/auth/auth_center.rs` 的 `enforce_connection_policy` / `session_active` + `utils/session_gateway.rs` 实现迁回 `src-tauri/src/`（去掉垫片化）；**P0-1 同机**：session_gateway 的 8 个产品 api 名与参数形状随迁归宿主，机制 crate 不再被产品 wire 污染；`invoke_auth_method` 并入 `host_api/auth_center.rs`（机制侧，pub）；`test_tokens` **留 wasm-core 常编译**（依赖 wasm-core 内部 `test_seed_plugin_secret`，是产物闭环测试夹具非宿主功能；lib 经垫片消费）；lib 的 `server/ports_impl.rs` / `lib.rs` 调用点改直引用；`host_api/auth*.rs` 留 wasm-core；`block_on_async` / `host_api::auth_center` 模块 / `test_seed_plugin_secret` 提 pub | lib `cargo check` 绿；wasm-core `cargo check` 绿 |
| **05b** | **测试资产重构（用户裁定）：session_e2e 迁宿主侧**——3,720 行迁 `src-tauri/tests/session_e2e.rs`，脚手架重构为 lib 公开面（`PluginHost::new` + lib 侧 helper）；对已回 lib 的桥接直接 lib 真源；产品常量归宿主 | src-tauri `cargo test --test session_e2e` 全绿；wasm-core `cargo check --lib --tests` 绿（session_e2e 移除后无残留引用） |
| **05c** | **批量迁其余产品产物测试**：ws_e2e / system_component_test / task_e2e(ai-chatbox) / auth_center_perf / ws_output_perf / a03_probe 迁宿主 tests/；SDK fixture 用例留 wasm-core；l2_gating_test 扫描根与白名单收缩 | 全部迁出的 `src-tauri/tests/*` 各自绿；wasm-core `cargo test --lib` 全量绿（迁移后基线） |
| 06 | 契约收口：锁更新（crate_boundary_lock / capabilities_lock / hot_path_logging_lock / 反双份锁）+ 文档（code-map / CHANGELOG 双语）+ ADR 0038 落档 | `rg` 全仓核验桥接引用完整迁移 |
| 07 | **contract** 独立验收：crate 根 `cargo test` + src-tauri `cargo test` 全量 + cross-end-tests | 与基线零回归；lens_diagnostics mode=all 无 blocker |
| 08 | **P0-2：fs_auth 豁免表宿主注入**：`FIRST_PARTY_TRUSTED_DIRS`（const 含 agent-hub / terminal-session 产品插件 id + 目录约定）改为 lib 装配时注入 `FsAuthChecker`（构造参数 / 装配链），判定逻辑（`first_party_dir_matches_with_home`）留 crate（机制）；`auth_policy::overview` 读模型与 `FirstPartyDirEntry` 只读投影改经 checker 取表 | 无头测试（`None` 注入）语义逐字不变（无豁免表 = 无免弹窗项）；两 crate `cargo test` 全绿；前端 authPolicy 读模型对照零漂移 |
| 09 | **P0-3：UiConfig 拆出 AppConfig**：`system/config.rs` 的 `ui` 段（theme/theme_palette/font/language/bg_image/animations 等产品 schema）迁 lib 侧产品配置面；engine 段（network/channels/terminal/log）留 crate；`save_app_settings` 等 lib 命令改读 lib 配置；host-config 原语行为不变（实测只读 `network.port`） | lib `cargo test` 绿；wasm-core `cargo test` 绿；旧 config.json 含 ui 段照读、新写不含（向后兼容） |

---

## 6. 红线与门禁自检

### 6.0 票 05b 施工图（交接文档，2026-10-06 写定，新会话动工）

**目标**：`session_e2e`（3,720 行）从 wasm-core 迁至宿主 `src-tauri/tests/session_e2e.rs`，
wasm-core 内不再含任何跨 crate 集成测试。

**施工前必读**：本 spec §5.0（裁定 + 迁移清单）+ 本图；动工从 git status 认领在途改动开始。

#### 6.0.1 test_support 基建上提（先行步骤，独立可验收）

在 wasm-core 建 **常编译公开测试基建** `src/test_support.rs`（`pub mod test_support`，lib.rs 声明），
承载 lib 集成测试需要的全部脚手架（按 test_tokens 常编译先例，ADR 0037 D7 同款裁决；
不引入 feature 机制——项目至今无 feature 门控，保持简单）：

| 上提符号 | 来源（现状） | 迁入形态 |
| --- | --- | --- |
| `setup_wasm_runtime()` / `setup_wasm_runtime_with_config(core)` | `manager/runtime.rs` mod tests 私有（~100 行） | 完整迁入，内部 `use crate::…` 不变（同 crate 可直接引用）；`CONFIG_INIT`/`aot_cache_dir`/`install_capability_domain_ports`/`set_task_engine` 装配逻辑原样 |
| `plugin_db_root()` | 同上 | 迁入 |
| `session_plugin_db_guard()` | 同上（`SESSION_PLUGIN_DB_LOCK` 静态） | 迁入 |
| `registry_gate()` / `hold_registry_desk()` | `host_api/auth_center.rs` `#[cfg(test)] pub(crate)` | **提为常编译 pub**（lib 外部消费；被 6 个测试文件用，上提后 wasm-core 内部照用） |
| `reset()` | `host_api/auth_center.rs` `#[cfg(test)]` | 提为常编译 pub（system_component_test / session_e2e 用） |
| `lock_auth_center_desk()` | session_e2e 本地（~10 行，包 `registry_gate`） | 迁入 test_support |

**验收**：wasm-core `cargo check --lib --tests` 绿；13 个既有测试文件（含留存的 SDK fixture 测试）
经 `use crate::test_support::*` 或改指后全绿；lib 侧 `cargo check --tests` 绿（新基建可见）。

#### 6.0.2 session_e2e 迁移步骤（串行）

1. **wasm-core 删** `manager/runtime/tests/session_e2e.rs`（git rm）+ `runtime.rs` mod tests 删 `mod session_e2e;` 声明
2. **lib 建** `src-tauri/tests/session_e2e.rs`（git mv 保历史，再改写）
3. **符号改写表**（迁移模式，实测依赖清单见 §6.0.3）：
   - `use super::*`（mod tests 继承）→ 显式 `use bedcode_wasm_core::test_support::{setup_wasm_runtime, plugin_db_root, session_plugin_db_guard, lock_auth_center_desk};`
   - `crate::utils::session_gateway::*`（46 处）→ `crate::utils::session_gateway::*`（**lib 真源已回 lib**，直接同路径）
   - `use crate::utils::auth::auth_center as bridge`（4 处）→ `crate::utils::auth::auth_center as bridge`（lib 真源）
   - `crate::intercall::call_api`（5 处）→ `crate::wasm_core::intercall::call_api`
   - `crate::host_api::auth_center::registry_gate`（1 处）→ `bedcode_wasm_core::test_support::registry_gate`
   - `crate::db::*`（1 处）→ `crate::wasm_core::db::*`（或 lib `bedcode_desktop_lib::db`）
   - `env!("CARGO_MANIFEST_DIR")` 产物路径（8 处）→ lib 相对路径（wasm-core `../../resources/` → lib `../resources/`，以 pty_session_chain 的 `bundled_plugins_dir()` 为模板）
   - `plugin.activate()` / `plugin.get_manifest()` / `plugin.invoke_command()` / `plugin.deactivate()`（`LoadedWasmPlugin` 已 pub，方法经 lib `bedcode_wasm_core::manager::runtime::LoadedWasmPlugin` 可见）
4. **测试逻辑零改动**：不改断言、不改流程，只改符号路径
5. **编译到绿**：`cargo check --test session_e2e` → `cargo test --test session_e2e`

#### 6.0.3 实测依赖清单（动工对照）

- 继承符号（需 test_support）：`setup_wasm_runtime` ×12、`plugin_db_root` ×13、`session_plugin_db_guard` ×13、`lock_auth_center_desk` ×12
- `host_ctx` 字段访问：`host_ctx.permission` ×12、`host_ctx.api_registry` ×3、`host_ctx.message_bus` ×2、`host_ctx.clone` ×12
- `wasm_runtime` 方法：`load_plugin_from_file`（`plugin = wasm_runtime.load_plugin_from_file(&wasm_path, id, host_ctx, &[], None)`）、`compile_component`、`instantiate_component`（均 pub）
- `plugin` 方法：`activate` ×2、`deactivate`、`get_manifest`、`invoke_command`、`plugin.lock`/`plugin.db`（字段）
- cfg(test) 依赖（上提常编译）：`registry_gate`、`hold_registry_desk`、`reset`
- 产品 api 名 / 插件 id：随测试归宿主（`SESSION_PLUGIN_ID` 本地常量即可；session_e2e 已有 `com.bedcode.terminal-session` 字面量多处）

**验收（票 05b）**：`cargo test --test session_e2e`（src-tauri 侧）全绿；wasm-core `cargo check --lib --tests` 绿（session_e2e 移除后无残留）；`rg "session_e2e"` wasm-core 仅剩 spec/文档引用。

#### 6.0.4 后续票（05c 批量迁移）提示

- `ws_e2e` / `system_component_test` / `task_e2e`(ai-chatbox 用例) / `auth_center_perf` / `ws_output_perf` / `a03_probe` 同模式迁移
- `system_component_test` 的 `bridge::enforce_connection_policy`（裁决闭环断言）→ lib 真源直接可用（已回 lib）——比 wasm-core 内重建等价物更省
- `l2_gating_test` 锁：L2_CONSUMER_ALLOWLIST 删 wasm-core 已删路径、扫描根保留 lib、BRIDGE_PUBLIC_SURFACE 改钉机制面（`invoke_auth_method`）——**票 06 做**
- 遗留待处理（票 05 生产面已迁完）：wasm-core `cargo check --lib --tests` 当前因测试引用已删桥接层而红——**test_support 上提后先修复**，否则 05b 无从编译

#### 6.0.5 动工前置（环境）

- **磁盘治理（必须先做）**：2026-10-06 收工时磁盘 99%（2.7G 可用），`src-tauri/target` 11G（超 AGENTS §3 的 15GB 阈值一半）+ `bedcode-desktop/target` 3.4G。动工前先 `cd bedcode-desktop/src-tauri && cargo clean`（纯产物可重建；上轮清理释放 17.3G）；确认无并行 cargo 进程（ps aux | grep cargo）后再清。
- **在途改动认领**：dev 分支，ADR 0037 前作未提交（git status 195 项 R/M rename）+ 本次票 02/03/05 的改动（bedcode-pty-engine 新 crate、wasm-core 垫片化、lib 回迁）——全在工作区，不碰不回滚。


- **§5.1 B1-B6 零命中**：只搬既有实现，不新增任何业务名词类型 / 编排 / 默认值；pty 引擎
  本就零业务语义（头部注释「只收调用方算好的 argv」）。
- **§5.1.2 三问**：① 离宿主能实现吗——引擎 crate 化不改归属（机制仍是宿主引擎，只是物理
  位置）；② 携带产品语义吗——否（pty 引擎零业务语义）；③ ⇒ 进宿主 ✓。
- **不触发 ABI**：world 22 import 不动，guest 面零变化（host-pty 的 interface 形状不变，
  只是 impl 位置变了——能力域先例已验证此形态）。
- **不回归既有承诺**：ADR 0022 四类薄壳判定不变；ADR 0036 schema 真源不动；ADR 0037
  垫片纪律保持。

---

## 7. 风险

| # | 风险 | 缓解 |
| --- | --- | --- |
| R1 | E3（platform→wsl）处理不当使 wasm-core 反向依赖引擎 crate | D3 判 ①：wsl 留 wasm-core，依赖方向单向 |
| R2 | 引擎 crate 与 wasm-core 循环依赖 | 引擎 crate 只依赖 base/kit/plugin-api/wasmtime；wasm-core 依赖方向引擎 crate 单向 |
| R3 | host-pty WIT impl 迁出的 add_to_linker 双注册（wasm-core 旧行 + 新 crate 自报） | 票 04 同删同增，编译期即暴露（重复注册 = linker 错） |
| R4 | 既有已装 .wasm 是否受影响 | host-pty 是 world 22 import 之一，interface 形状不变 ⇒ 无需重建（ABI 未变）；票 06 用既有 fixture 验证 |
| R5 | 磁盘（85%） | 每票 `cargo check` 用小目标隔离，全量放票 06 |

---

## 8. 开放问题（等用户终裁）

- **Q1**：本期范围 = A（只 pty）/ A+B / A+B+C？（B/C 是安全薄壳，拆了需端口化
  WasmHostContext，建议单独评估）
- **Q2**：D2 垫片方向（wasm-core `pub use` 兜底 vs lib 直改）
- **Q3**：D6 crate 名 `bedcode-pty-engine` 可接受？

---

## 9. Comments

- 2026-10-06：spec 成稿。全部量测取自工作区实测（pty 引擎 2,469 行 / host_api/pty 599 /
  pty_output 337 / enums/pty_status 33 / auth 系 1,408 / session_gateway 254）。
  用户原始诉求点名 pty + authcenter；深挖后呈报 auth 属安全薄壳的性质差异（§1.2），
  留 Q1 终裁。
- 2026-10-06：能力域先例（ws/peer/http/mdns）的「provider bindgen + HostModule 自报 +
  白名单锁」是本票迁移形态的既有模板，非发明新形态。