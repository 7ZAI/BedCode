# 02: host-pty 契约定稿（ABI v16）+ spawn→ring-fetch 最小贯通

**What to build:** 一条最小 tracer bullet 穿过所有层：插件在 manifest 声明 `pty:spawn` / `pty:io` 后，能 spawn 一个**只属于自己**的裸伪终端跑真实命令，并用 offset 游标把输出字节拉进插件内；未授权插件被拒，他人句柄对自己不可寻址。WIT 契约、宿主实现、SDK 调用面、组件接线、fixture 演示插件在本票内一次贯通。

**Blocked by:** 01（PTY 引擎前置改造——退出码 + 输出汇可注入）

**Status:** code-complete（2026-09-19；契约 v15→v16 一次定稿 + spawn→ring-fetch 全链路贯通，lib 971 绿 / 真 PTY e2e 绿 / SDK 双 target 绿；验收第 3 项的「B 订阅不到 A 的 topic」子断言顺延票 04/06，桌面唯一红项为认证中心线在途日志，详见 Comments）

## 已定案的行为（spec D5/D7/D8/D10/D11）

**契约一次写全，ABI 只 bump 一次。** 本票定稿全部 6 函数签名（含票 03/04 才实装语义的 write/resize/kill/is-running），desktop **v14 → v16**（v15 预留给认证中心线 host-auth，两线不撞号）。宿主侧 `version > 当前 → 拒绝`，旧插件（≤14）零迁移仍可加载。签名与 ring 语义以 spec D7 代码块为准，不在票内复制。

- **注册表与属主**：全局 `pty_id → PtyEntry { session, owner, ring }`；句柄 `pty-<uuid>`；全部函数先查属主，非属主 → `not owner of pty handle`（同 mdns/ws 的 NOT_OWNER 先例）。
- **权限两域**：`pty:spawn`（spawn/kill，任意命令执行高风险面）+ `pty:io`（write/resize/ring-fetch/is-running，数据面），可独立授予/审计。
- **spawn 最小原语**：只收裸命令 + 参数数组 + env/cwd/cols/rows（camelCase config-json）；**不做** shell 包装、WSL 路径转换、危险字符校验、默认 shell 探测（ADR 0022 裁剪线，业务性包装归插件层）。失败只回错误、不发任何事件。
- **PtyRing 自持实现**（VecDeque + 全局单调 offset）：本票实装「写入 → 按游标拉取 → next-offset 续拉」主干；淘汰/truncated/resync 的边界语义归票 05。
- **零接入业务线**：插件 PTY 不进业务会话注册表、不注册业务输出总线、不参与业务会话事件链；与 host-terminal / host-session / terminal-hooks 的边界写死在文档注释里。
- **SDK**：类型化 `HostPty` 调用面 + spawn config-json 组装助手，沿用既有 plugin world（**无可选导出**，push 模型已被 spec D3 否决）。
- **构建 target**：宿主运行时 async 化未落地（属认证中心线票 02），fixture 与测试产物沿用宿主**现役** target；wasip3 产物在 p2 sync 宿主上不可加载，本票禁止混用。
- **接线**：组件层 Host trait 实现 + linker 接线 + verify_abi。
- **移动端零改动**：WIT/ABI/SDK 不跟演（AGENTS §7 的文档化偏离，收口在票 07）。

## 验收

- [ ] fixture 演示插件 spawn 真实短命命令成功、拿到句柄，`ring-fetch` 拉到命令输出的字节，二次按 next-offset 拉取不重复内容
- [ ] 未声明 `pty:spawn` 的插件调用 spawn → permission denied（Rust 端最终仲裁，前端仅快速失败）
- [ ] 跨插件：A 的句柄被 B 调用 → `not owner`；B 无法订阅到 A 的 owner 作用域 topic
- [ ] 权限五同步点全部到位（SDK 权限常量与 API 映射 / 打包 CLI 合法集合 / 前端合法集合 / 宿主能力清单 / host_impl 权限门）——漏任一处即视为未完成
- [ ] ABI v14 → v16：WIT `abi` 版本演进注释补 v16 并注明 v15 预留 host-auth；版本断言测试同步更名；≤v14 旧插件仍可加载
- [ ] 插件 PTY 不出现在宿主业务会话列表/事件链中（业务线零感知）
- [ ] 宿主单测（真 PTY + 权限三态 + 属主隔离 + ring 主干）全绿；SDK `cargo test` + wasm32 check 全绿
- [ ] 桌面 `cargo test` + `pnpm run test:run` 全绿；根目录 `pnpm exec eslint .` 0 error；测试后清理 PTY 进程

