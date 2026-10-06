# wasm-core 纯净性收口：引擎面与宿主薄壳迁出 bedcode-wasm-core

## 状态

**已实施**（2026-10-06，桌面端；**ABI / WIT / 协议零变动**：`world plugin` 的 22 个
import 一个未动，`ABI_VERSION` 不变，已装 `.wasm` 无需重建）。移动端零改动
（ADR 0018 契约独立）。
spec：`.scratch/2026-10-06-wasm-core-purity/spec.md`（票 01-09；01-05c 已实施，
06 为契约收口 / 07 独立验收，08/09 为 P0-2/P0-3 后续票）。前作：`.scratch/
2026-10-06-wasm-core-whole-crate/`（整核抽出，ADR 0037）+ `.scratch/
2026-10-04-wasm-core-lib-split/`（能力实现出内核，ADR 0035）。边界单一事实源仍是
**ADR 0022**；本 ADR 是 0037 的**后继细化**（0037 spec §8 Out of scope 第 3 项
「L2 interface 出 core」在此兑现第一步）。

## 背景

ADR 0037 把 wasm_core 整核（含引擎面）搬进了 `bedcode-wasm-core` crate。用户指出：
**crate 应只装 wasm 相关机制**——pty / authcenter 等其他功能的代码不应留在其中，
否则污染 crate 的单一职责，第三方宿主想复用插件机制时被迫背上 portable_pty /
认证中心桥接等无关引擎。

实测 `bedcode-wasm-core` 里混着三类「非机制」代码：

| 组 | 模块 | 行数 | 性质 |
| --- | --- | --- | --- |
| **A. pty 引擎面** | `pty/`（pty_process/pty_ring/pty_reader/output_sink/lifecycle/wsl） | 2,469 | portable_pty 封装，**零 wasm 依赖**（只收 argv 的纯引擎 + WSL 发行版列举） |
| **B. 认证中心桥接** | `utils/auth/auth_center.rs` + `test_tokens.rs` | 216 + 130 | 认证中心宿主桥接门（问中心一次 + deny_kind 三态）+ 测试夹具 |
| **C. 会话窄转发** | `utils/session_gateway.rs` | 254 | 零解析窄转发（ADR 0022 四类薄壳④） |

深挖后的性质差异（呈报用户）：pty（A 组）确属「引擎面」，迁移形态干净（与 ADR 0035
四域先例同构）；**auth / session（B/C 组）不是业务功能，而是宿主安全薄壳**（ADR 0022
明示宿主可留四类薄壳：② 安全闸门 ④ 零解析窄转发）。用户两轮终裁：

1. **pty → 独立引擎 crate** `bedcode-pty-engine`；
2. **auth / session 桥接 → 回宿主 lib**——「应该拆到宿主层面，不应该留在 wasm core 中」。
   回到 lib 的不是「能力 crate」而是**宿主薄壳原位**（依赖方向干净：lib → crate 单向，
   沿用 0037 垫片方向，桥接在 lib 里引用 `bedcode_wasm_core::*` 不构成环）。

另有外部审查发现的同类实例（P0-1/P0-2/P0-3）：`session_gateway` 硬编码 8 个产品互调
api 名（B1）+ 具名产品参数（B4）+ 问产品判据（B6）——**已不是零解析窄转发**，属
业务代码落机制 crate；`security/fs_auth.rs` 硬编码两个产品插件 id 的安全豁免表（B1+B5，
票 08）；`system/config.rs` 的 `AppConfig.ui`（产品外观 schema，B1，票 09）。P0-1 随
票 05 同机回 lib，P0-2/P0-3 为后续票。

## 决策

### D1 · PTY 引擎体迁出为 `bedcode-pty-engine`（引擎 crate，非完整能力域 crate）

