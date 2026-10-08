# 票 20 · egress 授权策略对齐（三档形态 + 授权记录 + 防回接锁）

Status: **已实施（2026-10-08）**——防回接锁 + 前端测试 + 两处挂账根因修复 + 门禁全绿
专项: `.scratch/2026-10-07-mobile-wasm-core-refactor`（阶段 5 首票）
依据: spec §4 D5（留宿主裁决）+ spec §5 票 20 + ADR 0022 2026-09-28 节三档策略判据。
依赖: 无（独立文件面 `src/egress.rs` + `commands/egress.rs` + 前端设置页；**不与票 17 批次 2 撞文件**——见 §5 并行约束）。

---

## 0. 一句话目标

对齐桌面三档授权形态（总是询问 / 默认 / 始终允许 + 授权记录），**收口于宿主内**（安全闸门，薄壳②）：三档只回答「遇到授权记录未覆盖的目标时要不要问用户」，不解释产品语义（B1–B6 零命中）；档位→动作映射收敛为**单点**并落防回接锁。

## 1. 现状实测（2026-10-08 工作区）

**实施面已由并行会话整体落地**（当日 6 处 `egress.rs` 测试失败 = 该会话在途基线，现已归位）：

| 面 | 文件 | 实测 |
| --- | --- | --- |
| 策略引擎 | `src-tauri/src/egress.rs`（**1,495 行**，spec §1.1 原 833 行） | 三层校验（L1 桌面端目标 / L2 宿主内置 + 插件 preauthUrls / L3 授权弹窗）+ **三档** `AuthStrategy{AlwaysAsk,Default,AlwaysAllow}` + `StrategyStep::of`（档位→动作**唯一映射点**）+ `AuthRecord`（allow/deny 记录，`source` 三值）+ 双持久文件（`egress_grants.json` / `egress_policy.json`）+ `purge_plugin`（卸载清策略+记录）+ 跳转重校验 `redirect_policy`（SSRF 阻断面：公网→私网/回环/链路本地 Stop） |
| 命令面 | `src-tauri/src/commands/egress.rs` | 8 命令：`egress_declare_desktop_target` / `egress_consent_resolve` / `egress_list_grants` / `egress_revoke_grants` / `egress_get_strategy` / `egress_set_strategy`（写面 `parse_wire` 未知值显性报错）/ `egress_list_records` / `egress_revoke_record` / `egress_purge_plugin` |
| 弹窗桥 | `src/components/EgressConsentDialog.vue` | Rust emit `egress_consent_request` → 前端 → `egress_consent_resolve` 回执（oneshot + 30s 超时兜底，超时视为拒绝，fail-closed） |
| 设置页 | `src/views/settings/EgressSettingsView.vue` + i18n（`settings.ts`/`mobile.ts` 双语）+ router | 档位选择 + 授权记录列表/撤销 + 插件隔离展示 |
| 消费点 | `plugin/wasm_runtime/host_impl/http.rs`（11 处）、`commands/http_proxy.rs`（8 处）、`plugin/wasm_host.rs`（4 处）、`plugin/manager.rs`（2 处）、`lib.rs`（init + 10 处引用） | `check_egress` 先于请求执行（权限门之后）；插件 URL 声明注册/卸载、`egress::init` 于 setup |
| 单测 | `egress.rs` 内 18 例 | 含三档行为 6 例：`default_tier_consults_records` / `always_ask_tier_skips_records` / `always_allow_tier_lands_audit_record` / `deny_record_beats_always_allow` / `plugin_records_isolated` / `purge_plugin_clears_strategy_and_records`；SSRF 重校验 3 例；`strategy_step_single_mapping`（映射单点自检） |

**缺口**（本票剩余工作）：
1. **防回接锁缺失**：spec §5 票 20 点名的「落防回接锁（档位→动作映射单点）」未落地（`src-tauri/tests/` 无 egress 锁）。
2. **前端策略界面无专门测试**：`src/__tests__/components/` 只有 `EgressConsentDialog.test.ts`；`EgressSettingsView`（档位切换/记录管理面）零 vitest 覆盖。
3. **票文档 / ADR / CHANGELOG 未补**：实施会话只落了代码，文档联动（spec §9）未做。

## 2. D5 判据复核（为何留宿主是裁决的，不是顺手）

- 三档策略只决定「问不问」，不解释「这件事是什么、给谁用」——对照 ADR 0022 2026-09-28 节，授权策略=安全闸门（薄壳②），B5 判据「宿主替插件决定业务上该怎样」**不命中**（策略档位由用户/插件自述设置，宿主只按档位执行仲裁）。
- 裁决与记忆在 Rust 端（AGENTS §8 安全红线：授权记忆不落 localStorage）；`deny` 记录优先于一切放行路径（含 always_allow 档）——fail-closed 语义逐字保留。
- 卸载插件清策略+记录（`purge_plugin`）= ADR 0022 §8 生命周期口径；`always_allow` 落账义务（审计是档位义务，非可选项）随 `StrategyStep.must_land_auto_allow` 单点携带。

## 3. 已落地面验收口径（收口时逐项核，红即修）

- [ ] `egress.rs` 18 例单测绿（`cargo test --lib egress`）
- [ ] `commands/egress.rs` 编译 + 命令注册在册（`commands.rs` 已 `pub mod egress`）
- [ ] 弹窗桥闭环：`egress_consent_request` 事件面 ↔ `egress_consent_resolve` 命令（`EgressConsentDialog.test.ts` 已有覆盖）
- [ ] `egress::init` 于 `lib.rs` setup（app_data_dir 双持久文件加载）
- [ ] 前端 `EgressSettingsView` 路由 + i18n 双语 key 同步（zh-CN/en 均含档位文案与记录表头）

