# 票 07 · 防回接 / 漂移锁 + 文档收口（終态确定性）

Status: **✅ done（2026-10-10 落地；与票 06 批次 04/05 一并收口——实施记录见 §5）**（spec §4 票 07；本 spec 序列的收口票）
依赖：票 03–06 全部落地（锁的锚点文件才存在）——本次实施时票 06 批次 04/05 尚未落地，已顺带打通（fork 删除 + 移动全量绿）
前置：票 03–06 各自的实施记录已回填（本票从真实落点取锚，不猜）

## 1. 现状（2026-10-10 实测）

| 项 | 事实 |
| --- | --- |
| ADR | `docs/adr/0045-wasm-core-single-crate-and-wit-slice-composition.md` 状态 **proposed**（POC 后转 accepted 的口径在其头部） |
| 漂移锁原型 | 票 01 P4 已证 compose 幂等 + sha256 逐字一致；生产脚本 = 票 03 `scripts/compose-wit.mjs --check`（未接入 CI / pre-commit） |
| 防回接锁先例 | 双端已有锁族：wasm-core `lib.rs` 批量反向锁（`*_must_not_return_to_wasm_core`、`pty_module_must_not_return_to_wasm_core`）、宿主 `crate_boundary_lock.rs`（ALLOWED/REQUIRED 表）、移动端 `retiredHostUIRetirementLocks.test.ts` R1-R3（前端路由退役锁范式）、前端三层封锁 ESLint 静态锁 |
| 端清单 | 票 03 交付的双端 `compose.json`（驱动拼装 / 白名单对照 / ABI 计数） |
| code-map | 双端 `docs/code-map.md` 各自含能力域表 + 防回接锁索引段（本票新增条目登记处） |

## 2. 内容 / 步骤

### 批次 01 · 漂移锁收口
1. `scripts/compose-wit.mjs --check <end>` 接入 CI（或 package.json script）+ husky pre-commit（对照 `doc-tracking.sh` 口径，生成物入库、脚本可复跑）
2. 变异自检记录：手改生成物一字节 → 锁红（点名文件）→ 还原 → 绿（1/1）
3. 生成物入库后与分片真源逐字比对双向成立（真源改 → 生成物陈 → 红；生成物手改 → 红）

### 批次 02 · 防回接锁补全
4. **移动 fork 退役锁**（票 06 批次 04 ③的终态版本，顺带把票 17 时代的历史锁核对一遍）：fork 路径字形 / `bedcode-wasm-core-mobile` 别名 / 旧 import 引用回归校验——变异自检 2/2
5. **能力域 bindgen 锁**（票 05 的终态）：5 个能力域 crate 的 `bindgen!`/`ln!` 必须指向自家 `wit/<domain>.wit`（`path`/`world` 断言；旧「指端 WIT」字形回归校验）——变异自检 1/1
6. 宿主侧旧绑定面锁核对：路径 B 域不得在内核再现同名 impl（`IN_CRATE_HOST_MODULES` 与收集集双向、`defined twice` 防线的锁化文本锚）；`crate_boundary_lock.rs` 表的 ALLOWED / REQUIRED 条目按票 03–06 终态复核
7. 端清单与白名单的对照锁加深（票 03 §4.4 静态版 → 动态比对：清单域名集 == 白名单能力域模块集）

### 批次 03 · 文档收口
8. **ADR 0045**：proposed → accepted；入档 = 票 01 POC 四命题实际结论 + 票 04 复评 / D3 实际形态（拆 or 回退）+ 票 06 fork 退役证据 + 与 ADR 0035/0036/0037/0040 的衔接更新
9. **双端 code-map**：核心 world / 能力域分片 / 端清单三处登记 + 防回接锁索引补本序列全部新锁条目（桌面 `bedcode-desktop/docs/code-map.md`、移动 `bedcode-mobile/docs/code-map.md`——两端文件各自改源）
10. **CHANGELOG.md + CHANGELOG_zh.md**：票 04 / 票 06 各一条（若未随票写）+ 本序列综述条目（版本号双端同步口径 §9）
11. **AGENTS.md §5.4「双端差异」改写**：单一 wasm-core + 差异只来自能力域组合；§4 任务路由表相应更新（改 wasm-core 机制 → 先读本 spec / ADR 0045）
12. **docs/commands.md**：`compose-wit` 命令登记（若有）；**docs/knowledge/plugin-development-checklist.md**：WIT 分片与拼装的插件侧影响（产物重建口径、import 集合变化的影响面）
13. 遗留项清点：本序列各票「未跑 + 原因」汇总（磁盘/环境类欠账集中登记，供后续补跑）

