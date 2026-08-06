# AGENTS.md

## Project Overview

BedCode 是一个跨平台应用，桌面端作为主机，移动端作为远程终端，通过 WebSocket + HTTP 通信。也可作为通用远程终端使用。

**Tech Stack:** Tauri 2.0 + Vue 3 + TypeScript + TailwindCSS + Rust (Tokio) + SQLite + vue-i18n@9

**Monorepo 结构:**
- `bedcode-desktop/` — 桌面端主机（Tauri + Vue 3）
- `bedcode-mobile/` — 移动端远程终端（Tauri + Vue 3）
- 各项目独立 `src/` (前端) 和 `src-tauri/` (Rust 后端)

---

## Code Exploration

项目根目录已有 `.codegraph/` 索引（tree-sitter 解析的知识图谱），**探索代码优先使用 CodeGraph 工具**，取代人工维护的 `docs/code-map.md` 模块索引。

### 工具选择

- **几乎任何问题** — “X 如何工作”、架构、调用链（“X 如何到达 Y”）、浏览代码区域、改动前影响评估 → `codegraph_explore`（首选，接受自然语言问题，返回按文件分组的源码 + 紧凑的依赖影响面，一次调用通常足够）
- 仅定位符号位置 → `codegraph_search`
- 调用/被调用关系 → `codegraph_callers` / `codegraph_callees`
- 超出 explore 影响面的深入影响分析 → `codegraph_impact`
- 单个符号完整源码或重载名 → `codegraph_node`
- 符号名不确定时，先用 `semble search "概念或描述" .` 定位符号
- 字面量问题（字符串内容、注释、日志、配置文本）或已定位的小范围 → 原生 grep/read
- Store/handler action（Pinia、route map 等）被索引为真实符号，直接用 `codegraph_explore` / `codegraph_node`，无需通读整个 store 文件

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
```

> **前端测试规范**：`npm run test` 等于 `vitest`（watch 模式），执行完不退出、会一直挂着监听文件变化。
> 统一使用 `npm run test:run`（即 `vitest run`，一次性跑完并退出），也可直接 `npx vitest run`。
> 注意 `vite` 命令本身是 dev server / 构建工具，不执行测试，不能替代 vitest。

编译前检查 `src-tauri/target` 目录大小，超过 15GB 执行 `cargo clean`。

桌面端 `npm run tauri:build`（`scripts/tauri-build.js`）自动解析 updater 签名密钥（`TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_FILE` / `.env`）；未配置时自动禁用升级包生成（`createUpdaterArtifacts=false`），本地构建无需私钥。正式发布由 GitHub Actions Secrets 签名，密钥说明见 `docs/knowledge/release-workflow.md`。

---

## Rust Backend

### Module Organization

桌面端和移动端模块均按领域扁平组织在 `src/` 下，无中间层级。

桌面端核心模块：`commands/`、`db/`、`enums/`、`events/`、`plugin/`、`pty/`、`server/`、`session/`、`system/`、`utils/`

移动端核心模块：`auth/`、`commands/`、`connection/`、`enums/`、`handler/`、`model/`、`router/`、`system/`、`session.rs`、`state.rs`

### File Naming

模块入口文件与目录同名（`module.rs`），不使用旧式 `mod.rs`。

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

### Styles & Layout

修改 CSS / layout / animation / theme / 移动端安全区时，加载 `frontend-styles` skill。

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
| Rust module | snake_case |
| Rust test | `*_test.rs` |

---

## Security

- JWT 认证（HS256，7 天过期）
- QR Token 一次性使用，可配置 TTL
- 配对码 60 秒过期
- 设备指纹验证
- Plugin Token 用于 Claude Code hooks 认证
- 当前 WebSocket 通信未加密（`ws://`），端到端加密（X25519 + AES-GCM）计划中

---

## Architecture Decisions

1. Multi-Project Monorepo（各自独立 `src/` 和 `src-tauri/`）
2. Separation of Concerns（composables 处理 API，stores 管理状态，components 只做 UI）
3. Async Everywhere（Rust Tokio，前端 async/await + Tauri commands）
4. Event-Driven（PTY 输出通过 `broadcast` 通道分发）
5. Graceful Shutdown（`AtomicBool` 信号通知后台任务关闭）
6. Flat Module Structure（按领域扁平组织）
7. Plugin System（Rust API crate + 前端加载器双层架构）

---

## Android

