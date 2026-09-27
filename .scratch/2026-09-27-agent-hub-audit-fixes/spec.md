# Agent Hub 实现审查修复 spec —— 可读性 / 状态正确性 / spec 漂移收敛

- 状态：全部七张票已实施完毕（09 / 10 / 12 / 13 / 14 / 15 于 2026-09-27 先行，11 票 07 补齐同日收口）
- 母 spec：`.scratch/2026-09-13-agent-hub/spec.md`（本 spec 是它的修复/收敛附属，不替代）
- 审查对象：`bedcode-desktop/wasm-apps/agent-hub/`（前端 5 431 行 / Rust 5 923 行 / 8 张票 6 张 resolved）
- 证据存档：本目录 `evidence/`（14 张六分区明暗截图 + 对比度实测原始输出 + 复现脚本）
- 术语：见根 `CONTEXT.md`「Agent Hub」域

## 1. 背景

> **审查基线：工作区（working tree），不是 HEAD。** 审查时 `wasm-apps/agent-hub/` 有 9 个文件带**未提交的在途改动**（票 04 错误信封残留清扫：CliCard / InstallTab / OverviewTab / ProviderApply / SkillEditor / SkillsTab / useUsage / en.ts / zh-CN.ts，共 +32/-23）。本 spec 全部行号引用以**工作区**为准；实施前若那些改动被提交或回退，先按 `git diff` 重校行号（已知会漂移的是 `useUsage.ts:78` 之后全部行号，票 04 在该文件 +3 行）。

Agent Hub 六分区（概览 / 安装与更新 / Skills / 供应商 / 使用统计 / 会话日志）功能骨架完整，Rust 98 测试、前端 22 测试、tsc 0 error、eslint 0 error 全绿。审查发现的问题分三类：

1. **可读性缺陷**：浅色主题下大量关键文字低于 WCAG AA 4.5:1，实测最低 1.75:1
2. **状态正确性缺陷**：会话列表分页游标在两个 tab 间串味，可产生重复行与页码/行数不符
3. **spec 漂移**：票 07 整票未实施（spec §2 v1 范围内），另有 3 项已落地但无任何文档记录的功能

审查方法与可复现命令见 §7。

## 2. 目标与非目标

**目标**：把「截图能看出问题」的部分全部收敛到 token 合法 + 对比度达标 + 状态自洽，并让 spec 与实现双向对齐。

**非目标**：
- 不做视觉改版 / 不改信息架构（六分区与变体 B 结论不动）
- 不引入新依赖、不改宿主任何模块、不动 WIT / ABI
- 不做 Windows 实机验证（沿用既有遗留项）
- 不重写 `useUsage` 之外的其他 composable

## 3. 缺陷清单（按严重度，全部有实测/可复现证据）

### P1-1 浅色主题关键文字对比度不达标

`evidence/ah-contrast.txt` 浏览器内实测（warm 主题，`.ah-shots/light-warm-*.png` 肉眼同步确认）：

| 元素 | 选择器 | LIGHT | DARK | 出现位置 |
| --- | --- | --- | --- | --- |
| 语义徽章·成功 | `.ah-cli-tag.ok` | **1.98** | 5.91 | 概览 4 卡 / 安装行 / Skills 三态 / 供应商来源 / 统计水位与会话 tag |
| 语义徽章·警告 | `.ah-cli-tag.warn` | **1.86 / 1.75** | 6.12 / 6.66 | 双安装、测速推荐、落后、桥接冲突 |
| 语义徽章·中性 | `.ah-cli-tag` | **3.60** | 5.00 | 未安装、仅规范库、手工、掩码 |
| 辅助文字 | `--text-tertiary` | **2.42** | **2.96** | 表头 / 列时间 / 列 tokens / 分页信息 / 源路径 / 总数标签 / 每日条数值 / 会话行 meta / 技能目录 / 降级提示 |
| 次级文字 | `--text-secondary` | 4.46 | 5.76 | 图例、描述、版本对比、模型行、meta、原始 JSONL（差 0.04 未达 4.5） |
| 工具事件正文 | `.ah-msg.role-tool .ah-msg-text` | 3.91 | 5.95 | 会话日志工具卡 |
| 助手角色文字 | `.ah-msg.role-assistant .ah-msg-role` | **2.09** | 8.08 | 会话日志角色标签 |

