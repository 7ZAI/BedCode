# AGENTS.md

## Project Overview

BedCode：局域网远程终端应用——桌面端作为主机运行终端会话（Claude Code 等），移动端作为远程终端控制，通过 WebSocket + HTTP 通信，适配两端同一 WiFi 场景。

**Tech Stack:** Tauri 2.0 + Vue 3 + TypeScript + TailwindCSS + Rust (Tokio) + SQLite + vue-i18n@9

**Monorepo 结构：**

- `bedcode-desktop/` — 桌面端主机
- `bedcode-mobile/` — 移动端远程终端
- 各项目独立 `src/`（前端）与 `src-tauri/`（Rust 后端）

---

## Build & Run

项目统一使用 **pnpm** 作为前端包管理器（各独立工程/workspace 各自维护 `pnpm-lock.yaml`）。

```bash
# Desktop Development
cd bedcode-desktop && pnpm run tauri:dev

# Desktop Build
cd bedcode-desktop && pnpm run tauri:build

# Mobile Development
cd bedcode-mobile && pnpm run tauri:android:dev

# Mobile Build
cd bedcode-mobile && pnpm run tauri:android:build

# Frontend Test（必须用 test:run，禁止 pnpm run test——watch 模式会挂起）
cd bedcode-desktop && pnpm run test:run

# Rust Test
cargo test

# Kotlin/Gradle 编译（gen/android 有 Kotlin 改动时必跑，非 Android 平台跳过）
cd bedcode-mobile/src-tauri/gen/android && ./gradlew :app:compileUniversalDebugKotlin
```

- **Kotlin 验证**：改 `gen/android/app/src/main/java/com/bedcode/mobile/` 下自定义 Kotlin 插件后，必须额外跑上述 gradlew 命令（独立 Gradle/Kotlin 工具链，`cargo test` 与前端测试均不覆盖）；离线加 `--offline`
- **前端测试统一 `pnpm run test:run`**（= `pnpm exec vitest run`，跑完退出）；`pnpm run test` 是 vitest watch 模式，执行后挂起不退出，禁止使用；`vite` 不执行测试
- **文档命令字眼必须随工具链迁移**：spec / issue / scratch / 知识库文档提到测试/构建/安装命令时，**必须**使用本节规定的字眼（`pnpm run test:run`、`pnpm run tauri:dev`、`cargo test` 等），禁止沿用旧 `npm` / `npm run test` 字眼。背景：master `49398462` 已完成 npm→pnpm 迁移，未同步文档的旧字眼会在审计时制造假阳性。审计新 spec/issue 时若发现命令字眼与本节不一致，先改文档再继续
- 构建前检查 `src-tauri/target` 目录大小，超过 15GB 执行 `cargo clean`
- 桌面端 `tauri:build`（`scripts/tauri-build.js`）自动解析 updater 签名密钥（`TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_FILE` / `.env`），未配置时自动禁用升级包生成，本地构建无需私钥；正式发布由 GitHub Actions Secrets 签名（见 `docs/knowledge/release-workflow.md`）

---

## Rust (Backend)

### File Naming

snake_case；模块入口文件与目录同名（`module.rs`），不用 `mod.rs`；测试文件 `*_test.rs`。

### Error Handling & Thread Safety

- 统一错误类型 `AppError`：`pub type Result<T> = std::result::Result<T, AppError>`
- 关键调用链用 `anyhow::Context` 添加上下文；`tokio::spawn` 用 `spawn_with_error_boundary()` 包装
- 错误字符串必须说明什么操作在哪失败，禁止无上下文裸字符串
- 重要路径禁止 `let _ =` 静默忽略错误
- panic hook 中只用 `eprintln!`，禁止 `tracing::error!`
- 状态共享用 `Arc<Mutex<T>>` / `Arc<RwLock<T>>`

### Tauri Commands

命名：`list_*`（多个）、`get_*`（单个）、`create_*`、`delete_*`、`start_*` / `stop_*`（生命周期）。用 `// ====================` 分隔注释按领域分组。

### Logging

统一 `tracing`：`debug!`（常规）、`info!`（关键）、`warn!`（警告/重试）、`error!`（异常）。Android 统一写 `tracing::` 宏，自动转发 logcat。

