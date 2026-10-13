# 票 06 · 契约 / 文档 / 锁 / ADR 收口

> 输入：`spec.md` §5 T-06 + §8 越线条款。前置：T-04 + T-05（绑定归位完成）。
> **依赖**：T-04 + T-05 → **本票**（唯一收口；无双并行依赖）。
> **测试节奏**：**全量测试收尾在此集中执行**（用户裁定：所有测试在完成 spec 全部 ticket 后进行）——wasm-core / 桌面 / 移动 `cargo test`、两端 `pnpm run test:run`、根 eslint、`check:wit`、apps 重建、`cross-end-tests`（按指示）、`cargo fmt`/`clippy`，一次跑完贴结果。
> 目的：双端契约与 ABI 维持各自演进（ADR 0018 不动），锁、文档、CHANGELOG、ADR 对齐新终态。

## 1. 契约确认（不合并）

- 双端 `compose.json` **保持现状**：桌面 = core + cap-pty/http/ws/desktop（ABI 37）；移动 = core + cap-mobile（ABI 20）。
- 不做 union world、不合并 ABI、不单 SDK 化（原 spec 方向作废）——**契约零变更**。
- `pnpm run check:wit` 通过（生成物逐字核对，应无 diff）。
- 移动 3 app 的 SDK 引用不动；`cross-end-tests` 协议面零变化（用户指示可跳过时跳过）。

## 2. 防回接锁重写（spec §4 B8）

| 锁 | 现语义 | 新语义 |
| --- | --- | --- |
| `bedcode-mobile/src-tauri/tests/retired_mobile_wasm_core_fork_lock.rs` | 防移动 fork crate 回接 | 防任何「第二份 wasm-core 织体」（fork / rename / 独立绑定副本）回归；wasm-core 只一份 |
| `bedcode-mobile/src-tauri/tests/mobile_wasm_core_purity_lock.rs` | 钉 wasm-core 形态壳纯净性（6 例：移动独有域不回流 / 生产装配段无 Host impl / 壳不含域逻辑 / 共享核在场 / 壳行数上限 / 退役引擎不复活） | 按新终态调整：原「壳」词汇消失（绑定已不在 wasm-core），改写为「wasm-core 禁一切 cap/端名/WASI/双 SDK 词汇」全集判据 + 正面向导（机制核当场） |
| wasm-core `lib.rs` 内各域防回接锁（pty/http/crypto/auth/path-b/api_call/sliced-interfaces，lib.rs tests 模块） | 逐域防回接 | 汇总为一套「wasm-core 无绑定」顶层判据（可选，若各域锁仍有效可保留，但须核对与绑定归位后的新事实一致） |
| `crate_boundary_lock.rs` / 能力域 crate 侧锁 | 拆分产物登记 | 绑定上移是否产生新拆分（如交集绑定独立 crate）→ 登记表同步 |

**变异自检**：每把改写的锁注入违背形态 → 红 → 还原 sha256 → 绿，逐锁记录（惯例同 mobile_wasm_core_purity_lock 的 4/4 变异表）。

## 3. 文档

- `packages/bedcode-wasm-core` 文档注释：全谱 `common-host` / `bare-host` 解释更新为「单形态 + 绑定归位」；lib.rs 模块头改写。
- 两端 `docs/code-map.md`：wasm-core 段改写（机制核 + 交集绑定在 core / 无 cap）；桌面 plugin/ 段补绑定层承接；移动 plugin/ 段补绑定层承接；**锁索引**登记新锁名（§2）。
- `AGENTS.md` §5.4 / §4：双端差异表述从「组合了哪些能力域 + 契约分片」更新为「wasm-core 单形态 + 绑定在宿主/SDK 组合」（若 §5.4 现文与之矛盾，一并修订）。
- `.scratch/2026-10-11-wasm-core-single-form/`：spec/ticket 状态转完成。

## 4. ADR

- **新 ADR**（记本次决议）：wasm-core 单形态（机制零差异）+ 绑定层归位宿主 + 契约差异经 SDK compose 组合（否决 union world）——含用户裁决原文、落地批次、遗留裁决（T-03 Option 结果）。
- **ADR 0018 维持不撤销**：契约独立演进保留（现状 37/20）；如实现中出现契约触达（T-03 交集绑定是否影响 ABI 判定），评估并记录，不自行撤销。
- 既有 ADR 0045/0047 相关条款：核对与本终态的一致性，冲突处修订（如 0047 的「形态壳只做端口接线」——本合并后壳已迁宿主，表述更新）。

## 5. CHANGELOG

