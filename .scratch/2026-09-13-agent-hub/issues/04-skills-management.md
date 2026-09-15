# 04: Skills 管理

**What to build:** Skills 分区端到端：以 `~/.agents/skills` 为规范库（SKILL.md frontmatter 解析 name/description/allowed-tools）浏览 + 查看/编辑（mtime/hash 冲突检测 + 保存前 diff 预览）；分发 = 复制到各 CLI 私有目录（`~/.claude/skills`、`~/.pi/agent/skills`），扫描时 hash 比对检测副本落后、一键重新分发；从 GitHub 仓库/子目录 URL 拉取含 SKILL.md 的目录入规范库（host-http 直连，不可达时提示代理/镜像）；本地目录导入（host-platform 选目录）。启用/禁用/删除不在本票（v2 按家适配）。

**Blocked by:** 02

**Status:** resolved

- [x] 规范库列表 + 详情/编辑可用，编辑有冲突检测与 diff 预览（代码路径就绪并过测试；真机操作列入剩余验证）
- [x] 「编辑 → 分发 → 手动改分发副本 → 检测落后 → 重新分发」闭环（代码路径就绪：保存后重算 hash/分发状态；真机走一遍列入剩余验证）
- [x] 从一个公开 GitHub URL 安装一个真实 skill 入规范库；不可达时的提示文案走 i18n（代码路径就绪；真机安装一次列入剩余验证）
- [x] 本地目录导入可用（代码路径就绪；真机操作列入剩余验证）

## Answer

**交付**：票 04 全套——guest 新增 `skills.rs`（约 1150 行含 17 组单测），前端新增 `SkillsTab.vue` + `SkillEditor.vue` + `useSkills.ts` + `utils/diff.ts`（LCS 行 diff）+ diff 单测（vitest include 追加 agent-hub，沿 scheduler 先例）；manifest 补 7 条命令声明（无新增 permissions）；产物已重建落 `src-tauri/resources/plugins/desktop/com.bedcode.agent-hub/`（wasm 793KB）。

**架构决策（三个 WIT 约束的绕行方案，均只用既有原语、无 WIT 改动/ABI bump）**：
- **目录枚举**：host-fs 无列举原语 → 经 host-process 平台分派（unix `find -type f` / Windows `dir /s /b /a:-d`——`/a:-d` 排除目录项），沿用 detect.rs `== 分段 ==` 标记输出。根目录缺失属常态，**不以 exit code 判失败**（区别于 detect.rs），仅超时（30s）视为失败。
- **编辑冲突检测**：WIT 无 stat（mtime 不可得）→ 用「保存前重读 + 内容比对」（spec 的 mtime/hash 二选一取 hash 语义，严格更强）；保存前 diff 预览由前端 LCS 行 diff（`utils/diff.ts`）两击确认呈现。编辑范围 v1 = SKILL.md（编辑即改 frontmatter）。
- **GitHub 安装**：非流式 host-http 响应体强制 UTF-8（宿主 http.rs），二进制 tarball 不可行 → repo API（缺 ref 时查 default_branch）+ recursive trees API 一次拿全量路径 + raw.githubusercontent.com 逐文件文本下载；**非 UTF-8 文件（图片等）跳过并计数记录**（fs_write 自动建父目录）。多 skill 仓库（顶层目录各自含 SKILL.md）整批安装；已存在同名 skill 需覆盖确认（返回 exists 名单，前端二击确认后携 `overwrite` 重试）；URL 限制：含 `/` 的分支名不支持、`blob/` 文件链接拒绝（单测锁定）。

