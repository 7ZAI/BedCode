# 票 07 · 防回接 / 漂移锁 + 文档收口（終态确定性）

Status: **todo**（spec §4 票 07；本 spec 序列的收口票）
依赖：票 03–06 全部落地（锁的锚点文件才存在）
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

（待执行后回填：每条锁的变异自检记录；ADR 0045 定稿 diff；code-map 索引核对结果；eslint / cargo 门禁实跑输出；遗留欠账汇总表。）