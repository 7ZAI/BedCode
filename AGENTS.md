# AGENTS.md

## Project Overview

BedCode 是局域网远程终端应用：桌面端作为主机运行终端会话（Claude Code 等），移动端作为远程终端控制，通过 WebSocket + HTTP 通信，当前适配两端同一 WiFi 场景。也可作为通用远程终端使用。

**Tech Stack:** Tauri 2.0 + Vue 3 + TypeScript + TailwindCSS + Rust (Tokio) + SQLite + vue-i18n@9

**Monorepo 结构:**
- `bedcode-desktop/` — 桌面端主机（Tauri + Vue 3）
- `bedcode-mobile/` — 移动端远程终端（Tauri + Vue 3）
- 各项目独立 `src/` (前端) 和 `src-tauri/` (Rust 后端)

---

## Code Exploration

项目根目录已有 `.codegraph/` 索引（预建知识图谱：全部符号、调用边、依赖，30+ 语言），**代码探索与改动前分析必须直接使用 CodeGraph 工具，禁止用 grep/read 循环重复推导结构**。explore 返回的源码视为已 Read，可直接 Edit。

### 核心原则（官方最佳实践）

- **`codegraph_explore` 是唯一主力工具**：接受自然语言问题或符号/文件名组合，一次返回相关符号的逐字源码（按文件分组）+ 调用路径（含 grep 追不上的动态分派：回调、事件、interface→impl）+ 影响面摘要。其他工具（node/search/callers/callees/impact）的信息已内联在 explore 的返回中，仅当 explore 不足以回答时才补用
- **信任结果，禁止用 grep 重新验证**——结果来自完整 AST 解析，grep 复检更慢、更不准且浪费上下文
- **编辑前后都用 explore**：改代码前先查目标符号（谁调用它、改它影响什么），改完后再查关联面
- **响应出现 `⚠️` staleness banner**：banner 列出的文件刚被编辑、索引尚未同步（滞后约 1s）——只对这些文件用 Read 取最新内容，banner 之外的文件仍然可信
- **「Already sent earlier in this conversation」是提示不是缺口**：本会话先前已返回过该文件且未变化——不要重新获取
- **不要将结构探索委派给读文件的 subagent**——subagent 重新读文件会重复 CodeGraph 已做的工作；仅当 subagent 自己也用 CodeGraph 时例外

### 工具选择

- 符号名不确定时，先用 `semble search "概念或描述" .` 定位符号
- 仅定位符号位置 → `codegraph_search`；调用/被调用关系 → `codegraph_callers` / `codegraph_callees`；超出 explore 影响面的深入分析 → `codegraph_impact`；单个符号完整源码或重载名 → `codegraph_node`
- Store/handler action（Pinia、route map 等）被索引为真实符号，直接用 `codegraph_explore`，无需通读整个 store 文件
- 字面量问题（字符串内容、注释、日志、配置文本）或 CodeGraph 不索引的内容（docs、配置文件）→ 原生 grep/read
- 无 `.codegraph/` 索引的项目 → 停止调用 CodeGraph，用内置工具

### 预算与边界

- 只读结构性问题默认最多 **2 次** CodeGraph 调用：`codegraph_explore` + 必要时一次 `codegraph_node(includeCode:true)`
- 首次调用已显示决定性类型/签名/关系时立即作答，不再二次调用
- 前两次结果冲突或用户明确要求更多证据时才用第三次调用
- 优先决定性边界符号（公共类型/schema、保存加载函数、请求构建器、命令/路由 handler、适配器），不深入工具函数/回调/UI 组件
- 避免重复获取同一符号源码；只读问题不运行 `git diff` / `git status`

## Build & Run

```bash
# Desktop Development
cd bedcode-desktop && npm run tauri:dev

# Desktop Build
cd bedcode-desktop && npm run tauri:build

# Mobile Development
cd bedcode-mobile && npm run tauri:android:dev

# Mobile Build
cd bedcode-mobile && npm run tauri:android:build

# Frontend Test（必须用 test:run，禁止 npm run test）
cd bedcode-desktop && npm run test:run

# Rust Test
cargo test

# Kotlin/Gradle 编译（gen/android 有 Kotlin 改动时必跑，非 Android 平台跳过）
cd bedcode-mobile/src-tauri/gen/android && ./gradlew :app:compileUniversalDebugKotlin
```