## Comments

（实施记录追加此处）

### 2026-09-19 开工基线核实（先落不重叠的独立模块 PtyRing）

**共享文件基线与票面假设的两处偏离（开工前实测）**

1. **ABI 已在 v15**：认证中心线 host-auth / secret-store 已在工作区落地（`abi.rs` `ABI_VERSION = 15`、WIT `interface host-auth` + `world plugin` import、SDK/前端权限 `auth`）。本票的 bump 实际是 **v15 → v16**，票面「v14 → v16」按现状执行；WIT `abi` 版本演进注释需补 v15 + v16 两条（auth 线只补了 `abi.rs`，未补 WIT 注释）。
2. **宿主 async 化已落地**：`wasm_runtime.rs` 已 `config.wasm_component_model_async(true)`，且 `plugin-component-test` / `plugin-ws-test` / `plugin-sdk-test` 等内联 fixture 构建已全量迁到 **`wasm32-wasip3`**（`WASIP3_NIGHTLY = nightly-2026-09-16`，产物即组件、不再 `component new` 编码）。票面「async 化未落地、沿用现役 p2 sync target、禁止 wasip3 产物」的前提已失效——**新 fixture `plugin-pty-test` 改随现役 wasip3 target + nightly 工具链**，与 ws/component 系 fixture 保持同步。

开工基线：`cargo check --tests --offline` EXIT=0（认证线在途文件可编译）。

### 2026-09-19 已落地：`pty/pty_ring.rs`（spec D3 环形缓冲，零共享文件冲突）

**改动文件**：新增 `src-tauri/src/pty/pty_ring.rs`（实现 133 行 + 13 例单测）；`src/pty.rs` 追加 `pub mod pty_ring;` 与 `pub use pty_ring::{PtyRing, PtyRingFetch, PtyRingSink};`（各 1 行）。

**形态**：`VecDeque<RingChunk>` + 全局单调偏移（`min_offset`/`max_offset`）+ 字节容量上限；`PtyRingSink` 实现票 01 的 `PtyOutputSink`，`PtyRingSink::paired(capacity)` 一次给出（读线程侧 sink, 宿主侧 `Arc<Mutex<PtyRing>>`），即 `PtySession::with_private_sink` 的注入点。淘汰在 `push` 内均摊 O(1) 完成，源侧零等待；跨块合并 + 半块裁头由环负责（插件面只看到连续字节区间）。

**行为契约（13 条，逐条对测试）**

