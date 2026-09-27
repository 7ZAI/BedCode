# 15: 前端测试补齐（composable + 组件 + 对比度矩阵）

**What to build:** 当前 `wasm-apps/agent-hub/src/__tests__/` 只有 3 个纯函数文件（`format` / `diff` / `providers`，22 用例），5 个 composable、6 个组件、全部交互逻辑零覆盖——三个 P1 bug 全部落在无测试区。补三层：(a) **composable 层**：`useUsage` 的分页/查询/来源增删（用 mock context 驱动 `agent-hub.list-usage-sessions` 等命令）、`useDetection` 的 seq 乱序过滤与 25s 超时复位、`useSkills` / `useProviders` / `useInstall` 的关键状态机；(b) **组件层**：用组件里已有的 `data-testid`（`preset-editor` / `apply-conflict` / `import-confirm` / `distribute-<dir>` / `filter-from` / `filter-to` 等）做 mount 测试，断言行为而非快照；(c) **对比度矩阵**：照搬 `bedcode-mobile/src/__tests__/config/terminalThemes.test.ts` 的做法，解析 `styles.css` 自动枚举 agent-hub 实际使用的 (前景 token, 背景 token, 最小字号) 组合 × 6 套 palette × 明暗，断言 ≥ 4.5:1（≥18px 或 14px+bold ≥ 3.0），豁免项显式登记。

**Blocked by:** 09 / 10 / 13（先有修复与新 token，测试才有稳定断言面）

**Status:** resolved

- [x] `useUsage` 覆盖：共享查询条件变更触发两处列表各自重载第 1 页；`load-more` 与 `goPage` 游标互不干扰；`openSession` / `closeSession` / 来源增删的成功与失败分支
- [x] `useDetection` 覆盖：seq 乱序旧事件被丢弃、detecting 超 25s 强制复位并 `refresh()`、`applyState` 的 `clear envError` 语义
- [x] 组件测试覆盖：ProviderApply 四种 keyMode + 桥接冲突两击确认、SkillsTab GitHub 覆盖确认与导入覆盖确认、SessionLogsTab 查询/重置/翻页/开详情/切原始页签、InstallTab 行状态机五态
- [x] 对比度矩阵测试存在且会失败（先在修复前跑一次证明它能抓出 P1-1）
- [x] 变异自检：每个新增测试至少做一次「改坏实现 → 测试变红」验证
- [x] `pnpm exec vitest run wasm-apps/agent-hub` 全绿，用例数从 22 提升到有意义的量级（不以覆盖率为目标）
- [x] 无恒真断言 / 无只测 mock 的断言（每个 mock 调用都要有行为断言）

## Answer（2026-09-27 实施完毕）

**用例数：22 → 172（7 个文件）**，新增 4 个测试文件 + 1 个 helper。

| 文件 | 用例 | 覆盖 |
| --- | --- | --- |
| `useUsage.test.ts` | 20 | U1 追加语义 / U2 按页语义 / U3 游标互不干扰（两条复现路径完整序列）/ U4 共享条件重载 / U5 查询入参 / U6 失败不破坏数据 / U7 详情开关 / U8 来源增删两态 / U9 autoScanDone |
| `useDetection.test.ts` | 12 | D1 首轮探测有无状态两分支 / D2 seq 乱序·同 seq·seq=0 / D3 25s 兜底（未收敛触发、中途收敛不触发、非 detecting 不挂表、卸载清理）/ D4 探测失败不卡死 |
| `components.test.ts` | 33 | A1 ProviderApply key 四选一 + 桥接冲突两击 + 目标切换 + 命令失败 / A2 弹窗 Esc·焦点进出·focus trap·三条关闭路径 / A3 syncedTag 适配器求和 + syncing 态 + 加载更多 / A4 日志分页自洽·翻页·日期回显·重置·详情·原始页签·空态 / A5 GitHub 与本地导入的覆盖确认 + 授权拒绝 / A6 安装行七种状态 + 两击换源 + 失败只进日志 |
| `styleGuards.test.ts` | 85 | S1 文字对比度矩阵（12 主题 × 59 项）/ S2 无文本图形 3:1（7 项）/ S3 图表双段 ΔE 三条 / S4 豁免理由非空 / S5 模板类名↔样式双向 + 动态前缀 / S6 语义 token 不当文字色 / S7 写死色值白名单 / S8 100vh / S9 死代码与输入框规格 |
| `helpers/contrast.ts` | — | 对比度数学 + 主题层叠合成 + 受审清单单一真源（只被测试 import，不进 lib 产物） |

**对比度矩阵不是手写常量**：前景色从 `src/styles.css` 用 CSS 规则解析器**实际读出**，
主题 token 从宿主 `src/style.css` 按真实层叠（`:root` → `:root.dark` → `[data-palette]` → `.dark[data-palette]`）
合成 12 套；承载面是唯一手写的结构信息。数学与浏览器实测交叉验证：secondary-on-hover 3.60 /
tertiary-on-card 2.42(light) / 2.96(dark) 与 `evidence/ah-contrast.txt` 逐项一致。
离线脚本 `evidence/ah-matrix.mjs` 与测试共用同一底座，可脱离浏览器复核。

**「先证明它能抓出 P1-1」**：把 `.ah-cli-tag.ok` 文字改回 `var(--color-success)`（审查前原样）后，
S1 该项与 S6 立即变红 —— 矩阵不是恒真断言。

**变异自检（3 次，全部杀死）**：

| 变异 | 期望变红 | 实测 |
| --- | --- | --- |
| `goPage` 改写 `statLoaded`/`statSessions`（复现旧游标串味） | U3 两条 | ✅ 2 failed |
| `.ah-cli-tag.ok` 文字回语义色 | S1 + S6 | ✅ 2 failed |
| 覆盖导入不发 `force` | A5 一条 | ✅ 1 failed |

复原后 172/172 复绿。

**无反模式自查**：无 `expect(true)`；无只断言 mock 被调用（A1 断的是命令入参 + 渲染结果双侧；
A5 额外断确认条消失）；无快照；无 sleep / 真实时间（D3 用 `vi.useFakeTimers`）；
测试数据显式构造（`row(i)` 造 45 条会话）；每个 mock 返回值都符合真实契约
（`list-usage-sessions` 按 offset/limit 真切片，不是写死数组）。

**未覆盖风险**：
- `useInstall` / `useSkills` / `useProviders` 三个 composable 的**内部状态机**未直接单测
  （SkillsTab 走真实 composable 的导入流程已覆盖，P2/SkillEditor 弹窗未覆盖）
- `SkillEditor` 组件零覆盖（本期未改动其行为面，仅票 04 遗留）
- i18n 双语键一致性依赖 `MessageSchema` 编译期保证，未另加运行时用例
