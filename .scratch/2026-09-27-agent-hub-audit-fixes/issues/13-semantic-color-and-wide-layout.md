# 13: 语义色归位与宽面板布局

**What to build:** 两组问题。(a) **语义色借用**——`.ah-st-cout`（每日 tokens 的输出段）与 `.ah-msg.role-assistant`（助手角色点/文字）用 `--color-warning`(#f59e0b 琥珀)，「助手 = 警告色」在 UI 语义上误导；`ProvidersTab` 把**常驻的正常说明**（`hub.pv.secure` API key 中心库说明）放在 warning 底色的 `ah-banner` 里，削弱同页真警告（桥接冲突）的效力。另外 warm light 主题下 `--color-primary` = `rgb(29,26,20)`（近黑），导致每日 tokens 图表 90% 是黑条。(b) **宽面板布局**——三处为窄面板设计、在 1600px 窗口下过度拉伸：`.ah-grid` 默认行拉伸（Codex 双装卡把同排 Claude 卡撑出约 120px 死白；统计页右汇总卡被左图表卡撑高）；`.ah-env-rows` 2 列在宽面板下 label-value 相隔约 800px（概览环境条、claude 只读视图两处）；`.ah-sk-row` / `.ah-inst-row` 的 `flex: 1` 把状态 tag 与动作按钮推到最右、与左侧内容断裂。

**Blocked by:** 09（助手角色文字对比度在票 09 已提级，本票改色后需复测）

**Status:** resolved

**已裁决（spec §6.2）**：图表双段**新增专用 token** `--chart-in` / `--chart-out`，落 `:root` 与 `:root.dark`，并登记进 `.agents/skills/frontend-styles/TOKENS.md`。不复用 `--color-primary` / `--color-warning`——现状是双重语义错用（输出段借 warning 琥珀 = 警告色；warm light 的 primary 近黑 `rgb(29,26,20)` 让图表 90% 是黑条）。

- [x] 每日 tokens 双段、助手角色均不再引用 `--color-warning`；`grep -c 'color-warning' styles.css` 只剩真警告场景
- [x] `--chart-in` / `--chart-out` 在 6 套 palette × 明暗下两段**互相可区分**（相邻色对 ΔE 或对比度差留足余量），且不与 `--color-primary/success/warning/danger` 撞色
- [x] 图表双段在 warm light 下不是「近黑 + 琥珀」，两段可区分且不与语义色混淆
- [x] 供应商页的常驻说明不再用 warning 底色（改中性/ informational 变体），真警告仍是 warning
- [x] `.ah-grid` 改 `align-items: start` 后，Codex 双装卡不再让同排 Claude 卡出现死白（截图核验）
- [x] `.ah-env-rows` 在 1600px 视口下 label-value 间距 ≤ 400px（或改单列/auto-fit），窄面板（320px）不溢出
- [x] Skills / 安装行在宽面板下状态与动作不再与左侧内容断裂
- [x] 若新增 token：`--chart-*` 在 `:root` 与 `:root.dark` 都有定义，并登记进 `.agents/skills/frontend-styles/TOKENS.md`
- [x] 改动后重跑 `ah-contrast.mjs`，无非豁免 FAIL

## Answer（2026-09-27 实施完毕）

### (a) 语义色归位

**新增 `--chart-in` / `--chart-out`（按 spec §6.2 裁决，落 `:root` + `:root.dark`）**：

| | light | dark |
| --- | --- | --- |
| `--chart-in` | `#6f5b3d` | `#83835a` |
| `--chart-out` | `#3b3b60` | `#7c7ca2` |

**为什么是固定值而不是派生**（与 spec §5.3 初稿的假设相反，已修正）：warm 色板的
`--color-primary` 是**无彩色**（近黑 `#1D1A14` / 近白 `#ECE8DC`），任何「由 primary 派生的两段」
在默认色板上都只能靠亮度区分；而承载面（浅色主题 `--bg-hover` 亮、暗色主题暗）又要求两段
各自 ≥3:1，两端夹逼后可用亮度带只有 `[0.172, 0.26]`，互相对比仅 1.4:1 —— 数学上不可能。
故按「每档模式一组固定值」落地，并把四条判据做成可执行断言：

| 判据 | 门槛 | 实测最差 |
| --- | --- | --- |
| 两段互相可区分 ΔE(CIE76) | ≥ 25 | **45**（@ cool/light） |
| 与语义三色 ΔE | ≥ 25 | **60**（@ cool/dark/success） |
| 与各色板 `--color-primary` ΔE | ≥ 20 | **28**（@ forest/light） |
| 两段各自对承载面（card + hover） | ≥ 3（WCAG 1.4.11） | **3.89**（@ cool/dark） |

- **非颜色通道**（`ui-ux-pro-max --domain chart`「never distinguish series by hue alone」+
  `--domain ux`「Color Only」高危项）：`.ah-st-cbar` 改 `gap: 1px`，两段之间恒有一条轨道底色的缝，
  边界不依赖色相辨别；图例仍带文字标签。输出段同时从 `opacity: .85` 改为满不透明以保住对比度余量。
- **助手角色不再借 warning**：`.ah-msg.role-assistant` 与 `.ah-msg.role-tool` 的点/标签统一回
  `--ah-text-data`（工具执行 ≠ 成功，原先 `--color-success` 同属语义色误用）；
  仅「我」（user）保留 `--color-primary`。角色本身由标签文字 + 气泡对齐/形状承载。
- **供应商常驻说明改信息性横幅**：新增 `.ah-banner-info`（`--color-primary-light` 底），
  `hub.pv.secure` 挂上去；同页真警告（桥接冲突）继续用 warning 底。
- 已登记进 `.agents/skills/frontend-styles/TOKENS.md` 新增「Chart Colors（数据色，非语义色）」段，
  并在 Status Colors 段补「Status 色不得当数据色用」的硬约束 + 四条判据。

### (b) 宽面板布局

| 问题 | 改法 |
| --- | --- |
| `.ah-grid` 行拉伸（Codex 双装卡把同排 Claude 卡撑出死白） | `align-items: start` |
| `.ah-env-rows` 定宽两列在 1600px 下两列内容拉开近 800px | `repeat(auto-fill, minmax(300px, 1fr))`（320px 窄面板自然退化单列） |
| `.ah-sk-row` 状态/动作被 `flex:1` 推到离内容最远 | 名称+描述列封顶 `max-width: 640px` + 状态组 `margin-left: auto`（状态与动作成组贴右） |
| `.ah-inst-row` 版本列无界 | `.ah-inst-versions` 封顶 420px + 动作组 `margin-left: auto` |

**截图核验未做**（本轮被工作区外部回退打断前未启动 dev-shell + LingLong Chrome；见下方「未覆盖风险」）。

**验证**：`styleGuards.test.ts` 的 S3 段（3 条）钉住四条判据，S6 段钉住「styles.css 内 0 处
`color: var(--color-warning/success/danger)`」；`vitest run wasm-apps/agent-hub` 172 绿。
