# Code Review Batch 2026-09-05

## 任务

审 dev 分支工作区全部未提交改动（107 文件、+3396/-3577），按功能切 8 个 commit，逐桶审修，最后 push 到 origin/dev。

## 跳过

- `.scratch/splash-design/`
- `.scratch/splash-replica/`
- `bedcode-mobile/src/components/SplashScreen.vue`
- `bedcode-mobile/src/composables/useAppStartup.ts`
- `bedcode-mobile/src/config/splash.ts`
- `bedcode-mobile/src/__tests__/composables/useAppStartup.test.ts`
- `.pi-lens.json`（本地 lens 配置，不入库）
- `bedcode-desktop/.claude/settings.local.json`（本地 Claude 配置，不入库）

## 9 个 commit 计划（修订：含 race condition 补抓的 5 文件）

| # | commit | 文件数 | spec | 状态 |
|---|--------|--------|------|------|
| C1 | chore(agent): 文档/skill/agents 同步 | 14 | ❌ | 待审 |
| C2 | chore(pi): 废弃 .pi/extensions/subagent 目录清理 | 2 | ❌ | 待审 |
| C3 | feat(ai-chatbox): sensenova provider 接入双端 | 4 | ❌ | 待审 |
| C4 | feat(mobile): plugin lifecycle 真实状态上报（含 file-transfer/ocr devMock 配套 + lib.rs + message.rs） | 43 | ✅ | 待审 |
| C5 | feat(auto-task): api 通道 + 面板/历史 + 桌面 hooks 升级 | 9 | ❌（无 spec.md） | 待审 |
| C6 | refactor(desktop): plugin/host+PTY+session+server+peer+parser | 22 | ❌ | 待审 |
| C7 | feat(mobile): connection+ocr+peer+file_service+plugin 子集 | 17 | ❌ | 待审 |
| C8 | chore(mobile): 移动端 frontend+i18n+scripts | 8 | ❌ | 待审 |
| C9 | chore(deps): plugin-sdk-mobile pnpm-lock 跟进 vite/vitest 升级 | 1 | ❌ | 待审 |

## Race condition 防御

工作区有后台进程（pi-lens / vite watch / vitest watch / IDE autofix / pnpm install）会持续修改 tracked 文件。
已观察到 5 个文件在初次 `git diff HEAD` 之后被修改（env.d.ts / file-transfer.ts / hooks.rs / pnpm-lock / devMock.ts / mock.ts）。

**纪律**：
- 每个 commit 前：`git diff --cached --name-only | wc -l` 必须等于预期
- 每个 commit 用 `git commit --only <pathspec...>` 或先 `git restore --staged <others>`
- 每桶结束再跑 `git status --porcelain` 截图对比，发现新 partial 立刻 add
- 进度文件 `.scratch/code-review-batch-2026-09-05/` 不入库（工作区保留）

## 审修纪律

- 每桶结束必跑 `cargo test`（Rust 改动）或 `pnpm run test:run`（前端改动）
- 每桶 commit 前列 diff stat 给用户过目
- 发现违反 AGENTS.md 架构决策（dev-shell 不含具体业务 mock 等）→ 停手询问
- 发现 scope creep 或方向错误 → 停手询问
- 不删 / 不重写他人代码，只做最小修复

## 执行节奏

(a) 顺序：先小后大，但 C4 (lifecycle) 是 spec 已知大头优先；最终顺序：

1. C2（最小，2 文件）
2. C3（最小，4 文件）
3. C1（14 文件纯文档，低风险）
4. C5（7 文件 spec 已知）
5. C8（8 文件 i18n+frontend）
6. C7（17 文件移动端后端）
7. C6（22 文件桌面端后端）
8. C4（38 文件，最大头最后做，确保前面没把上下文撑爆）

每桶结束：报告 + commit 摘要 + 等用户确认进下一桶。
最后全部完成：列 commit log 给用户过目 → `git push origin dev`。
