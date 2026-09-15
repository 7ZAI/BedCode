# Agent Hub spec —— Agent CLI 可视化管理

- 状态：已过 grill 对齐（2026-09-13），待实现
- 术语：见根 `CONTEXT.md`「Agent Hub」域（Agent Hub / Agent CLI / CLI 适配器 / Skill / 使用记录 / 供应商预设）
- 决策记录：Round 1/2 用户逐条确认，未标注处均为「按推荐」

## 1. 背景与目标

在桌面端提供对多个 Agent CLI（claude / codex / opencode / pi）的统一可视化管理：环境检测与一键安装、Skills 管理、供应商统一配置、使用统计。auto-task 插件的能力将来并入本插件，v1 不动 auto-task，架构上预留。

## 2. 范围

**v1 做**：桌面端插件，四 CLI 适配；安装/版本检测/一键更新；npm 镜像测速与换源；Skills 浏览/编辑/分发/GitHub 安装/本地导入；供应商预设与应用（**v2 起中心凭据库存 key**，见 §4.4）；使用统计（增量扫描、会话/天/项目/模型维度、无 $ 成本）；会话日志解析视图（会话列表 → 归一事件流 + 原始行切换）。

**v1 不做**：移动端任何部分（无协议改动）；auto-task 并入；CLI 卸载；$ 成本估算；实时 tail；Skills 启用/禁用/删除（v2 按家适配）；AI Chatbox 供应商打通（预设模板后续可复用）。

## 3. 架构与载体

- **独立内置插件** `plugins/agent-hub/`，id `com.bedcode.agent-hub`，形态 `rust-ts`（参照 file-transfer）。UI 经 `registerSidebarPanel` 注册单个侧栏面板（order 建议 ~240）。
- **只用既有 WIT 原语**（host-process / host-fs / host-http / host-plugin-database / host-storage / host-platform / host-bus / host-log），**无 WIT 改动、无 ABI bump**。
- **manifest permissions（以 SDK `PERMISSION_API_MAP` 为准）**：`process:run`（高危，宿主审计）、`network:http`、`storage`、`fs:read`、`fs:write`、`ui:sidebar`。
- **fs 授权（激活时一次批量弹窗，ADR 0007 模式）**：`~/.codex/`、`~/.pi/`、`~/.config/opencode/`、`~/.local/share/opencode/`、`~/.agents/`、`~/.npmrc`；`~/.claude/` 已在 fs_auth 路径白名单免弹窗。授权被拒 → 对应 CLI 的功能降级置灰，不阻塞其他 CLI。
- **不改动** auto-task、ai-chatbox、宿主任何模块；不新增宿主命令。

### 3.1 跨平台兼容（Windows / Linux，全票强制）

用户明确要求实现全程双系统兼容，规则如下（票 02 已按此实现，03 起沿用）：

- **命令执行平台分派**：unix 走登录 shell `bash -lc`（保 nvm PATH 注入，与宿主 PTY 同模式）；Windows 走 `cmd /C` 链式命令（GUI 进程 PATH 来自注册表用户环境，npm shim 经 PATHEXT 解析）。等价命令对照：`which -a` ↔ `where`、`2>/dev/null` ↔ `2>nul`。
- **路径规范**：进程输出路径一律规范化为 `/` 分隔后存储与归类；安装方式特征匹配统一 lowercase（Windows 路径大小写不敏感）。Windows 特征目录追加：`%APPDATA%\Roaming\npm`（npm 全局 shim）、`%PROGRAMFILES%\nodejs`。
- **安装 recipe（票 03）双平台**：npm 包同一命令（`npm install -g <pkg>`，Windows 经 npm.cmd shim）；claude native 更新 `claude update` 双平台同命令；opencode standalone 更新 Windows 走官方 PowerShell 脚本（实施时核对）。
- **镜像持久切换（票 03）**：写 `{HomeDir}/.npmrc`——两端同一物理文件位置，备份/还原逻辑平台无关。
- **host-app 随包 CLI**：宿主已双平台（Windows 注册表 PATH / unix symlink），无需插件侧处理。
- **验证策略**：Linux 实机全量验证；Windows 依赖逻辑层单测（Windows 形态样本：反斜杠路径、盘符、'xxx' is not recognized 报错行）+ Windows 实机人工验证——列入票 03/04 的遗留验证项。

#### 双平台对照矩阵（票 03–07 实施时逐行对照）

