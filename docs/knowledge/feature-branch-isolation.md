# 功能分支隔离与 dev 清理实施操作文档

> 目的：为将来发布版本与功能独立开发，将「移动端 OCR 插件」「桌面端定时调度插件」「桌面端代码查看组件」三个功能各自隔离到独立功能分支，回到 `dev` 删除各功能的相关代码与文档，保证 `dev` 提交干净。
>
> 本文档是按步骤可执行的 runbook。执行前请先读 `docs/knowledge/release-workflow.md`（发布流程）与 `AGENTS.md`（Git Rules / Git Hooks / 回滚规范），本文档的命令字眼与仓库工具链保持一致。

---

## 0. 背景与目标

### 0.1 三个待隔离功能

| # | 功能 | 端 | 形态 | 分支建议名 |
|---|------|----|------|-----------|
| 1 | OCR 插件（`com.bedcode.ocr`，离线 PP-OCRv4 识别） | 移动端 | 已实现：WASM 插件壳 + 宿主 Rust 引擎 + Kotlin 桥 + onnxruntime 模型 | `feature/ocr-plugin` |
| 2 | 定时调度插件（`com.bedcode.scheduler`，cron 触发 shell 脚本/内联命令，CLI 入口 bedtask） | 桌面端 | 已实现：插件 + Rust 调度引擎 + CLI + 只读面板 | `feature/task-scheduler` |
| 3 | 代码查看组件（Code Viewer，只读文件树 + 标签页，嵌入终端窗口） | 桌面端 | **未开发，仅文档**：spec + ADR 0023/0024 + CONTEXT 词汇 + 5 张实施票据 | `feature/code-viewer` |

> **假设确认**：「定时调度」指 `com.bedcode.scheduler`（计划任务插件）。若实际意图是 `auto-task` 插件的定时任务子功能，请先暂停——auto-task 两端深度集成且在活跃开发中，隔离成本与影响面完全不同，需要另行评估。

### 0.2 目标状态

- `dev`：三个功能相关代码/文档全部删除，`cargo test`、`pnpm run test:run`、Kotlin 编译、两端构建全绿
- 三个功能分支：各自包含该功能的完整产物（代码 + spec/ADR/票据），可独立构建、测试、继续开发
- 未来发布：功能分支按发布节奏合入 `master`（发布基线），`dev` 不夹带未发布功能

### 0.3 先例

`feature/disk-cleaner-issue-04`：disk-cleaner 插件代码已隔离到该分支（基座 `92475c13`），`dev` 上无插件代码、仅保留 `.scratch/disk-cleaner-plugin/` 交接文档。**教训**：该分支与 `dev` 已漂移约 1.1 万行差异，合并成本高——新分支务必定期同步 `dev`（见 §7）。

---

## 1. 现状快照（执行时需重新核对）

| 项 | 当前值（2026-09 快照） |
|----|----------------------|
| 当前分支 | `dev`，HEAD `2289b441` |
| 应用版本 | 两端 `2.0.10`（`src-tauri/tauri.conf.json` 与 `Cargo.toml`） |
| 最新发布 tag | `v2.0.0`（`02c91d95`）——**不包含**上述三个功能 |
| dev 工作区 | **存在大量未提交改动**（file-transfer / peer-net / 加密 / 主题等 WIP） |
| 发版决策（已确认） | **可能直接发布 v2.1.0**（改动太大太多）；隔离操作在发版之后择机执行，版本节点取 `v2.1.0` 发布 tag |

关键事实：三个功能的代码/文档**只存在于 `dev` 历史中**，所有旧 tag（`v2.0.0`/`v1.1.0`）均不含。因此分支必须基于**当前 dev 的版本节点**创建，而不是旧 tag。

---

## 2. 版本节点选择

**已确认方案：以 `v2.1.0` 发布节点为版本节点（发版先行，隔离随后）。**

- 三个功能当前已在 dev 上、将随 v2.1.0 一起发布；发版后 `v2.1.0` tag 同时包含功能代码与全部 WIP 改动，从该 tag 拉出的分支天然自洽，无需 cherry-pick
- 发版流程按 `docs/knowledge/release-workflow.md`（版本号 + tag + GitHub Actions），**不要**借隔离操作推送 `v*` tag
- 发版前 dev 的 WIP 会被正式提交并随版本发布，隔离时工作区自然干净，无需额外 stash