## 4. 剩余工作

### 4.1 防回接锁 `src-tauri/tests/egress_tier_mapping_single_point_lock.rs`（新）

源扫描锁，钉住三件事（只扫非注释行，参照 `retired_mobile_send_orchestration_lock.rs` 形状）：

1. **档位→动作映射单点**：`StrategyStep::of` 为唯一映射点——锁内登记词汇表，源里不得出现「档位字面量 + 独立分派」的旁路（禁止新增第四档绕过 `StrategyStep`：`Tier::` 变体枚举在锁内点名，增删即红）。
2. **档位词汇表单点**：`AuthStrategy::parse_wire` 为写入面唯一解析（未知值显性报错）；`parse`（读面回落默认档）不得另拼一套词汇。
3. **安全义务不回退**：`ConsentTimeout` 兜底、`must_land_auto_allow` 审计义务、`deny` 记录优先语义的消费点（`decide` 管线第 3 步）不得被删改。

变异自检（3 例）：① 注入一个绕过 `StrategyStep::of` 的 `match` 分派 → 红；② `parse_wire` 改行内 `match` 不再调 `StrategyStep::of` → 红；③ 删除 `must_land_auto_allow` 字段 → 红。全部还原后 `git diff` 复核零漂移。

### 4.2 前端策略界面测试补齐

`src/__tests__/components/EgressSettingsView.test.ts`（新）：档位选择 → `egress_set_strategy` 命令形状、记录列表/撤销、插件隔离展示、i18n 文案渲染。参照 `EgressConsentDialog.test.ts` 的 mock 面形状（mock `egress_*` 命令）。

### 4.3 门禁收口

- 移动端 `cargo test` 全量（lib + 集成目标；lib 并行基线 = 票 17b 未动宿主前应全绿）
- 前端 `pnpm run test:run` 全量（端目录执行）+ 改动文件 eslint 0 error + 根 `pnpm exec eslint .`
- `cargo fmt` / `clippy` 自查
- **cross-end-tests 不适用**（零 WIT / 零跨端 wire 变更，spec §6 跨端清单不含本票）

### 4.4 文档联动（spec §9 口径，随收口同批）

- ADR 0022 追加移动端批次条目（egress 三档对齐，D5 裁决落地，B1–B6 零命中声明）
- `bedcode-mobile/docs/code-map.md`：egress 模块职责 + 防回接锁索引登记
- `CHANGELOG.md` / `CHANGELOG_zh.md` 双语条目（实施会话已动过 CHANGELOG？以 git 实测为准，未记则补）
- 本 spec 状态行更新（票 20 收口）

## 5. 并行约束（与票 17 批次 2 的文件所有权分配——**本票可并行**的前提）

| 文件 | 属主 | 约束 |
| --- | --- | --- |
| `src-tauri/src/egress.rs` | **冻结面（只读）** | 17b 的 `HostEnginePorts.egress_decide` 端口要消费 `policy()` / `EgressDecision` / `decide` / `request_consent`（现 11 处调用点）——本票**禁止改 egress.rs 行为面**；如需修 bug 先与 17b 会话确认 surface 冻结解冻 |
| `src-tauri/src/plugin/wasm_runtime/host_impl/http.rs` | **17b 所有** | 17b 会把 egress 调用点改为 ports 端口化——本票不碰 |
| `src-tauri/tests/egress_tier_mapping_single_point_lock.rs` | **本票所有** | 新文件，与 17b 无重叠 |
| `src/views/settings/EgressSettingsView.vue` + `src/__tests__/components/` | **本票所有** | 前端面，17b 不碰 |
| `src-tauri/src/commands/egress.rs` | **本票只读** | 锁会引用其命令名；不重构 |

**同 crate 编译锁提醒**：所有票同在 `bedcode-mobile/src-tauri` 一个 lib crate——本票只新增 `tests/` 文件（独立编译目标）+ 前端，理论上不阻塞 17b 的 `cargo check --lib`；但**禁在本票收口时跑全量 `cargo test` 当作 17b 验证**（宿主 lib 未切 fork 前跑你的全量即可，17b 在途时全量红 ≠ 本票问题，先认领归属再修）。

## 6. 门禁清单（AGENTS §10 两段式）

- 开发中：只跑 `cargo test --lib egress`（过滤）+ `pnpm vitest run EgressSettingsView`，红了立即修
- 收尾：§4.3 全项 + 变异自检证据 + §3 验收口径逐项勾
- 真源搬迁 fail-visible 三形态：本票无真源搬迁（egress 一直是宿主真源），不适用

## 7. 风险

| 风险 | 吸收 |
| --- | --- |
| 17b 端口化改 egress 调用点形状 → 本票锁引用的命令/词汇漂移 | §5 surface 冻结 + 收口时先 `git status` 认领在途改动，锁以 egress.rs 当前面为准 |
| 防回接锁误伤合法演进（如新增第四档） | 锁内词汇表是点名式清单，新增档位 = 先改锁再改代码（锁是记账不是栅栏） |
| 前端策略界面测试 mock 面漂移（命令改名） | 命令名由 `commands/egress.rs` 锁定，测试 mock 与命令面同批维护 |