- 根 `CHANGELOG.md` + `CHANGELOG_zh.md`：`[未发布]` 段新增双语条目：
  - wasm-core 单形态（机制零分叉、上下文合一、feature 收敛）；
  - 绑定层归位宿主（桌面/移动各端自持 bindgen + Host impl + 装配）；
  - 契约零变更（ABI 37/20 各端演进、compose 组合不变、app 契约面不变）；
  - 锁/文档/ADR 同步。

## 6. 统一门禁（收尾全清单，实跑贴结果）

- wasm-core（无 feature）`cargo test --lib` 全绿；桌面 `cargo test`；移动 `cargo test`；两端 `pnpm run test:run`；根 `pnpm exec eslint .`（0 error）；`pnpm run check:wit`；
- 3 个移动 app 重建产物（T-05 已跑则复核）；
- `cross-end-tests`（协议面零变化按用户指示）；
- 锁变异自检逐锁记录；`cargo fmt` / `cargo clippy` 自查。

## 7. 交付物

- 锁重写 + 变异记录；文档/AGENTS/code-map 更新；新 ADR + CHANGELOG 双语；门禁结果汇总。
- 遗留项声明（如 T-03 降级 Option B 的残余差异，明列不隐藏）。

---

## 8. 状态：已完成（2026-10-13 会话）——本票收口

> 会话基线 = HEAD `8c71aebdc` + T-04 / T-05 工作区改动（同一工作区，未提交）。

### 8.1 §1 契约确认（零变更）

- `pnpm run check:wit` **通过**：双端 `core.wit` + 桌面 4 个 cap + `cap-mobile` +
  双端 `bedcode.wit` 逐字一致，**无 diff**（票前跑一次、票末复核一次，两次同果）。
- ABI 未动：桌面 **37** / 移动 **20**，双端 `compose.json` 未改。
- 移动 3 app 的 SDK 引用未动；`cross-end-tests` 按用户指示跳过（协议面零变化）。

### 8.2 §2 锁：核对 + 补判据 + 修陈旧路径

**改写的两把移动锁**（票面表格的「现语义 → 新语义」已在票 05 落地一轮，本票在其上补齐）：

| 锁 | 本票处置 |
| --- | --- |
| `retired_mobile_wasm_core_fork_lock.rs` | 泛化为「禁任何第二份 wasm-core 织体」已在本票前完成；本票**补第 5 判据** `repo_has_exactly_one_wasm_core_body`——盯「换个新名字重新长一份」（`bedcode-wasm-core` 之外不得有任何 `bedcode-wasm-core*` 目录，扫仓库根下 4 层，跳过 target/node_modules/.git）。票 04/05 把双端绑定迁出内核后，「复制一份内核自带绑定」是最省事也最危险的动作，原 4 判据盯不到 |
| `mobile_wasm_core_purity_lock.rs` | 6 判据已在票 05 落地；本票**补判据 7「端词汇全集」**：`7a` 内核代码行零 cap 分片 world 名（**注释放行**——`core_binding.rs` / `bare.rs` 的模块文档必须说明「cap 组合不在内核」，把文档判红等于要求内核对自己的边界闭嘴；首版按全行判定已实测判红并修正）· `7b` 声明位零端名（ADR 0047 D2，豁免走 `DECLARED_NAME_EXEMPTIONS` 三元组登记表，**理由与匹配名分列**——首版把理由写进匹配串导致豁免静默失效，已修正）· `7c` 移动端 SDK 词汇封顶在唯一已知落点 `manager/runtime/bare/component.rs`（双 SDK 收敛遗留项的看门狗，见 §8.6）· `7d` 正向锚点（`core_binding.rs` 确含 `bindgen!` 且指向 `wit/core.wit`，防 7a–7c 空转） |

**票面 §2 行 2 的一处修正（原表述作废）**：票面写「改写为禁一切 cap/端名/**WASI**/双 SDK 词汇」，其中「禁 WASI 词汇」已被 2026-10-12 用户裁决 3 作废——WASI 装配面住内核、双端默认全开、开/关走 `CoreConfig::wasi`，它是**共享机制不是形态差异轴**，锁它等于把裁决锁反。同理「双 SDK 词汇」不做零命中（会立刻判红），改为封顶 + 遗留声明。

**wasm-core 内域锁核对**（票面 §2 行 3：「可选保留，但须核对与绑定归位后的新事实一致」）——核对结论 + 一处必改：

