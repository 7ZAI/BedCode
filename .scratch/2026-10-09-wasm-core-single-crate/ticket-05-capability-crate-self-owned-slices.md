# 票 05 · 能力域 crate 改 bindgen 自持分片（契约面脱端）

Status: **✅ done（2026-10-10 落地，实施记录见 §6）**（spec §4 票 05；spec §1.2 第一性问题的最终解决：能力域在契约面不再绑端 WIT）
依赖：票 03（cap 源文件与端清单在场；生成物流水线可供端组合验证）；票 04 若先合并则分片为切片形态，否则全接口形态（§3 两种都可行）
前置：**对 spec 票面的现状修正**——原清单里的 task / crypto / process / app / timer / api-call 已在票 02 批 04/05 走路径 B 迁宿主（`src-tauri/src/plugin/*.rs` 自带 bindgen，住所在宿主 `bindings.rs`），**不再有「能力域 crate bindgen」问题**，本票不覆盖它们；仍自带 WIT 绑定面的能力域 crate 实测共 **5 个**：

| crate | 绑定落点（2026-10-10 实测） | 分片文件（本票交付） |
| --- | --- | --- |
| `bedcode-pty-engine` | `src/plugin_binding.rs:244` `bindgen!` → 端 WIT `world: "plugin"` | `wit/pty.wit` + `world cap-pty` |
| `bedcode-server-http` | `src/plugin_binding.rs:251` 同款 | `wit/http.wit` + `world cap-http` |
| `bedcode-server-websocket` | `src/plugin_binding.rs:1196` 同款 | `wit/ws.wit` + `world cap-ws` |
| `bedcode-server-peer-net` | `src/plugin_binding.rs:624` 同款 | `wit/peer.wit` + `world cap-peer` |
| `bedcode-discovery-engine` | `src/lib.rs` 的 `ln!`（自制绑定宏） | `wit/mdns.wit` + `world cap-mdns` |

（宿主的路径 B 域继续用 `plugin/bindings.rs` 整 world 绑定——宿主是端组合根，绑全 world 是本分，不参与自持分片。）

## 1. 每域步骤（pty 样板，逐域独立）

1. **建分片源** `crates/…/wit/<domain>.wit`：`package bedcode:plugin;` + 本域 interface 原段（从端 WIT **逐字**复制，含文档注释）+ `world cap-<domain> { import <interface>; }`
2. **bindgen 换源**：`path` 改 `wit/<domain>.wit`、`world` 改 `"cap-<domain>"`；原是 `exports: { default: async }` 的按需保留 / 调整（世界若无 export，`exports:` 配置可能报错——执行期实测后二选一：去配置，或 world 加最小 export 满足绑定）
3. **装配面零改动**：`impl Host` / `submit_module!` / `HOOKS` / 端口 trait / `MODULE_NAME` / `MODULE_INTERFACES` / `MODULE_PERMISSIONS` / `DESC` 全部不动——分片只换**绑定源**，不换装配语义；ADr 0035 D5 `defined twice` 风险照旧由白名单双向校验兜底（宿主侧该域不新增同名 impl）
4. **白名单双向绿**：宿主 `expect_host_module!(MODULE_NAME)` + 内核 `IN_CRATE_HOST_MODULES`（若仍点名该域）== 收集集
5. **域 crate 测试全绿**：`cargo test --features desktop-host`（pty-engine 同款基线，注意既有 flaky 清单如 `output_notify_is_rate_limited_and_owner_scoped`）
6. **headless-host-probe 复跑**：`packages/bedcode-headless-host-probe` 的 `cargo tree` 零 WIT / 零 SDK 门禁不变（默认形态 = 纯引擎，`desktop-host` 不开 ⇒ 分片仍在 feature 门控内）
7. **桌面宿主回归**：`cargo check` + `cargo check --tests` 回基线（零新增；`ws_e2e` 的 `EndpointAuth` 基线红除外）

