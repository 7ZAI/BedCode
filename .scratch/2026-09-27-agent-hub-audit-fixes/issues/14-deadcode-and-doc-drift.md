# 14: 死代码、硬编码色板与文档漂移清理

**What to build:** 清掉 spec §3 的 P3 全部条目，让代码与文档双向对齐。(a) 删 `components/TabPlaceholder.vue`（零引用）+ `hub.placeholder.*` 6 个 i18n key（schema + zh-CN + en）；(b) 处理 5 个「模板在用、CSS 未定义」类名：`.ah-overview` / `.ah-cli-card` / `.ah-pv-apply` / `.ah-lg-col-agent|title|project` / `.ah-pv-keymode`——要么补规则要么从模板摘掉；(c) `AgentIcon.vue:33-40` 六色渐变硬编码（抄宿主 `LetterAvatar.vue` 同款债，违反 frontend-styles「不得引入第三方 hex 色板」）——按 spec §6.3 裁决执行；(d) 修 `styles.css:4-8` 头注释与 `tailwind.config.js` 的矛盾（注释称插件 SFC 的 Tailwind 类不生成样式，实际已纳入 content 且有护栏测试）；(e) 消除 `100vh` 魔数（`.ah-sk-editor-body` / `.ah-lg-events` / `.ah-lg-raw-card` 的 `calc(100vh - 260px/320px)` 与宿主 chrome 高度静默耦合）；(f) `ProvidersTab` 弹窗 `@keydown.esc` 挂无 `tabindex` 的 div 上、焦点不在子元素时 Esc 无效、无 focus trap；(g) `useUsage.autoScanDone` 在 `scan()` resolve 前置位、失败后本次会话不再自动重试；(h) `usage.rs:903` LIKE 关键词未转义 `%` / `_`；(i) 两处 eslint warning（`SkillsTab.vue:60` `good`、`vite.config.ts:7` `pluginId`）归零。

**Blocked by:** None

**Status:** resolved

**已裁决（spec §6.3）**：`AgentIcon.vue` 的六色渐变**只在本插件内**换成中性派生（`--bg-hover` + `--text-primary`），**不动宿主 `LetterAvatar.vue`**。理由：AGENTS §0 最小改动——宿主那份是既有债，改它会把范围从 wasm 应用扩到宿主组件并连带其它引用方，超出本 spec 边界。宿主那笔债另立条目处理。

- [x] `grep -rn TabPlaceholder src/` 零命中；`hub.placeholder` 在 schema/zh-CN/en 三处均零命中
- [x] 模板中每个 `class="ah-*"` 都能在 `styles.css` 找到规则（可写脚本核验并入 eslint 或单测）
- [x] `styles.css` 与 `AgentIcon.vue` 无写死 hex/rgba 主题色（品牌标固定色与数据色 token 化，例外需在注释写明理由）
- [x] 宿主 TitleBar 高度变化（如 `h-10` → `h-12`）时，三处编辑/聊天区域不塌陷也不溢出（改用 flex 链，去掉 `100vh`）
- [x] 弹窗内任意焦点位置按 Esc 均可关闭；打开时焦点进入面板、关闭后回到触发按钮
- [x] 扫描失败后本次会话可再次自动/手动触发（`autoScanDone` 语义明确）
- [x] 关键词输入 `%` / `_` 不再匹配全表
- [x] `pnpm exec eslint bedcode-desktop/wasm-apps/agent-hub` **0 error 且 0 warning**
- [x] 母 spec 与本 spec §3 的 P3 清单逐条标注「已修 / 豁免（理由）」

## Answer（2026-09-27 实施完毕，P3 逐条）