- `binding_surface_must_not_return_to_wasm_core`（**原名 `desktop_binding_surface_…`**）：
  ① 文档里的「合法残余」仍写着 `bare/component.rs` 的移动端 `bindgen!`——票 05 已删，**必须改**（AGENTS §6 注释与代码同步）；
  ② 判据反向②只查**桌面**两壳 ⇒ 改为查**双端四壳**（用新增的 `SHELL_FILES` 常量，与 `engine.rs` 内同名常量同源两份并交叉注明）；
  ③ 锁名带端名，违反 ADR 0047 D2 ⇒ **去端名化**改名（3 处自引用同步）。
- `pty` / `http` / `crypto` / `auth` / `path_b` / `api_call` / `sliced_interfaces` 七把域锁：逐条核对与新事实一致，**无需改动**（`api_call` / `sliced_interfaces` 的文档里「`component.rs` 的 cfg(test) 替身是合法残余」已随票 04/05 迁出而失效，本票同批修正措辞）。

**票面 §2 行 4（拆分产物登记表）——结论：无需新增**。绑定上移**没有产生新 crate**：`packages/` 实测只有一份 `bedcode-wasm-core`，core 交集留内核（`core_binding.rs`）、移动域进移动宿主已有的 `src-tauri`（没有「交集绑定独立 crate」这一步），故 `SPLIT_CRATES` 无需登记。

**顺带修掉的三处陈旧锁路径**（票面未列，属同款「路径变了我却没跟」——核对域锁时顺带发现）：

1. `retired_mobile_host_terminal_hooks_lock.rs`：保留面清单里一条仍钉 `packages/bedcode-wasm-core/src/terminal_stream_gateway.rs`——票 05 §7.4 已把它合并进移动宿主 `plugin/terminal_stream.rs` 并**删除内核那份** ⇒ 指向已删文件，`mobile_terminal_stream_retained_face_stays` 判红。改为钉宿主真源 `pub fn terminal_stream_gateway()`，并把四跳迁移史写进注释（内核侧「不得回接」的反向判据交给 purity 锁判据 5，本锁不重复）。
2. `hot_path_logging_lock.rs`：`LOCKED_SITES` 两条 wasm-core 条目用 `../packages/…`（解析到 **2026-10-08 已迁空的** `bedcode-desktop/packages/`），同表 `bedcode-server-websocket` 条目早已用 `../../` ⇒ 前两条自整核迁根起就一直判红。改 `../../`。
3. `retired_mobile_terminal_link_lock.rs`：一次测试运行中读到的内容与随后 `git status` 不一致，复跑即绿——记录为**瞬时读数不一致**，未改代码（内容已是票 05 同步后的宿主路径）。

### 8.3 锁变异自检（票面 §2 要求逐锁记录）

脚本 `.scratch/2026-10-11-wasm-core-single-form/mutation_check_t06.sh`（可复跑），**5 条注入 / 10 项检查全过**：

| # | 注入的违背形态 | 期望 | 结果 |
| --- | --- | --- | --- |
| 1 | 判据 7a：`bare.rs` 代码行加 `const _: &str = "cap-mobile";` | 红 | ✓ 红 → `cp` 还原 sha256 一致 → 绿 |
| 2 | 判据 7b：`manager/types.rs` 加 `pub struct T06MobileProbe` | 红 | ✓ 红 → sha256 一致 → 绿 |
| 3 | 判据 7c：`bus.rs` 加 `use bedcode_plugin_api_mobile::…`（第二个落点） | 红 | ✓ 红 → sha256 一致 → 绿 |
| 4 | fork 锁判据 5：新建 `packages/bedcode-wasm-core-shim/` | 红 | ✓ 红 → 目录删除 → 绿 |
| 5 | wasm-core `lib.rs` 反向②：`bare/component.rs` 加带 `bindgen!` 的代码行 | 红 | ✓ 红 → sha256 一致 → 绿 |

收尾核对：注入目录无残留、四个被变异文件 `T06_` 标记 0 处。

### 8.4 §3 文档