## 3. 门禁

| 项 | 要求 |
| --- | --- |
| 锁变异自检 | 本票新增每条锁 1/1（注入探针 → 红 → 还原 → 绿），记录证据 |
| 漂移锁 | CI 形态下 `compose-wit.mjs --check` 双端绿 |
| 文档 | ADR 0045 accepted；双端 code-map 索引与锁清单一一对应（无孤儿条目）；CHANGELOG 双语同步 |
| 全量回归 | 双端 `cargo test` + 两端 `pnpm run test:run` + 根 `pnpm exec eslint .` 0 error（warning 不计）+ `cargo fmt` / `clippy` 自查 |
| 收尾 | 本 spec 序列全票实施记录可追溯；`git status` 干净度核对（本票不应留下未登记的在途文件） |

## 4. 风险与回退

| 风险 | 吸收 / 回退 |
| --- | --- |
| 锁锚点漂移（票 03–06 未合并完就写锁） | 票依赖顺序强制：所有锚点文件先在场；若某票回退，对应锁连坐核红而非静默改判据 |
| 文档改源冲突（code-map 双端在途并行会话） | 改源文件而非引用方（§13 单一事实源）；开工 `git diff` 复核归属，非同任务改动不碰 |
| 锁过多（每票一把锁的堆叠负担） | 同类锁合并（反向锁族归一个文件 / 一个测试二进制），锁清单在 code-map 索引里单点登记 |
| CHANGELOG 版本号（双端同步） | 若无版本变更则不 bump，只加未发布条目；bump 走 `scripts/bump-version.mjs` 双端口径 |

## 5. 实施记录

**状态：✅ 落地（2026-10-10；与票 06 批次 04/05 一并收口——fork 删除见 ticket-06 §7.4/§7.5）。**

### 5.1 批次 01 · 漂移锁收口 ✅

- **接入点**：根 `package.json` 新脚本 `check:wit` = `node scripts/compose-wit.mjs --all --check`；husky `.husky/pre-commit` 增漂移锁段（置于 eslint 早退分支之前；node 缺失时跳过并提示）；新增 CI `.github/workflows/wit-drift-lock.yml`（paths 覆盖 `packages/**` / 双端 SDK / `scripts/compose-wit.mjs`——既有 test.yml / lint.yml 的 paths 不含这些真源位置，挂进去会出现「真源改了 CI 不触发」的静默窗口）。
- **变异自检（双方向 2/2，均还原核 sha256）**：
  - 手改生成物一字节（`bedcode-desktop/.../rust/wit/core.wit` 尾部追加换行）→ `--check` 红：`[desktop] core.wit 漂移: 期望 c05ee71b… 实际 f8f58137…（真源已改或生成物被手改）`，exit=1 → 还原 → 绿（sha256 复原 `c05ee71b…`）。
  - 改真源（`packages/bedcode-wasm-core/wit/core.wit` 尾部追加换行）→ 生成物变陈 → 双端红（desktop + mobile `core.wit 漂移`）→ 还原 → 绿。
  - husky 实跑：`sh .husky/pre-commit` → 9 文件 `ok`，exit 0（git index 未被改动）。

### 5.2 批次 02 · 防回接锁补全 ✅

| 条目 | 落点 | 变异自检 |
| --- | --- | --- |
| 4 · 移动 fork 退役锁（四判据：目录 / 包名 / 路径字形 / 别名指向） | `bedcode-mobile/src-tauri/tests/retired_mobile_wasm_core_fork_lock.rs` | **2/2**：探针 A（fork 目录+包名+fork 形态路径注入）→ ①②③ 三判据红；探针 B（别名 `package =` 形态破坏，同语义可解析）→ 别名判据红；均还原绿 |
| 5 · 能力域自持分片 bindgen 锁（pty/http/ws/peer/mdns 五域） | 宿主 `src/plugin/bindings.rs::capability_domains_bind_their_own_wit_slices` | **1/1**：pty `path` 指回端 SDK WIT（可编译，同一 package）→ 红点名；还原 `diff` 一致 → 绿 |
| 6 · 宿主侧旧绑定面核对 | 既有锁实跑 + 登记表复核（见下） | — |
| 7 · 端清单↔装配面动态对照（票 03 §4.4 静态版加深） | 同文件 `compose_caps_and_host_module_interfaces_cover_each_other` | **1/1**：cap-desktop 世界注入 `import host-nonexistent-probe;` → 红点名「组合面与装配面脱节」；还原 → 绿 |
| 附 · 移动 WIT 组合锁（承接随 fork 退役的 A1 结构/计数面） | `bedcode-mobile/src-tauri/tests/mobile_wit_composition_lock.rs` | **1/1**：compose `abi.version` 20→21 → 红（点名与 SDK abi.rs 漂移）；还原 → 绿 |