引擎本体（`pty_process` / `pty_ring` / `pty_reader` / `output_sink` / `lifecycle` /
WSL 列举 + `PtySessionStatus` 词汇）迁入 `bedcode-desktop/packages/bedcode-pty-engine/`。
**依赖方向只向下**：`bedcode-server-base`（错误 / 常量）+ 第三方（portable-pty / tokio /
encoding_rs…），**零 wasm 依赖、零宿主依赖**——任何 Tauri 宿主、任何需要终端能力的
程序都可直接 path 依赖。**host-pty WIT 绑定面留 wasm-core**（`host_api/{pty,
pty_output}.rs` 是机制，与 `manager/runtime/component.rs` 的 impl Host 同侧；wasm-core
依赖 pty-engine，`src/pty.rs` 以 `pub use` 垫片保 `crate::pty::*` 路径零改动）。

**为什么不是完整能力域 crate**（与 ADR 0035 四域不同）：四域（mdns/ws/peer/http）自带
provider 侧 `bindgen!` + `HostModule` 自报；host-pty 的 WIT 面在 wasm-core 内与
component.rs 装配链深度耦合，引擎与绑定面分居两 crate、由 wasm-core 单向依赖，同样
达成「任何宿主可复用引擎」且无环。引擎不感知宿主配置：AppConfig 两处读点改为
构造参数注入（`PtyEngineConfig`，默认 16/4096 与旧默认一致）。

### D2 · auth / session 桥接回宿主 lib（宿主薄壳原位，去掉垫片化）

- `utils/auth/auth_center.rs` 的**裁决面/桥接门**（`enforce_connection_policy` /
  `session_active`）与 `utils/session_gateway.rs`（窄转发 254 行）从 wasm-core 实现
  回迁 `src-tauri/src/`（不再是垫片，是真源实现）；
- **P0-1 同机闭环**：session_gateway 的 8 个产品互调 api 名与具名产品参数形状随迁归
  宿主，机制 crate 不再被产品 wire 污染；
- `invoke_auth_method`（host-auth WIT 原语 `auth-method-invoke` 的实现链）**留
  wasm-core**，并入 `host_api/auth_center.rs`（机制非薄壳，被 `host_api/auth.rs:329`
  生产调用）；
- 注册表（`host_api/auth_center.rs`，WIT 绑定面 + 单中心注册表）**留 wasm-core**；
  lib 单向依赖取用（不回 lib：回 lib = wasm-core → lib 反向依赖 = Cargo 环）；
- `test_tokens.rs` **留 wasm-core 常编译**（依赖 crate 内部 `test_seed_plugin_secret`，
  是产物闭环测试夹具非宿主功能；lib 集成测试经
  `bedcode_wasm_core::utils::auth::test_tokens` 消费——集成测试看不见依赖方的
  cfg(test) 项，故常编译，不引入 feature 门控）。

### D3 · 测试资产重构：wasm-core 只留测机制本体的用例

用户裁定：「session e2e 应该迁移到宿主侧，wasm-core 内不应该包含任何跨 crate 集成
测试」。全部**加载真实产品插件产物**（`resources/plugins/desktop/*.wasm`）的
wasm-core 测试迁宿主侧 `src-tauri/tests/`（独立测试二进制，与 pty_session_chain 并列）：
`session_e2e`（3,720 行）→ `tests/session_e2e.rs`；`ws_e2e` / `ws_output_perf` /
`auth_center_perf` 整迁；`system_component_test` / `task_e2e`（ai-chatbox 真实产物用例）/
`a03_probe`（产物闭环 + production 异步 store 用例）拆分迁出。**只测机制本体、用本地
SDK fixture（`plugin-sdk-fixtures`）的测试留 wasm-core**（sdk_e2e / wasi_e2e /
component_e2e / engine_limits / pty_e2e / task_e2e 其余 / a03_probe 机制项等）。

配套基建：wasm-core 建**常编译公开测试基建** `src/test_support.rs`（`setup_wasm_runtime`
/ `plugin_db_root` / `session_plugin_db_guard` / `registry_gate` / `hold_registry_desk` /
`reset` / `lock_auth_center_desk`，按 test_tokens 常编译先例；不引入 feature 门控）；
`host_api::ws` 模块 pub 化 + `ws::purge_for_plugin`、`LoadedWasmPlugin::call_capability_export`、
`host_api::grant_permissions` 提顶层常编译 pub（lib 集成测试消费）。

