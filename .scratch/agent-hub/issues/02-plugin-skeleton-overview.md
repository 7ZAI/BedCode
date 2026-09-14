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

**探测链路**：activate 解析数据目录（`config_get(HomeDir)` → `~/.bedcode/agent-hub/`）→ 一次批量 fs 授权（6 路径一弹窗，拒绝落 storage 降级、激活照常 Ok）→ `agent-hub.detect` 以 `bash -lc` + PATH 引导并发 5 个 host-process 采集（`which -a` 双安装检测 + `--version` + node/npm/pnpm/registry）→ `on_process_done` 读输出解析 → storage 持久化 → `plugin:agent-hub:detection` 事件推前端。安装方式归类以 PATH 首位为准（standalone/npm-global/native/unknown）。

**验证证据**：
- 插件 crate：`cargo test`（rust/）9/9 通过（解析器：codex/claude/pi/opencode 双安装/未安装、env 分段、manifest、auth_dirs）
- src-tauri：`cargo test` 全量无 FAILED（两次确认）
- 前端：`pnpm run test:run` 64 文件 / 597 用例全绿（两次确认）
- eslint：根目录 0 error
- 全量构建通过，产物已落 `src-tauri/resources/plugins/desktop/com.bedcode.agent-hub/`（index.js + plugin.json + icon.svg + bedcode_plugin_agent_hub.wasm 533KB）

**剩余验证（真机人工）**：`pnpm run tauri:dev` 启动后人工确认三件事——① 侧栏出现 Agent Hub 面板且六 tab 可切换；② 概览页自动触发授权弹窗（拒绝后面板出横幅、卡片仍可用）与真实探测数据（四家版本/安装方式/opencode 双安装警告）；③ 明暗主题下卡片观感。若有问题重开本票。

**已知事项**：① vitest worker 偶发 `ERR_IPC_CHANNEL_CLOSED`（本次会话出现 2 次、复跑即绿，属基础设施抖动，与本票无关）；② `src-tauri/target` 超 15GB 阈值，已按 §3 执行 `cargo clean`（清 20.6GiB，下次构建为全量重编）；③ 根因注意：`npm run test` 字样存在于 `.claude/agents/*.md`（历史遗留，另票处理）。

**跨平台修正（2026-09-13 追加，用户要求 Win/Linux 双兼容，规则固化至 spec §3.1）**：初版探测为 Linux-only（`/bin/bash -lc` + `which -a`）。已改为平台分派——unix `bash -lc`（nvm PATH）/ Windows `cmd /C`（注册表用户 PATH + PATHEXT）；`which -a` ↔ `where`；路径输出统一规范化 `/` 分隔 + lowercase 特征匹配（追加 Windows npm 全局 `%APPDATA%\Roaming\npm` 与 `%PROGRAMFILES%\nodejs` 特征）；pnpm 未装报错行过滤加 Windows「not recognized」形态。新增 Windows 形态单测 2 个（反斜杠路径/盘符识别、not recognized 过滤），插件 crate 11/11 通过。Windows 实机验证列入票 03 遗留项。

**PATH 引导修正（2026-09-14，实机 Deepin 25 复现）**：`bash -lc` 实测检不出 node/npm/pnpm/registry（及 nvm 全局的 pi/codex/opencode，仅 `~/.local/bin` 的 claude 幸免）——登录 shell 不读 `~/.bashrc`，而 `.bashrc` 的交互守卫 `case $- in *i*)` 连 `~/.profile` 的 source 一并拦截，nvm 注入失效。修复：`detect.rs` 新增 `path_bootstrap_unix()`——脚本头部 `export PATH="$(bash -ic 'printf "%s\n" "$PATH"' 2>/dev/null | tail -n 1)"`，从交互子 shell 提取 PATH（`2>/dev/null` 吞 ioctl 警告、`tail -1` 防 rc 输出污染），外层仍是非交互登录 shell。env/cli 两脚本均拼前缀，段标记契约不变；新增单测 `scripts_carry_path_bootstrap`。插件 crate 84/84 通过。曾评估 `bash -ic` 直改：可行但输出混 ioctl 警告且受 rc 启动输出污染，弃。

**detecting 卡死修复（2026-09-14，实测复现）**：PATH 引导修复后探测全部成功（storage 落库 ok、envError 残留 exit=127 与 envStatus=ok 并存暴露竞态），但界面仍永久"检测中"。根因：探测期间 `push_state` 多次 emit 全量状态（mark-detecting 1 次 + 5 个进程完成各 1 次），前端 `detecting` 完全依赖**最后到达**的事件 payload——事件乱序/丢失时停在中间态（某探测项仍 detecting）。修复三层：① 后端 `push_state` payload 加单调递增 `seq`（`STATE_SEQ` AtomicU32），前端按 seq 过滤乱序旧事件只接受最新全量；② 前端 `useDetection` 超时兜底：detecting 超过 25s（> 宿主 20s 进程超时）强制复位并 `refresh()` 拉 storage 权威态——事件丢失/卡住不再永久"检测中"；③ 后端 `apply_output` 成功时清 `envError` 残留。前端类型 `AgentHubState.seq?`。插件 crate 84/84、前端 22/22、eslint 0 error。wasm 1068326B / index.js 118.7kB，双目录（`src-tauri/resources` + `target/debug/resources`）已同步。生效需重启插件或 App。

**目录授权修复（2026-09-14，实测复现）**：症状①点击"授权目录"无反应——目录早已授权（`fs_granted_paths` 命中）→ `request_auth` 不弹窗直接 true；且 `authGranted` 从不落库（`push_state` 只在事件 payload 更新），get-state 恒 false → 横幅永显。症状②访问未授权目录不弹窗——宿主 `save_granted_path` 无条件提取父目录，agent-hub 预授权 `~/.codex`/`~/.pi`/`~/.agents`/`~/.npmrc`（父目录=home 根）→ 授权记录落 home 根，任何访问前缀命中"已授权"，按需弹窗兑底被架空。修复：① 宿主 `fs_auth.rs` 授权粒度精确化——目录→本身、已存在文件→本身、不存在路径→父目录（测试兼容：两个测试路径均不存在仍走父目录分支）；② agent-hub `request_auth` granted 后同步落库 `detection.authGranted`；③ 用户库旧宽泛记录清理（精确 6 目录）+ authGranted=true。宿主 cargo test 604/604、插件 84/84、前端 22/22。wasm 1068746B 双目录同步。生效需重启插件或 App。
