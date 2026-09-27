# 09: 语义徽章与辅助文字可读性

**What to build:** 修 `wasm-apps/agent-hub` 在浅色主题下大面积低于 WCAG AA 的文字对比度。改两件事：(a) 语义徽章（`.ah-cli-tag.ok/warn/err` + 中性）不再用语义色上文字，**色彩只留在 `.ah-cli-dot`**；(b) 引入数据级 token `--ah-text-data`（`color-mix(in srgb, var(--text-secondary) 55%, var(--text-primary))`，验算最差 **5.84:1**），把当前用 `--text-tertiary`（实测 2.42 light / 2.96 dark）承载的数据（表头 / 列时间 / 列 tokens / 分页信息 / 会话源路径 / 总数标签 / 每日条数值 / 会话行 meta / 技能目录 / 降级提示 / 事件 meta / token 明细）全部提级。不改宿主任何 token 值。详见 spec §5.1 / §5.2。

**注意**：原方案「数据用 `--text-secondary`」实测**不成立**（4.46:1，差 0.04 未达 AA，6 种承载面最差 3.60），已改为 `color-mix` 派生。

**Blocked by:** None

**Status:** resolved

- [x] 六分区 × 明暗 × 6 套 palette 实测：非豁免项全部 ≥ 4.5:1（≥18px 或 14px+bold ≥ 3.0），`evidence/ah-contrast.mjs` 输出 LIGHT/DARK 两栏 FAIL 归零
- [x] 引入 `--ah-text-data`（`color-mix(in srgb, var(--text-secondary) 55%, var(--text-primary))`）作为数据级，12 套主题 × 6 种承载面最差 ≥ 4.5（验算值 5.84）
- [x] 语义色仍可区分三态（色彩只落在圆点上，状态不依赖颜色单独承载），文字回 `--text-primary` / `--ah-text-data`
- [x] 工具事件正文（实测 3.91）提到 `--ah-text-data`；助手角色文字（实测 2.09）随票 13 一并改色后达标
- [x] 新增对比度矩阵单测（组合表由解析 `styles.css` 自动枚举，不手写常量）
- [x] **显式豁免清单**落档：装饰级 `--text-tertiary`（图例 / 空态整句 / 占位符 / 副标题）实测 2.42(light)/2.96(dark) 无解达标（与 secondary 任意比例 `color-mix` 最差仅 3.06），豁免理由逐条记录，不留「静默不过」
- [x] `cargo test` / `pnpm exec vitest run wasm-apps/agent-hub` / `tsc --noEmit` / 根 `pnpm exec eslint` 全绿

## Answer（2026-09-27 实施完毕）

**改法**（`src/styles.css`）：新增 `:root` 插件派生 token 段，全部由宿主 token 现场 `color-mix` 而来，
**未改宿主任何 token 值**：

| token | 定义 | 12 套主题 × 9 承载面最差 |
| --- | --- | --- |
| `--ah-text-data` | `color-mix(in srgb, var(--text-secondary) 55%, var(--text-primary))` | **5.84:1** |
| `--ah-text-danger` | `danger 45% + text-primary` | 5.58:1 |
| `--ah-text-warning` | `warning 50% + text-primary` | 4.51:1 |
| `--ah-text-success` | `success 45% + text-primary` | 4.83:1 |
| `--ah-dot-success/-warning/-danger` | 语义色 68% + text-primary（色相不变，只压深） | 3.14~4.82:1（WCAG 1.4.11 门槛 3） |
| `--ah-bubble` | `primary 82% + text-primary` | 承载用户气泡，前景 `--ah-on-primary` = `primary-contrast` → 4.86:1 |

- **徽章**：`.ah-cli-tag.ok/.warn/.err` 文字一律回 `--text-primary`，色彩只落 `.ah-cli-dot`
  （语义色的**派生**版本，满足 1.4.11 的 3:1，而纯语义色只有 1.84~3.11）。中性徽章文字用 `--ah-text-data`。
- **数据文字提级**：表头 / 列时间·时长·tokens / 分页信息 / 会话源路径 / 总数标签 / 每日条数值 /
  会话行 meta / 技能目录 / 降级提示 / 事件 meta / 工具正文 / 原始 JSONL / 导航项 / 图例 全部 → `--ah-text-data`。
- **一并清零 `--text-secondary` 的文字用法**（本插件内 0 处），它只作为 `--ah-text-data` 的混合输入。

**审查未列、实测额外挖出并修掉的 4 处**（warm light 之外的主题全部未采样，靠 12 主题矩阵才暴露）：

| 位置 | 症状 | 处理 |
| --- | --- | --- |
| `.ah-msg.role-user .ah-msg-meta` | `--text-secondary` 在 primary 气泡上 **1.07:1**（cool/light） | 气泡底改 `--ah-bubble` + 前景用满强度对比色 |
| `.ah-inst-outdated` | warning 当文字 **2.07:1**（全部浅色色板） | `--ah-text-warning` |
| `.ah-cli-error` / `.ah-sk-raw-error` | danger 当文字 3.62:1 | `--ah-text-danger` |
| `.ah-sk-diff-line.add/.del` | success/danger 当文字 ~1.98:1 | `--ah-text-success` / `--ah-text-danger` |
| `.ah-tab` / `.ah-lg-tab` | secondary 在 sidebar 上 3.74:1 | `--ah-text-data` |
| `.ah-lg-badge-current` | primary 在 primary-light 上 3.42:1 | `--ah-text-data` |
| `.ah-btn-warn` | warning 文字 2.07:1 | `--ah-text-warning`（描边保留纯 warning） |

**显式豁免清单（2 项，理由已落进测试常量，非空且逐条可查）**：

| 选择器 | 理由 |
| --- | --- |
| `.ah-st-empty` | 空态整句是「无数据」的唯一线索，但不承载任何数值/时间/路径；抬到数据级会让空态比有数据的行更抢眼。`--text-tertiary` 是宿主全局 token，改它影响面超出本插件。与 secondary 任意比例 `color-mix` 最差仅 3.06，无解达标。 |
| `.ah-lg-col-go` | 「›」进入箭头是纯字形，行标题与 `title` 已说明可点击，读不出不影响任何信息获取。 |

> spec §5.2 初稿把「图例 / 占位符 / 副标题 / 字段标签」也列入装饰级豁免，**实施时未采纳**：
> 图例是图表的唯一解码线索、占位符与字段标签决定用户知道这一格在筛什么，
> 三者都承载信息，一律提到数据级。豁免面因此从 5 类收窄到 2 类。

**护栏**：`src/__tests__/styleGuards.test.ts` S1/S2 段（85 用例）。前景色**从 `styles.css` 实际解析**，
主题 token 从宿主 `style.css` 按真实层叠合成 12 套（`:root` → `:root.dark` → `[data-palette]` → `.dark[data-palette]`），
离线脚本 `.scratch/.../evidence/ah-matrix.mjs` 与测试共用同一份数学底座。
数学与浏览器实测交叉验证过：secondary-on-hover 3.60 / tertiary-on-card 2.42(light) 2.96(dark) 三项与 `ah-contrast.txt` 完全一致。

**变异自检**：把 `.ah-cli-tag.ok` 文字改回 `var(--color-success)` → S1 与 S6 两条同时变红 ✅

**验证**：`pnpm exec vitest run wasm-apps/agent-hub` 172/172 绿；`ah-matrix.mjs` exit 0（文字 59 项 FAIL 0 · 图形 7 项 FAIL 0）；tsc 0 error；agent-hub eslint 0 error 0 warning。