```bash
# 发版后执行（v2.1.0 tag 已推送、release 已出）：
# 远程 dev 是只读镜像，禁止 git pull/merge/rebase origin/dev（AGENTS.md Git Rules）；本地 dev 即真源
git fetch origin --tags
git status   # 确认本地 dev 工作区干净（发版后 WIP 已提交）
BASE=v2.1.0
```

> **替代场景（若隔离先于发版）**：以清理干净后的 `dev HEAD` 为节点，可打**本地** tag 固化（禁止推送，避免触发 `release.yml`）。仅当发版计划变更时走此路径。

> ⚠️ **发布 tag 的推送只在发版流程中做一次**：`release.yml` 在推送 `v*` tag 时自动触发 GitHub Actions 发布构建（Windows NSIS + Android APK）。版本节点 tag（如 `v2.1.0`）由发版流程推送；隔离操作本身**不再推送任何新 tag**（本地 tag 不触发构建，远程 tag 才触发）。若后续发版，按 `release-workflow.md` 流程走（含 `TAURI_SIGNING_PRIVATE_KEY` 等 Secrets），不要借隔离操作推送。
>
> 不推荐以旧 tag（`v2.0.0` 等）为节点：三个功能代码不在其历史中，分支创建后还需手工搬运代码，徒增出错面。

---

## 3. 前置准备

### 3.1 处理 dev 未提交 WIP（发版先行时自动满足，隔离先行时强制执行）

```bash
cd /home/binblink/project/tauriProject/BedCode
git status            # 逐项确认在途改动归属，禁止盲目 stash 覆盖他人改动
```

- 若 WIP 属于本次会话/可独立提交 → 先提交：
  ```bash
  git add <确认过的文件>
  git commit -m "chore: dev WIP 归档（进入功能隔离前快照）"
  ```
- 若 WIP 需要保留不提交 → `git stash push -m "feature-isolation-wip"`（后续 `git stash pop` 恢复）
- **禁止** `git checkout -- <file>` / `git restore` 回滚含他人改动的文件（AGENTS.md 回滚规范）

### 3.2 核对保护路径与 hooks

- `docs/`、`CONTEXT.md`、`.scratch/` 在 `dev` 上**正常跟踪**（保护只在 `uat`/`master` 生效），删除即真实删除、正常提交
- 提交信息**禁止** AI 协作者标记（`Co-Authored-By` 等）

### 3.3 确认分支命名与基座

| 分支 | 基座 | 内容 |
|------|------|------|
| `feature/ocr-plugin` | 版本节点（dev HEAD） | 移动端 OCR 全部代码 + `.scratch/ocr-plugin/` 设计文档 |
| `feature/task-scheduler` | 版本节点（dev HEAD） | 桌面端 scheduler 插件全部代码 + `.scratch/task-scheduler-plugin/` 设计文档 |
| `feature/code-viewer` | 版本节点（dev HEAD） | 纯文档：`.scratch/code-viewer/` + ADR 0023/0024 + CONTEXT 词汇节 |

> **文档归属决策（已确认：功能专属文档随分支）**：与 disk-cleaner 先例（scratch 留 dev）不同，本文档采用**功能专属文档随分支**方案——`.scratch/ocr-plugin/`、`.scratch/task-scheduler-plugin/`、`.scratch/code-viewer/` 及 ADR 0023/0024、CONTEXT 词汇节随各自分支，分支自洽、可独立开发；`dev` 只保留与功能无关的通用 scratch（`code-review-batch-*` 等）。理由：代码查看器**本身就是文档**，文档不随分支则该分支空无一物；OCR/scheduler 的 spec 是后续开发的依据，留在分支可避免开发时跨分支翻文档。

---

## 4. 阶段一：创建功能分支（基于版本节点）

WIP 处理干净后执行：

```bash
BASE=<版本节点 ref：v2.1.0（发版先行，见 §2），或替代场景下的 dev HEAD commit SHA>

git switch -c feature/ocr-plugin        $BASE   # 或：git checkout -b feature/ocr-plugin $BASE
git switch -c feature/task-scheduler    $BASE
git switch -c feature/code-viewer       $BASE

git switch dev

# 推送功能分支（feature 分支推送正常，触发 lint CI 属预期；dev 推送不触发 CI，见 AGENTS.md Git Rules）
git push -u origin feature/ocr-plugin
git push -u origin feature/task-scheduler
git push -u origin feature/code-viewer
```

要点：

