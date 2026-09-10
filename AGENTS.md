# AGENTS.md

## 0. 规则优先级与冲突裁决

当文档/规则冲突时，按下述优先级裁决：

1. **用户当前明确指令**（最新指令优先于一切文档规则）
2. **安全与架构红线**（§8 安全红线、§5 高内聚低耦合等）——不可被普通任务越过；确实需要越线时停下向用户确认，禁止自行放松
3. **本文档硬约束**（"必须/禁止"字眼，下同）
4. **skill 规范**（`frontend-styles` 等在对应场景强制）
5. **code-map / 领域文档**（含 docs/、docs/adr/）
6. **通用工程经验**

**最小改动原则**：只改任务必要文件；禁止顺手重构相邻代码、擅自升级依赖版本（升级先做双端影响评估，如 wasmtime / SDK）；不确定的设计取舍先问用户，不猜。

---

## 1. 项目速览

BedCode：局域网远程终端应用——桌面端作为主机运行终端会话，移动端作为远程终端控制，WebSocket + HTTP 通信，适配两端同一 WiFi 场景。

**Tech Stack:** Tauri 2.0 + Vue 3 + TypeScript + TailwindCSS + Rust (Tokio) + SQLite + vue-i18n@9 + wasmtimer + WASM（wasmtime Component Model）

**Monorepo 结构：**

- `bedcode-desktop/`（桌面主机）、`bedcode-mobile/`（移动远程终端）——各自独立 `src/`（前端）与 `src-tauri/`（Rust 后端），各自维护 `pnpm-lock.yaml`
- `plugins/<plugin-id>/` 插件工程、`packages/plugin-sdk-*/` 插件 SDK（WIT 契约）

---

## 2. 环境与工具链

| 项 | 版本/要求 | 来源 |
| --- | --- | --- |
| 包管理器 | **pnpm**（根与两端 `packageManager: pnpm@12.2.1`），全局禁止 npm | package.json |
| Node | LTS（CI 用 `lts/*`；无 `.nvmrc`，本地对齐 LTS） | CI workflows |
| Rust | stable + edition 2021（无 `rust-toolchain.toml`，与 CI `dtolnay/rust-toolchain@stable` 对齐） | CI workflows |
| Tauri | 2（两端） | src-tauri/Cargo.toml |
| wasmtime | **47，两端锁死**，升级必须双端同步（ADR 0019） | src-tauri/Cargo.toml |
| 版本号 | 桌面/移动 package.json 与 Cargo.toml **同步维护**；变更记录根 `CHANGELOG.md` | 仓库现状 |
| Android | JDK/Gradle 由 `gen/android` 分发包维护；SDK/NDK 随其管理 | — |

---

## 3. 黄金命令（构建 / 测试 / 运行）

```bash
# Desktop Development / Build
cd bedcode-desktop && pnpm run tauri:dev
cd bedcode-desktop && pnpm run tauri:build

# Mobile Development / Build
cd bedcode-mobile && pnpm run tauri:android:dev        # 开发
cd bedcode-mobile && pnpm run tauri:android:dev:log    # 落盘日志（见 logging.md）
cd bedcode-mobile && pnpm run tauri:android:build      # 发布（aarch64）

# Frontend Test —— 统一 pnpm run test:run（= vitest run，跑完退出）
# 禁止 pnpm run test（vitest watch 挂起不退出）；vite 不执行测试
cd bedcode-desktop && pnpm run test:run
cd bedcode-mobile && pnpm run test:run

# Rust Test（桌面/移动各在各自 src-tauri 下）
cd bedcode-desktop/src-tauri && cargo test
cd bedcode-mobile/src-tauri && cargo test

# Kotlin/Gradle 编译（改 gen/android 下 Kotlin 代码后必跑；离线加 --offline）
cd bedcode-mobile/src-tauri/gen/android && ./gradlew :app:compileUniversalDebugKotlin

# Lint（根目录，覆盖两端前端；0 error 门禁，warning 暂不计入）
pnpm exec eslint .
```