> **Kotlin 编译验证规范**：改 `gen/android/app/src/main/java/com/bedcode/mobile/` 下的自定义 Kotlin 插件（SafPickerPlugin/SafTransferPlugin 等）后，仅靠 `cargo test` / 前端测试**无法覆盖** Kotlin 代码（独立 Gradle + Kotlin 工具链），必须额外跑 `./gradlew :app:compileUniversalDebugKotlin`（离线模式加 `--offline`）验证编译。曾因漏验导致 `OpenableColumns.MIME_TYPE` 不存在等真实编译错误（见 `.scratch/lan-file-transfer-plugin/issues/08` Comments）。

> **前端测试规范**：`npm run test` 等于 `vitest`（watch 模式），执行完不退出、会一直挂着监听文件变化。
> 统一使用 `npm run test:run`（即 `vitest run`，一次性跑完并退出），也可直接 `npx vitest run`。
> 注意 `vite` 命令本身是 dev server / 构建工具，不执行测试，不能替代 vitest。

编译前检查 `src-tauri/target` 目录大小，超过 15GB 执行 `cargo clean`。

桌面端 `npm run tauri:build`（`scripts/tauri-build.js`）自动解析 updater 签名密钥（`TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_FILE` / `.env`）；未配置时自动禁用升级包生成（`createUpdaterArtifacts=false`），本地构建无需私钥。正式发布由 GitHub Actions Secrets 签名，密钥说明见 `docs/knowledge/release-workflow.md`。

---

## Rust (Backend)

### File Naming

Rust 文件均为 snake_case：模块入口文件与目录同名（`module.rs`），不使用旧式 `mod.rs`；测试文件 `*_test.rs`。

### Error Handling

- 使用 `AppError` 统一错误类型：`pub type Result<T> = std::result::Result<T, AppError>`
- 关键调用链用 `anyhow::Context` 添加上下文
- `tokio::spawn` 用 `spawn_with_error_boundary()` 包装
- panic hook 中禁止调用 `tracing::error!`，只使用 `eprintln!`
- 禁止 `unsafe impl Send/Sync`，禁止在重要路径上 `let _ =` 静默忽略错误
- 错误字符串应说明什么操作在哪失败，禁止无上下文的裸字符串

### Thread Safety

使用 `Arc<Mutex<T>>` 或 `Arc<RwLock<T>>` 进行状态共享，禁止 `unsafe impl Send/Sync`。

### Tauri Commands

命名规范：`list_*`（返回多个）、`get_*`（返回单个）、`create_*`、`delete_*`、`start_*` / `stop_*`（生命周期）。用 `// ====================` 分隔注释按领域分组。

### Logging

全部使用 `tracing`，日志级别：`debug!`（常规）、`info!`（关键）、`warn!`（警告/重试）、`error!`（异常）。Android 平台统一写 `tracing::` 宏，自动转发到 logcat。

**日志落盘（调试/排查用，主要为编程 agent 提供可查询路径）：**

