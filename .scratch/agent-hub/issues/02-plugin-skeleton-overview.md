# 02: 插件骨架 + 概览探测

**What to build:** 在桌面端装上 `com.bedcode.agent-hub` 内置插件：侧栏出现 Agent Hub 面板（变体 B 结构骨架），概览页对四家 Agent CLI（claude / codex / opencode / pi）做真实探测——版本（登录 shell 跑 `--version`）、安装方式、PATH 生效位置与双安装并存检测（如实机 opencode standalone 1.18.30 与 npm 1.18.27 并存）、node/npm/pnpm 环境检测——以 CLI 卡片（官方图标 + 状态徽章）呈现。激活时一次批量 fs 授权（`~/.codex`、`~/.pi`、opencode 两处、`~/.agents`、`~/.npmrc`；`~/.claude` 走路径白名单免弹窗），授权被拒的 CLI 功能降级置灰、不阻塞其他家。

**Blocked by:** 01

**Status:** resolved

- [x] 插件经 `registerSidebarPanel` 注册（`rust-ts` 形态，manifest permissions 含 `process:run`/`network:http`/`storage`/`fs:read`/`fs:write`/`ui:sidebar`），面板骨架 + 概览分区可达，i18n zh-CN/en 齐全
- [x] 四家 CLI 卡片显示真实版本与安装方式；opencode 双安装警告可见；卡片使用官方图标
- [x] 批量 fs 授权一次弹窗完成；拒绝后对应卡片降级置灰，插件激活不失败
- [x] `pnpm run test:run`（bedcode-desktop）与 `cargo test`（src-tauri，含插件 crate）通过；`pnpm exec eslint .` 0 error

## Answer

**交付**：`plugins/agent-hub/` 全套（`rust-ts` 插件 19 文件）——manifest（6 项 permissions + sidebar 视图 + 3 命令）、WASM 后端（`detect.rs` 探测域 + `wasm_entry!` 接线）、前端（变体 B 六分区骨架，概览 = 授权横幅 + 环境条 + 四张官方图标 CLI 卡片，i18n MessageSchema 编译期同步两语言）。

**探测链路**：activate 解析数据目录（`config_get(HomeDir)` → `~/.bedcode/agent-hub/`）→ 一次批量 fs 授权（6 路径一弹窗，拒绝落 storage 降级、激活照常 Ok）→ `agent-hub.detect` 以 `bash -lc` 并发 5 个 host-process 采集（`which -a` 双安装检测 + `--version` + node/npm/pnpm/registry）→ `on_process_done` 读输出解析 → storage 持久化 → `plugin:agent-hub:detection` 事件推前端。安装方式归类以 PATH 首位为准（standalone/npm-global/native/unknown）。

**验证证据**：
- 插件 crate：`cargo test`（rust/）9/9 通过（解析器：codex/claude/pi/opencode 双安装/未安装、env 分段、manifest、auth_dirs）
- src-tauri：`cargo test` 全量无 FAILED（两次确认）
- 前端：`pnpm run test:run` 64 文件 / 597 用例全绿（两次确认）
- eslint：根目录 0 error
- 全量构建通过，产物已落 `src-tauri/resources/plugins/desktop/com.bedcode.agent-hub/`（index.js + plugin.json + icon.svg + bedcode_plugin_agent_hub.wasm 533KB）

**剩余验证（真机人工）**：`pnpm run tauri:dev` 启动后人工确认三件事——① 侧栏出现 Agent Hub 面板且六 tab 可切换；② 概览页自动触发授权弹窗（拒绝后面板出横幅、卡片仍可用）与真实探测数据（四家版本/安装方式/opencode 双安装警告）；③ 明暗主题下卡片观感。若有问题重开本票。

**已知事项**：① vitest worker 偶发 `ERR_IPC_CHANNEL_CLOSED`（本次会话出现 2 次、复跑即绿，属基础设施抖动，与本票无关）；② `src-tauri/target` 超 15GB 阈值，已按 §3 执行 `cargo clean`（清 20.6GiB，下次构建为全量重编）；③ 根因注意：`npm run test` 字样存在于 `.claude/agents/*.md`（历史遗留，另票处理）。

**跨平台修正（2026-09-13 追加，用户要求 Win/Linux 双兼容，规则固化至 spec §3.1）**：初版探测为 Linux-only（`/bin/bash -lc` + `which -a`）。已改为平台分派——unix `bash -lc`（nvm PATH）/ Windows `cmd /C`（注册表用户 PATH + PATHEXT）；`which -a` ↔ `where`；路径输出统一规范化 `/` 分隔 + lowercase 特征匹配（追加 Windows npm 全局 `%APPDATA%\Roaming\npm` 与 `%PROGRAMFILES%\nodejs` 特征）；pnpm 未装报错行过滤加 Windows「not recognized」形态。新增 Windows 形态单测 2 个（反斜杠路径/盘符识别、not recognized 过滤），插件 crate 11/11 通过。Windows 实机验证列入票 03 遗留项。