| # | 项 | 处置 |
| --- | --- | --- |
| P3-1 | `TabPlaceholder.vue` 零引用 + `hub.placeholder.*` 18 个键 | **已修**：文件删除；schema / zh-CN / en 三处各删 6 键；`.ah-placeholder*` 三块 CSS 一并删 |
| P3-2 | 5 个「模板在用、CSS 未定义」类名 | **已修（摘标记而非补空规则）**：`.ah-overview` / `.ah-cli-card` / `.ah-pv-apply` / `.ah-lg-col-agent\|title\|project` 全是纯标记（承载样式的是同元素上的 `.ah-card` / `.ah-lg-cell-*` / grid 显式列），从模板摘除。`.ah-pv-keymode` 本身有规则（`margin-bottom: 0`），保留 |
| P3-3 | `AgentIcon.vue` 六色 hex 渐变 | **已修（按 spec §6.3 裁决）**：改为 `--ah-avatar-0..5`（`--bg-hover` 按 6% 步进朝 `--text-primary` 加深的中性派生），前景由 `#fff` 改 `--text-primary`（≥12:1）。同一名字恒得同一档（FNV-1a 取模，逻辑不变）。宿主 `LetterAvatar.vue` 未动 |
| P3-3 | `styles.css:356,360` 品牌 chip 底色 `rgba(...)` | **保留并写明理由**：品牌识别色（非语义/非数据），只出现在 34px 图标底上、不承载文字，对比度门禁不适用；集中在 `.ah-cli-ic.brand-*` 两行便于维护。`S7` 用白名单显式登记 |
| P3-4 | 头注释与 `tailwind.config.js` 矛盾 | **已修**：改为说明「tailwind content 已显式纳入 `./wasm-apps/agent-hub/src/**`（并有 `tailwindContentCoverage.test.ts` 护栏），但独立 CSS 不被宿主加载故仍需 `?inline` 注入」 |
| P3-5 | 3 处 `calc(100vh - …)` 魔数 | **已修**：`.ah-sk-editor-body` / `.ah-lg-events` / `.ah-lg-raw-card` 全部改 `flex: 1 1 0` 纯 flex 链。唯一保留的 `100vh` 是 `.ah-modal-panel` 的 `max-height`（它 Teleport 到 body，不在 flex 链内）——`S8` 断言「除它外 ≤1 处」 |
| P3-6 | 弹窗 Esc 挂无 tabindex 的 div + 无 focus trap | **已修**：Esc 改 document 级监听（任意焦点位置可关）、打开时焦点进面板、Tab/Shift+Tab 循环、关闭后焦点回触发按钮、`autofocus` 属性移除（改由逻辑接管） |
| P3-7 | `autoScanDone` 在 `scan()` resolve 前置位 | **已修**：标志改由 `scan()` 独占持有 —— 进入即置位、**发起失败回滚**，语义变为「已成功发起过」。于是面板重开 / `refresh()` 拿到 idle 时能重试；扫描成功时锁保持，防事件风暴 |
| P2-5 | LIKE 关键词未转义 `%` / `_` | **已修**：`escape_like_pattern()` + SQL `ESCAPE '\'`，2 个 Rust 用例 |
| P3-8 | 2 处 eslint warning | **已修**：`SkillsTab` 的 `good` 变量、`vite.config.ts` 的 `pluginId` 均删。agent-hub 现 **0 error 0 warning** |

**额外挖出并修掉的真 bug（本轮不在 spec 清单内）**：
`SkillsTab` 的「覆盖导入」确认条点了等于没点 —— `importLocal(force)` 把 `force` 只当分支选择器，
正常路径仍发 `{}` 给 guest，点「覆盖」会再问一次 `exists`，确认条永远消不掉。
已改为「有 `importPending` 即视为覆盖，携 `{ path, force: true }` 重入」，并有单测钉住。

**护栏**：`styleGuards.test.ts` S5（模板类名 ↔ styles.css 双向一致，含死规则反向检查 +
动态类名前缀检查）、S7（写死色值白名单）、S8（100vh）、S9（死代码/输入框规格）共 85 用例。

**验证**：`pnpm exec vitest run wasm-apps/agent-hub` 172/172 绿；`cargo test` 100/100 绿；
tsc 0 error；agent-hub eslint 0 error 0 warning；根 `pnpm exec eslint .` 0 error（118 warning 为全仓既有）。

**未覆盖风险**：本轮**未做**真机截图核验（P2-7 的视觉项、`S7` 白名单是否过宽等）。
原因是 08:43 有并发进程对工作区执行 stash 类操作，本轮为防再次丢失提前收尾，未启动
dev-shell + LingLong Chrome。补做方式见 spec §7 的三条命令。