| 端 | 行为 | 日志位置（电脑端） | 说明 |
|----|------|--------------------|------|
| 桌面端 | 始终落盘（dev/release 均写文件） | `%LOCALAPPDATA%\com.bedcode.app\logs\`（如 `C:\Users\<user>\AppData\Local\com.bedcode.app\logs\`） | `runtime.*.log` 全级别（dev 强制 debug）、`error.*.log` 仅 ERROR，按天轮转 |
| 移动端 | **电脑端落盘需用 `npm run tauri:android:dev:log`**（普通 `tauri:android:dev` 只打控制台）；release 走 logcat | `bedcode-mobile/.dev-logs/android-dev.YYYY-MM-DD.log`（本地日期，与设备 logcat 日期线一致） | 脚本把 Tauri CLI 转发的 logcat 同时写文件（无 ANSI 码，可 grep）；移动端进程在手机上，**无法直接写电脑磁盘**，故不设手机内部落盘（agent 读不到） |

确认实际路径：桌面端 dev 控制台首行 `Logging initialized. Log directory: ...`；移动端 `dev:log` 启动打印 `[dev-log] 电脑端日志落盘: <路径>`。

排查链路问题时优先看两端 `runtime.*.log`（含 DEBUG 级）：搜索 `file_service`、`peer_changed`、`MessageBus`、`reqwest::connect`（桌面端代理劫持痕迹 `proxy(...) intercepts`）等关键词。

---

## Frontend (Vue 3 + TypeScript)

### Component Structure

使用 `<script setup lang="ts">`。

### Composables

业务逻辑放在 composables，组件只负责 UI。命名：`use<Resource>` / `use<Action>`。

### Stores

全局状态使用 Pinia store 包装 composables。

### Platform Detection

使用 `@tauri-apps/plugin-os`，**禁止使用屏幕宽度检测桌面/移动端**。

### Styles & Layout（必读 skill，强制）

**任何前端 UI 改动（新建 Vue 组件、布局重构、CSS/Tailwind 类、design token、动画/过渡、深浅色主题、响应式适配、移动端安全区、字体/行高）都必须先加载 `frontend-styles` skill，并以其规范为准，禁止凭通用前端经验自行发挥。**

该 skill 给出：token-bound 取值优先级、class 书写顺序、safe-stack z-index 层级、过渡/动画规范、反模式清单、新组件 checklist；配套文件（`TOKENS.md` / `ANIMATIONS.md` / `MOBILE.md` / `BLUEPRINTS.md` / `I18N.md` / `PERFORMANCE.md` / `VUE3-STYLING.md` / `MODERN-CSS.md` / `LINTING.md`）位于 `.agents/skills/frontend-styles/`。

---

## UI 组件规范

**禁止使用系统原生 UI 控件外观**（原生 `<select>` 下拉、`<input type="checkbox/radio">`、
`<input type="date/datetime-local/time/range/color">` 等），移动端与桌面端一致适用。

例外（系统强关联，允许原生）：

- 文件选择弹窗（`@tauri-apps/plugin-dialog` / 系统文件选择器）
- 系统通知弹窗、系统授权弹窗（如 FsAuthDialog）
- 完全自绘外观的 input/textarea（外观 100% 由 CSS token 定制、无系统观感，如宿主 Input.vue）
- 隐藏原生控件仅作交互内核的自绘组件（如 Toggle.vue 内部的 checkbox）

正确做法（按优先级）：

1. 宿主/SDK 共享组件：宿主内部用 `src/components/`；插件用 SDK 子路径（桌面端
   `@binblink/plugin-sdk-desktop/ui`、移动端移动 SDK 的 `./ui`），禁止插件自实现一套
2. 成熟开源 Vue 组件（如 `@vuepic/vue-datepicker`），并用主题 token
   （`var(--bg-*)` / `var(--mobile-*)`）适配深浅色主题，禁止裸用默认样式
3. 自实现小型组件（自绘外观 + 原生交互内核），放入共享组件库（宿主或 SDK）供复用

新增共享组件须同时考虑桌面端与移动端（或至少放入对应 SDK 供插件引用）。

> 所有组件的视觉实现（颜色/圆角/阴影/间距/动画/响应式）一律遵循 `frontend-styles` skill 的 token 体系（token-bound），禁止硬编码视觉值。

---

## i18n

使用 vue-i18n@9 Composition API，zh-CN（默认）和 en。

- 翻译 key 命名：`{domain}.{section}.{key}`
- **新增 key 必须同时添加到 zh-CN 和 en**
- Vue 模板用 `$t()`，脚本用 `t()`（来自 `useI18n`）
- Composable（模块级代码）用 `i18n.global.t()`，不能用 `useI18n()`
- **composable 中禁止中文硬编码字符串**，状态变量存 i18n key，throw 中使用 i18n key
- 不翻译：代码注释、console 调试、终端输入、品牌名称

---

## Code Comments

- 注释解释为什么而非是什么
- Rust：模块级 `//!`，pub 项 `///`，内联 `//`
- TypeScript：文件头 `/** */`，export 项 JSDoc，Vue 组件 `<script setup>` 顶部加说明
- 分隔注释：`// ==================== Section ====================`
- 注释语言：中文，技术术语保留英文
- 禁止注释掉的代码，禁止冗余注释

---

## File Naming

| Type | Pattern |
|------|----------|
| Vue Component | PascalCase (`TitleBar.vue`) |
| Composable | camelCase with `use` prefix |
| Store | camelCase |

> Rust 文件命名见上文 `## Rust (Backend)` 的 File Naming。

---

## Architecture Decisions

1. Multi-Project Monorepo（各自独立 `src/` 和 `src-tauri/`）
2. Async Everywhere（Rust Tokio，前端 async/await + Tauri commands）
3. Event-Driven（PTY 输出通过 `broadcast` 通道分发）
4. Graceful Shutdown（`AtomicBool` 信号通知后台任务关闭）
5. Flat Module Structure（按领域扁平组织）
6. Plugin System（Rust API crate + 前端加载器双层架构）