**根因分两类**：
- (a) 徽章把语义色同时用在**文字**上：`color: var(--color-success)` 叠 `background: var(--color-success-light)`。宿主自己从不用这个组合（`--color-*-light` 在 `src/` 内只喂 datepicker 主题桥接），是 agent-hub 引入的
- (b) `--text-tertiary` 在 warm 主题实测只有 2.42 / 2.96，**宿主 token 本身偏淡**；agent-hub 把它用在**数据**（时间、时长、token 数、会话源路径、表头）而非纯装饰上，放大了问题

LIGHT 采样 32 项中 30 项未达 4.5:1；DARK 采样 5 项未达标（全部是 `--text-tertiary` 类）。

### P1-2 会话列表分页游标在两 tab 间串味

`src/composables/useUsage.ts` 的 `sessions` / `page` / `loadedOffset`（:38、:121、:152-154）是**单份共享状态**，但两个消费方语义不同：

- `SessionLogsTab` = **按页 replace**（`goPage` → `fetchSessionsPage((page-1)*PAGE_SIZE, true)`）
- `StatsTab` = **追加 load-more**（`loadMoreSessions` → `fetchSessionsPage(loadedOffset, false)`）

可复现的两条错误路径：

| 路径 | 操作序列 | 用户看到的 |
| --- | --- | --- |
| A 重复行 | 统计 tab 加载更多到 45 行 → 日志 tab 翻到第 2 页（`loadedOffset` 被重置为 15）→ 回统计 tab 点「加载更多」 | 明细列表出现**整段重复行**（offset=15 的结果被追加到已在屏的 15–29 行） |
| B 页码不符 | 统计 tab 加载到 45 行 → 切日志 tab | 表格显示 45 行，分页器仍写「第 1 / 共 N 页」 |

附带：**日期筛选器不回显**——`SessionLogsTab.vue:101-102` 的 `fromInput/toInput` 初始为 `null`，而 `usage.rangeFrom/rangeTo` 是持久的；设了日期范围后切 tab 再回来，两个日期框显示空、列表却仍被过滤，此时点「查询」会**静默清掉**日期条件（`filterAgent`/`keyword` 都正确回填了，唯独日期漏了）。

### P1-3 票 07 整票未实施（spec §2 v1 范围内）

`rust/src/usage.rs:48` → `const ADAPTERS: [&str; 2] = ["claude", "pi"];`

spec §2 把「使用统计」列为 v1、§4.5 数据源明写 **opencode = SQLite**。当前后果：

- 装了 opencode（概览四家都在列），统计看板与会话日志**永远没有 opencode 数据**，且 UI 无任何解释（不像授权缺失有 `auth-required` 横幅）
- 票 07 另两项也没做：**codex「已装·未初始化」状态**（`CliCard` 只有 installed / not-installed / error / detecting / idle 五态，spec §9 明列该态）、**数据保留策略的手动清空**（`grep clear|purge` 在 `useUsage.ts` / `usage.rs` 零命中）

### P2 项