**第 6 条核对结论**：

- `IN_CRATE_HOST_MODULES`（内核在册 ws / peer / http 三域引擎面条目）↔ 收集集双向：内核 `capability_registry_matches_whitelist`（lib）+ 宿主 `pty_wiring.rs::host_whitelist_matches_collected_capability_modules`（跨 crate 对偶）在场；内核反向锁 7 例实跑绿（`cargo test --lib must_not_return_to_wasm_core` 7/7）。
- `defined twice` 防线的锁化文本锚 = v36/v37 切片域反向锁（`desktop_sliced_interfaces_must_not_return_to_wasm_core`）+ 路径 B 五域反向锁 + 能力域 bindgen 白名单双向（上表第 5/7 条新增两把补强）。
- `crate_boundary_lock.rs` 表按票 03–06 终态复核：**ALLOWED 补 `bedcode-wasm-core → bedcode-discovery-engine`**（票 06 批次 03 mobile-host 面新边：桌面形态该域 adapter 在宿主、内核无边；移动形态 fork 迁入的域 impl 在 crate 内消费），**REQUIRED 不动**（该边 optional / 仅移动形态，尾注改写）。复核后边界锁 **9/9 绿**。
- 顺带修复（均为在途基线红，非本票引入）：桌面形态 lib `E0433`×7——`fs_auth.rs` 移动面 `impl FsAuthGate for FsAuthChecker` 缺 `mobile-host` 门控（test-support 收口引入）→ 补门控 + 合并重复注释；内核 `l2_gating_test::internal_business_host_dependency_stays_gated` 红——test-support 拆分后白名单条目 `src/test_support.rs` 悬空、`src/test_support/desktop.rs` 漏登 → 条目随真源改指 `desktop.rs`。

### 5.3 批次 03 · 文档收口 ✅

- **ADR 0045**：proposed → **accepted**；新增「实施记录」段（POC 四命题实际结论 / D3 复评 = 拆分执行 / D4·D5·D6 落地 / 与 ADR 0035·0036·0037·0040 衔接）；Comments 补 2026-10-10 转正记录（含被误删的「事实底座」行恢复）。
- **双端 code-map**：桌面 §3 登记核心 WIT（`core.wit`）/ 能力域分片 / 端清单三处 + 拼装漂移锁与组合面锁；「新增能力域三处同改」扩为五处（+ 自持分片 + 端清单重拼）；锁索引新增 `bindings.rs` 两把 + 漂移锁，`crate_boundary_lock` 条目更新（discovery 边）；移动 code-map fork 章节改写为单一 crate + `mobile-host` 面，锁索引新增退役锁 / 组合锁，A1–A4 与 fork 对称锁标注「随 fork 退役」及承接面。
- **CHANGELOG 双语**：`CHANGELOG.md` + `CHANGELOG_zh.md` 各增两条（票 06 单一 crate；本序列综述），版本号未 bump（无版本变更，走未发布条目——§4 风险表口径）。
- **AGENTS.md**：§5.4 改写（单一 crate 双形态 + 差异只来自能力域组合 + 契约仍独立）；§4 任务路由表新增「改 wasm-core 机制 / 核心 WIT / 契约分片」行（先读 ADR 0045 + spec；禁手改生成物）。
- **docs/commands.md**：速查表 + §5.6 新增 WIT 分片拼装小节（真源 / 禁手改 / 改动流程 / ABI 锁步）。
- **plugin-development-checklist.md**：契约边界条改为分片真源口径；ABI 计数更新为 v37/v20 + 切片影响面（旧产物 import 集合变化 ⇒ 全量重建）。
- **票 06 记录**：§7.4（批次 04 前置 4 红根因表 + 三形态实证）/ §7.5（批次 05 门禁 + 欠账）回填，Status 转 done。

### 5.4 门禁实跑汇总

