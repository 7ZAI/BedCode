# Agent Hub 概览：CLI 卸载功能

- 日期：2026-09-27
- 范围：`bedcode-desktop/wasm-apps/agent-hub/`（Rust + 前端 + i18n + plugin.json）+ SDK dev-shell mock
- 需求：用户原话「agent hub 概览里面添加对 agent cli 的卸载功能」
- 背景：原 spec（`.scratch/2026-09-13-agent-hub/spec.md` §v1 不做）明确排除 CLI 卸载，本次按用户新指令扩scope。

## 设计决策

1. **recipe 白名单（与安装同一安全底线）**：卸载命令只接受白名单 cli 名 + 固定模板，
   method 取自探测状态（guest 端最终仲裁，不信任前端传参），无用户自由输入拼接面：
   - npm-global（四家 CLI）→ `npm uninstall -g <pkg>`（镜像无关，卸载本地完成）
   - claude native → 官方文档命令：unix `rm -f ~/.local/bin/claude` + `rm -rf ~/.local/share/claude`；
     Windows cmd `del /f` + `rd /s /q`（官方安装文档原文）
   - opencode standalone → `opencode uninstall --force`（官方命令，--force 非交互跳过确认）
   - unknown / 其他 method → guest 拒绝（v1 提示手动）
2. **共用安装域 run 管线**：active 互斥（安装/更新/卸载同一时刻只跑一个）、输出尾部回显轮询、
   取消、完成后自动全量重探测刷新卡片——卸载不新增机制，只是新增一种 run kind（action=uninstall）。
3. **UI：概览卡片「卸载」按钮 + 两击确认**（第一击 arm 4s 自动复位，第二击才发命令，与换源/
   清空数据同一交互模式）。仅已装卡片出现；busy（任意在途 run）禁用；双安装 / 未知安装方式 /
   npm-global 缺 node → 不给按钮、提示手动。卸载不依赖目录授权（process:run 已在 manifest 声明，
   process 不经 fs_auth 弹窗路径）。
4. **错误面**：guest 拒绝（并发 run / 探测未就绪）→ useInstall.uninstall 返回 false →
   AgentHubView 落瞬态信号 → 卡片显示友好 i18n「卸载失败，请重试」（命令原文只进日志，ADR 0030）。
5. **不 bump ABI**：纯插件命令面新增（`agent-hub.uninstall`），无 WIT/host-* 变化。

## 落地文件

- Rust：`install/recipe.rs`（build_uninstall_script + 5 个新单测）、`install/mod.rs`（uninstall 命令 +
  handle_process_done 泛化 kind）、`lib.rs`（命令路由 + on_process_done 分支 + PendingRun kind 注释）
- 前端：`CliCard.vue`（卸载按钮/两击确认/不可用原因）、`OverviewTab.vue`（props 下传 + emit 上抛）、
  `AgentHubView.vue`（handleUninstall + 失败信号）、`useInstall.ts`（uninstall() 命令分发）、
  `types.ts`（action 联合加 'uninstall'）、i18n 7 个新 key（zh-CN/en/messages 同步）
- 声明：`plugin.json`（contributes.commands 加 agent-hub.uninstall + 描述提及卸载）
- SDK dev-shell mock：`agent-hub.ts`（uninstall 剧本 + runSpec 重构 + 播完翻转探测为未安装）
- 测试：Rust recipe 5 例；前端 A8（CliCard 卸载 10 例）/ A9（OverviewTab 接线 3 例）/
  useInstall.test.ts（命令分发 4 例）

## 验证

- `cargo test`（agent-hub rust）：135 通过（+5）
- 桌面前端全量 `pnpm run test:run`：104 文件 1272 通过（+17）
- `pnpm exec eslint .`：0 error（118 warning 全为既有）
- clippy/fmt：本任务文件零新增告警（clippy 既有 15 条在 detect/usage/providers）
- `pnpm run build`（含 wasm32-wasip3 + wasmHash）：通过，源/产物 manifest 逐字一致（除注入 hash）
- SDK dev-shell：tsc --noEmit 通过
- **未跑**：dev-shell/真机浏览器核验（无 GUI 交互手段）

## 未做 / 后续

- InstallTab 行内不加卸载入口（用户指定概览；控制台会自动回显卸载 run）
- 双安装 / 未知安装方式的自动卸载（v1 手动提示；需按家适配官方卸载面）
- describe-uninstall 降级路径（node 缺失时复制命令）——v1 仅提示