| 契约 | 来源 | 规则 | 测试 |
| --- | --- | --- | --- |
| C-001 | spec D3 | push 后从头拉 = 全量，`next_offset` = 累计产出 | `push_then_fetch_returns_all_bytes_and_advances_cursor` |
| C-002 | 票 02 主干 | 按 `next_offset` 续拉不重复 | `fetch_from_next_offset_returns_only_new_bytes` |
| C-003 | spec D3 | 游标追平 → 空 + `truncated=false` | `fetch_at_produced_end_returns_empty_and_not_truncated` |
| C-004 | D7 `max-bytes` | 截断到 `max_bytes`，游标停在截断处 | `fetch_caps_data_at_max_bytes_and_keeps_cursor` |
| C-005 | 消费者视图 | 跨块按序合并、块边界不可见（含游标落块内裁头） | `fetch_merges_chunks_in_produce_order` |
| C-006 | D3 满淘汰 | 超容量丢最旧，`min_offset` 前移、`max_offset` 不回退 | `push_beyond_capacity_evicts_oldest_chunks` |
| C-007 | D3 truncated/resync | 游标落后环起点 → `truncated=true` 且从 `min_offset` 起返 | `fetch_behind_ring_start_reports_truncated_and_resumes_at_min_offset` |
| C-008 | 假设（自愈） | 未来游标钳到产出端、回带 `max_offset` | `fetch_with_future_cursor_clamps_to_produced_end` |
| C-009 | 业务环同惯例 | 单段超容量 → 整环腾清仍留该段 | `push_larger_than_capacity_keeps_only_that_chunk` |
| C-010 | 边界 | `max_bytes=0` → 空且不前进游标 | `fetch_with_zero_max_bytes_returns_empty_without_advancing` |
| C-011 | 边界 | 空投递不改偏移/不产生块 | `push_empty_bytes_leaves_offsets_unchanged` |
| C-012 | ADR 0022 | sink 落自备环、业务会话环零留痕 | `ring_sink_delivers_to_own_ring_without_touching_session_bus` |
| C-013 | 线程契约 | 读线程写 / 宿主函数读跨线程可见且守恒 | `ring_is_visible_across_producer_and_consumer_threads` |

`ASSUMPTION`：C-008 的未来游标处理票面/spec 未定义，取「按已追平处理 + 回带 `max_offset` 自愈」，与 ws `is-connected` 自愈哲学一致；若要 fail-visible（Err）在宿主 `ring-fetch` 层加判错更合适，环本身不判错。

**实跑证据**

- `cargo test --offline pty::pty_ring` → `13 passed; 0 failed`
- `cargo test --offline pty::` → `61 passed; 0 failed`（含票 01 真 PTY 用例，未回归）
- `rustfmt --edition 2021 --check src/pty/pty_ring.rs` → 无 diff（仅格式化本文件，未跑全仓 `cargo fmt`，避免重排认证线在途文件）
- `cargo clippy --offline --tests` → `pty_ring` 无告警
- 测试后无 `bash -lic` / `sleep 30` / `pty-reaper` 残留进程

**变异自检（实跑后全部还原）**

| 变异 | 结果 |
| --- | --- |
| `push` 淘汰条件短路（`while false && ...`，环变无界） | 3 例失败：C-006 淘汰、C-007 truncated、C-009 单块超容量 ✅ 杀死 |
| `truncated` 判据 `from_offset <= min_offset`（off-by-one） | 4 例失败：C-001 / C-002 / C-004 / C-005（`!truncated` 断言） ✅ 杀死 |
| `start` 去掉 `.min(max_offset)` 钳位 | 1 例失败：C-008（`next_offset` 回带 999 而非 6） ✅ 杀死 |

**未覆盖风险（留给票 05 / 票 02 接线）**

- 极小块风暴下的**条目数**上限未设（业务环有 `max_chunks` 防御）：本环只按字节上限淘汰，1 字节块理论上可堆到 `capacity` 个条目——限额归票 05（D9 `PLUGIN_PTY_RING_BYTES` 一并评估）。
- 容量常量取值（D9）与 `PLUGIN_PTY_RING_FETCH_MAX_BYTES` 单次边界拷贝上限未定，容量以构造参数传入。
- 环与 `PtySession` 生命周期耦合（退出后何时摘环）属票 04。

### 2026-09-19 主干贯通落地（WIT → 宿主 → 接线 → SDK → 权限 → fixture）

**用户裁决**：① 现在就动共享文件（认证线 v15 在途文件以「只追加段落」方式改，禁止整文件回滚/重排）；② fixture 与测试产物随**现役 `wasm32-wasip3`**（票面「禁止 wasip3」前提已被认证中心线的 async 化落地推翻）。

**改动文件（按契约→实现→接线→权限顺序）**