## 2. 每域提交

`feat(<crate>): bindgen 自持分片 cap-<domain>（票 05）`——每域一个提交，独立回退边界；实施记录逐域回填实际命令与门禁输出。

## 3. 与票 04 的两种合并顺序

| 情形 | cap 文件形态 | 注意 |
| --- | --- | --- |
| 票 04 已合并（切片形态，推荐） | `cap-ws.wit` 同时定义 `host-websocket`（核心 5）**与** `host-websocket-server`（10）两个 interface + `world cap-ws { import 两个; }` —— server-websocket 的 impl 覆盖两者，bindgen 生成两份 `Host` trait | 分片是 crate 自己的 package 实例，核心 5 副本与 `core.wit` 里的核心 5 是**不同文件的同名定义** ⇒ 不冲突（票 03 §2 的 host-mdns/peer 同款摆法）；`host-fs` 桌面扩展同理在 cap-desktop（宿主路径 B 面） |
| 票 04 未合并 | `cap-ws.wit` 先放桌面 15 全量，票 04 合并时再拆 | 无额外动作，换源即可 |

peer / mdns 两域注意：interface 既是 11 全等（交集，定义在 core.wit）又是能力域自持（crate 要 bindgen）——crate 的分片 `wit/peer.wit` / `wit/mdns.wit` 自带同名完整定义供**自己** bindgen，端组合不用它们（core.wit 已提供）；两处定义的**函数集必须逐字一致**，靠票 03 漂移锁 + 本票第 1 步的逐字复制纪律守护，并在实施记录里留一次显式 diff 证据。

## 4. 门禁（每域收口）

- 白名单双向锁绿（多出 / 少了均红）
- 域 crate `cargo test --features desktop-host` 全绿（与票 02 批次基线的差异 = 0）
- 装配期**实测一次全接口实例化**（无 `defined twice`：起一个真实加载流程或等价测试，逐域做一次）
- 内核 `cargo check --tests` 0 error（新增告警归零）
- 桌面宿主 `cargo check --tests` 回基线
- headless probe `cargo tree` 门禁不变

收尾：五域全绿；`cargo tree`（能力域默认形态）零 wasmtime / 零 SDK 命中；`git status` 无本票之外的意外文件。

## 5. 风险与回退

| 风险 | 吸收 / 回退 |
| --- | --- |
| bindgen 对「无导出 world」的行为差异（pty / ws 现带 `exports: { default: async }`） | 实测后二选一（去配置 / world 加最小 export），差异记录在实施记录 |
| 分片文件与端生成物漂移（两份同名定义慢慢分叉） | 票 03 漂移锁 + 本票第 3 节显式 diff 证据；发现即红 |
| `defined twice` 回归（宿主侧该域 impl 残存） | 白名单双向校验 + 装配期实测；执行前先 grep 宿主侧 `impl .*Host for` 该域判定清单 |
| 回退 | 单域 revert：删 `wit/<domain>.wit` + bindgen 两行还原；端组合仍用票 03 生成物，不受影响 |

## 6. 实施记录

**状态：✅ 已落地（2026-10-10）**。五域 bindgen 全部换自持分片，契约面脱端完成（spec §1.2 第一性问题闭环）。全部改动未提交（与票 02/03/04 在途批次同存工作区，每域提交边界由用户统一裁决）。

### 6.1 与票 04 的合并顺序实测

工作区现状 = 票 04 已合并（切片形态）：core.wit 已收拢交集切片（`host-http` 出站 1 / `host-websocket` 客户端 5），桌面生成物 cap-http.wit / cap-ws.wit 已是扩展 interface（`host-http-endpoint` 2 / `host-websocket-server` 10），http / ws 两 crate 的 `Host` impl 已覆盖两个 interface。按票面 §3 第一种情形执行。

### 6.2 逐域改动

