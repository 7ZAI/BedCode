# Auto Task Plugin (Mobile)

移动端自动任务插件：作为远程控制端，把任务排队交给桌面端的 Claude Code 依次自动执行，支持定时任务、自动应答与任务历史。Rust WASM 层为极简实现（仅激活/停用日志；`invoke_command` 显式拒绝所有命令），业务逻辑由 TS 前端通过 HTTP API 与桌面端插件通信完成。

## 功能

- **任务队列**：添加 / 移除 / 清空，执行中可取消；队列顺序调度，空闲时自动下发首轮任务
- **自动执行**：开启后队列任务自动调度到目标会话（Claude Code / opencode / pi 等 CLI Agent）
- **自动应答**：Agent 提问时按预设内容自动回复
- **预设任务**：常用任务一键预存，随时入队
- **定时任务**：按时间点自动触发；关联会话配置与任务提示词
- **任务历史**：状态筛选、下拉刷新、分页加载
- **工具栏入口动态可见**：仅当当前会话使用后端适配的 agent（白名单来自 Rust `AGENT_PROFILES`）时才显示入口，避免未适配 agent 打开后发现不可用

## 使用

终端会话工具栏「自动任务」或工具箱入口 → 选择目标会话后创建任务入队，开启自动执行即按队列顺序调度；工具箱内「任务记录」查看历史任务，「定时任务」标签页管理定时任务。

## 架构

> 📊 架构图：[architecture.html](./docs/architecture.html)

- **Rust WASM 层**：`manifest()` 从 `plugin.json` 读取（单一事实来源）；`activate` / `deactivate` 仅日志；`invoke_command` 显式拒绝所有命令（业务走 TS/HTTP）
- **TS 前端**：`index.ts` 入口（i18n 注册、面板挂载、工具箱注册、会话/连接 `watch`、工具栏可见性）；`api.ts` 业务 API 封装；`AutoTaskToolboxView` / `AutoTaskPanelHost`（队列面板）、`ScheduledJobsTab`（定时任务）、`TaskHistoryTab`（任务记录）、composables（`useScheduledJobs` / `useTaskHistory`）
- **与桌面端通信**：通过宿主 HTTP API 访问桌面端插件端点（`/api/plugin/com.bedcode.auto-task/…`），队列调度与 hooks 安装等核心逻辑在桌面端

## 目录结构

```
auto-task/
├── plugin.json          # 插件清单（权限、命令、toolbox 视图、生命周期钩子）
├── package.json         # 依赖与构建脚本（bedcode-plugin build/package）
├── icon.svg             # 插件图标（工具栏 / 工具箱入口）
├── tsconfig.json        # TypeScript 配置
├── vite.config.ts       # Vite 构建配置
├── rust/
│   └── src/
│       └── lib.rs       # WASM 入口（manifest 读 plugin.json；invoke_command 显式拒绝）
└── src/
    ├── index.ts         # 前端入口：i18n / 面板挂载 / 工具箱注册 / 会话 watch / 工具栏可见性
    ├── api.ts           # 桌面端 REST API 强类型封装（队列 / 历史 / 定时任务 / 会话 / 支持 agent）
    ├── state.ts         # 插件前端共享状态（面板可见性）
    ├── i18n.ts          # 插件翻译表（zh-CN / en）
    ├── panel.css        # 任务队列面板样式
    ├── toolbox.css      # 工具箱页样式
    ├── env.d.ts         # TS 环境声明
    ├── components/      # AutoTaskToolboxView / AutoTaskPanelHost / ScheduledJobsTab / TaskHistoryTab
    └── composables/     # useScheduledJobs / useTaskHistory
```

## 构建

```bash
cd bedcode-mobile
node scripts/plugin-build.js --plugin com.bedcode.auto-task
```

产物复制到 `src-tauri/resources/plugins/mobile/com.bedcode.auto-task/`（进 APK 资源）。

## 生命周期钩子

`plugin.json` 的 `contributes.lifecycle` 声明以下钩子为 `true`（启用）；当前插件未显式注册 handler，会话/连接相关逻辑由 `watch(activeSessionId)` 与 `watch(isConnected)` 驱动（`index.ts` 内），插件 SDK 负责事件派发。

| 钩子 | 触发时机 |
|------|----------|
| `onAuthSuccess` | 连接桌面端认证成功 |
| `onDisconnect` | 与桌面端断开 |
| `onSessionCreated` | 桌面端会话创建 |
| `onSessionStopped` | 会话停止 |

## 插件权限

| 权限 | 用途 |
|------|------|
| `session:read` | 读取会话信息（目标会话选择） |
| `storage` | 插件独立数据库（队列 / 定时任务 / 历史） |
| `ui:input` | 终端工具栏按钮（打开队列面板） |
| `ui:toolbox` | 工具箱「自动任务」入口 |

## 命令（plugin.json `contributes.commands`）

移动端 WASM `invoke_command` 显式拒绝所有命令（业务逻辑走 TS/HTTP 到桌面端），命令 ID 仅用于插件清单声明。

| 命令 ID | 用途 |
|---------|------|
| `auto-task.list-queue` | 查询会话任务队列 |
| `auto-task.add-task` | 添加任务到队列 |
| `auto-task.remove-task` | 从队列移除任务 |
| `auto-task.clear-queue` | 清空任务队列 |