| # | 问题 | 证据 |
| --- | --- | --- |
| P2-1 | **`.ah-input` 重复定义**：`styles.css:509` 与 `:1466` 重复定义 `.ah-input` / `:focus` / `::placeholder`，两处 height(32/30) / background(transparent/bg-page) / border 冲突；且 1466 处注释「InstallTab 仍用 ah-input」自相矛盾（InstallTab 正是用它的自定义源输入框，而注释声称该块是给日志页的，日志页一个 `ah-input` 都没用） | `styles.css:509,1466` |
| P2-2 | **应用内三套输入框规格**：`.ah-input` 与 `.ah-sk-url` 均 30px / `--radius-button` / border-strong / bg-page；`SessionLogsTab` 用 Tailwind 宿主 token = 36px(`--input-height`) / `--radius-input` / border-input / bg-input | 实测 `.ah-input` height = 30px |
| P2-3 | **查询条一行四种控件高度**：实测 `select 36 / keyword 36 / date 32 / button 26`，肉眼可见参差 | `evidence/screenshots/light-warm-6-logs.png` |
| P2-4 | `StatsTab.vue:135` `syncedTag` 硬编码 `adapters.claude.parsed + adapters.pi.parsed`；票 07 补 opencode 后**静默少算**，无编译期保护 | `StatsTab.vue:135` |
| P2-5 | Rust 关键词过滤未转义 LIKE 通配符：`usage.rs:903` `format!("%{q}%")`，用户输入 `%`/`_` 匹配全表/单字符（参数化故无注入，纯语义） | `usage.rs:903` |
| P2-6 | 语义色借用：`.ah-st-cout`（输出 tokens 段）与 `.ah-msg.role-assistant`（助手角色）用 `--color-warning`(#f59e0b 琥珀)——「助手 = 警告色」误导；且 `ProvidersTab` 把**常驻的正常说明**（`hub.pv.secure`）放在 warning 底色 `ah-banner` 里，削弱真警告（桥接冲突）的效力 | 实测输出段 `rgb(245,158,11)`；`--color-primary` 在 warm light = `rgb(29,26,20)`（近黑），导致每日 tokens 图表 90% 是黑条 |
| P2-7 | 布局为窄面板设计、在宽面板过度拉伸：`.ah-grid` 默认行拉伸（Codex 双装卡把同排 Claude 卡撑出 ~120px 死白；统计右卡被左卡撑高）；`.ah-env-rows` 2 列在 1600px 窗口下 label-value 相隔 ~800px（概览环境条、claude 只读视图两处中招）；`.ah-sk-row` / `.ah-inst-row` 的 `flex:1` 把状态与动作推到最右，中间断裂 | `evidence/screenshots/light-warm-1-overview.png`、`-5-stats.png`、`-3-skills.png` |
| P2-8 | 前端测试只覆盖 3 个纯函数文件（`format`/`diff`/`providers`，22 用例），**5 个 composable + 6 个组件 + 全部交互逻辑零覆盖**；三个 P1 bug 全部落在无测试区。票 03–06 的「代码路径就绪并过测试」实际靠 dev-shell 人工截图 | `src/__tests__/` 仅 3 文件 |

### P3 项

| # | 问题 | 位置 |
| --- | --- | --- |
| P3-1 | 死代码：`TabPlaceholder.vue` 零引用（票 06 已删 import 但文件留存）+ `hub.placeholder.*` 6 个 i18n key × 2 语言 × schema | `components/TabPlaceholder.vue`、`i18n/messages.ts` |
| P3-2 | 模板在用但 CSS 未定义：`.ah-overview` / `.ah-cli-card` / `.ah-pv-apply` / `.ah-lg-col-agent|title|project` / `.ah-pv-keymode`（只有 `-actions` 有规则） | 4 处组件 |
| P3-3 | 硬编码色板：`AgentIcon.vue:33-40` 六条 `#6366f1/#4f46e5/…` 渐变（抄自宿主 `LetterAvatar.vue` 同款硬编码，规则上仍违反 frontend-styles「不得引入第三方 hex 色板」）；`styles.css:356,360` `rgba(217,119,87,.16)` / `rgba(245,158,11,.16)` 品牌底色；`:1113` 徽章 `font-size: 10px` | 3 处 |
| P3-4 | 文档漂移：`styles.css:4-8` 头注释称「宿主 Vite 构建只扫描宿主 `src/`（tailwind content），不会为插件 SFC 里的 Tailwind 类生成样式」——**已不成立**，`tailwind.config.js` 末两行明确纳入 `./wasm-apps/agent-hub/src/**`（还有 `src/__tests__/plugin/tailwindContentCoverage.test.ts` 护栏）。注释会误导维护者以为不能用 Tailwind | `styles.css` 头 |
| P3-5 | `100vh` 魔数：`.ah-sk-editor-body` / `.ah-lg-events` / `.ah-lg-raw-card` 用 `calc(100vh - 260px/320px)`，与宿主 chrome 高度（TitleBar `h-10`=40px + PluginStatusBar）静默耦合 | `styles.css` 3 处 |
| P3-6 | 弹窗 Esc 关闭不可靠：`ProvidersTab` 弹窗 `@keydown.esc` 挂在无 `tabindex` 的 div 上，焦点不在子元素时无效；无 focus trap | `ProvidersTab.vue` |
| P3-7 | `useUsage.autoScanDone`（:176、:183-184）在 `scan()` resolve 前置位，扫描失败后本次会话不再自动重试 | `useUsage.ts:176` |
| P3-8 | `SkillsTab.vue:57` `good` 变量未使用（eslint warning）；`vite.config.ts:7` `pluginId` 未使用（eslint warning） | 2 处 |

### 无 spec/票记录的功能（单向漂移，需补文档）

| 功能 | 落地位置 |
| --- | --- |
| 正在使用的项目会话（读 `~/.claude.json` 的 `lastSessionId`/`lastStartTime`） | `rust/src/usage.rs:12-19,671` `state.activeSessions` |
| 日志来源自定义目录增删 | `list/add/remove-usage-source` + `useUsage.ts:45-86` |
| 会话日志改为「查询表格 + 二级详情（聊天记录 / 原始 JSONL）」，偏离 spec §4.6 的「主从布局（左侧列表 + 右侧事件流）」 | `SessionLogsTab.vue` 全文 |

## 4. 票据
| 票 | 标题 | 覆盖 | 阻塞 | 状态 |
| --- | --- | --- | --- | --- |
| 09 | 语义徽章与辅助文字可读性 | P1-1 + P2-6 的对比度部分 | None | ready-for-agent |
| 10 | 会话列表查询域拆分与筛选回显 | P1-2 | None | ready-for-agent |
| 11 | 票 07 补齐：opencode SQLite 适配 + codex 预留 + 数据清空 | P1-3 | None | resolved（2026-09-27） |
| 12 | 表单控件规格统一 | P2-1 / P2-2 / P2-3 | 09 | ready-for-agent |
| 13 | 语义色归位与宽面板布局 | P2-6 配色 / P2-7 | 09 | ready-for-agent（§6.2 已裁决：新增 `--chart-*` token） |
| 14 | 死代码、硬编码色板与文档漂移清理 | P3 全部 | None | ready-for-agent（§6.3 已裁决：仅插件内改） |
| 15 | 前端测试补齐（composable + 组件 + 对比度矩阵） | P2-8 + 09/10/13 的回归护栏 | 09 / 10 / 13 | ready-for-agent |

建议实施顺序：**09 → 10 → 13 → 12 → 14 → 15 → 11**（09 先行因为 12/13/15 都依赖它的文字与徽章改动；11 独立且工作量最大，可并行或最后做）。

## 5. 关键设计决策

### 5.1 徽章改色法（票 09）

放弃「语义色同时上文字 + 底色」，改为**色彩只承载在图形上，文字回到可读 token**：

```
.ah-cli-tag.ok   { background: var(--color-success-light); color: var(--text-primary); }
.ah-cli-tag.ok   .ah-cli-dot { background: var(--color-success); }
```

理由：(a) WCAG AA 对 11px 文本要求 4.5:1，半透明底色上放纯语义色在任何浅色主题都做不到；(b) 圆点是无文本图形，1.9:1 的 ΔL 足够承载状态识别，且状态在**形状 + 颜色**双通道上，不依赖色觉；(c) 改完明暗两态都是「深字 + 淡底 + 彩点」，视觉一致性反而更高。

### 5.2 数据文字与装饰文字分级（票 09）

> **2026-09-27 实测修正**：本节初稿写「数据用 `--text-secondary`」，**实测不成立**——`--text-secondary` 在 warm light 对 `--bg-card` 只有 **4.46:1**，差 0.04 未达 AA；且在 6 种承载面里最差仅 3.60。改用 `color-mix` 派生（frontend-styles「偏好顺序」第 3 条明确允许派生色）。

定三级：

| 级别 | 取值 | 全主题 × 全承载面最差对比度 | 用途 |
| --- | --- | --- | --- |
| **主** | `--text-primary` | 12.35:1 ✅ | 标题、会话名、正文、气泡 |
| **数据** | `--ah-text-data` = `color-mix(in srgb, var(--text-secondary) 55%, var(--text-primary))` | **5.84:1** ✅ | 表头、列时间/时长/tokens、总数标签、每日条数值、分页信息、会话源路径、技能目录、降级提示、事件 meta、token 明细 |
| **装饰** | `--text-tertiary` | 2.42（light）/ 2.96（dark）❌ | **仅限显式豁免位**：图例、空态整句、占位符、副标题、不可读性不承载信息的地方 |

**为什么装饰级无解**：`--text-tertiary` 与 `--text-secondary` 无论按什么比例 `color-mix`，最差都到不了 4.5（ter@40+sec 2.73 / ter@20+sec 3.06）——要达标得混 60% 以上到 secondary，那已经是数据级了。所以装饰级只能**登记豁免**而不是硬凑。

**不改宿主 token**（`--text-tertiary` 是全局 token，动它影响面超出本票），只改 agent-hub 的选用面 + 引入一个插件内派生变量 `--ah-text-data`。

对比度验算口径：12 套主题（6 palette × 明暗）× 6 种承载面（`--bg-card` / `--bg-page` / `--bg-hover` / `--bg-sidebar` / success-light 叠 card / warning-light 叠 card）取最差值；`color-mix(in srgb, …)` 的实际 sRGB 线性插值结果与浏览器一致，用同一公式在脚本里复算。

### 5.3 对比度护栏（票 09 + 15）

照搬 `bedcode-mobile/src/__tests__/config/terminalThemes.test.ts` 的做法：写一个**对比度矩阵测试**，对 6 套 palette × 明暗 = 12 套主题，断言 agent-hub 实际使用的 (前景 token, 背景 token, 最小字号) 组合全部 ≥ 4.5:1（≥18px 或 14px+bold 时 ≥3.0）。组合表由**解析 `styles.css`** 自动枚举，不手写常量，避免新增样式漏检。

### 5.4 查询域拆分（票 10）

`useUsage` 拆成共享与独立两层：

```
共享（两个 tab 都要）: listFilter / searchText / rangeFrom / rangeTo
统计 tab 私有:        statSessions / statTotal / statLoaded
日志 tab 私有:        logSessions / logTotal / logPage / logLoading
```

`buildQuery()` 读共享条件；`StatsTab` 的「加载更多」与 `SessionLogsTab` 的「翻页」各走自己的游标。`SessionLogsTab` 的 `fromInput/toInput` 改为**从 `usage.rangeFrom/rangeTo` 初始化**（与已正确回填的 `filterAgent`/`keyword` 对齐）。

同时把 `StatsTab` 的 `syncedTag` 改成遍历 `Object.values(state.adapters ?? {})` 求和。

### 5.5 票 07 出路 —— **已裁决：方案 A（补齐）**

用户 2026-09-27 裁决走方案 A：补齐 opencode SQLite 适配 + codex 骨架 + 手动清空，**母 spec §2 不动**（v1 承诺保持「四家适配器」）。

选 A 的依据（2026-09-27 实机核实，见 `issues/11`）：聚合层几乎是白送——`session` 表是扁平列，与 `usage_session` 一对一映射；真正的成本只在事件流解析（claude/pi 是 JSONL 直读，opencode 要解 `message` / `part` 的 `data` JSON blob）。

## 6. 裁决记录（原「待用户裁决」，2026-09-27 全部关闭）

| # | 事项 | 裁决 | 理由 |
| --- | --- | --- | --- |
| 6.1 | 票 07 走 A 还是 B | **A：补齐** | 聚合层成本远低于预估；v1 承诺（spec §2 四家 / §4.5 opencode=SQLite）不降级。票 11 解除 `blocked-on-user-decision` |
| 6.2 | 图表配色是否新增专用 token | **新增 `--chart-in` / `--chart-out`**，落 `:root` 与 `:root.dark`，并登记进 `.agents/skills/frontend-styles/TOKENS.md` | 现状是**双重语义错用**：输出 tokens 段借 `--color-warning`(#f59e0b 琥珀=警告)，而 warm light 的 `--color-primary` = `rgb(29,26,20)` 近黑导致图表 90% 是黑条。复用语义 token 无论挑哪个都不对；`color-mix` 派生能解决对比度但解决不了「数据色 ≠ 语义色」 |
| 6.3 | `AgentIcon` 渐变色板是否连宿主一起 token 化 | **只在本插件内换中性派生，不动宿主 `LetterAvatar`** | AGENTS §0 最小改动原则：`LetterAvatar` 的硬编码是宿主既有债，改它会把本票范围从 wasm 应用扩到宿主组件（连带其它引用方），超出本 spec 边界。插件内改用 `--bg-hover` + `--text-primary` 派生，视觉一致且零扩散 |

## 7. 验证（完成定义）

审查与复现命令（**本机 Chrome 在 LingLong 容器里**）：

```bash
# 1) dev-shell 起插件前端（必须用绝对路径：vite.config 用 process.cwd() 解析相对路径）
cd bedcode-desktop/packages/plugin-sdk-desktop/dev-shell
BEDCODE_DEV_PLUGINS=$PWD/../../../wasm-apps/agent-hub BEDCODE_DEV_PORT=5193 \
  node ./node_modules/vite/bin/vite.js

# 2) headless Chrome（本机无 chrome 可执行文件，只有 LingLong 层；见 §8）
CHROME=/persistent/var/lib/linglong/layers/9d0eb9fda1fd61a24753083adc152df37f804fc95fec39e21c47996284eb9941/files/bin/google/chrome/google-chrome
"$CHROME" --headless=new --no-sandbox --disable-gpu --remote-debugging-port=9222 \
  --user-data-dir=/tmp/chrome-prof about:blank

# 3) 截图六分区 × 明暗 + 对比度实测
node .scratch/2026-09-27-agent-hub-audit-fixes/evidence/ah-shot.mjs
node .scratch/2026-09-27-agent-hub-audit-fixes/evidence/ah-contrast.mjs
```

每票收尾必须实际运行并贴出结果：

- 改动落在样式 → 重跑 `ah-contrast.mjs`，**LIGHT/DARK 两栏 FAIL 数归零**（或每条 FAIL 都有显式豁免记录）
- 改动落在 Rust → `cd bedcode-desktop/wasm-apps/agent-hub/rust && cargo test` 全绿
- 改动落在前端 → `cd bedcode-desktop && pnpm exec vitest run wasm-apps/agent-hub` 全绿
- `cd bedcode-desktop/wasm-apps/agent-hub && pnpm exec tsc --noEmit -p tsconfig.json` 0 error
- 根目录 `pnpm exec eslint bedcode-desktop/wasm-apps/agent-hub` 0 error（票 14 后 warning 也应归零）
- 改了 i18n → 新增/修改 key 同步出现在 `zh-CN.ts` 与 `en.ts`（`MessageSchema` 编译期保证）
- 母 spec `.scratch/2026-09-13-agent-hub/spec.md` 与票 07 状态同步更新（§5.5 方案 B 需改 §2）
- 双向漂移清零：`grep` 确认 §3「无 spec/票记录的功能」三项已写进 spec 或票
- 测试后清理：本机 LingLong Chrome 与 dev-shell 进程、`:9222` / `:5193` 端口无残留

## 8. 环境事实（本机，2026-09-27 实测）

- **Chrome 152.0.7977.64 装在 LingLong 容器里**，路径：
  `/persistent/var/lib/linglong/layers/9d0eb9fda1fd61a24753083adc152df37f804fc95fec39e21c47996284eb9941/files/bin/google/chrome/google-chrome`
  （`which google-chrome chromium` / `/usr/bin/google*` / `/opt/apps` 全都找不到；`~/.config/google-chrome` 有活跃配置但系统无 PATH 条目）
- `browser-tools` skill 的 `browser-start.js` **硬编码 macOS 路径**，本机不可用；手动起 Chrome + `--remote-debugging-port=9222` 后，`browser-nav.js` / `browser-eval.js` 可正常复用（它们只连 localhost:9222）
- 无 xvfb、无 playwright/puppeteer 自带 chromium；`--headless=new --no-sandbox` 可用
- dev-shell 的 `BEDCODE_DEV_PLUGINS` **必须绝对路径**：`vite.config.ts` 的 `parseDevPlugins` 用 `resolve(dir)` 走 `process.cwd()`，相对路径按启动目录而非 dev-shell 目录解析
- `pkill -f vite` / `pkill -f dev-shell` 会**匹配到本命令自身**导致 shell 被 SIGTERM（exit 143）——清理时改用 `ss -ltnp | grep <port> | grep -oP 'pid=\K[0-9]+' | xargs kill`