- 包名：Desktop `com.bedcode.app`，Mobile `com.bedcode.mobile`
- `gen/android` 重建后需恢复自定义 Kotlin 文件（ForegroundService.kt、ForegroundServicePlugin.kt、BiometricKeyPlugin.kt、PluginAssetExtractor.kt、DownloadsDirPlugin.kt、FileDeletePlugin.kt、SafPickerPlugin.kt）、AndroidManifest.xml、key.properties、keystore、drawable 资源

---

## Git Hooks：分支级文档跟踪

文档与配置文件只在 **dev** 入库，其他分支（master / uat / milestone）自动剔除。实现在 `scripts/doc-tracking.sh` + `scripts/hooks/`，通过 `core.hooksPath` 生效。

### 启用（clone 后每人执行一次）

```bash
git config core.hooksPath scripts/hooks
```

### 受保护路径

`docs/`、`AGENTS.md`、`CLAUDE.md`、`CONTEXT.md`、`.pi` 配置（`agents/`、`extensions/`、`prompts/`、`settings.json`）、`.scratch/`（issue 文档）。定义在 `scripts/doc-tracking.sh` 的 `PROTECTED_PATHS`，与 `.gitignore` 对应段落保持同步。

### 行为规则

| 场景 | 自动行为 |
|------|----------|
| dev 提交 | 正常跟踪，hooks 不干预（已跟踪文件不受 .gitignore 影响） |
| 非跟踪分支 `pre-commit` | 静默剔除受保护文件，防止泄漏入库 |
| 切换分支 `post-checkout` | 剔除 index 中的受保护文件 + 从 dev 恢复工作区副本（仅供本地查阅） |
| 成功合并 `post-merge` | 剔除合并带入的受保护文件，以暂存删除形式待提交 |

- `.pi/sessions/` 会话日志**始终忽略、不入库**；新增 .pi 文件时用 `git add -f .pi/<子路径>` 精确添加，**禁止 `git add -f .pi` 整目录**
- 非跟踪分支上剔除产生的暂存删除，随下次提交落库（或 `git commit -m 'chore: untrack docs'`）
- dev→master 合并若产生 modify/delete 冲突（hooks 在冲突时不运行），手动解决：

```bash
sh scripts/doc-tracking.sh untrack && git commit
```

- 新增受保护路径：同时改 `PROTECTED_PATHS` 和 `.gitignore`
- 跟踪分支白名单可用环境变量 `DOC_TRACKING_BRANCHES` 覆盖（默认 `dev`）

---

## Git Rules

**禁止在 commit message 中添加 `Co-Authored-By: Claude ...` 行。**

---

## Constraints

- 禁止 `unsafe impl Send/Sync`
- 禁止屏幕宽度检测平台
- 禁止 composable 中文硬编码字符串
- 禁止注释掉的代码
- 禁止 commit 中 AI 协作者标记
- 禁止 panic hook 中调用 `tracing::error!`
- 禁止无上下文的裸字符串错误
- 禁止 `.pi/sessions/` 会话日志入库（用 `git add -f .pi/<子路径>`，勿整目录添加）
- 修改样式时加载 `frontend-styles` skill

---

## Done When

- 所有修改的 Rust 代码 `cargo test` 通过
- 所有修改的前端代码 `npm run test:run`（vitest run）通过
- i18n key 同步出现在 zh-CN 和 en 文件中
- 公开项有文档注释
- 错误处理使用 `AppError` 而非裸字符串

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

| Agent | 用途 | 模型 |
|-------|------|------|
| `scout` | 代码侦察，返回压缩上下文 | deepseek-v4-flash |
| `planner` | 制定实现计划（只读） | deepseek-v4-flash |
| `reviewer` | 代码审查（只读） | deepseek-v4-flash |
| `worker` | 通用实现（完整能力） | deepseek-v4-flash |
| `tester` | 运行测试并报告 | deepseek-v4-flash |

三种模式：
- 单任务：`{ agent, task, agentScope: "both" }`
- 并行：`{ tasks: [{ agent, task }, ...], agentScope: "both" }`（最多 8 个，4 并发）
- 链式：`{ chain: [{ agent, task }, ...], agentScope: "both" }`，步骤间用 `{previous}` 占位符传递输出

工作流 prompt 模板（`.pi/prompts/`）：`/implement`（scout → planner → worker）、`/scout-and-plan`（只出计划）、`/implement-and-review`（worker → reviewer → worker）、`/implement-and-test`（worker → tester）。

适用场景：大范围代码调查、可并行的独立子任务、需要隔离上下文的重型任务。简单的定位/小改动直接用 codegraph 工具即可，不必启动 subagent。