| 事项 | Linux | Windows |
| --- | --- | --- |
| 命令执行 | `bash -lc`（nvm PATH 注入） | `cmd /C`（注册表用户 PATH + PATHEXT 解析 `.cmd` shim） |
| 全路径枚举 | `which -a <cli>` | `where <cli>`（未装报错走 stderr，`2>nul` 抑制） |
| 未装报错形态 | `bash: xxx: command not found` | `'xxx' is not recognized as an internal or external command` |
| 路径分隔/大小写 | `/` | `\` 输出 → 统一规范化 `/` 存储；匹配统一 lowercase |
| 家目录真源 | `config_get(ConfigKey::HomeDir)` = `$HOME` | 同一调用 = `%USERPROFILE%` |
| 插件数据目录 | `{HomeDir}/.bedcode/agent-hub/` | 同构（runs/ 探测输出、后续统计库，fs 由宿主直接落盘无平台差） |
| npm 全局 bin | nvm：`~/.config/nvm/versions/node/<v>/bin` | `%APPDATA%\Roaming\npm`（.cmd shim）；nvm-windows / `%PROGRAMFILES%\nodejs` |
| claude native | `~/.local/bin/claude` | `%USERPROFILE%\.local\bin\claude.exe`（官方安装器两端同位） |
| opencode standalone | `~/.opencode/bin/opencode` | `%USERPROFILE%\.opencode\bin\opencode.exe` |
| claude 配置/转录 | `~/.claude/`（fs_auth 路径白名单按 `.claude/` 段匹配，两端同样免审） | `%USERPROFILE%\.claude\`（同上） |
| pi 配置/会话 | `~/.pi/agent/` | `%USERPROFILE%\.pi\agent\`（官方目录同名，票 06 实施时按官方文档核对） |
| codex 配置/会话 | `~/.codex/` | `%USERPROFILE%\.codex\`（同上，待实机校准） |
| opencode 会话库 | `~/.local/share/opencode/opencode.db` | Windows 数据目录形态待核对（大概率 `%LOCALAPPDATA%`，票 07 实施时按官方文档确认） |
| npmrc（镜像持久切换） | `{HomeDir}/.npmrc` | 同一物理文件 `%USERPROFILE%\.npmrc`，备份/还原逻辑平台无关 |
| npm 包安装/更新 | `npm install -g <pkg>`（--registry 临时源同形） | 同命令（npm.cmd shim） |
| claude 更新 | `claude update` | 同命令 |
| opencode standalone 更新 | 官方 install.sh | 官方 PowerShell 脚本（票 03 实施时核对命令面） |
| 随包 CLI（host-app） | unix symlink + PATH | 注册表 `HKCU\Environment` PATH（宿主已实现，插件无需处理） |
| skills 目录 | `~/.agents/skills`、`~/.claude/skills`、`~/.pi/agent/skills` | `%USERPROFILE%` 下同名相对目录（票 04 实施时核对） |

## 4. 功能需求

### 4.1 概览与环境检测

- CLI 卡片：每家一张（名称/图标/已装版本/安装方式/最新版本/状态徽章）。
- 探测：`<cli> --version` 经 host-process 走登录 shell（`$SHELL -lc`，保 nvm PATH）；解析 PATH 实际生效位置（`which` 语义），**检测双安装并存**（如实机 opencode：npm 全局 1.18.27 与 standalone 1.18.30 并存）并提示。
- Node 环境检测：node/npm/pnpm 版本、`npm config get registry` 当前源。
- 测速：host-http 分别 GET npmjs 与 npmmirror（一次 HEAD/小 GET 计时），面板展示对比；npmmirror 显著更快时提示换源。

### 4.2 一键安装与版本管理

- **执行模型**：应用内直接执行（host-process），输出流式回显（output_path 文件增量读取）；探测不到 node 环境时降级为生成命令 + 复制按钮（附 Node 安装指引）。
- **镜像策略**：默认本次安装临时 `--registry=https://registry.npmmirror.com`；另提供「持久切换」——确认后改写 `~/.npmrc`（改前备份，UI 一键还原）。
- **版本管理**：最新版经 registry HTTP API 查询（`GET https://registry.npmjs.org/<pkg>/latest`，免 shell，走 host-http）；落后时一键更新。
- **安装 recipe（按家，npm 包名已实机核实）**：
  - pi：`npm install -g @earendil-works/pi-coding-agent`
  - codex：`npm install -g @openai/codex`
  - opencode：`npm install -g opencode-ai`（注意与 standalone 安装并存；standalone 更新走官方脚本，v1 提示手动）
  - claude：native 安装（本机 installMethod=native）→ 更新走 `claude update` 或官方安装脚本；npm 通道 `@anthropic-ai/claude-code` 仅在探测到 npm 安装时使用
- 安装/更新进程用**固定 recipe 白名单构造**，不拼接用户自由输入。

### 4.3 Skills 管理