| 域 | 分片 | bindgen | 测试（`--features desktop-host --lib`） |
| --- | --- | --- | --- |
| pty-engine | `wit/pty.wit`（票 03 已建，零改动） | path→`wit/pty.wit`、world→`cap-pty` | 101/101 绿 |
| server-http | `wit/http.wit` 重写：+`host-http` 核心副本 + cap world 双 import | 同款→`cap-http` | 83/83 绿 |
| server-websocket | `wit/ws.wit` 重写：+`host-websocket` 核心副本 + cap world 双 import | 同款→`cap-ws` | 51/51 绿 |
| server-peer-net | 新建 `wit/peer.wit`（host-peer 自 core.wit 逐字复制 + cap-peer） | 同款→`cap-peer` | 49/49 绿 |
| discovery-engine | 新建 `wit/mdns.wit`（host-mdns 同款） | `lib.rs` bindgen→`cap-mdns` | 35/35 绿 |

装配面（`impl Host` / `submit_module!` / `HOOKS` / 端口 trait / `MODULE_*` / `DESC`）逐域零改动——分片只换绑定源。

### 6.3 bindgen `exports` 配置实测结论（票面风险表二选一）

`exports: { default: async }` 在**无 export 成员的 cap world** 下 wasmtime 48 不报错（default 是通配键，无生效对象）——**五域全部实测编译绿，保留配置**（与宿主同款注释口径），未走「去配置」或「world 加最小 export」分支。

### 6.4 双定义一致性 diff 证据（票面 §3 硬要求）

core.wit 段 vs crate 分片段（awk 按 `/// <首句>` 起始行抽取到 interface 闭括号，逐字 diff）：

```text
PEER-SEG-SAME   （core.wit host-peer ↔ peer.wit host-peer，19 函数）
MDNS-SEG-SAME   （core.wit host-mdns ↔ mdns.wit host-mdns，5 函数）
HTTP-SEG-SAME   （core.wit host-http ↔ http.wit host-http 副本，1 函数）
WS-SEG-SAME     （core.wit host-websocket ↔ ws.wit host-websocket 副本，5 函数）
```

### 6.5 关键裁决：compose.json caps.http/ws 改指端真源 wit-src（票面未预见的必要接缝）

compose 流水线按**整文件**复制 caps 条目为生成物 `cap-<key>.wit`。若 caps.http/ws 继续指向 crate 分片，票面 §3 要求的「分片自持核心交集副本」会随整文件进入端合成 package，与 core.wit 同名定义冲突（resolve 报错，违反票 03 §2「合成 package 每个 import 名有且仅有一份定义」）。处置：

1. 端真源落 `plugin-sdk-desktop/rust/wit-src/cap-http.wit` / `cap-ws.wit`（= 票 04 后的生成物内容 + 真源说明注释），compose.json caps.http/ws 改指 wit-src；
2. 重拼生成物（`node scripts/compose-wit.mjs desktop`）→ 漂移锁 `--check` 全 ok → 幂等连跑两次 sha256 一致；生成物仅 cap-http.wit / cap-ws.wit 随真源注释更新（注释不影响绑定语义，SDK `cargo check --features wasm` 绿实证）；
3. pty 分片（无副本）继续作为 caps.pty 真源；peer / mdns 不进 caps（core.wit 提供端组合面）——与票 03 §2 host-mdns/peer 摆法一致。

### 6.6 装配期 `defined twice` 实测（票面门禁第 3 条）

