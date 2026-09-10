# pi 工具专属协作手册（仅 pi agent 生效）

> 从主 `AGENTS.md` 外链而来的工具操作手册。**非 pi agent**（Claude Code / Codex / Gemini / OpenCode 等）不读本节，按主文档「代码查找纪律」的 code-map 默认规范执行。
>
> 适用范围：pi-lens 代码查询纪律、subagents 编排、vision 视觉 subagent、scipq Rust 精确引用。

---

## 1. Code Query Discipline（pi-lens 增强）

**Gate：** 如果当前 agent 具备以下 pi-lens 工具（`lens_diagnostics` / `lsp_diagnostics` / `symbol_search` / `module_report` / `read_symbol` / `read_enclosing` 任一），走本节纪律；否则按主文档 code-map 默认规范（`ls` / `rg` + `read`）执行。

pi-lens 提供 IDE 级语义能力（词索引 + tree-sitter 结构 + LSP 诊断），配合 code-map 的目录索引形成「**目录 → 文件 → 符号**」三级漏斗，替代盲目全仓 `rg` 和整文件 `read`。

**闭环三阶段：**

| 阶段            | 命令                                                                                          | 目的                                                     |
| --------------- | --------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| 1. **Orient**   | code-map 定位模块目录 →（可选）`project_report` 全局鸟瞰                                      | 明确当前工作子系统                                       |
| 2. **Discover** | `symbol_search` 找候选文件 → `module_report` 看大纲 → `read_symbol` / `read_enclosing` 精准读 | 用符号粒度替代整文件 read，省 token 并保 read-guard 覆盖 |
| 3. **Verify**   | 编辑前 `lsp_diagnostics` 探活 → 编辑后 `lens_diagnostics mode=all` 收尾                       | 编辑前确认基线无阻塞，编辑后确认无遗留                   |

**强制规则：**

- 目标文件 > 200 行时，**禁止 `read` 整个文件**，必须走 `module_report` 大纲 → `read_symbol` / `read_enclosing` 定点读
- 找「谁在用它」类符号引用，**禁止 `rg 符号名`**，用 `symbol_search` 或 `lsp { action: "references" }`；Rust 类型精确引用用 `scipq`（见文末「scipq」节）
- 找「某段文案 / 日志字样 / 配置项」才是 `rg` 的场景（字符串字面量、注释、日志 tag 不是符号）
- turn 结束前必须跑一次 `lens_diagnostics mode=all` 收尾；报告 🔴 blocker 未清前不算 done
- cascade 上报的 LSP 假阳性（Vue 组件类型误判、Vite shim 解析不到、跨 tsconfig 边界的类型缺失）按 `[slop]` / `[code-smell]` 建议对待，不是阻塞；确认后可用 `lens_diagnostic_mark` 记录 false-positive

**Fallback：**

- `symbol_search` 返回 `available: false` → word index 未构建，等 10s 重试；仍不可用 → 退化为 code-map + `ls` / `rg`
- `lsp_diagnostics` / `lens_diagnostics` 报 `unavailable` / `unconfirmed` / `cold` → LSP 冷启动或诊断超时，视为 inconclusive（**不能算 clean**，见 pi-lens honesty contract），必要时 `lens_diagnostics mode=full` 重扫
- 情境工具 `ast_grep_search` / `lsp_navigation` / `lens_diagnostic_mark` 首次使用前先 `pi_lens_activate_tools` 激活（该工具明确返回 "Available starting next turn"，同一 turn 内不要重试）

---

## 2. Subagents

pi 使用 `pi-subagents` 包（用户级安装 `git:github.com/nicobailon/pi-subagents`）将任务委派给隔离上下文的专用 agent；项目 agent 定义在 `.pi/agents/*.md`，自动被发现，同名 agent 优先于包内置的 scout/worker/reviewer。