- **真源模型**：`~/.agents/skills` 为规范库；安装/编辑/GitHub 安装/导入都落规范库；**分发** = 复制到各 CLI 私有目录（`~/.claude/skills/`、`~/.pi/agent/skills/`，含项目级提示）；扫描时 hash 对比检测**副本落后**并提示重新分发。
- **浏览/查看**：列规范库 + 各分发点，SKILL.md 渲染（frontmatter：name/description/allowed-tools）。
- **编辑**：写回前 mtime/hash 冲突检测 + diff 预览。
- **GitHub 安装**：输入仓库/子目录 URL → host-http 直拉（raw 或 codeload tarball）→ 解出含 SKILL.md 的目录入规范库；GitHub 不可达时提示代理/镜像（v1 仅提示）。
- **导入**：host-platform 选本地目录（含 SKILL.md）复制入规范库。

### 4.4 供应商统一管理

- **供应商预设**：CRUD（名称/baseUrl/api 方言/模型列表 + **中心凭据 key**）。内置模板复用 chatbox 四套（DeepSeek/通义/OpenAI/Anthropic）+ 自定义。
- **中心凭据库（v2，2026-09-14 用户决策：删除「key 不落 hub」红线）**：`provider_preset.api_key` 明文存插件库——一处配置 key、分发到多个 agent（claude/pi/opencode；codex 待格式校准后开放）。key 明文只落本库：列表/状态/导入结果只出掩码（前 3 字符 + 长度），日志只记长度。
- **反向导入**：读取各 CLI 现有配置生成预设并**把源 key 一并收进中心凭据库**——pi（`models.json` providers + `auth.json`）、opencode（`opencode.json` provider.*）、claude（settings.json env / 桥接现状只读展示，不生成预设）。
- **应用**：写入目标 CLI 原生配置文件（真源始终是 CLI 自己的配置）；key 来源四选一——**stored 中心库**（预设已有 key 时默认）/ inline 现场输入 / source 内存直拷 / none 保留目标既有凭据。
- **claude 特例**：写 `~/.claude/settings.json` 的 `env` 块（`ANTHROPIC_BASE_URL/AUTH_TOKEN/MODEL`）；检测到现有 `provider-config.sh`/`anthropic-bridge.mjs` 桥接体系时提示冲突、不覆盖，由用户选择。
- 应用后提示该 CLI 需重启会话生效。

### 4.5 使用统计

- **数据源**：各 CLI 本地会话数据，适配器归一（claude=JSONL、pi=JSONL、opencode=SQLite、codex=官方格式预留）。与 auto-task 任务记录**并存不合并**。
- **解析策略**：应用打开面板时增量扫描——文件级水位（size/mtime，记录于插件库 `parse_watermark`）；opencode 的 SQLite 源以 db 文件 mtime 触发重扫（数据量小）。**不实时 tail**。
- **归一字段**：会话 ID、项目、起止时间、时长（首末事件差）、模型、tokens（input/output/cache_read/cache_write/reasoning）、cost（pi/opencode 有则存，null 不估算）。
- **维度**：按 CLI / 按天 / 按项目 / 按模型；简版明细列表（会话级）。

### 4.6 会话日志解析视图

主从布局：左侧会话列表（按 CLI/项目/时间过滤），右侧**归一事件流**——用户/助手/工具/系统四类角色标签，助手消息带模型与 token 明细（输入/输出/缓存），顶部显示适配器与源文件路径；「原始 JSONL」按钮切换原始行视图。数据完全复用 4.5 的适配器层（归一事件是使用记录的超集，解析一次两处消费）。UI 形态以原型变体 B 为准。

## 5. 数据模型草案（插件私有库，host-plugin-database）

```sql
-- 解析水位
parse_watermark(id INTEGER PK, adapter TEXT, source_path TEXT, size INTEGER, mtime INTEGER, parsed_at INTEGER,
                UNIQUE(adapter, source_path))
-- 会话聚合（统计查询在此表上做）
usage_session(id INTEGER PK, adapter TEXT, cli_session_id TEXT, project TEXT, title TEXT,
              started_at INTEGER, ended_at INTEGER, duration_ms INTEGER,
              model TEXT, tokens_in INTEGER, tokens_out INTEGER,
              tokens_cache_read INTEGER, tokens_cache_write INTEGER, tokens_reasoning INTEGER,
              cost_total REAL,            -- 可空，不估算
              first_seen_at INTEGER, updated_at INTEGER,
              UNIQUE(adapter, cli_session_id))
-- 供应商预设（v2 起含中心凭据列 api_key，见 §6 修订）
provider_preset(id INTEGER PK, name TEXT, base_url TEXT, api_style TEXT, models_json TEXT,
                api_key TEXT NOT NULL DEFAULT '',  -- v2 中心凭据（明文，用户决策删红线）
                notes TEXT, created_at INTEGER, updated_at INTEGER)
```

