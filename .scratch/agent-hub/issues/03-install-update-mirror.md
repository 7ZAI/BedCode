# 03: 测速换源 + 一键安装/更新

**What to build:** 概览页环境条提供 npm 官方源与 npmmirror 的实测计时（host-http GET 计时）并给出推荐；一键安装按每家 recipe 执行（host-process 走登录 shell `$SHELL -lc` 保住 nvm PATH，输出流式回显；探测不到 node 环境时降级为生成命令 + 复制按钮 + Node 安装指引）；镜像默认本次安装临时 `--registry=https://registry.npmmirror.com`，另提供「持久切换」——确认后改写 `~/.npmrc`（改前备份、UI 一键还原）；最新版本经 registry HTTP API（`GET /<pkg>/latest`）比对，落后可一键更新。安装/更新命令仅来自固定 recipe 白名单（`@earendil-works/pi-coding-agent`、`@openai/codex`、`opencode-ai`；claude native 走 `claude update` / 官方脚本），不拼接用户自由输入。

**Blocked by:** 02

**Status:** resolved

- [x] 测速面板显示两源耗时并给出推荐（复用 host_impl/http.rs 的客户端模式）
- [ ] 实际完成一次 npm 类 CLI 的安装或更新，输出流式可见，完成后卡片状态刷新（代码路径就绪，待真机 `pnpm run tauri:dev` 人工执行一次，见剩余验证）
- [x] 持久切换改写 `~/.npmrc` 前自动备份，UI 可一键还原
- [x] 命令构造无用户自由输入拼接；token/凭据不进日志（`key.len()` 模式）
- [x] 平台分派（spec §3.1）：unix `bash -lc` / Windows `cmd /C`；npm 包安装命令两端同形；claude 更新 `claude update` 双平台；Windows 形态有逻辑层单测（Windows 实机验证列入遗留）

## Answer

**交付**：票 03 全套——guest 新增 `install.rs`（约 700 行含 9 组单测），前端新增 `InstallTab.vue` + `useInstall.ts`，概览环境条并入测速行；manifest 补 9 条命令声明；产物已重建落 `src-tauri/resources/plugins/desktop/com.bedcode.agent-hub/`。

**能力链路**：
- **测速**：guest 对两源各做一次 `GET /semver/latest` 小 GET，计时经 `ConfigKey::CurrentTimeMs`（wasm 无系统时钟，`Instant::now()` 会 panic）；推荐规则 = 镜像可达且（官方失败或更快）→ npmmirror。概览环境条与安装页共用同一状态。
- **最新版本**：`GET /<pkg>/latest`（scoped 包名 `%2f` 转义），官方源失败回落 npmmirror；guest 端宽松 semver 比较得 `outdated`（前端不复刻比较逻辑）。
- **安装/更新**：recipe 白名单纯函数 `build_install_script`（cli 名 + 固定模板，method/installed 取自探测状态而非前端传参）；`npm install -g <pkg>` 双平台同命令（Windows 经 cmd PATHEXT 解析 npm.cmd），claude native → `claude update`，opencode standalone 生效 → 拒绝并提示手动；超时 15 分钟。执行为 run-id 异步进程，前端 1.2s 轮询 `get-run-output`（guest 截尾部 16KB 控载荷）回显，完成后自动全量重探测刷新卡片；取消走 `process_kill`，`cancelRequested` 使终态落 cancelled；停用时兜底终止在途 run。
- **持久换源**：`rewrite_registry` 只动 `registry=` 行（key 两侧空白/大小写容忍），authToken 等其余行原样保留、内容不落日志不回传前端；改前备份到 `~/.npmrc.agent-hub-backup`（已有备份不覆盖，保留用户原始文件），还原保留备份可重复执行。授权被拒时 fs 类动作置灰降级（避免逐次弹窗），测速/检查更新走 host-http 不受限。

**平台分派修复（重要）**：票 02 的 `shell_invocation` 用 `cfg!(windows)` 在 wasm 目标（`wasm32-unknown-unknown`）下恒为 false，Windows 宿主会误走 unix 分支。本票改为 activate 时经 `ConfigKey::OsPlatform` 缓存宿主 OS + 纯函数 `shell_invocation(script, windows)`（双平台形态有单测）；detect 与 install 共用。Windows 实机验证仍列遗留。

**安全**：`~/.npmrc` 含密文件只解析 registry 行回显（非敏感），整份内容不出 guest、不进日志；命令全部白名单构造，无用户输入拼接面；描述用命令经 `agent-hub.describe-install` 由同一白名单解析，前端不复刻包名。

**验证证据**：
- 插件 crate：`cargo test`（rust/）21/21 通过（recipe 白名单/claude 分派/standalone 拒绝/未知 cli 拒绝、版本比较、npmrc 改写与提取、输出尾部截断 char boundary、默认状态形状、shell 分派双平台、Windows 路径解析等）
- `cargo fmt` 已过；`cargo clippy --all-targets` 0 error（余 2 条 detect.rs 票 02 既有 collapsible_match warning，非门禁不顺手重构）
- 宿主：`cargo test`（bedcode-desktop/src-tauri）604 单测 + 8 集成 + doc-tests 全部 0 failed
- 前端：`pnpm run test:run` 64 文件 / 597 用例全绿（`ERR_IPC_CHANNEL_CLOSED` 基础设施抖动复现 2 次，复跑即绿，与票 02 记录一致）
- `pnpm exec tsc --noEmit`：i18n MessageSchema 两语言同步通过（仅 .vue/.css 模块解析的工具性报错，票 02 已存在）；根目录 `pnpm exec eslint .` 0 error
- 插件 wasm 构建 + componentize + 产物复制成功（wasm32 目标编译亦验证无原生时间/平台 API 误用）
- 测试后无残留进程/监听端口（后台另有 `BedCode-wasm-core` 目录的 cargo test 与 gradle daemon 属用户其他会话，未触碰）

**剩余验证（真机人工，Windows/Linux 各一轮）**：
- `pnpm run tauri:dev` 实际执行一次 npm 类 CLI 安装或更新，确认输出流式回显、完成后卡片版本/安装方式刷新
- 测速两源耗时合理、推荐徽章与持久切换/还原闭环（改写前后 `~/.npmrc` 内容 diff 仅 registry 行）
- Windows 实机：探测/安装走 `cmd /C` 分派、npm shim 可用、路径规范化展示（spec §3.1 遗留验证项）

**已知事项**：① fs 授权弹窗的「记住」语义由宿主 fs_auth 决定（parent-prefix 持久化），插件按 `authGranted` 置灰降级，未改宿主；② vitest worker 偶发 `ERR_IPC_CHANNEL_CLOSED` 沿袭票 02 记录，复跑即绿。