- WIT `packages/plugin-sdk-desktop/rust/wit/bedcode.wit`：新增 `interface host-pty`（`record ring-fetch-result` + `spawn/write/resize/kill/ring-fetch/is-running` 六函数，签名一次定稿）、`world plugin` 追加 `import host-pty`、`abi` 版本演进注释补 **v15（host-auth）+ v16（host-pty）** 两条（auth 线只补了 `abi.rs`，WIT 注释此前停在 v14）。
- `abi.rs`：`ABI_VERSION` 15 → **16** + v16 演进注释 + 断言测试更名 `test_abi_version_is_v16`。宿主侧兼容语义不变：`version > ABI_VERSION → 拒绝`，≤v15 旧插件零迁移可加载。
- 引擎地基补充（票 01 的另外一半）`pty/pty_process.rs`：新增 `PtyCommandSource { Business, Raw }` 与 `PtySession::with_private_command`——**命令来源可注入**。此前 `start()` 硬绑 `build_command`（`bash -lic` 包装 / WSL 转换 / cwd 兜底 / `BEDCODE_SESSION_ID` 注入），插件私有 PTY 无法做到 spec D5 的「裸 argv exec」；业务线继续走 `Business`（含 env 注入位置与现役完全一致），仅新增 `Raw` 分支。`PtySessionState.config` → `command: Option<PtyCommandSource>`（`start()` 一次性取走，与 `pair` 同形）。
- `pty/pty_ring.rs`（上一节）→ `host_impl/pty.rs`：`LazyLock<Mutex<HashMap<pty_id, PtyEntry{owner, session, ring}>>>` + `pty_spawn`（config-json camelCase 校验 → 裸 argv `CommandBuilder` → `with_private_command` → `start`）+ `pty_ring_fetch`（环锁在注册表锁外应答；`Ok(None)` = 游标追平；单次截断到 `PLUGIN_PTY_RING_FETCH_MAX_BYTES`）+ 属主仲裁 `not owner of pty handle`。
- `component.rs`：`impl bedcode::plugin::host_pty::Host`（6 函数转发，`PtyRingFetch` → WIT `RingFetchResult` 单点映射）+ `host_pty::add_to_linker` 接线 + `host_impl` 导入追加 `pty`。
- 能力清单 `capability.rs`：`HOST_PRIMITIVE_CAPABILITIES` 追加 `host-pty`（19 → 20 组）。
- 限额常量 `system/constants/plugin.rs`：`PLUGIN_PTY_RING_BYTES`（256 KiB，刻意低于业务会话环量级，理由见注释）+ `PLUGIN_PTY_RING_FETCH_MAX_BYTES`（16 KiB）；每插件会话数与 write 上限留票 05。
- SDK：新增 `host/pty.rs`（`HostPty` trait + `PtyRingFetch` + `PtySpawnConfig` 组装助手 + `PTY_EXIT` + `pty_event_topic`，文档写死订阅时序/truncated/resync/exit 即摘除等契约）、`host/mod.rs` 导出、`wasm_host.rs` `impl HostPty for WasmHost`、`permission.rs` 两域常量 + `VALID_PERMISSIONS` + `PERMISSION_API_MAP`（`pty.spawn/kill`、`pty.write/resize/ringFetch/isRunning`）。**未进 `HostApi` 聚合 trait**——与 v15 `HostAuth` 同惯例（避免 mock 连带扩面）。
- 权限同步点：打包 CLI `bin/cli.js` 合法集合 + 前端 `src/plugin/permission.ts` 合法集合与 `PERMISSION_API_MAP`（WASM-only，映射为空数组）。
- fixture：新增 `packages/plugin-pty-test`（`plugin.json` 声明 `pty:spawn`/`pty:io`；`lib.rs` activate 期订阅 `pty:exit.<owner>` + `pty-spawn` / `pty-ring-fetch` / `pty-state` 三命令）；宿主 e2e `test_pty_spawn_ring_fetch_roundtrip`（A/B 双实例、双 runtime，见验收）。

**契约边界记录（供票 03/04/05 接续）**