- 分支内容 = 版本节点快照，功能代码/文档已在其中，**无需额外 commit**
- 每个分支**只读校验自洽**（抽查即可）：
  ```bash
  git ls-tree -r --name-only feature/ocr-plugin | rg "plugins/ocr|src-tauri/src/ocr" | head
  git ls-tree -r --name-only feature/task-scheduler | rg "plugins/scheduler|task-scheduler-plugin" | head
  git ls-tree -r --name-only feature/code-viewer | rg "code-viewer|0023|0024" | head
  ```
- 推送后本地误删可从 `origin/feature/*` 恢复，**先推再删**是安全顺序

---

## 5. 阶段二：回 dev 删除功能代码（每个功能独立 commit）

> 每个功能一个删除 commit（scope 前缀），便于单独 revert / 追溯。删除前先 `git status <file>` 确认无他人改动（AGENTS.md 回滚规范）。以下文件清单以执行时 `rg`/`git ls-files` 复核为准。

### 5.1 功能一：移动端 OCR（`feature/ocr-plugin`）

**删除清单：**

```bash
# 插件工程（源码 + 锁文件；node_modules 为忽略产物不用管）
git rm -r bedcode-mobile/plugins/ocr

# 宿主 Rust OCR 引擎
git rm -r bedcode-mobile/src-tauri/src/ocr

# Kotlin 桥与 Android 插件（CameraPlugin.kt 先核对是否仅 OCR 使用，见下方核对项）
git rm bedcode-mobile/src-tauri/gen/android/app/src/main/java/com/bedcode/mobile/OcrModelExtractorPlugin.kt
git rm bedcode-mobile/src-tauri/gen/android/app/src/main/java/com/bedcode/mobile/OcrImageDecoder.kt
# CameraPlugin.kt：若 grep 确认仅 OCR 相机取图使用则一并删除，否则保留

# onnxruntime 与 OCR 模型资源（先确认 jniLibs/ocr_models 下无其他功能共用文件）
git rm bedcode-mobile/src-tauri/gen/android/app/src/main/jniLibs/arm64-v8a/libonnxruntime.so
git rm bedcode-mobile/src-tauri/gen/android/app/src/main/jniLibs/x86_64/libonnxruntime.so
git rm -r bedcode-mobile/src-tauri/gen/android/app/src/main/assets/resources/ocr_models

# 模型下载脚本
git rm bedcode-mobile/scripts/fetch-ort-android.sh

# 设计文档（随分支，见 §3.3 决策）
git rm -r .scratch/ocr-plugin
```

**集成点（必须同步修改，勿只删插件目录）：**

| 文件 | 改动 |
|------|------|
| `bedcode-mobile/src-tauri/src/lib.rs` | 删 `pub mod ocr;`、`.plugin(...ocr_model_extractor_plugin())` 注册、invoke_handler 中 4 个 `plugin_ocr_*` 命令 |
| `bedcode-mobile/src-tauri/src/plugin/android_plugins.rs` | 删 `mod ocr;` / `mod ocr_models;` 及对应 `pub use` |
| `bedcode-mobile/src-tauri/src/plugin/android_plugins/ocr.rs`、`ocr_models.rs` | 整文件删除 |
| `bedcode-mobile/src-tauri/src/plugin/android_plugins/picker.rs` | **共享文件**：只删 OCR 相关的 `OcrImageSource` 函数与 `parse_ocr_image_response` 调用（file-transfer 等可能复用 picker，先 `rg` 核对） |
| `bedcode-mobile/src-tauri/src/plugin/commands.rs` | 删 `plugin_ocr_recognize / engine_status / delete_models / restore_models` 命令与 `crate::ocr::*` 类型依赖 |
| `bedcode-mobile/src-tauri/Cargo.toml` | 删 `ort`、`ndarray` 依赖（先 `rg "use ort|use ndarray"` 确认仅 OCR 使用）；核对 build.rs 无 OCR 引用 |
| `bedcode-mobile/packages/plugin-sdk-mobile/rust/src/permission.rs` | 删 `PERMISSION_OCR` 常量与 `VALID_PERMISSIONS` 列表对应项 |
| `bedcode-mobile/src/plugin/commands.ts` | 删 4 个 `plugin_ocr_*` invoke 封装 |
| `bedcode-mobile/src/plugin/permission.ts` | 删 `'ocr': [...]` 权限块 |
| `bedcode-mobile/src/plugin/context.ts` | 删 `requireOcrPermission` / `OcrApi` / `context.ocr` |
| `bedcode-mobile/src/locales/{zh-CN,en}/mobile.ts` | 删 `noOcrPermission` key（i18n key 双端同步删） |
| `bedcode-mobile/scripts/plugin-build.js` | `EXCLUDED_PLUGINS = ['ocr']` 移除该排除项（插件已不存在） |

