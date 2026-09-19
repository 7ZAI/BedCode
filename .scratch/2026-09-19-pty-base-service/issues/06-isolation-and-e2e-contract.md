# 06: 隔离与契约固化——fixture e2e 全链路矩阵

**What to build:** 用最高 seam 一次性证明能力闭环并把它冻结成回归基线：一条 fixture 演示插件把「WIT 契约 → 宿主实现 → 组件接线 → 权限门 → SDK 调用面 → 真 PTY 行为」全链路跑通，覆盖全部 6 个函数的正反例、跨插件隔离矩阵、权限三态与停用回收。后续任何人改 host-pty，都必须让这张矩阵保持绿。

**Blocked by:** 03、04、05（数据面 / 生命周期 / 背压限额三条线全部就绪）

**Status:** done（2026-09-19；矩阵 e2e `test_pty_isolation_and_contract_matrix_roundtrip` 落地（①-⑤ 分格自带定位）+ 宿主「零授权」分格补齐 + ADR 0017 互调门禁分格补口，桌面 lib 999 全绿、集成 8 目标绿，变异 N6 实证「单函数破口只翻对应分格」）

## 已定案的行为（spec §Testing ①②）

- **矩阵覆盖**（断言只打外部行为，不测注册表内部字段、不 mock PTY 本体）：
  - 正路径闭环：spawn → ring-fetch 断言输出 → write 回读 → resize → kill → 收 exit 事件
  - 权限三态：grant 双域可用 / 只 grant io 时 spawn 被拒 / 完全不 grant 全被拒
  - 属主隔离：跨插件对**每一个**句柄型函数都验一次 not owner（漏一个函数即契约破口）
  - 事件定向：非属主收不到他人 owner 作用域 topic
  - 停用回收：deactivate 后进程消失 + 注册表空 + 只碰本人
  - 缺口语义：offset 落后 → truncated + resync 可续拉
- **fixture 归属**：演示插件放桌面 packages 下的 fixture 工程位（命名对齐 ws/mdns fixture 惯例）；mock/种子数据归插件工程自身入口导出，**禁止落 dev-shell**（AGENTS.md §7）。
- **构建 target 硬约束**：宿主运行时 async 化尚未落地（属认证中心线票 02），fixture 必须沿用宿主**现役**构建 target；wasip3 产物在 p2 sync 宿主上不可加载，不得混用。
- **宿主侧单测补齐**：`build_host_ctx` 模式下补权限门/回收/配额的分格用例，与 fixture 矩阵不重复造轮子。

## 验收

- [x] 上表六类断言在 fixture e2e 中全部通过，且失败时任一断言都能独立定位（不打包成单个大 assert）
- [x] 跨插件隔离矩阵覆盖全部句柄型函数（spawn 除外，无句柄入参）——4 个在 e2e、`kill` 在宿主层（见「矩阵落点表」②）
- [x] 权限三态用例齐备，未声明 api 的互调路径一并验证（ADR 0017）
- [x] 断言只依赖外部行为；无恒真断言、无快照替代行为断言（unit-test-discipline 自查）
- [x] 桌面 `cargo test` 全绿；测试后清理：无 PTY 进程残留、无端口占用、fixture 产物不入库（AGENTS.md §3/§9）

## Comments

### 2026-09-19 实施落地

**票面「构建 target 硬约束」的前提已失效**（同票 02 的开工基线核实）：宿主 async 化与 fixture 全量 `wasm32-wasip3` 已由认证中心线落地，本票沿用现役 wasip3 target + `nightly-2026-09-16`，未混用 p2 产物。

**改动文件**

- `packages/plugin-pty-test/src/lib.rs`：新增 `pty-call-undeclared-api` 命令（经 `HostBus::bus_publish` 打 `bedcode.api.<self>.<api>`，验 ADR 0017 互调门禁；fixture 的 manifest `api: []` 即负向前提）。
- `wasm_runtime.rs` 测试模块：新增 `pty_error_of`（guest error 载荷 → `Option<String>`，成功载荷视为破口）与矩阵 e2e `test_pty_isolation_and_contract_matrix_roundtrip`；`pty_command_error`（无标签、缺载荷即 panic）保留给「必定失败」的分格。
- `host_impl/pty.rs`：新增 `every_api_without_any_permission_is_denied_before_any_lookup`——零授权插件打全部 6 函数，断言一律 `permission denied: pty:*` **且不得出现 `not found`**（即权限门先于属主仲裁与句柄查表），并验注册表零副作用。

**矩阵落点表（六类断言 × 分层，一格一落点，不重复造轮子）**