- **前端可单测过滤**：`pnpm exec vitest run <测试文件路径>`（测试文件在 `src/__tests__/`，两端同构）
- **Rust 可单测过滤**：`cargo test <名称前缀>`
- **Kotlin 独立工具链**：上述 gradlew 命令是 `gen/android` 下 Kotlin 改动的唯一验证（`cargo test` 与前端测试均不覆盖）
- **文档命令字眼必须随工具链迁移**：spec / issue / scratch / 知识库文档里提及测试/构建/安装命令，**必须**用本节字眼（`pnpm run test:run`、`pnpm run tauri:dev`、`cargo test` 等），禁止旧 `npm` / `npm run test` 字眼；审计文档时若发现不一致，先改文档再继续
- 构建前检查 `src-tauri/target` 大小，超 15GB 执行 `cargo clean`
- 桌面 `tauri:build` 自动解析 updater 签名密钥（`TAURI_SIGNING_PRIVATE_KEY(_FILE)` / `.env`），未配置时自动禁用升级包，本地构建无需私钥；正式发布由 GitHub Actions Secrets 签名（`docs/knowledge/release-workflow.md`）

---

## 4. 任务路由：改 X 先读 Y

| 任务 | 动手前必读 |
| --- | --- |
| 改前端 UI / 样式 / 布局（组件、CSS、token、动画、主题、响应式） | **先加载 `frontend-styles` skill**（`.agents/skills/frontend-styles/SKILL.md`，强制）+ 对应端 code-map |
| 改 Rust 后端（任意模块） | 对应端 code-map → 模块目录 → §6 Rust 规范 + 相关 ADR（docs/adr/） |
| 改插件 | §7 插件检查清单 + WIT（`packages/plugin-sdk-*/rust/wit/bedcode.wit`）+ ADR 0017/0019/0022 |
| 改数据库 / schema | §9 数据规范 + `bedcode-desktop/src-tauri/src/db/` |
| 改跨端协议（HTTP/WS/QR/认证） | §9 协议规范 + `docs/knowledge/mobile-desktop-auth.md`，两端同步评估 |
| 排查日志 / 无日志问题 | `docs/knowledge/logging.md` + `.scratch/adb-fd0-bug/bug-report.md` |
| 启动多任务 / 需要规划 | `.scratch/<task>/` 记录（项目未设计 GitHub PR 流程，开发过程文档走这里） |
| 定位代码 | §12 代码查找纪律 |

---

## 5. 架构硬约束

**目标：无业务内核（Businessless Kernel）**——底座内核只含「应用无关的通用引擎」：进程（PTY）、网络（HTTP/WS/mDNS）、存储（SQLite/文件）、安全（JWT/密钥/TLS/信任）、通信（消息总线/插件互调）+ wasmtime 运行时。一切产品概念（会话、终端、设备连接、文件传输、AI……）都是插件。演进路线的阶段划分见 `.scratch/plugin-kernel-roadmap/spec.md`；终态愿景见 `.scratch/platform-kernel/spec.md`。

**架构红线（强制）：**

- **高内聚、低耦合**：内核只做引擎原语与安全边界，禁止携带产品语义；业务代码内聚到各自插件工程；插件间只经互调 API（ADR 0017）与消息总线通信，**禁止跨插件直接耦合**
- **新增能力优先评估「放哪个插件」而非「改内核」**；核心业务（会话/终端/设备连接/认证）暂留宿主侧，按上述路线逐步下沉
- **裁剪线（ADR 0022）**：宿主能力只暴露「离宿主无法实现、且无业务语义」的原语；业务编排一律在插件层
- 技术决策记录在 `docs/adr/`（Multi-Project Monorepo / Async Everywhere / Event-Driven / Graceful Shutdown / Flat Module Structure / Plugin System / 无业务内核 / 插件 Mock 归属），新增决策走 ADR