- **wasm-core 模块头 / 注释**（注释即事实面，AGENTS §6 强制同步）：`lib.rs`（新增「形态与绑定」终态段 + `compile_error!` 判据注释改写——原注释写「两形态各挂一套 bindgen」，票 04/05 后该理由**已不成立**，现改为「形态装配树并存」并指向遗留项；`host_api` / `intercall` 模块文档）· `manager.rs` · `manager/runtime.rs`（形态壳描述 + 「绑定不在 engine.rs」段）· `runtime/bare.rs`（三处「Host 接线 / 17 组 Host 接线」等失效表述）· `host_api.rs`（分叉段落）。`context.rs:570` 的 `instantiate_component` 引用经核**仍存在**，不动。
- **两端 code-map**：桌面 `§2` 增 `manager/runtime/` 三份机制面行（engine / core_binding / wasi / guest_events + 壳的边界）· `src/plugin/` 树注改成「票 04 起本目录是桌面 WIT 绑定唯一落点」· 锁索引修 2 处陈旧路径 + 修 2 条已迁走的「合法残余」措辞 + **新登记 3 把锁**（端绑定面 / 端词汇全集 / 第二份织体）；移动锁索引同步 7 判据 + 5 判据与变异记录、`bare-host` 面三层描述。
- **`AGENTS.md`**：§4 任务路由「改 wasm-core」行指向 ADR 0049 + 加上「要往 wasm-core 加 `bindgen!` / `impl … Host` 先停下」；§5.4 重写——单形态 + 绑定归位终态（含分层图）、WASI 降级为共享机制、契约各自演进单独成段、形态纪律补「内核 `#[cfg(test)]` 替身已在票 04/05 删除，别再照旧文加回来」。
- **`.scratch/`**：本票 §8 状态（本文）+ spec §5 T-06 状态转完成。

### 8.5 §4 ADR

- **新 ADR 0049**（`docs/adr/0049-wasm-core-single-form-and-binding-in-host.md`，accepted）：D1 单形态 / D2 绑定归位（逐载荷落点表）/ D3 事件派发 / D4 WASI 住核心 / D5 否决 union world / D6 上下文合一 / **D7 移动 core-8 不切 `core_binding` 的两处不同源（权限门 + 私有库布局）** / D8 契约与锁收口口径；代价与风险、验证门禁、**遗留项（4 条）**、Out of scope、票 T-01–T-06 实施记录 + **6 条踩坑与判决性事实**。
- **ADR 0018 维持不撤销**（票面 §4 行 2）：契约零变更，未触碰。
- **既有 ADR 0045 / 0047 冲突条款修订**（票面 §4 行 3）：0045 D5「移动无 WASI / 桌面 WASI p3」加作废批注（指向 0049 D4）；0047 D1「形态」含义变更加批注 + 门禁第 7 项（「零 `wasi` 字样」作废 / 「零端名」从一次性 grep 升级为常驻锁）重写。

### 8.6 §5 CHANGELOG

双语 `[未发布]` 各一条（`CHANGELOG.md` + `CHANGELOG_zh.md`），覆盖票面 §5 四项：单形态（机制零分叉 / 上下文合一 / feature 未收敛如实写明）、绑定归位宿主（逐端落点）、契约零变更（ABI 37/20 + ADR 0018 不撤销 + 否决 union world）、锁/文档/ADR 同步。

### 8.7 §6 全量门禁（实跑结果）

**绿**：

| 门禁 | 结果 |
| --- | --- |
| `pnpm run check:wit` | ✅ 双端 9 项全 ok，无 diff（跑两次） |
| wasm-core `cargo test --lib` | ✅ **572 passed / 0 failed** |
| wasm-core 三态 `cargo check --lib`（default / `--no-default-features` / `bare-host`） | ✅ 0 error ×3 |
| 移动 `cargo test`（全量，含 test-support 面） | ✅ **239 lib + 全部集成目标 0 failed**（含新 component_e2e 10 / ws 1 / terminal_session 1、purity 锁 7、fork 锁 6） |
| 移动端 `pnpm run test:run` | ✅ **895 / 895 passed**（78 文件） |
| 移动 3 个 wasm-app 重建 | ✅ `pnpm run plugins:build` **3/3** |
| 桌面其余 19 个测试目标 | ✅ 全 ok（本票修掉 `hot_path_logging_lock` 后） |
| 根 `pnpm exec eslint .` | ✅ **0 error**（97 warning，warning 不计门禁） |
| 锁变异自检 | ✅ 5 条注入 / 10 项检查 |

**红 / 未跑（全部为在途基线或环境限制，非本票引入）**：