**残留产物清理（忽略文件，不产生 commit，但发布构建前必须清）：**

```bash
# 1. git 跟踪的构建源（tauri.conf.json bundle.resources 输入）
rm -rf bedcode-mobile/src-tauri/resources/plugins/mobile/com.bedcode.ocr

# 2. gen/android assets 副本（⚠️ 漏这一层 = 下架插件仍会进 APK）
#    该目录被 .gitignore 忽略（**/src-tauri/gen/android/app/src/main/assets/resources/plugins/），
#    git rm / git status 均不可见；tauri android 构建只向 assets 复制、不做删除同步，
#    残留目录会持续被打进 APK assets，运行时由 PluginAssetExtractor 从 assets 解压，
#    因「仍在当前 assets 列表」而永远触发不了 cleanupRemovedPlugins 清理。
rm -rf bedcode-mobile/src-tauri/gen/android/app/src/main/assets/resources/plugins/mobile/com.bedcode.ocr
# OCR 还额外残留过模型资源（若 assets/resources/ 下存在则一并删）
rm -rf bedcode-mobile/src-tauri/gen/android/app/src/main/assets/resources/ocr_models

# 3. gradle 构建缓存层（输入变化后多数任务会重跑，但为防 up-to-date 误判一并清）
rm -rf bedcode-mobile/src-tauri/gen/android/app/build/intermediates/compressed_assets/*/compress*Assets/out/assets/resources/plugins/mobile/com.bedcode.ocr
rm -rf bedcode-mobile/src-tauri/gen/android/app/build/intermediates/assets/*/merge*Assets/resources/plugins/mobile/com.bedcode.ocr
```

> **教训（2026-09-09 实测）**：`git rm -r plugins/ocr` 提交后 `rg "com.bedcode.ocr"` 全仓为空、`cargo test` / `pnpm run test:run` 全绿，但 dev 真机仍加载 OCR 插件。原因是 rg 默认尊重 `.gitignore`，gen/android assets 下的残留目录不在其搜索结果内——**git 层面的「干净」不等于构建产物层面的干净**。此类残留的插件若 WASM 是旧 ABI（core module）产物，宿主会以 frontend-only 降级加载（`Scanned N dir(s)` 计数比预期多 1），前端 UI 照常出现。定位手段：`adb shell logcat | grep PluginAssetExtractor` 看 `Extracted bundled plugin:` 行，或读 `.dev-logs/android-dev.*.log`。

**验证（本功能必跑，按 AGENTS.md Done When）：**

```bash
cd bedcode-mobile
cargo test                                    # Rust 全绿（含移除后无悬挂引用）
pnpm run test:run                             # 前端全绿
cd src-tauri/gen/android && ./gradlew :app:compileUniversalDebugKotlin   # Kotlin 编译
```

**提交：**

```bash
git add -A
git commit -m "refactor(ocr): 移除移动端 OCR 插件至 feature/ocr-plugin"
```

### 5.2 功能二：桌面端定时调度（`feature/task-scheduler`）

**删除清单：**

```bash
git rm -r bedcode-desktop/plugins/scheduler
git rm -r .scratch/task-scheduler-plugin     # 设计文档随分支（§3.3 决策）
```

**集成点核对（这些引用经核实为通用测试夹具/注释，先确认再决定是否改动）：**

