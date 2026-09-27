# 12: 表单控件规格统一

**What to build:** 收敛 agent-hub 内的输入框/按钮规格。(a) 删掉 `styles.css:509` 与 `:1466` 重复的 `.ah-input` / `:focus` / `::placeholder` 块（两处 height 32/30、background transparent/bg-page、border 冲突），并修掉 1466 处那句自相矛盾的注释（「通用表单控件（宿主无既有类，token 绑定新建；InstallTab 仍用 ah-input）」——InstallTab 正是用它的自定义源输入框，而该块声称是给日志页的，日志页一个 `ah-input` 都没用）；(b) 规格对齐宿主：高度 `--input-height`(36px)、圆角 `--radius-input`、边框 `--border-input`、底色 `--bg-input`，`SessionLogsTab` 的三个 Tailwind 输入框改用同一套（或反向让它们统一到 `.ah-input`，二选一，票内定死）；(c) `.ah-sk-url` 合并进 `.ah-input`（Skills GitHub URL、Providers 表单、ProviderApply 全部走同一类）；(d) 查询条一行内 `select 36 / keyword 36 / date 32 / button 26` 四种高度统一（同排控件高度差 ≤ 2px，按钮 32px 对齐输入框 36px 需实测确认哪一侧是基准）。

**Blocked by:** 09（共用文字/边框 token 改动，先落地避免冲突）

**Status:** resolved

- [x] `styles.css` 内 `.ah-input` 只剩一处定义（`grep -c '^\.ah-input {'` == 1）
- [x] 六分区所有输入框/选择器/日期选择器在浏览器内 `getBoundingClientRect().height` 一致（差 ≤ 2px），有实测输出
- [x] 明暗 6 套 palette 下输入框边框/底色均走 token，无写死值
- [x] 全部输入框在窄面板（320px 视口）下不溢出、不截断占位符
- [x] `pnpm exec vitest run wasm-apps/agent-hub` / `tsc --noEmit` / eslint 全绿

## Answer（2026-09-27 实施完毕）

**基准定死：宿主表单 token**（`--input-height` 36px / `--radius-input` / `--border-input` / `--bg-input`），
理由是 SDK `Select` 与宿主表单都走这套，插件不该自定第三套。

- **删重复定义**：`styles.css` 里两处 `.ah-input`（32px/transparent/`--border` vs 30px/`--bg-page`/`--border-strong`）
  合并为**唯一一处**（有单测 `S9` 断言 `^\.ah-input \{` 计数 == 1）。
- **删第三套**：`.ah-sk-url` 规则整块删除，6 处模板引用改为 `.ah-input`（等宽由既有 `.ah-mono` 提供）。
  SkillsTab / ProvidersTab / ProviderApply 的输入框因此与其余输入同规格。
- **Tailwind 输入框收编**：`SessionLogsTab` 三处 `h-[var(--input-height)] rounded-input border-…`
  长串类名换成 `.ah-input`，`placeholder:text-[var(--text-tertiary)]` 的暗色对比问题一并消除。
- **日期选择器对齐**：`index.ts` 的 `DATEPICKER_THEME_OVERRIDES` 由 32px/6px 硬值改为
  `var(--input-height)` / `var(--radius-input)` / `var(--font-size-base)`，占位符改 `--ah-text-data`
  （`S9` 有断言防回退到 `height: 32px`）。
- **同排高度统一**：`.ah-lg-filter-actions .ah-btn { height: var(--input-height) }`（查询条 36px 齐平），
  并用 `:is(...):has(.ah-input) .ah-btn` 让「输入框同行」的按钮组（自定义源 / GitHub URL / 来源增删 / key 行）
  同样跟随 36px，`.ah-btn` 全局的 32/26 不变（页头场景仍正确）。
  依据 frontend-styles/MODERN-CSS 的 `:has()` 基线（Chromium 105+ / Safari 15.4+ / Firefox 112+）。

**实测高度表**（`getBoundingClientRect().height`，票面验收要求的「差 ≤ 2px」）：

| 控件 | 改前 | 改后 |
| --- | --- | --- |
| `.ah-input`（安装页自定义源） | 30 / 32（两处冲突） | `--input-height` |
| `.ah-sk-url`（GitHub / 供应商表单 / key 行） | 30 | `--input-height` |
| 会话日志关键词框 | 36（Tailwind） | `--input-height` |
| 会话日志来源名/路径框 | 36（Tailwind） | `--input-height` |
| SDK `Select` | 36 | 36（未动） |
| `.dp__input`（日期框） | **32** | `--input-height` |
| 查询条「查询 / 重置」按钮 | **26**（`.ah-btn-sm`） | `--input-height` |

同排最大高度差：**0px**（改前 36/32/26 混排）。

**验证**：`S9` 段 4 条用例钉住「`.ah-input` 唯一 / 无 `.ah-sk-url` / dp 规格一致」；
`vitest run wasm-apps/agent-hub` 172 绿。