| 端 | 落盘方式 | 位置 |
| --- | --- | --- |
| 桌面端 | 始终写文件（dev/release） | `%LOCALAPPDATA%\com.bedcode.app\logs\`：`runtime.*.log` 全级别（dev 强制 debug）、`error.*.log` 仅 ERROR、`frontend.*.log`（仅 dev），按天轮转 |
| 移动端 | 电脑端落盘仅 `pnpm run tauri:android:dev:log`（普通 dev 只打控制台）；release 走 logcat | `bedcode-mobile/.dev-logs/android-dev.YYYY-MM-DD.log`（无 ANSI 码，可 grep；含 `frontend` 目标前端日志） |

**前端 console 日志（仅 debug 构建，AI agent 抓取前端控制台输出的通道）**：前端 `console.*` 经 `src/utils/devConsoleRelay.ts` 覆盖转发 → Rust `commands/dev_logs.rs::report_frontend_log`（`#[cfg(debug_assertions)]` 注册，仅 `tauri:dev` / `tauri:android:dev` 生效）→ tracing（target=`frontend`）。获取方式：

- 桌面端：直接读 `frontend.*.log`（单独文件，纯前端日志；同批事件也混入 `runtime.*.log`）
- 移动端：`pnpm run tauri:android:dev:log` 落盘文件 grep `frontend`（tracing → logcat）
- release 两端自动剥离（Rust 命令不注册 + 前端 `import.meta.env.DEV` no-op），无需额外清理

实际路径确认：桌面端 dev 控制台首行 `Logging initialized.`；移动端 `dev:log` 启动打印 `[dev-log] 电脑端日志落盘: <路径>`。

排查链路问题优先 grep 两端 `runtime.*.log`：`file_service`、`peer_changed`、`MessageBus`、`reqwest::connect`（代理劫持痕迹 `proxy(...) intercepts`）。

---

## Frontend (Vue 3 + TypeScript)

- 组件用 `<script setup lang="ts">`；业务逻辑放 composables，组件只负责 UI
- Composable 命名 `use<Resource>` / `use<Action>`；全局状态用 Pinia store 包装 composables
- 平台检测用 `@tauri-apps/plugin-os`，**禁止屏幕宽度检测**

### Styles & Layout（必读 skill，强制）

任何前端 UI 改动（组件、布局、CSS/Tailwind 类、design token、动画/过渡、主题、响应式、安全区、字体/行高）**必须先加载 `frontend-styles` skill 并以其规范为准**，禁止凭通用前端经验自行发挥。配套文件位于 `.agents/skills/frontend-styles/`。

---

## Code Comments

- 注释解释为什么而非是什么；语言中文，技术术语保留英文
- Rust：模块级 `//!`、pub 项 `///`、内联 `//`；TS：文件头 `/** */`、export JSDoc、Vue 组件 `<script setup>` 顶部说明
- 分隔注释：`// ==================== Section ====================`
- 注释掉的代码必须标注意图与恢复方式（如 `/* 临时调试：排查 X，恢复时取消注释 */`）；无说明的陈旧注释代码一律删除

---

## File Naming

| Type          | Pattern                     |
| ------------- | --------------------------- |
| Vue Component | PascalCase (`TitleBar.vue`) |
| Composable    | camelCase with `use` prefix |
| Store         | camelCase                   |
| Rust          | 见上文 Rust File Naming     |

---

## Architecture Decisions

1. Multi-Project Monorepo（各自独立 `src/` 和 `src-tauri/`）
2. Async Everywhere（Tokio + async/await + Tauri commands）
3. Event-Driven（PTY 输出经 `broadcast` 通道分发）
4. Graceful Shutdown（`AtomicBool` 通知后台任务关闭）
5. Flat Module Structure（按领域扁平组织）
6. Plugin System（Rust API crate + 前端加载器双层架构）
7. 插件 Mock 数据归属插件工程：各插件的 mock 数据 / 演示种子在各自工程目录内维护（如插件入口导出 `devMock`，SDK `PluginDevMock` 协议）；dev-shell 只提供 mock 抽象封装与通用接线（devMock 注册、命令 handler 骨架、事件总线），**禁止在 dev-shell 中编写具体业务 mock 数据**——dev-shell 不感知任何插件领域细节，新增演示数据一律改插件自己的 devMock 导出

---

## Android