| 文件 | 状态 |
|------|------|
| `bedcode-desktop/src-tauri/src/plugin/api_registry.rs` | `com.bedcode.scheduler` 仅出现在该文件自身单元测试与注释（通用注册/注销夹具）——**不改**，`cargo test` 验证 |
| `bedcode-desktop/src-tauri/src/plugin/wasm_runtime/host_impl/bus.rs`、`host_impl/api.rs` | `com.bedcode.scheduler` 仅作为 `#[cfg(test)]` 单元测试的通用插件 id 夹具（注册/发布/注销路径）——**不改**，`cargo test` 验证 |
| `CONTEXT.md` §计划任务 (Task Scheduler) 词汇条目 | ⚠️ **2026-09-09 实测漏删项**：词汇节引用 `com.bedcode.scheduler`，按 §3.3「功能专属文档随分支」决策本应随本分支删除，原 §5.2 未列入清单。决策二选一：（a）`git rm` 不可行（CONTEXT.md 为共享文件）——用 edit 逐段删除该词汇节；（b）明确保留作为「待合并回来」的预留词汇并在本节标记。本次隔离未处理，待负责人决定 |
| `.scratch/auto-task-dag-orchestration/spec.md` 的「与计划任务插件的整合不在本规格内」边界声明 | 属 auto-task 的 scope 声明（引用外部系统），非 scheduler 专属文档——**保留**，即使 dev 上无 scheduler 代码，边界声明本身仍成立 |
| `bedcode-desktop/packages/plugin-sdk-test/src/lib.rs`、`plugin-sdk-desktop/rust/types.rs`、`rust-macros/src/lib.rs` | 同上为测试夹具/示例——核对后通常不改 |
| `bedcode-desktop/scripts/plugin-build.js` | scheduler **本就不在** `PLUGINS` 构建表（其构建走插件自身 `scripts/build.js`）——无需改动 |
| `bedcode-desktop/src-tauri/tauri.conf.json` | `resources/plugins/` 是目录级打包，无需逐插件删除 |

**残留产物清理：**

```bash
rm -rf bedcode-desktop/src-tauri/resources/plugins/desktop/com.bedcode.scheduler
```

**验证与提交：**

```bash
cd bedcode-desktop
cargo test
pnpm run test:run
# 可选：pnpm run tauri:build 确认打包无 scheduler 残留

git add -A
git commit -m "refactor(scheduler): 移除桌面端定时调度插件至 feature/task-scheduler"
```

### 5.3 功能三：桌面端代码查看组件（纯文档，`feature/code-viewer`）

**删除清单：**

```bash
git rm -r .scratch/code-viewer
git rm docs/adr/0023-code-viewer-window-expansion-keeps-terminal-viewport.md
git rm docs/adr/0024-code-viewer-host-native-not-plugin.md
```

- `CONTEXT.md` §代码查看（Code Viewer）词汇节（约 406 行起至下一顶级章节前，含 代码面板/终端视口/展开收起/根目录锚定/标签页/自动重载 词条）用 edit 工具逐段删除，**禁止整文件回滚**（CONTEXT.md 可能含其他在途改动）
- 无代码删除项；该功能当前零代码，分支即文档载体

**验证：** 无编译面影响；`rg "code-viewer|代码查看|0023|0024" CONTEXT.md docs/ .scratch/` 确认无残留引用（注意：`CONTEXT.md` 词汇节删除后，若其他文档引用这些词条需一并核对）。

**提交：**

```bash
git add -A
git commit -m "docs(code-viewer): 移除代码查看器设计文档至 feature/code-viewer"
```

### 5.4 收尾：AGENTS.md 与文档同步

- AGENTS.md「gen/android 重建后需恢复」清单：移除 OCR 专属 Kotlin 文件（`OcrModelExtractorPlugin.kt`、`OcrImageDecoder.kt`，及确认后已删的 `CameraPlugin.kt`）与 `scripts/fetch-ort-android.sh` / `libonnxruntime.so` 相关描述，保持恢复清单与实际一致（AGENTS.md 全分支正常跟踪，直接提交）
- 本文档 §5 的「文档归属决策」如需变更执行方式，同步更新 §3.3

---

## 6. 阶段三：全量验证（AGENTS.md Done When）

```bash
# 桌面端
cd bedcode-desktop && pnpm run test:run && cargo test
# 移动端
cd bedcode-mobile && pnpm run test:run && cargo test
# Kotlin（若 gen/android 有改动）
cd bedcode-mobile/src-tauri/gen/android && ./gradlew :app:compileUniversalDebugKotlin
# 残留引用核查（应为空或仅剩通用夹具）
# ⚠️ rg 默认尊重 .gitignore，gitignored 的构建产物（如 gen/android assets）搜不到——
# 必须加 --hidden --no-ignore 才能覆盖全部残留，否则此步会给出假阴性「干净」
# .pi/sessions（会话日志）、.dev-logs（运行日志）、文档本身属历史记录，不计残留
rg -n --hidden --no-ignore "com.bedcode.ocr|com.bedcode.scheduler" \
  --glob '!**/node_modules/**' --glob '!**/target/**' --glob '!**/dist/**' \
  --glob '!**/build/intermediates/**' --glob '!**/.gradle/**' \
  --glob '!**/.pi/**' --glob '!**/.dev-logs/**' --glob '!docs/**' .
# 构建产物残留（忽略文件，确认已清理）
ls bedcode-desktop/src-tauri/resources/plugins/desktop/ bedcode-mobile/src-tauri/resources/plugins/mobile/
# gen/android assets 层残留（移动端 APK 真实输入，git 不可见——用 find 而非 rg 检查）
find bedcode-mobile/src-tauri/gen/android/app/src -iname '*ocr*' -o -iname '*scheduler*'
# target 体积（超 15GB 执行 cargo clean，AGENTS.md 要求）
du -sh bedcode-desktop/src-tauri/target 2>/dev/null; du -sh bedcode-mobile/src-tauri/target 2>/dev/null
```