---

## 6. 代码规范

### Rust

- 文件命名 snake_case；模块入口文件与目录同名（`module.rs`），不用 `mod.rs`；测试文件 `*_test.rs`
- 统一错误类型 `AppError`：`pub type Result<T> = std::result::Result<T, AppError>`
- 关键调用链在错误构造/转换处带操作描述（`AppError::X(format!(...))`、`io::Error` 自描述包装或 `anyhow::Context` 跨桥），**禁止裸 `?` 透传无上下文错误**（含 `io::Result` 契约内）；`tokio::spawn` 用 `spawn_with_error_boundary()` 包装；重要路径禁止 `let _ =` 静默忽略错误
- panic hook 中只用 `eprintln!`，禁止 `tracing::error!`
- 状态共享用 `Arc<Mutex<T>>` / `Arc<RwLock<T>>`
- Tauri Commands 命名：`list_*`（多个）、`get_*`（单个）、`create_*`、`delete_*`、`start_*` / `stop_*`（生命周期）；用 `// ====================` 分隔注释按领域分组

### Frontend（Vue 3 + TypeScript）

- **任何 UI 改动（组件、布局、CSS/Tailwind 类、design token、动画/过渡、主题、响应式、安全区、字体/行高）必须先加载 `frontend-styles` skill 并以其规范为准**，禁止凭通用前端经验自行发挥
- **禁止用 viewport 宽度 / UA 字符串推断平台**；平台判断统一走 Tauri API（如 `@tauri-apps/plugin-os` 的 `platform()`），两端渲染容器不一致时以 API 为准
- **前端错误处理**：统一 `logger`（`logger.error/info` 带上下文），禁止静默 `catch`；用户可见错误/状态文案一律走 i18n，禁止 composable / 组件内硬编码中文字符串
- **i18n**：文件位于两端 `src/locales/{zh-CN,en}/`；新增/修改 key 必须同步出现在 zh-CN 与 en 两文件，命名跟随既有分组；复数/日期/数字走 vue-i18n 机制

### 注释与命名（通用）

- 注释解释为什么而非是什么；语言中文，技术术语保留英文
- Rust：模块级 `//!`、pub 项 `///`、内联 `//`；TS：文件头 `/** */`、export JSDoc、Vue 组件 `<script setup>` 顶部说明；分隔注释 `// ==================== Section ====================`
- 注释掉的代码必须标注意图与恢复方式（如 `/* 临时调试：排查 X，恢复时取消注释 */`）；无说明的陈旧注释代码一律删除
- 文件命名：Vue 组件 PascalCase（`TitleBar.vue`）、Composable camelCase 带 `use` 前缀、Store camelCase

---

## 7. 插件开发检查清单

插件位于 `plugins/<plugin-id>/`（独立 package：`plugin.json` + `rust/` WASM 后端 + `src/` TS 前端 + `vite.config.ts`）。开发/修改插件逐项核对：

- [ ] manifest 声明 `permissions`（前端快速失败 + Rust 端最终仲裁；文件系统走 fs_auth 三层校验：路径白名单 → 插件白名单 → 弹窗授权）
- [ ] 对外可调 API 在 manifest `api` 字段声明，经 `#[plugin_api]` 宏 + JSON-RPC 2.0；**未声明不可调**（ADR 0017）
- [ ] 契约边界单点维护在 WIT（`packages/plugin-sdk-*/rust/wit/bedcode.wit`）；改 WIT 必须双端同步 + ABI bump（wasmtime 47 两端锁死）
- [ ] 宿主能力经 `host-*` 原语访问（进程/网络/存储/安全/通信），能力**不得携带业务语义**（ADR 0022）
- [ ] 插件导出：`activate`/`deactivate`、`command`、`_http_endpoint`、terminal hooks、生命周期/输入扩展点
- [ ] 存储：插件独立库（私有 SQLite）/ 主库前缀隔离（表名强制 `plugin_id_` 前缀）；**禁止在 dev-shell 写具体业务 mock**——mock 数据/演示种子归各自插件工程（插件入口导出 `devMock`）
- [ ] 日志：target=`bedcode_lib::plugin::plugin_log`，`[plugin:xxx]` 前缀，WASM trap backtrace 不得关闭（详情见 `docs/knowledge/logging.md`）