### D4 · 锁与登记表同步落地（票 06 收口）

- `SPLIT_CRATES` 登记表（真源 `bedcode-wasm-core/src/crate_boundary_lock.rs`）登记
  `bedcode-pty-engine`；lib `server/crate_boundary_lock.rs` 的 `ALLOWED_DOWNWARD_EDGES`
  增 `pty-engine → base` 与 `wasm-core → pty-engine` 两条边，`REQUIRED_DOWNWARD_EDGES`
  对应加两条必需边；lib Cargo.toml 显式声明 `bedcode-pty-engine`（断言③：宿主清单声明
  全部拆分产物——lib 代码不直接消费，经 wasm-core 垫片取用）；
- 反双份锁补扫：crate `enums.rs` 的 wire 垫片锁把 `enums/pty_status.rs`（已垫片化，
  真源迁 pty-engine）纳入；lib.rs 新增 `pty_shim_file_contains_no_definitions`（钉
  `src/pty.rs` 只允许 re-export，防引擎定义被复制回内核 / `PtySessionStatus` 类型身份
  分裂）；lib 侧既有 `wasm_core_whole_crate_lock` 不动；
- `l2_gating_test` 白名单随桥接回迁同步（扫描根与路径更新已随票 05b/05c 落地）；
- `capability_crates_no_product_ids` / `capability_crates_unit_tests_only`（他会话票）
  把 `bedcode-pty-engine` 纳入管辖。

### D5 · 不拆的边界（防回接）

以下**不可回 lib**（WIT 绑定面，`component.rs` impl Host 在 wasm-core 内直接调用；
回 lib = wasm-core → lib 反向依赖 = Cargo 环）：`host_api/auth.rs`（host-auth WIT 绑定面
+ secret-store 密钥托管）、`host_api/auth_center.rs`（host-auth 注册面 + 单中心注册表）、
`invoke_auth_method`（并入 auth_center）。`db/` 保留（ADR 0036「机制与真源同侧」，
用户已确认不动 db）。`host_api/` 其余域（fs/platform/process/connection/crypto…）均为
WIT 绑定面 = 机制，本期不碰。

## 验证（票 06 完成时）

- wasm-core `cargo test --lib` **709 passed / 1 failed**（唯一失败 = 既有基线
  `perf_p2_guest_ring_fetch_batch_curve` 5ms 墙钟 flake，低负载单跑常绿，非本票回归；
  05c 基线 708/1，+1 为本票新增垫片反双份锁）；锁测试（crate_boundary / pty_shim /
  enums wire shim）全绿
- src-tauri 侧：迁移的集成测试二进制（session_e2e / ws_e2e / ws_output_perf /
  auth_center_perf / system_component_test / task_ai_chatbox_e2e / a03_probe_products）
  各自绿（05b/05c 已验）；crate_boundary_lock 断言①③（pty-engine 登记 + lib 声明）绿
- `rg` 全仓核验：wasm-core 生产面与测试面零产品互调 api 名 / 零 `session_gateway` /
  零 `auth_center` 桥接残留（P0-1 闭环）
- cross-end-tests：**未跑**（票 07 独立验收；本票仅锁/文档/登记，不影响任一跨端协议面）

## 后续（本期明确不做）

- **票 07**：contract 独立验收（crate 根 `cargo test` 全量 + src-tauri 全量 +
  cross-end-tests）
- **票 08（P0-2）**：`security/fs_auth.rs` 的 `FIRST_PARTY_TRUSTED_DIRS` 豁免表改为
  宿主注入（判定逻辑留 crate，产品清单 lib 装配时传入）
- **票 09（P0-3）**：`AppConfig.ui`（产品外观 schema）拆出 AppConfig 迁 lib
- **tauri 反转为零依赖**（把 wasm-core 的 119 处 tauri 引用变成端口）：独立立项
  （0037 D2 已声明）