| 分格 | 最高 seam（fixture e2e） | 宿主层单测 | 环 / SDK 层 |
| --- | --- | --- | --- |
| ① 正路径闭环 | `test_pty_isolation_and_contract_matrix_roundtrip`：①-a 句柄形状 → ①-b write 回报长度 → ①-c ring-fetch 拉到 `OUT:` 前缀（进程加工证据） → ①-d resize ok → ①-e is-running true → ①-f kill ok → ①-g/①-h exit 事件 topic + reason → ①-i 摘除后 `not found` | 票 02/03/04 各自分格（`spawn_runs_real_command_*` / `write_response_comes_from_process_not_tty_echo` / `resize_changes_kernel_winsize_*` / `kill_terminates_handle_*`） | — |
| ② 属主隔离（全句柄函数） | ②-b 循环验 `pty-write` / `pty-resize` / `pty-ring-fetch` / `pty-is-running` 四函数 `not owner`；②-c 验拒绝零副作用（属主句柄仍 running） | `kill` 的属主分格：`kill_still_enforces_owner_before_its_own_gate`（e2e 侧 B 无 `pty:spawn`，若在此验 kill 只能撞到权限门，属主判据会被掩盖） | — |
| ③ 事件定向 | ③ B 全程无 PTY → 事件流必须为空，且不得含他人 ptyId | `exit_events_are_owner_scoped_and_exactly_once`（同一 bus 上双属主订阅，负向 + 恰好一次） | — |
| ④ ADR 0017 互调 | ④ `pty-call-undeclared-api` → `not declared` | `host_impl/api.rs` / `bus.rs` 既有门禁用例（通用能力，不属 host-pty） | — |
| ⑤ 权限三态 | grant 双域＝①全绿分格；只 grant io → ⑤ spawn 与 kill 均 `permission denied: pty:spawn` | 完全不 grant → `every_api_without_any_permission_is_denied_before_any_lookup`（6 函数 + 门序 + 零副作用）；两域独立 → 票 02 `spawn_with_io_permission_only_is_denied` / `ring_fetch_without_io_permission_is_denied`、票 03 `io_apis_without_io_permission_are_denied` | SDK/CLI/前端/能力清单/宿主门五同步点漂移锁 `permission_sync_points_all_know_pty_domains`（票 02） |
| ⑥ 缺口语义 | `test_pty_declared_ring_backpressure_roundtrip`（小环 + 落后游标 → truncated / nextOffset 续拉） | `small_declared_ring_evicts_*` / `ring_fetch_is_capped_per_call_*` | `PtyRing` C-004 / C-006 / C-007 / C-014 / C-015（票 02 + 票 05） |
| 停用回收 | `test_pty_exit_event_and_purge_roundtrip`（含 deactivate 接线锁） | `purge_for_plugin_retires_all_owned_handles_and_touches_nobody_else` | — |
| 配额 | —（e2e 打满 8 条成本过高，属主独立性与归还已在宿主层锁） | `pty_quota_is_per_plugin_and_rejects_overflow_without_side_effects` | — |

**分格自带定位**：②-b 的失败信息带命令名与真实载荷（`②-b pty-write 必须被属主仲裁拒绝，got 成功载荷: {"len":3,"ok":true}`），①-* 每步单独 assert 且消息带步骤号——票面「不打包成大 assert」由此满足，实证见下节变异 N6。

**实跑证据**

- `cargo test --offline --lib -- ::pty test_pty` → **74 passed / 0 failed**（引擎 + 宿主 + 5 条 e2e）
- `cargo test --offline --lib` → **999 passed / 0 failed**；`cargo test --offline --tests --no-fail-fast` 见票 05 同一轮收口记录
- fixture 产物：`packages/plugin-pty-test/target/` 属生成物，未入库（AGENTS §9）；`git status` 复核本次仅新增/修改 pty 线文件
- 测试后进程核查：无 `sed` / `sh -c read go` 载体残留、无监听端口（矩阵用例以 `pty-kill` + `purge_for_plugin` 双路清场）

**变异自检（实跑后已还原）**

| 变异 | 结果 |
| --- | --- |
| N6 只让 `pty_write` 绕过属主仲裁（其余函数不动） | e2e ②-b 翻红且消息自带分格名 `pty-write`（`got 成功载荷: {"len":3,"ok":true}`）；宿主 `io_apis_enforce_owner` 在其 write 断言处翻红；**矩阵其余分格与另 3 个函数的负向断言全绿** → 证明「单函数破口 → 单分格定位」✅ |

**与票面的偏离**

- 票面「宿主侧单测补齐 `build_host_ctx` 分格用例」在本票只补了「零授权全函数」一格：其余分格（配额、回收、两域独立、事件恰好一次）在票 02-05 已各自落地并绿，重复实现会制造同义测试。
- 票面「跨插件隔离矩阵覆盖全部句柄型函数」在 e2e 覆盖 4/5，第 5 个（`kill`）落在宿主层——理由见矩阵落点表 ②：e2e 里 B 无 `pty:spawn`，对 B 的 kill 调用必然先撞权限门，属主判据被掩盖；若为此给 B 补 `pty:spawn`，就得再引入第三个实例来保住「B 无 spawn」的 ⑤ 分格。

**未覆盖风险（交票 07 / 后续）**

- 矩阵未覆盖「配额打满 + 属主隔离」的端到端组合（A 满 8 条时 B 仍可建满自己的 8 条），当前只在宿主层验属主独立计数；真实多插件并发下的注册表竞争未压测。
- `pty-call-undeclared-api` 走的是 publish 侧门禁；请求-响应式互调（`bedcode.api.reply.`）分支不属 host-pty 语义，由 `host_impl/api.rs` 既有用例覆盖。
- 非 Linux：矩阵全部门控 `#[cfg(target_os = "linux")]`（真 PTY 依赖），Windows/macOS 需发布前实机跑一次（票 07 的 ConPTY 注记）。