---

## 8. 安全、日志与可观测性红线

### 安全红线（不可违反）

- **禁止提交密钥/凭据**：仓库内唯一例外是签名真源 `bedcode.keystore`（私有仓库设计，见 §9 Android）；新增的任何密钥、token、密码禁止入库、禁止进日志、禁止写进文档/备注；API token 泄露按仓库规范删除重建
- 认证链路（JWT / 设备指纹 / 二维码 / 生物凭证）只走既有 auth 模块，禁止旁路；**日志与存储中凭据只记长度不落明文**（`token.length()` 模式）
- 输入校验与权限仲裁在 Rust 端，前端校验仅是 UX；WebSocket/HTTP 接入必须过认证与过滤链（TrafficFilterChain）

### 日志红线

统一 `tracing`（Android 自动转发 logcat）。级别语义：

| 级别 | 适用场景 | 反例（禁止） |
| --- | --- | --- |
| `debug!` | 常规运行细节：连接/订阅建立、请求进出、状态迁移、过滤命中/放行 | 热路径逐帧日志（PTY 输出、WS 每帧）——高频循环内克制 |
| `info!` | 关键生命周期：启动完成、服务器起停、会话创建/销毁、WS 连接/断开、配置保存 | 每个请求都打 info（高频 API 走 debug） |
| `warn!` | 可恢复异常/重试：过滤拒绝、授权降级、超时重试、缓存禁用、队列丢弃 | 已由调用方处理的正常分支 |
| `error!` | 不可恢复异常：操作失败且影响功能；`AppError` 传播点 | panic hook 内（只用 `eprintln!`）；guest 自报的可处理错误 |

- **结构化字段（强制）**：`session_id` / `device_id` / `plugin_id` / `request_id` / `batch_id` / `node_id` 一律 `key = %value` 字段形式，**禁止拼进消息字符串**；消息只写人类可读描述（中文 + 英文术语），错误信息必须带操作上下文
- 落盘机制 / 排障（non_blocking 缓冲、日志路径、移动端无日志排查、插件 WASM 日志细节）见 `docs/knowledge/logging.md`

---

## 9. 数据、协议与产物

### 数据库（SQLite）

- **主库 schema 单一事实源**：`bedcode-desktop/src-tauri/src/db/schema.sql`；列级迁移写在 `database.rs::run_migrations()`，**迁移必须幂等**（可对旧库重跑），禁止手改生产库；改 schema 必须补迁移幂等测试
- 插件存储隔离见 §7（独立库 / 主库 `plugin_id_` 前缀）
- 测试数据：Rust 走临时目录 + `with_default`（日志），禁止污染真实数据/日志目录

### 网络协议 / 跨端兼容

- 协议（HTTP / WS / QR 配对 / 认证）改动**必须两端同步部署**（桌面主机 + 移动端），字段演进遵循「老端忽略未知字段」的增量原则，禁止破坏性替换
- 认证/配对协议文档：`docs/knowledge/mobile-desktop-auth.md`；宿主/插件契约见 §7 WIT
- wasmtime 版本升级必须两端同步（ADR 0019）

### 产物与生成文件

- **禁止提交**：`**/target/`、`node_modules/`、`.dev-logs/`、Android 构建产物（`build/`、`.gradle/`、`.cxx` 等）
- **锁文件**：`Cargo.lock` / `pnpm-lock.yaml` 只经包管理器变更（`pnpm install` / `cargo update`），**禁止手工编辑**
- **`gen/android` 例外**：`app/src/main/java/com/bedcode/mobile/*.kt` 等手写 Kotlin 源码是版本跟踪的一部分，`tauri android init` 重建后需手工恢复；改 Kotlin 后必须跑 gradlew 验证（§3）