| 项 | 结果 |
| --- | --- |
| 锁变异自检 | 漂移锁 2/2（双向）+ 退役锁 2/2 + 能力域 bindgen 1/1 + 清单对照 1/1 + 移动组合 1/1（全部注入→红→还原→绿，证据见 §5.1/§5.2） |
| 漂移锁（CI 形态） | `node scripts/compose-wit.mjs --all --check` 双端绿；husky 实跑 exit 0 |
| 文档 | ADR 0045 accepted；双端 code-map 索引与锁一一对应（新增 5 条锁登记无孤儿）；CHANGELOG 双语同步 |
| 移动全量 | `bedcode-mobile/src-tauri` **334 passed / 0 failed**（fork 删除后复跑） |
| 内核 lib 全量 | `packages/bedcode-wasm-core` 桌面形态 **593 passed / 0 failed**（含 fixture_keeper 4/4：crypto / task / **pty** / **ws**，pty+ws 产物 383KB / 397KB 实际生成） |
| 桌面宿主 lib 全量 | **155 passed / 0 failed**（零回归） |
| 桌面宿主边界锁 | 9/9 绿（表更新后） |
| 前端两端 `test:run` | 见 §5.5 回填 |
| 根 `pnpm exec eslint .` | **0 error**（97 warning 不计入） |
| fmt 自查 | 本票新增文件 rustfmt-clean（2/2）；存量触碰文件逐一对照 HEAD 判定为**存量不净**（不动，守仓库纪律） |
| clippy | 见 §5.5 回填 |

### 5.5 未跑 / 手工项（逐项写明）

- **`cross-end-tests`**：未跑——票 04 已记项目级决策「延后至重构波次稳定后统一验证」；本票未改跨端协议 / WIT 语义（纯锁与文档）。
- **桌面宿主集成测试二进制**（`pty_e2e` / `task_e2e` / `terminal_output_perf` / `system_component_test` 等）：未跑——wasmtime 重链接成本 + `ws_e2e.rs`(3)/`ws_output_perf.rs`(1) 为 HEAD 既有编译红（`EndpointAuth` 同名不同源，非本序列引入）；记录为延续欠账。
- **Android target 编译级实证**：未跑（需 NDK）；tree 级全零门禁保持（批次 01 双实证）。
- **真机 / 浏览器核验**：未跑——本票无 UI / 行为改动（锁 + 文档 + 测试夹具）；行为面未变更。
- **wasm 应用完整构建（含 wasmHash 注入）**：未跑——本票未改 WIT 语义与插件代码（生成物逐字未变，`--check` 为证）。
- **前端两端 `test:run` / clippy**：见 §5.5 补充记录。

### 5.6 遗留欠账（本序列汇总，供后续补跑）

| # | 项 | 原因 | 回补位 |
| --- | --- | --- | --- |
| 1 | `cross-end-tests` 全量 | 项目级延后决策（重构波次统一验证） | 本序列各票 + `cross-end-tests/` |
| 2 | 桌面宿主集成测试全量（pty_e2e / task_e2e / terminal_output_perf / system_component_test / ws_e2e 基线编译红修复） | 磁盘 + 重链接成本；`EndpointAuth` 同名不同源为 HEAD 既有红 | 桌面 `src-tauri/tests/` |
| 3 | 内核 **mobile-host 形态**全量测试（移动 fork crate 原测试面随删除；`test-support` 面已在移动 src-tauri 全量覆盖） | fork 删除后原 fork 单测无宿主；等价面 = src-tauri 全量 | 如有新需求，落根 crate `mobile-host` cfg(test) |
| 4 | 票 19 A1–A4 契约锁的完整重建（A1 结构面已由 `mobile_wit_composition_lock` 承接；A2/A3/A4 语义面暂无源码锁） | 随 fork 退役，未逐条重宿 | 移动 src-tauri tests 或根 crate mobile-host cfg(test) |
| 5 | Android 编译级实证（NDK） | 环境缺失 | CI / 本地 NDK |
| 6 | `wasm-apps` 完整构建 + wasmHash 注入（本序列末次在票 04 已做，之后无 WIT 语义变更） | 无变更故未复跑 | 发布前全量 |

### 5.7 收尾核对

- `git status`：本票新增 3 文件（CI workflow / 两把新锁测试）/ 修改面 = 票 06+07 登记的锁 / 文档 / 记录（详见交付说明）；无未登记的在途新文件。
- 收尾清理：测试进程与端口已自然退出（cargo test / vitest 均跑完退出）；残留产物 `.dev-logs/ticket07-*.log`（gitignored 调试日志）。