- `write` / `resize` / `is-running` / `kill`：**契约已定稿、语义未实装**。四者一律先过权限门与属主仲裁，再返回 `host-pty <api> is not implemented in this build (lands in ticket 03/04)`——fail-visible，绝不静默成功；`PtyEntry.session` 因此带 `#[allow(dead_code)]`（它仍是进程存活的持有者）。
- `ring-fetch` 的 `Ok(None)` 语义由本票定稿为「游标已追平」；「句柄不存在/非属主/缺权限」一律 `Err`（D7 的 `option` 载荷不含 not-found 语义，避免与 `Err` 混用）。
- `PtyRing` 的未来游标（`from_offset > max_offset`）取「钳到产出端 + 回带 `max_offset`」自愈口径，不报错（记为 `ASSUMPTION`）。

**并发会话证据（同 `wasm_runtime.rs` 在途）**：本轮 e2e 编译验证被认证中心线在途改动挡住——`test_devices_plugin_artifact_lifecycle`（对侧新增，`wasm_runtime.rs:3130`）用 `host_ctx.db.lock().unwrap()`，而 `db` 是 `tokio::sync::Mutex` → `error[E0599]`。本票全程未碰对侧用例（AGENTS §11 / 票 01 同型记录），只做 `pty` 相关追加；对侧收尾后复跑全量。

### 2026-09-19 验收清单

- [x] **fixture spawn 真实短命命令 → `ring-fetch` 拉到输出 → 二次按 `next-offset` 不重复**：e2e `test_pty_spawn_ring_fetch_roundtrip`（`/bin/echo <marker>`，实返 37 字节含 `\r\n`，`truncated=false`，`nextOffset == data.len`，续拉回 `{none:true}`）+ 宿主单测 `spawn_runs_real_command_and_output_reaches_ring_fetch` / `second_fetch_from_next_offset_returns_nothing_new`
- [x] **未声明 `pty:spawn` → permission denied（Rust 端最终仲裁）**：e2e 里 B 实例（只授 `pty:io`）调 `pty-spawn` → error 载荷含 `permission denied: pty:spawn`；宿主单测 `spawn_without_permission_is_denied` / `spawn_with_io_permission_only_is_denied`（两域独立）
- [x] **跨插件 A 的句柄被 B 调用 → `not owner`**：e2e（同一产物双 runtime 双实例）+ 宿主单测 `ring_fetch_on_foreign_handle_is_not_owner`。`B 无法订阅到 A 的 owner 作用域 topic` 的负向断言**顺延票 04/06**——`pty:exit` 事件本票未落地（票 04），此刻断言的是不存在的事件
- [x] **权限五同步点全部到位**：SDK 常量+合法集合+API 映射 / 打包 CLI / 前端合法集合+映射 / 宿主能力清单（`host-pty`，19→20 组）/ `host_impl/pty.rs` 权限门；并补**漂移锁** `permission_sync_points_all_know_pty_domains`（SDK 与能力清单走行为断言，CLI/前端走字面量断言，漏一处即红）
- [x] **ABI v15 → v16**（票面「v14→v16」按现状执行，见开工基线核实）：WIT `abi` 注释补 v15+v16、`abi.rs` 注释与断言测试 `test_abi_version_is_v16`；旧插件零迁移可加载由 `version > ABI_VERSION → 拒绝` 语义不变保证，实证：v14/v15 产物 fixture（component / sdk / ws / system 四套 e2e）在本次全量 lib 测试中照常实例化并通过
- [x] **插件 PTY 业务线零感知**：`spawned_pty_is_absent_from_business_session_lines`（`GlobalOutputManager::has_session` 假 + `session_manager.get_session` None + `list_sessions()` 空）；`PtyRingSink` 侧另有票 01 的「自备 sink 业务环零留痕」用例
- [x] **宿主单测全绿**：`host_impl::pty` 14 例（权限三态 / 参数校验 / 属主隔离 / 真 PTY 输出 / argv 免 shell 解释 / env+尺寸落地 / 漂移锁）；`pty::` 全套 75 例（含票 01 真 PTY 用例无回归）
- [x] **SDK 验证全绿**：`cargo test` 84 passed（含 `host::pty` 4 例 + abi v16 断言）；`cargo check --features wasm --target wasm32-unknown-unknown`（stable，随 sdk-publish CI）与 `--target wasm32-wasip3`（nightly-2026-09-16，随现役 fixture 链）双通过
- [~] **桌面门禁**：lib `cargo test` **971 passed / 0 failed**；9 个集成目标 **8 绿 1 红**，红的正是唯一与本票无关的那条（下），**业务 PTY 链路集成 `tests/pty_session_chain.rs` 绿**（本票动 `pty_process.rs` 的业务线零变化证据）；`pnpm run test:run` **672 passed（70 files）**；根 `pnpm exec eslint` 对改动文件 0 error；测试后无 `stty`/`env`/`echo`/`pty-reaper` 残留进程、无僵尸。**唯一红项非本票引入**：集成目标 `tests/broadcast_shutdown.rs` 断言「流程内不得有 error 级日志」，命中 `bedcode_lib::utils::auth::jwt: jwt: secret store unavailable, falling back to process-random key`——抛出点在认证中心线**未跟踪新文件** `src/utils/auth/host_secrets.rs:127`（`jwt.rs` 亦有 109 行未提交改动），属对侧 secret-store 未在该集成夹具中初始化。本票不修对侧文件（AGENTS §11），已在此登记待对侧收尾
- [x] **移动端零改动**：WIT/ABI/SDK/插件全部只动 desktop（偏离文档化收口在票 07）