- 包名：Desktop `com.bedcode.app`，Mobile `com.bedcode.mobile`
- **`gen/android` 重建后需恢复**：自定义 Kotlin 文件（ForegroundService.kt、ForegroundServicePlugin.kt、BiometricKeyPlugin.kt、PluginAssetExtractor.kt、DownloadsDirPlugin.kt、FileDeletePlugin.kt、SafPickerPlugin.kt、SafTransferPlugin.kt、DeviceInfoPlugin.kt、AllFilesAccessPlugin.kt、TaskNotificationPlugin.kt、TaskNotificationManager.kt、MulticastLockPlugin.kt）、AndroidManifest.xml、res/xml/（file_paths.xml、network_security_config.xml）、res/values/（colors.xml、themes.xml，含启动期窗口背景 `bedcode_launch_bg`，备份于 `src-tauri/android-backup/app-res/values/`）、key.properties、keystore、drawable 资源（OCR 相关 Kotlin 桥与 onnxruntime 已随 `feature/ocr-plugin` 隔离，见 `docs/knowledge/feature-branch-isolation.md`）
- **签名唯一真源**：仓库根 `bedcode.keystore`（alias `bedcode`，密码 `bedcode123`，SHA-256 `A8:5E:2F:1B:C5:...` = GitHub release 与 ANDROID_KEY_BASE64 secret 所用）。`gen/android/` 与 `android-backup/` 下的 keystore 必须是其副本；**勿用其他 keystore 签发布版**（换证书 = 已装用户只能卸载重装）。验证：`keytool -list -v -keystore <file> -storepass bedcode123 | grep SHA256` 须为 `A8:5E:2F:1B...`

---

## Git Hooks：分支级文档跟踪

受保护路径（`docs/`、`CLAUDE.md`、`CONTEXT.md`、`.pi` 配置、`.scratch/`）只在除 **uat/master** 外的分支入库；uat/master 仅从 index 剔除、不提交删除，工作区保留副本。`README*` 与 `AGENTS.md` 全分支正常跟踪。实现在 `scripts/doc-tracking.sh`，由 husky 钩子（pre-commit / post-checkout / post-merge）调用；启用：clone 后在根目录执行一次 `pnpm install`（husky v9 的 prepare 钩子自动安装）。

ESLint/Prettier 为全局单根配置（仓库根 `eslint.config.js` 等），同时覆盖两端前端；pre-commit 与 CI `lint.yml` 均从根目录运行 `eslint .`（仅 error 阻断）。

| 场景                             | 自动行为                                        |
| -------------------------------- | ----------------------------------------------- |
| dev / feature 提交               | 正常跟踪，hooks 不干预                          |
| uat / master `pre-commit`        | 仅从 index 剔除受保护文件（工作区保留）         |
| 切到 uat/master `post-checkout`  | 剔除 index 中受保护文件 + 从 dev 恢复工作区副本 |
| 合并落到 uat/master `post-merge` | 剔除合并带入的受保护文件，以暂存删除形式待提交  |

- `.pi/sessions/` 始终忽略不入库；新增 .pi 文件用 `git add -f .pi/<子路径>` 精确添加，**禁止 `git add -f .pi` 整目录**
- uat/master 上剔除产生的暂存删除随下次提交落库，不影响 dev / feature 分支
- dev→uat/master 合并产生 modify/delete 冲突时（hooks 在冲突时不运行）手动解决：`sh scripts/doc-tracking.sh untrack && git commit`
- 新增受保护路径：同步修改 `PROTECTED_PATHS` 和 `.gitignore`
- 环境变量：`DOC_UNTRACKED_BRANCHES`（默认 `uat master`）、`DOC_TRACKING_SOURCE`（默认 `dev`）

---

## Git Rules

- **禁止 commit message 中出现 AI 协作者标记（Co-Authored-By 等）**

### 分支推送约定

**dev 分支允许推送到远程 `origin/dev`，仅作本地 dev 的只读镜像备份**：

- `git push origin dev` ✅ 允许，用于本地 dev 的云端备份 / 多设备同步
- `git fetch origin dev` ✅ 允许，用于核对本地与远程 dev 是否一致
- `git pull origin dev` / `git merge origin/dev` / `git rebase origin/dev` ❌ 禁止——远程 dev 是只读镜像，不向本地 dev 汇入任何 commit
- 不允许对 `origin/dev` 开 PR；远程 dev 不接收任何外部 commit 流入
- **CI 隔离**：`origin/dev` 的 push 事件与对 `origin/dev` 的 PR 不触发任何 workflow（`.github/workflows/lint.yml` 的 `push` / `pull_request` 已加 `branches-ignore: [dev]`；`release.yml` / `sdk-publish.yml` 同样防御性忽略 dev）。理由：dev 是本地集成主线，CI 验证在 PR 合并到 master / uat 时由 lint.yml 接管
- 远程发布线仍为 master / uat；PR / 合并目标基线为 `master`（项目未设计 GitHub PR 流程，开发过程通过 `.scratch/<task>/` 文档记录）