- **残存判定（执行前 grep）**：宿主 `src/plugin/bindings.rs` 中 `host_pty|host_http|host_websocket|host_peer|host_mdns` **零命中**、`impl .*Host for` 0 处——五域 impl/add_to_linker 无宿主残存，重复注册根因排除。
- **机制语义测试**（内核 `--lib`）：`test_add_to_linker_rejects_duplicate_registration` 绿——wasmtime 对已注册同名 interface instance 报 "defined twice" 的行为被钉住；`test_add_to_linker_registers_all_interfaces` + `test_loaded_plugin_component_roundtrip` 绿——内核 `add_to_linker()` 第二段 `install_all`（能力域 ModuleEntry 自动注册）对内核链接的 http/ws/peer 三域 crate 侧 `add_to_linker` 真实执行并完成组件实例化往返。
- **真实加载流程（最强形态）**：宿主 `cargo test --test pty_e2e` **5/5 绿**——SDK fixture 真实组件（import 桌面 plugin world 全接口，含五域与切片扩展名）经宿主装配（能力域自报 + 内核 core 本地）实例化成功，spawn→ring-fetch→退出事件→属主隔离全链路往返。pty 域装配期实测以真实组件覆盖。
- **mdns 域等价证据**：无 mdns fixture（fixtures 合集 feature 面 http/task/pty/sdk/ws/wasip3/crypto 不含 mdns）；以 ① crate check/test 绿（bindgen 生成物可编、`Host` impl 形状正确）② 宿主 `--lib plugin::` 75/75 绿（含 `expect_host_module!` 收集面断言）③ 与 pty 完全同构的装配路径（ModuleEntry register 一行 add_to_linker）④ 宿主 bindings.rs 零残存，四点合成。

### 6.7 门禁汇总

| 门禁 | 结果 |
| --- | --- |
| 白名单双向锁 | 内核 `capability_registry_matches_whitelist` 等 4/4 绿；宿主 `pty_wiring` 6/6 绿；宿主 `--lib plugin::` 75/75 绿 |
| 域 crate 测试 | 五域 101/83/51/49/35 全绿（与票 02 基线差异 = 0） |
| 内核 `cargo check --tests` | 0 error（32 告警为票 18 在途等存量；ws `AuthFrame` / peer 3 条为引擎面存量，与本票 bindgen 改动无涉） |
| 内核 `--lib` 全量 | 593/593 绿 |
| 桌面宿主 `cargo check` / `check --tests` | 回基线：唯一红 `ws_output_perf.rs:255` E0308 = 工作区在途基线（HEAD 即红）；原基线记录的 ws_e2e 3 处红已被在途改动修复，零新增 |
| headless probe `cargo tree` | 零 wasmtime / wit-bindgen / inventory / bedcode-host-kit / bedcode-plugin-api 命中（默认形态纯引擎不变——bindgen 在 `desktop-host` feature 门控内） |
| 生成物漂移锁 + 幂等 | `compose-wit.mjs desktop --check` 全 ok ×2 |

### 6.8 欠账（如实记）

- **pty / ws fixture 构建者缺位（票 02 迁移欠账，本票顺带补了 pty）**：内核 fixture keeper 只有 crypto / task 两个，宿主 `build_pty_test_component()` 只读不建——pty_e2e 实跑前产物缺失。本票按 `fixture_build.rs` 同款命令（`RUSTUP_TOOLCHAIN=nightly-2026-09-16`、`CARGO_TARGET_DIR=bedcode-desktop/target/fixtures`、wasm32-wasip3 release + `--features pty`）手动构建并归档 `bedcode_plugin_sdk_fixtures.pty.release.wasm`。ws fixture 同样缺位（本票未用到，未补）。建议票 07 收口时把 pty/ws keeper 补进 `fixture_keeper.rs`。**✅ 票 07 已补（2026-10-10）**：`fixture_keeper.rs` 增 `pty_fixture_artifact_is_built_for_host_e2e` / `ws_fixture_artifact_is_built_for_host_e2e`，keeper 4/4 实跑绿、pty（383KB）/ ws（397KB）产物实际生成——宿主只读场景的产物缺位闭合。
- **每域一个提交（票面 §2）**：按仓库惯例留待用户统一裁决提交切分，改动面见 6.2。
- **文档联动**（CHANGELOG / code-map / ADR 0045 accepted / 漂移锁 CI 化）：随票 07 统一收口（与票 03 §7.5 同口径）。