| 项 | 分类 | 证据 |
| --- | --- | --- |
| `tests/ws_e2e.rs`(3) / `ws_output_perf.rs`(1) | **HEAD 编译红**（在途基线） | 两文件 `git status` 为空（== HEAD）；`EndpointAuth` 同名不同型：测试传 `bedcode_plugin_api::EndpointAuth`，`bedcode_server_websocket::endpoint::register` 要 `server_websocket::wire::EndpointAuth`（ADR 0035 D5 同款） |
| `session_e2e` / `broadcast_shutdown` / `pty_session_chain` / `http_auth_biometric` / `ws_auth_rules` / `a03_probe_products`(2) / `system_component_test` | **随包 wasm 产物过期**（在途基线） | 7 个测试文件全部 == HEAD；均在 `activate com.bedcode.terminal-session (bundled artifact)` 处失败；`src-tauri/resources/plugins/desktop/*/*.wasm` 时间戳 **2026-10-10 09:41**，早于 T-04/T-05（10-11 起），缺 `host-connection-registry` 等新接口 |
| 桌面 app 产物重建 | **本机跑不了**（环境限制） | `rustup target add wasm32-wasip3` → `toolchain 'stable' has no prebuilt artifacts available for target 'wasm32-wasip3'`（低 tier 目标，需 nightly 或源码构建）。⇒ 上条那批红**在本环境无法通过重建转绿**，需带 nightly 的环境补跑 |
| `capability_crates_unit_tests_only` | 在途红（已知） | `bedcode-host-api-core/tests/` 存在于 crate 根；记忆条目已记「属在途红」 |
| `capability_crates_no_product_ids` | 在途红（待登记桶腐化） | `bedcode-headless-host-probe` / `bedcode-host-api-core` 未登记进 `SCANNED_CRATES` 或 `PENDING_SCAN_CRATES` |
| `empty_dir_lock` | 在途红 | `bedcode-mobile/plugins/` 目录不存在（布局变更后锁的约定根未跟进） |
| 桌面 `terminalPreview.test.ts` | HEAD 红（在途基线） | 文件 == HEAD；`Test Files 1 failed / 116 passed`，`Tests 1 failed / 1623 passed`；红点即记忆记录的 `resize 命令首次失败自动重试`（约 518 行） |
| `cargo fmt` | **未整 crate 执行（按纪律）** | `cargo fmt --check` 在三处 crate 均显示巨量既有漂移（wasm-core `lib.rs` 单文件 984 hunk）⇒ 项目本就不保持 rustfmt-clean，且记忆条目「插件 crate 禁 `cargo fmt` 整 crate」。本票**只对本轮新写的 3 把锁**跑了 `rustfmt`（净变化 27/6/8 行），未触碰任何既有代码的排版 |
| `cross-end-tests` | 按用户指示跳过 | 契约零变更，协议面未触 |
| `cargo clippy` | 三处 **0 error**（wasm-core / 移动） | 桌面 2 error = 上述 `EndpointAuth` 同款基线，非本票引入 |

收尾清理：无残留 cargo / 测试服务进程；8765 / 1420 / 5173 端口空闲；磁盘 76G 可用。

### 8.8 遗留项声明（票面 §7「明列不隐藏」）

1. **cargo feature 与双 SDK 词汇收敛未做**（票 05 §7.10-1 遗留项，本票按用户裁定只声明不修）：`common-host` / `bare-host` 仍在、`lib.rs` 的 `compile_error!` 守卫仍在（**判据注释已改写成如实说明现状**，不再是失效理由）、`bedcode-plugin-api` vs `bedcode-plugin-api-mobile` 类型词汇未合一。证据：内核对 `bedcode-discovery-engine` / `bedcode-ws-client-engine` 零引用（两者仍在 `bare-host` feature 列表，`Cargo.toml` 已注为待摘过渡项）；移动端 SDK 词汇在内核的落点已由 purity 锁判据 7c **封顶**（唯一落点 + 仅两个允许符号），防止收敛前先扩散。
2. **移动 core-8 不切内核 `core_binding`**（ADR 0049 D7）：权限门词表 + 插件私有库布局两处不同源，前置 = 双 SDK 权限词汇并集 + 私有库布局统一。
3. **`on_message_binary` 平名探测缺陷**（票 05 发现，未修）：`bare/component.rs::verify_abi` 用 `"bedcode:plugin/events-binary#on-message-binary"` 平名串，组件接口导出是嵌套实例形态 ⇒ 探测恒 `None`。修它 = 行为变更，留专项。
4. **桌面 wasm 产物重建需 nightly 环境**（见 §8.7）：本机 stable 无 `wasm32-wasip3` 预构建产物，7 个依赖随包产物的测试目标在本环境无法转绿。
5. **两处 crate 根 `tests/` / 待登记桶遗留**：`bedcode-host-api-core/tests/`、`bedcode-headless-host-probe` + `bedcode-host-api-core` 未登记、`bedcode-mobile/plugins/` 约定根缺失——三者均为在途红，不在本票范围。
6. **票面 §2 与实际的一处偏差（已在上文说明）**：「禁 WASI 词汇」因裁决 3 作废、「双 SDK 词汇」改为封顶而非零命中；「形态壳词汇消失」的描述对应票 05 交付物，本票在其上补判据 7 与两处陈旧路径修复。