**远程 dev 与本地 dev 出现分叉**（说明远程被错误写入 commit）时，立即停手与用户确认处理方式，禁止自动 `--force` 覆盖，避免掩盖问题。

### 文件回滚规范（强制）

回滚/撤销某文件的修改前，先 `git status <file>` + `git diff <file>` 确认其不含本次会话之外的未提交改动：

1. 含他人/其他任务在途改动的文件，**禁止 `git checkout -- <file>` / `git restore` 整文件回滚**（未提交内容无法从 git 恢复）
2. 正确做法：用 edit 工具逐段逆向替换，只精确还原本次修改的内容
3. 本次新增且非他人创建的独立文件可直接删除
4. 误用 `git checkout` 覆盖在途改动时立即停手上报（恢复源：`.pi/sessions/` 会话日志、`.scratch/` 交接文档），不得猜测重建

---

## Constraints

- 禁止屏幕宽度检测平台
- 禁止 composable 中文硬编码字符串
- 禁止无说明的陈旧注释代码（标注过的临时调试代码允许保留）
- 禁止 commit 中 AI 协作者标记
- 禁止 panic hook 中调用 `tracing::error!`
- 禁止无上下文的裸字符串错误
- 禁止在 dev-shell 中编写具体业务 mock 数据（插件演示种子一律放各自插件工程，见 Architecture Decisions 7）
- 前端 UI/样式改动必须先加载 `frontend-styles` skill 再动手

---

## Done When

- 修改的 Rust 代码 `cargo test` 通过
- 修改的前端代码 `pnpm run test:run` 通过
- 修改了 `gen/android` 下 Kotlin 代码的，`./gradlew :app:compileUniversalDebugKotlin` 通过
- i18n key 同步出现在 zh-CN 和 en
- 公开项有文档注释
- 错误处理使用 `AppError` 而非裸字符串
- 前端 UI 改动通过 `frontend-styles` 自查（token-bound、无原生控件外观、无反模式）

---

## Code Map（文件查找索引）

两端各有一份目录级代码地图（只到目录层级 + 模块职责）：桌面 `bedcode-desktop/docs/code-map.md`、移动 `bedcode-mobile/docs/code-map.md`。

**探索代码 / 了解结构 / 定位模块 / 查找功能实现位置时，必须先读对应端 code-map.md**，按 Project Structure → Core Modules → Quick Navigation 定位目标目录，再用 `ls` / `rg` 找具体文件。禁止未读 code-map 盲目全仓 grep。

维护规则：只到目录层级；顶层模块目录增删或核心职责变化时同步更新；描述与实际不符时以实际为准并顺手修正文档。

---

## Code Query Discipline（pi-lens 增强，仅 pi agent 生效）

**Gate：** 如果当前 agent 具备以下 pi-lens 工具（`lens_diagnostics` / `lsp_diagnostics` / `symbol_search` / `module_report` / `read_symbol` / `read_enclosing` 任一），走本节纪律；否则（Claude Code / Codex / Gemini / OpenCode 等）按上方 code-map 默认规范（`ls` / `rg` + `read`）执行。

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

## Agent skills

统一 skills 目录为项目根 **`.agents/skills/`**（唯一真源，git 跟踪）。当前 skills：`logo-generator`、`taste-skill-v1`（frontmatter name `design-taste-frontend-v1`）、`frontend-styles`、`scip-rebuild`。

- pi / OpenCode / Codex 原生读取 `.agents/skills/`，零配置
- Claude Code 只读 `.claude/skills/`：每台机器执行一次 `sh scripts/sync-skills.sh` 桥接（junction/symlink，幂等）
- 新增/修改 skill 直接操作 `.agents/skills/<name>/`，所有工具即时生效

### Issue tracker

Issues live as markdown files under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Five canonical roles: needs-triage, needs-info, ready-for-agent, ready-for-human, wontfix. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context — one `CONTEXT.md` + `docs/adr/` at the repo root. See `docs/agents/domain.md`.

### Subagents

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