**能力链路**：
- **扫描**（`agent-hub.scan-skills`，异步进程）：枚举 `~/.agents/skills` + 两分发根 → 归组（首段 = skill 目录，仅收根级含 SKILL.md 者）→ 逐 skill 读 SKILL.md 解析 frontmatter（YAML-lite：单行值 + allowed-tools 缩进列表两形态）+ 逐文件 FNV-1a hash → 逐文件与各分发目标比对（内容 hash；binary 退化存在性比对）得 none/distributed/stale → storage 持久化推送。状态 idle 时前端挂载自动触发一次。
- **分发**（`agent-hub.distribute-skill`）：库侧逐文件 fs_copy（字节级、自动建父目录，支持二进制），逐文件容错记录 errors，成功后重算该 skill 分发状态。目标白名单 claude/pi（opencode/codex 无 skills 目录约定，UI 静态提示）。
- **编辑**（`read-skill` / `save-skill`）：dir 参数拒绝路径分隔符与 `..`（防拼接逃逸）；保存成功重算该 skill frontmatter/hash/分发状态并推送（无需全量重扫）。
- **本地导入**（`agent-hub.import-skill`，异步进程）：host-platform pick-folder（取消返回 picked=false）→ 所选目录**一次批量授权弹窗**（fs_request_auth，记住前缀后幂等）→ 同名 skill 覆盖确认 → spawn 枚举进程（PendingRun.source 携 `源目录\n入库名`）→ 回灌时校验根级 SKILL.md → 逐文件 fs_copy 入库 → 结果落状态 → 自动触发重扫描。
- **事件**：全量状态经 `plugin:agent-hub:skills` 推送；授权被拒时 fs 动作置灰降级（与概览/安装页同一横幅语义）。

**安全**：命令无用户自由输入拼接面（GitHub URL 仅解析后拼 API/raw 白名单域，路径段百分号编码）；GitHub API 请求显式带 `User-Agent`（缺失会被 403）；skill 内容/路径不落日志；`~/.claude` 走白名单免审、`~/.agents`/`~/.pi` 走激活批量授权、导入目录现场批量授权。

**验证证据**：
- 插件 crate：`cargo test`（rust/）38/38 通过（frontmatter 双形态/缺失、列举解析 unix+Windows 反斜杠、前缀大小写不敏感与前缀重名陷阱、FNV 已知向量、组合 hash 顺序无关、分发状态规则、GitHub URL 六形态+四拒绝、tree 计划根/多 skill/子目录/truncated、raw URL 编码、扫描脚本双平台）
- `cargo fmt` 已过；`cargo clippy --all-targets` 0 error（余 2 条 detect.rs 票 02 既有 collapsible_match warning，非门禁不顺手重构）
- 宿主：`cargo test`（bedcode-desktop/src-tauri）604 单测 + 8 集成全绿 0 failed
- 前端：`pnpm run test:run` 65 文件 / 603 用例全绿（新增 diff.test.ts 6 用例；无 `ERR_IPC_CHANNEL_CLOSED` 抖动）
- `pnpm exec tsc --noEmit`：i18n MessageSchema 两语言同步通过（仅既有 .vue/.css 模块解析的工具性报错）
- 根目录 `pnpm exec eslint .` 0 error（120 warning 均为存量，非本票引入）
- 插件 wasm 构建 + componentize + 产物复制成功（wasm32 目标编译验证无原生时间/平台 API 误用）
- 测试后无残留进程/监听端口（既有 chrome/adb 端口为用户环境进程，未触碰）

**剩余验证（真机人工，Windows/Linux 各一轮）**：
- `pnpm run tauri:dev` 走通「编辑 → 分发 → 手动改分发副本 → 徽标转落后 → 重新分发」闭环
- 从一个公开 GitHub URL（如 `anthropics/skills` 某子目录）安装一个真实 skill；断网/直连失败时确认 i18n 代理/镜像提示呈现
- 本地目录导入一次（含授权弹窗、同名覆盖确认）；编辑器 diff 预览与冲突态（外部改文件后保存）
- Windows 实机：`dir /s /b /a:-d` 枚举形态、反斜杠路径归一化（spec §3.1 遗留验证项）

**已知事项**：① 库内超大文件（如误入 node_modules）会在扫描时被整读进内存做 hash——skills 目录约定为小文件，v1 接受该限制；② GitHub 未认证 API 限额 60 req/h/IP，批量安装大仓库可能触发限速（错误会落 github.last.error）；③ 规范库扫描/导入期间 listing 进程绕过 fs_auth 读目录清单（与 detect 同一通道语义），文件内容读取仍逐文件过 fs_auth。