hub 设置（镜像偏好、测速缓存等）走 host-storage KV。

## 6. 安全与合规

- `process:run` 为高危权限：命令仅限 recipe 白名单 + `--version` 探测；输出落盘路径固定于插件数据目录。
- **key 存储（2026-09-14 修订，用户决策删除「hub 存储面不落 key」红线）**：`provider_preset.api_key` 明文存插件库（中心凭据库，一处配置分发多 agent）。**保留**的纪律：UI/状态/导入结果只出掩码（前 3 字符 + 长度）；日志只记 `key.len()`（`preset key set (name = …, key_len = …)` 模式）；claude 只读视图 token 掩码；`PresetDraft` Debug 掩码化防误打日志/断言泄漏。明文落库风险已知且与各 CLI 原生配置同等：pi `auth.json` / claude `settings.json` env / opencode `opencode.json` 本身即明文存 key，插件库不新增额外暴露面。
- 读取含密文件（`auth.json`/`opencode.json`/`~/.npmrc`）解析结果即掩码，不整份透传前端。
- 日志遵守 §8：结构化字段（`session_id`/`adapter` 等 `key = %value`）、级别语义、`spawn_with_error_boundary`。

## 7. UI 信息架构

**原型评审结论（2026-09-13）：变体 B「顶部分段导航」胜出**——顶部 pill 分段六段：概览 / 安装与更新 / Skills / 供应商 / 使用统计 / 会话日志；概览 = 2×2 CLI 卡片网格（官方品牌图标 + 状态徽章）+ 环境检测条。四家图标：claude / openai / opencode 取自 simple-icons（CC0），pi 为官方标（用户提供，evenodd 镂空）。原型存档 `.scratch/agent-hub/prototype/index.html`（`#variant=b`），实现以此为准、按 `frontend-styles` 规范翻译成 Vue 组件；design token 全 token-bound，暗色随 CSS 变量；i18n key 同步 zh-CN/en。

## 8. 里程碑

| 阶段 | 内容 |
| --- | --- |
| M1 | 插件骨架 + 概览（探测/双安装检测/测速/换源）+ 一键安装与更新 |
| M2 | Skills（浏览/编辑/分发/落后检测/GitHub 安装/导入） |
| M3 | 供应商（预设 CRUD/反向导入/应用/claude 特例） |
| M4 | 使用统计与会话日志（四家适配器/水位/看板/明细/日志解析视图） |

## 9. 每-CLI 适配事实（本机实查，2026-09-13）

| 项 | claude | codex | opencode | pi |
| --- | --- | --- | --- | --- |
| 本机版本 | 2.1.263 native | 0.153.4 已装未初始化 | 1.18.30 standalone（npm 另有 1.18.27） | 0.85.1 |
| 安装 recipe | native 脚本 / `claude update` | `@openai/codex` | `opencode-ai` / 官方脚本 | `@earendil-works/pi-coding-agent` |
| 会话数据 | `~/.claude/projects/<cwd→->/<id>.jsonl`，ISO8601Z | `~/.codex/`（待实机校准） | `~/.local/share/opencode/opencode.db`，session 表扁平列，epoch ms | `~/.pi/agent/sessions/<cwd桶>/<ts>_<uuid>.jsonl`，ISO8601Z |
| token 路径 | `message.usage.input_tokens` 等 snake_case | 待校准 | 列 `tokens_input/output/reasoning/cache_read/cache_write` + `cost` | `message.usage.input/output/cacheRead/cacheWrite/reasoning` camelCase + `usage.cost` |
| 供应商配置 | settings.json env 块（现桥接冲突提示） | `~/.codex/config.toml`（待校准） | `opencode.json` `provider.<n>.options` | `models.json` + `auth.json` |
| 辅助统计源 | `~/.claude.json` projects.last* | — | — | `run-history.jsonl` |

## 10. 验证（完成定义，命令字眼随 §3 黄金命令）

- Rust（插件 WASM crate + 宿主无改动验证）：`cargo test`（bedcode-desktop/src-tauri）
- 前端：`pnpm run test:run`（bedcode-desktop）+ 根目录 `pnpm exec eslint .` 0 error
- 插件 checklist §7 逐项核对（permissions/api 声明/日志 target/devMock 归位）
- UI 改动过 `frontend-styles` 自查
- dev 验证：`pnpm run tauri:dev`（bedcode-desktop）真机走通四分区

## 11. 开放问题（不阻塞 M1）

1. GitHub 直连失败的代理/镜像策略细节（v1 提示，v2 可配置）
2. opencode 双安装并存的「当前生效」判定细节（M1 以 PATH 解析实现）
3. codex 会话/配置格式实机初始化后校准
4. auto-task 并入路线（另立 `.scratch/` 立项，本 spec 只预留适配器边界）