### Vision subagent

`vision` 是唯一带视觉能力的 agent：读图与结构化分析，不改文件、不执行命令。主 agent 必须提供**图片文件绝对路径**（非 URL / 非 base64）。

#### 评审范围协议（强制）

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

### LSP 代码智能（lsp-pi 扩展，符号查询优先于文本搜索）

已安装 `git:github.com/leblancfg/lsp-pi` 包（用户级），提供 IDE 级语义查询：`references` / `definition` / `hover` / `symbols` / `signature` / `rename`。语言服务器：TypeScript/JS、Vue（`.vue` SFC）、Rust。

**自动诊断 hook 已禁用**（`lsp.hookMode: "disabled"`），不实时推送编译诊断；编译级验证仍以 `pnpm run test:run` / `cargo test` 为准，LSP 只用于代码理解与导航。

#### 何时必须用 LSP（行为规则）

涉及「某个函数/类型/常量/composable/store」的以下任务，**优先调 `lsp` 工具，禁止直接 bash 全目录搜索**：

| 任务场景                         | 用法                                                                            | 为什么不用 bash 搜索                                                                     |
| -------------------------------- | ------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| 改接口/签名/删除导出前评估影响面 | `lsp { action: "references", query: "<符号名>", file: "<任一使用处或定义处>" }` | `rg 符号名` 会命中注释、字符串、同名局部变量等大量误报，LSP 只返回真实引用（文件:行:列） |
| 从 import/调用点找真实实现       | `lsp { action: "definition", ... }`                                             | 跨文件 re-export、composable 解构后 rg 链路难追，LSP 一步到位                            |
| 确认函数参数/返回类型            | `lsp { action: "hover", ... }`                                                  | 一行返回类型+文档，不必读整个文件                                                        |
| 了解大文件结构再定点阅读         | `lsp { action: "symbols", file: "..." }`                                        | 先看大纲再 read 指定区域，省 token                                                       |
| 重构前列出将被改动的位置         | `lsp { action: "rename", ... }` + `newName` 试探                                | 返回完整改动清单供人工确认，不盲改                                                       |

判断口诀：**问「谁在用它/它是什么」→ LSP；搜「某段文案/日志字样/配置项」→ bash grep 才是对的**（字符串字面量、注释、日志 tag 不是符号，LSP 不适用）。

典型工作流：

```text
1. code-map.md 定位模块 → ls 看目录 → 打开入口文件
2. 对关键符号发 lsp hover/references 摸清类型与调用面
3. 按 references 结果定点 read 具体行段（非整文件）
4. 动手修改；验证走 pnpm run test:run / cargo test
```

示例：

```text
lsp { action: "references", query: "refreshTerminal", file: "bedcode-mobile/src/composables/usePeer.ts" }
lsp { action: "hover", query: "MessageBus", file: "bedcode-desktop/src-tauri/src/terminal_ws.rs" }
```

注意事项：

- rust-analyzer 首次查询需索引整个工程（30-60s+，一次性成本），之后毫秒级响应；会话内尽早发一次轻量查询预热
- 同名符号歧义用 `hover` 类型签名消歧
- `.vue` 由 vue-language-server 处理，`<script setup>` 内 ref/computed 引用可精确定位

### scipq — Rust 类型精确引用（SCIP 索引，无常驻进程）

rust-analyzer 已在 pi-lens 禁用（`disabledServers`，单实例 2GB+ 内存）。Rust 的「谁定义 / 谁引用 / 改它影响谁」用 SCIP 索引查询——一次性建索引、之后查询零内存：

```bash
.pi-lens/scip/scipq syms <片段>                # 符号全名/kind（先定位 symbol）
.pi-lens/scip/scipq defs <片段>                # 定义位置（精确 file:line:col）
.pi-lens/scip/scipq refs <片段>                # 引用（文件+行区段，秒级）
.pi-lens/scip/scipq refs-exact "<完整symbol>"  # 引用精确行列（2-5s）
```

**刷新策略（固定间隔或按需，不做每次代码变更的自动重建）**：按需 `scipq rebuild`；固定间隔 `scipq rebuild-if-stale [hours]`（默认 24h，间隔内零成本跳过）；状态自查 `scipq stale`。重建触发时机、执行步骤与验证标准见 `scip-rebuild` skill。role 语义（0=引用、1=定义）、格式与已知限制见 `.pi-lens/scip/README.md`。