**变异自检（e2e + 宿主层，实跑后全部还原）**

| 变异 | 结果 |
| --- | --- |
| e2e 首拉 `truncated` 期望改反 | e2e FAILED（并打印真实载荷 `{"data":[66,69,…],"nextOffset":37,"truncated":false}`）✅ 杀死 |
| `pty_ring_fetch` 属主判据改恒真（绕过 owner） | 宿主单测 `ring_fetch_on_foreign_handle_is_not_owner` FAILED + e2e FAILED（B 能读到 A 的输出）✅ 双杀 |
| PtyRing 三发（淘汰短路 / truncated off-by-one / 游标钳位去除） | 见上一节，分别杀 3 / 4 / 1 例 ✅ |

**未覆盖风险（本票遗留，交后续票）**

1. `write` / `resize` / `kill` / `is-running` 为**契约已定稿、语义未实装**（权限+属主后返回明确 `not implemented … lands in ticket 03/04`）——插件在 ABI v16 上调用会得到 fail-visible 错误，不会被静默吞掉；票 03/04 落地时替换。
2. `pty:exit` 事件链未落地（票 04）：`PtyTerminated` → owner 作用域 topic 发布、exit 即摘环、`purge_for_plugin` 停用回收均未接线；fixture 已按契约在 activate 期订阅，票 04 可直接续用。
3. 限额只落了 ring 容量与单次拉取上限（`PLUGIN_PTY_RING_BYTES` / `PLUGIN_PTY_RING_FETCH_MAX_BYTES`），且**拉取上限的截断行为尚无专项用例**（当前用例 max-bytes 均小于上限）——每插件会话数、write 上限、淘汰/背压/resync 边界与相应用例归票 05。
4. 业务线零变化依赖票 01 的 `Hold` 策略回归锁；本票对 `pty_process.rs` 的改动只增 `Raw` 分支，`Business` 分支的 `build_command` + `BEDCODE_SESSION_ID` 注入顺序与现役逐行一致，由票 01 的集成链路与本票 75 例 `pty::` 用例共同兜住。
5. Windows/macOS 未验证（本机 Linux）：真 PTY 用例与 `stty size` 反查均 `#[cfg(target_os = "linux")]` 门控；`CommandBuilder` 裸 exec 在 ConPTY 下的行为待发布前实机验证（同票 01 的 ConPTY 注记，收口票 07）。