---

## Android

- 包名：Desktop `com.bedcode.app`，Mobile `com.bedcode.mobile`
- `gen/android` 重建后需恢复自定义 Kotlin 文件（ForegroundService.kt、ForegroundServicePlugin.kt、BiometricKeyPlugin.kt、PluginAssetExtractor.kt、DownloadsDirPlugin.kt、FileDeletePlugin.kt、SafPickerPlugin.kt、SafTransferPlugin.kt、DeviceInfoPlugin.kt、AllFilesAccessPlugin.kt、TaskNotificationPlugin.kt、TaskNotificationManager.kt）、AndroidManifest.xml、key.properties、keystore、drawable 资源

---

## Git Hooks：分支级文档跟踪

开发过程文档与配置文件（docs/、CLAUDE.md、CONTEXT.md、.pi 配置、.scratch 等，见下）只在除 **uat / master** 外的分支入库（dev、feature/* 等全部正常跟踪）；uat / master 仅从 index 剔除、不提交删除，工作区始终保留副本。README 与 AGENTS.md 不受此限（全分支跟踪）。实现在 `scripts/doc-tracking.sh` + `scripts/hooks/`，通过 `core.hooksPath` 生效。

### 启用（clone 后每人执行一次）

```bash
git config core.hooksPath scripts/hooks
```

### 受保护路径

`docs/`、`CLAUDE.md`、`CONTEXT.md`、`.pi` 配置（`agents/`、`extensions/`、`prompts/`、`settings.json`）、`.scratch/`（issue 文档）。定义在 `scripts/doc-tracking.sh` 的 `PROTECTED_PATHS`，与 `.gitignore` 对应段落保持同步。

**`README.md` / `README_en.md` 与 `AGENTS.md` 不在受保护路径中，所有分支（含 uat/master）均正常跟踪。**

### 行为规则

| 场景 | 自动行为 |
|------|----------|
| dev / feature 等分支提交 | 正常跟踪，hooks 不干预（已跟踪文件不受 .gitignore 影响） |
| uat / master `pre-commit` | 仅从 index 剔除受保护文件（工作区保留），防止入库 |
| 切换分支 `post-checkout` | 切到 uat/master：剔除 index 中的受保护文件 + 从 dev 恢复工作区副本（仅供本地查阅） |
| 成功合并 `post-merge` | 合并落到 uat/master：剔除合并带入的受保护文件，以暂存删除形式待提交 |

- `.pi/sessions/` 会话日志**始终忽略、不入库**；新增 .pi 文件时用 `git add -f .pi/<子路径>` 精确添加，**禁止 `git add -f .pi` 整目录**
- uat/master 上剔除产生的暂存删除，随下次提交落库（或 `git commit -m 'chore: untrack docs'`）——仅影响该分支自身，不会反向影响 dev / feature 分支
- dev→uat/master 合并若产生 modify/delete 冲突（hooks 在冲突时不运行），手动解决：

```bash
sh scripts/doc-tracking.sh untrack && git commit
```

- 新增受保护路径：同时改 `PROTECTED_PATHS` 和 `.gitignore`
- 不跟踪分支黑名单可用环境变量 `DOC_UNTRACKED_BRANCHES` 覆盖（默认 `uat master`）；恢复工作区副本的源分支用 `DOC_TRACKING_SOURCE`（默认 `dev`）

---

## Git Rules

**禁止在 commit message 中添加 `Co-Authored-By: Claude ...` 行。**

### 文件回滚规范（强制）

回滚/撤销对某文件的修改前，必须先确认该文件**不包含本次会话之外的未提交改动**：

1. 检查：`git status <file>` + `git diff <file>`，并核对本次会话开始时的内容
2. **若文件包含他人/其他任务的在途改动（未提交），禁止 `git checkout -- <file>` / `git restore` 整文件回滚** —— 这会把无关的在途工作一并覆盖丢失（git 无法恢复未提交内容）
3. 正确做法：**只精确删除本次修改的内容**（用 edit 工具逐段逆向替换，恢复为本次修改前的原文），保留其余行原样
4. 本次新增的独立文件可直接删除（前提是确认非他人创建）
5. 误用 `git checkout` 覆盖了在途改动时，立即停手并如实上报（可能的恢复源：`.pi/sessions/` 会话日志中的 Read 输出、`.scratch/` 交接文档），不得自行猜测重建

> 教训案例：2026-08 移动端 wasmtime 组件迁移的在途未提交改动（loader.rs / manager.rs 等）曾因整文件 `git checkout` 被一并覆盖丢失，分支与 git 均无法恢复。

---

## Constraints

- 禁止 `unsafe impl Send/Sync`
- 禁止屏幕宽度检测平台
- 禁止系统原生 UI 控件外观（select/checkbox/radio/date/range 等；文件选择、系统通知、授权弹窗等系统强关联场景除外）
- 禁止 composable 中文硬编码字符串
- 禁止注释掉的代码
- 禁止 commit 中 AI 协作者标记
- 禁止 panic hook 中调用 `tracing::error!`
- 禁止无上下文的裸字符串错误
- 禁止 `.pi/sessions/` 会话日志入库（用 `git add -f .pi/<子路径>`，勿整目录添加）
- 前端 UI/样式改动（组件、布局、主题、动画）必须先加载 `frontend-styles` skill 再动手

---

## Done When

- 所有修改的 Rust 代码 `cargo test` 通过
- 所有修改的前端代码 `npm run test:run`（vitest run）通过
- 修改了 `gen/android` 下 Kotlin 代码的，`./gradlew :app:compileUniversalDebugKotlin` 通过
- i18n key 同步出现在 zh-CN 和 en 文件中
- 公开项有文档注释
- 错误处理使用 `AppError` 而非裸字符串
- 前端 UI 改动通过 `frontend-styles` 自查（token-bound、无原生控件外观、无反模式）

---

## Agent skills

### Skills 共享布局

统一 skills 目录为项目根 **`.agents/skills/`**（唯一真源，git 跟踪），供 pi / OpenCode / Codex / Claude Code 共享：

| 工具 | 读取方式 |
|------|----------|
| pi | 原生读取项目级 `.agents/skills/`（cwd 起向上到 git root），零配置 |
| OpenCode | 原生读取 `.agents/skills/`，零配置 |
| Codex | 原生读取 `.agents/skills/`（CWD → 父目录 → repo root），零配置 |
| Claude Code | 只读 `.claude/skills/`，需桥接链接，见下 |

当前 skills：`logo-generator`、`taste-skill-v1`（frontmatter name `design-taste-frontend-v1`）、`frontend-styles`。

**新增/修改 skill**：直接在 `.agents/skills/<name>/` 操作，所有工具自动生效（Claude Code 若已跑过桥接脚本，junction 指向同一目录也即时生效）。

**clone 后每台机器执行一次**（Claude Code 桥接）：

```bash
sh scripts/sync-skills.sh
```

脚本为 `.agents/skills/` 下每个含 `SKILL.md` 的目录在 `.claude/skills/` 创建链接：Windows 用目录 junction（`mklink /J`，无需管理员权限），Unix 用 symlink。幂等，可重复执行；`.claude/` 已在 `.gitignore`，链接不入库。

> 历史副本说明：`.pi/skills/` 下保留指向真源的 symlink 以兼容旧配置；`~/.claude/skills/frontend-styles` 为个人全局副本，与项目内同名 skill 共存时 Claude Code 以个人级优先，如需严格单一来源可删除个人副本。

### Issue tracker

Issues live as markdown files under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Five canonical roles: needs-triage, needs-info, ready-for-agent, ready-for-human, wontfix. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context — one `CONTEXT.md` + `docs/adr/` at the repo root. See `docs/agents/domain.md`.

### Subagents

pi 已安装 subagent 扩展（`.pi/extensions/subagent/`），可将任务委派给隔离上下文窗口中的专用 agent。项目 agent 定义在 `.pi/agents/*.md`。

**调用 `subagent` 工具时必须传 `agentScope: "both"`**（默认 "user" 不会加载本仓库的 agent）。

可用 agent：

| Agent | 用途 |
|-------|------|
| `scout` | 代码侦察，返回压缩上下文 |
| `planner` | 制定实现计划（只读） |
| `reviewer` | 代码审查（只读） |
| `worker` | 通用实现（完整能力） |
| `tester` | 运行测试并报告 |
| `vision` | 视觉分析（图片识别 / UI 评审 / 设计稿解读），详见下节 |

三种模式：
- 单任务：`{ agent, task, agentScope: "both" }`
- 并行：`{ tasks: [{ agent, task }, ...], agentScope: "both" }`（最多 8 个，4 并发）
- 链式：`{ chain: [{ agent, task }, ...], agentScope: "both" }`，步骤间用 `{previous}` 占位符传递输出

工作流 prompt 模板（`.pi/prompts/`）：`/implement`（scout → planner → worker）、`/scout-and-plan`（只出计划）、`/implement-and-review`（worker → reviewer → worker）、`/implement-and-test`（worker → tester）。

### Vision subagent

`vision` 是**唯一带视觉能力的 agent**（模型支持图像理解），主 agent 需提供**图片文件路径**（非 URL / 非 base64）。它不修改文件、不执行命令，仅做读图与结构化分析。

#### 评审范围协议(Scope Protocol)

主 agent 必须在 `task` 字符串中用 `范围:` 或 `scope:` 一行显式指定评审范围:

| 指令 | 行为 |
|------|------|
| `范围: 完整` / `scope: full` | 评审整张图(含外壳) |
| `范围: 手机内部` / `scope: phone` | 只评手机模拟器内 |
| `范围: 桌面应用内` / `scope: desktop` | 只评桌面应用窗口内 |
| `范围: 忽略外壳` / `scope: ignore-chrome` | 自动识别 dev-shell 外壳并只评内部 |
| `范围: <自由描述>` | 按描述执行 |

**未指定范围时**:vision 自动识别 dev-shell(适用于 BedCode `bedcode-mobile` / `bedcode-desktop` 的 dev-shell 调试壳),忽略外壳只评内部。**指令冲突时主 agent 优先**(主 agent 可能有 vision 看不到的上下文,如只想看 dev-shell 自身的 UI bug、只想看错误堆栈)。

完整协议见 `.pi/agents/vision.md` 的 "## 评审范围协议" 段。

#### 标准调用

```javascript
// 显式指定范围 — 评审手机内部
subagent(agent: "vision", agentScope: "both", task: `
  范围: 手机内部
  截图: <绝对路径>
  ...评审要求...
`)

// 显式完整 — 评审 dev-shell 自身(顶栏/手机框/控制面板)
subagent(agent: "vision", agentScope: "both", task: `
  范围: 完整
  截图: <绝对路径>
  请评审 dev-shell 调试壳的顶栏按钮对齐、手机框定位、四周留白。
`)

// 零配置 — 默认自动识别 dev-shell
subagent(agent: "vision", agentScope: "both", task: "请分析截图 <绝对路径>")
```

#### 截图准备

主 agent 需先截图再传路径给 vision,常用方式:
- Chrome headless 直连截图:`chrome.exe --headless=new --screenshot=/path/to/out.png --window-size=1440,900 <url>`
- `browser-tools` skill:`browser-screenshot.js` / `browser-content.js`(注意该 skill 的 `browser-start.js` 仅 macOS 可用,Windows 用 Chrome headless 替代)
- 已有图片文件:直接传绝对路径

#### 协助 Skills

`design-taste-frontend-v1`(位于 `.agents/skills/taste-skill-v1/`) — UI 截图 / 设计稿评审时由 vision 自动加载,提供品味基线(`VARIANCE=8` / `MOTION=6` / `DENSITY=4`)与 AI 套路识别清单(第 7 节)、五大硬性指标(第 3 节)、Pre-Flight 自查(第 10 节)。原 skill 面向 React/Next.js,vision 评审时自动映射到 Vue 3 + Tauri 栈。

#### 适用场景

- 错误截图诊断(Tauri / Vue 报错 → 定位问题)
- UI 截图评审(对照 `frontend-styles` + `design-taste-frontend-v1` 给出量化反馈)
- 设计稿解读(提取颜色 / 间距 / 字体 / 组件结构)
- 架构图 / 流程图解析(转文字描述 + 代码骨架)
- 代码截图转文字(截图里的代码提取为可编辑文本)
- 图标 / Logo 识别

输出格式固定:基础描述 → 详细分析 → Pre-Flight 自查 → 建议。详见 `.pi/agents/vision.md`。

适用场景：可并行的独立子任务、需要隔离上下文的重型任务。**结构/探索类任务不要委派**（见上 Code Exploration：subagent 重新读文件是重复劳动），直接自己用 CodeGraph 回答；简单的定位/小改动也直接用 codegraph 工具，不必启动 subagent。