- i18n key 核查：`rg "noOcrPermission" bedcode-mobile/src` 应为空，且 zh-CN / en 同步
- `git log --oneline -3` 确认三个删除 commit 内容各自独立、信息无 AI 协作者标记
- 推送 dev：`git push origin dev`（远程 dev 为只读镜像，仅作备份；**禁止** `git pull/pull` 汇入远程）

---

## 7. 未来开发与合并路径

- **功能开发**：在 `feature/*` 分支继续开发（spec/票据随分支，直接用 `.scratch/<feature>/issues/` 记账）
- **同步防漂移**（disk-cleaner 分支的教训）：定期 `git switch <feature> && git merge dev`（用 merge 不用 rebase，避免重写已推送历史）；冲突按 AGENTS.md 回滚规范解决
- **发布**：功能就绪后，PR 基线为 `master`（发布基线），或按 `docs/knowledge/release-workflow.md` 走版本号 + tag 发布；`dev` 不夹带未发布功能
- **dev 恢复功能代码**（若需暂回 dev）：`git cherry-pick <功能分支上的实现 commit>` 或临时 `git merge feature/<slug>`，验收后按 §5 流程再隔离
- **远程 dev 分叉**：一旦发现 `origin/dev` 被错误写入，立即停手与用户确认，禁止 `--force` 覆盖（AGENTS.md Git Rules）

---

## 8. 风险与注意事项

| 风险 | 对策 |
|------|------|
| dev 未提交 WIP 被覆盖 | 先 §3.1 提交或 stash；禁止整文件 `git checkout` |
| 误删共享文件（picker.rs / CameraPlugin.kt / jniLibs 共用文件） | 删除前 `rg` 确认引用面；只删 OCR 专属部分 |
| 删除后编译悬挂引用 | 每个功能删除后立即跑 cargo test / pnpm run test:run，不攒到最后 |
| 误推 `v*` tag 触发 release.yml | 版本节点 tag 只打本地；推送前 `git push origin --tags` 必须确认 |
| 分支与 dev 漂移 | §7 定期 `git merge dev`；先推 origin 再删本地（可恢复） |
| Kotlin / jniLibs 恢复清单失配 | §5.4 同步更新 AGENTS.md；gen/android 改动必跑 gradlew |
| 删除 commit 误删 | 每个 commit 独立、scope 前缀；回滚用 `git revert <commit>`（会恢复该功能代码），不要用 reset 抹历史 |

---

## 9. 执行检查表（按序打勾）

- [ ] 前置：确认是否已发 v2.1.0（发版先行）——是则以 `v2.1.0` tag 为基座，dev 工作区已随发版提交干净；否则按 §2 替代场景处理
- [ ] §3.1：dev WIP 已提交或 stash，`git status` 干净
- [ ] §3.2：分支命名/基座确认；文档归属决策确认（§3.3）
- [ ] §4：三个分支已基于版本节点创建并推送 origin，自洽抽查通过
- [ ] §5.1：OCR 删除完成（含全部集成点）→ cargo test / pnpm run test:run / gradlew 全绿 → commit
- [ ] §5.2：scheduler 删除完成 → cargo test / pnpm run test:run 全绿 → commit
- [ ] §5.3：code-viewer 文档删除完成（CONTEXT 词汇节用 edit 逐段删）→ commit
- [ ] §5.4：AGENTS.md 恢复清单同步
- [ ] §6：全量验证通过；残留产物已清（`resources/plugins/` 下 ocr/scheduler 目录 + gen/android assets 层副本 + gradle intermediates，见 §5.1 三层清单与 §6 `--no-ignore` 核查）
- [ ] 推送 dev 备份；无 `v*` tag 误推