| Agent      | 用途                                 |
| ---------- | ------------------------------------ |
| `scout`    | 代码侦察，返回压缩上下文             |
| `planner`  | 制定实现计划（只读）                 |
| `reviewer` | 代码审查（只读）                     |
| `worker`   | 通用实现（完整能力）                 |
| `tester`   | 运行测试并报告                       |
| `vision`   | 视觉分析（唯一带视觉能力），详见下节 |

调用方式：单任务 `{ agent, task }`；并行与链式通过 `workflowScript` 编排（`await runs.run(key, { agent, task })` 顺序执行、`await runs.all([...])` 并行执行），步骤间用上一步结果的 `.output` 传递；旧的顶层 `tasks`/`chain`/`parallel` 参数已不支持。

适用场景：可并行的独立子任务、需隔离上下文的重型任务；简单定位/小改动不必启动 subagent。

---

## 3. Vision subagent

`vision` 是唯一带视觉能力的 agent：读图与结构化分析，不改文件、不执行命令。主 agent 必须提供**图片文件绝对路径**（非 URL / 非 base64）。

### 评审范围协议（强制）

主 agent 必须在 `task` 中用 `范围:` 或 `scope:` 一行显式指定评审范围：

| 指令                                      | 行为                              |
| ----------------------------------------- | --------------------------------- |
| `范围: 完整` / `scope: full`              | 评审整张图(含外壳)                |
| `范围: 手机内部` / `scope: phone`         | 只评手机模拟器内                  |
| `范围: 桌面应用内` / `scope: desktop`     | 只评桌面应用窗口内                |
| `范围: 忽略外壳` / `scope: ignore-chrome` | 自动识别 dev-shell 外壳并只评内部 |
| `范围: <自由描述>`                        | 按描述执行                        |

未指定范围时自动识别 dev-shell 并只评内部；指令冲突时主 agent 指令优先。完整协议见 `.pi/agents/vision.md`。

标准调用：

```javascript
// 显式指定范围
subagent({
  agent: 'vision',
  task: `
  范围: 手机内部
  截图: <绝对路径>
  ...评审要求...`,
})

// 零配置 — 默认自动识别 dev-shell
subagent({ agent: 'vision', task: '请分析截图 <绝对路径>' })
```

截图准备：Chrome headless 直连（`chrome.exe --headless=new --screenshot=<out.png> --window-size=1440,900 <url>`）、`browser-tools` skill 的 `browser-screenshot.js` / `browser-content.js`（`browser-start.js` 仅 macOS）、或已有图片文件直传绝对路径。

协助 skill：`design-taste-frontend-v1`（`.agents/skills/taste-skill-v1/`），vision 做 UI/设计稿评审时自动加载，提供品味基线（VARIANCE=8 / MOTION=6 / DENSITY=4）与硬性指标；React/Next.js 原始语境自动映射到 Vue 3 + Tauri 栈。

适用场景：错误截图诊断、UI 截图评审、设计稿解读、架构图/流程图解析、代码截图转文字、图标识别。输出固定格式：基础描述 → 详细分析 → Pre-Flight 自查 → 建议。

---

## 4. scipq — Rust 类型精确引用（SCIP 索引，无常驻进程）

Rust 的「谁定义 / 谁引用 / 改它影响谁」用 SCIP 索引查询——一次性建索引、之后查询零内存：

```bash
.pi-lens/scip/scipq syms <片段>                # 符号全名/kind（先定位 symbol）
.pi-lens/scip/scipq defs <片段>                # 定义位置（精确 file:line:col）
.pi-lens/scip/scipq refs <片段>                # 引用（文件+行区段，秒级）
.pi-lens/scip/scipq refs-exact "<完整symbol>"  # 引用精确行列（2-5s）
```

**刷新策略（固定间隔或按需，不做每次代码变更的自动重建）**：按需 `scipq rebuild`；固定间隔 `scipq rebuild-if-stale [hours]`（默认 24h，间隔内零成本跳过）；状态自查 `scipq stale`。重建触发时机、执行步骤与验证标准见 `scip-rebuild` skill。role 语义（0=引用、1=定义）、格式与已知限制见 `.pi-lens/scip/README.md`。