### Android 发布

- 包名：Desktop `com.bedcode.app`，Mobile `com.bedcode.mobile`
- **签名唯一真源：仓库根 `bedcode.keystore`**。`gen/android/` 与 `android-backup/` 下的 keystore 必须是其副本；**勿用其他 keystore 签发布版**
- 版本号两端同步维护（§2）；发布流程见 `docs/knowledge/release-workflow.md`

---

## 10. 完成定义与验证证据

以下命令**必须实际运行并贴出结果**；无法运行（环境缺失 / 平台限制）必须说明原因与风险：

- 改了 Rust → `cargo test` 通过（两端各自）
- 改了前端 → `pnpm run test:run` 通过（对应端）
- 改了 `gen/android` 下 Kotlin → `./gradlew :app:compileUniversalDebugKotlin` 通过
- 改了前端 → 根目录 `pnpm exec eslint .` 0 error（warning 不计入）；`cargo fmt` / `cargo clippy` 提交前自查（非 CI 门禁）
- i18n key 同步出现在 zh-CN 和 en
- 公开项有文档注释；错误处理用 `AppError` 而非裸字符串
- 前端 UI 改动通过 `frontend-styles` 自查（token-bound、无原生控件外观、无反模式）
- pi agent：收尾 `lens_diagnostics mode=all` 无 blocker（🔴 blocker 未清前不算 done）

CI 门禁（合并到 master/uat 时）：`lint.yml`（eslint 0 error）+ `test.yml`（两端 cargo test + vitest）。

---

## 11. 提交、回滚与 Git 规则

### 提交与分支

- **禁止 commit message 中出现 AI 协作者标记（Co-Authored-By 等）**
- 格式：conventional commits `<type>(<scope>): <subject>`；type ∈ feat/fix/docs/refactor/chore/test/perf，scope 常用 desktop / mobile / scratch / sdk
- 分支：`dev` 为本地集成主线；`feature/*` 开发；`uat` / `master` 为远程发布线。开发过程通过 `.scratch/<task>/` 文档记录（项目未设计 GitHub PR 流程）
- **CI 隔离**：`origin/dev` 的 push 事件与对 `origin/dev` 的 PR 不触发任何 workflow（lint/test/release/sdk-publish 均忽略 dev）；CI 验证由合并到 master/uat 时的 lint.yml / test.yml 接管。PR / 合并目标基线为 `master`
- 远程 dev 与本地 dev 出现分叉时，**立即停手与用户确认处理方式，禁止自动 `--force` 覆盖**
- 推送前过 pre-commit 钩子（husky）：eslint（根目录）+ 分支级文档跟踪

### 分支级文档跟踪（Git Hooks）

受保护路径（`docs/`、`CLAUDE.md`、`CONTEXT.md`、`.pi` 配置、`.scratch/`）只在除 **uat/master** 外的分支入库：uat/master 仅从 index 剔除、不提交删除，工作区保留副本。`README*` 与 `AGENTS.md` 全分支正常跟踪。实现在 `scripts/doc-tracking.sh`（husky pre-commit / post-checkout / post-merge 调用）。

| 场景 | 行为 |
| --- | --- |
| dev / feature 提交 | 正常跟踪，hooks 不干预 |
| uat / master `pre-commit` | 仅从 index 剔除受保护文件（工作区保留） |
| 切到 uat/master `post-checkout` | 剔除 index 中受保护文件 + 从 dev 恢复工作区副本 |
| 合并落到 uat/master `post-merge` | 剔除合并带入的受保护文件，以暂存删除形式待提交 |

- `.pi/sessions/` 始终忽略不入库；`.pi/` 整目录被根 .gitignore 忽略，新增 .pi 文件必须 `git add -f .pi/<子路径>` 精确添加，**禁止 `git add -f .pi` 整目录**
- `docs/`（含两端 `bedcode-desktop/docs`、`bedcode-mobile/docs`）**不受 .gitignore 忽略**，新文档文件正常 `git add`；其在 uat/master 的剔除完全由上表 hooks 负责
- dev→uat/master 合并产生 modify/delete 冲突时（hooks 在冲突时不运行）手动解决：`sh scripts/doc-tracking.sh untrack && git commit`
- 新增受保护路径：同步修改 `scripts/doc-tracking.sh` 内 `PROTECTED_PATHS`；默认忽略类条目（如 CLAUDE.md / CONTEXT.md）另加 `.gitignore`；env：`DOC_UNTRACKED_BRANCHES`（默认 `uat master`）、`DOC_TRACKING_SOURCE`（默认 `dev`）

### 文件回滚规范（强制）

回滚/撤销某文件的修改前，先 `git status <file>` + `git diff <file>` 确认其不含本次会话之外的未提交改动：

1. 含他人/其他任务在途改动的文件，**禁止 `git checkout -- <file>` / `git restore` 整文件回滚**（未提交内容无法从 git 恢复）
2. 正确做法：用 edit 工具逐段逆向替换，只精确还原本次修改的内容
3. 本次新增且非他人创建的独立文件可直接删除
4. 误用 `git checkout` 覆盖在途改动时立即停手上报（恢复源：`.pi/sessions/` 会话日志、`.scratch/` 交接文档），不得猜测重建

---

## 12. 代码查找纪律

两端各有一份目录级代码地图：桌面 `bedcode-desktop/docs/code-map.md`、移动 `bedcode-mobile/docs/code-map.md`。

**探索代码 / 定位模块 / 查找功能实现，必须先读对应端 code-map.md**，按 Project Structure → Core Modules → Quick Navigation 定位目标目录，再用 `ls` / `rg` 找具体文件。禁止未读 code-map 盲目全仓 grep。

维护规则：只到目录层级；顶层模块目录增删或核心职责变化时同步更新；描述与实际不符时以实际为准并顺手修正文档。

> pi agent 增强：见附录的 pi-lens 纪律（`docs/agents/pi-tools.md`）。

---

## 13. 文档索引

| 需求 | 入口 |
| --- | --- |
| 命令参考 | `docs/commands.md` |
| 代码地图 | `bedcode-desktop/docs/code-map.md` / `bedcode-mobile/docs/code-map.md` |
| 领域模型 / 术语 | 根 `CONTEXT.md`（单上下文）+ `docs/adr/`，规范见 `docs/agents/domain.md` |
| Issue tracker | issues 为 `.scratch/` 下的 markdown，见 `docs/agents/issue-tracker.md` |
| Triage 标签 | needs-triage / needs-info / ready-for-agent / ready-for-human / wontfix，见 `docs/agents/triage-labels.md` |
| 发布流程 | `docs/knowledge/release-workflow.md`（桌面 updater / 移动发布）、`docs/knowledge/sdk-publish.md`（SDK 发布） |
| 日志 / 排障 | `docs/knowledge/logging.md`、`.scratch/adb-fd0-bug/bug-report.md` |
| 插件 WASM 日志 spec | `.scratch/plugin-wasm-logging/spec.md` |
| 架构路线 | `.scratch/plugin-kernel-roadmap/spec.md`、`.scratch/platform-kernel/spec.md` |
| pi 工具手册 | `docs/agents/pi-tools.md`（附录） |

---

## 附录：pi 工具专属（仅 pi agent）

pi-lens 代码查询纪律（三阶段漏斗、符号级查询、诊断收尾）、subagents 编排、vision 视觉 subagent、scipq Rust 精确引用 —— 完整手册见 **`docs/agents/pi-tools.md`**。非 pi agent（Claude Code / Codex / Gemini / OpenCode）按 §12 code-map 默认规范执行。
