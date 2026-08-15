# Auto Task Plugin (Mobile)

移动端自动任务插件：作为远程控制端，把任务排队交给桌面端的 Claude Code 依次自动执行，支持定时任务、自动应答与任务历史。Rust WASM 层为极简实现（仅激活/停用日志），业务逻辑由 TS 前端通过 HTTP API 与桌面端插件通信完成。

## 功能

- **任务队列**：添加 / 移除 / 清空，执行中可取消；队列顺序调度，空闲时自动下发首轮任务
- **自动执行**：开启后队列任务自动调度到目标会话（Claude Code / opencode / pi 等 CLI Agent）
- **自动应答**：Agent 提问时按预设内容自动回复
- **预设任务**：常用任务一键预存，随时入队
- **定时任务**：按时间点自动触发，错过可重设；支持关联会话与任务提示词
- **任务历史**：状态统计、筛选、失败任务重试

## 使用

终端会话工具栏「自动任务」或工具箱入口 → 选择目标会话后创建任务入队，开启自动执行即按队列顺序调度；定时任务在「定时任务」标签页管理。

## 架构

- **Rust WASM 层**：仅 activate / deactivate 日志（业务逻辑在 TS 层）
- **TS 前端**：`AutoTaskToolboxView` / `AutoTaskPanelHost`（队列面板）、`ScheduledJobsTab`（定时任务）、`TaskHistoryTab`（历史）、composables（`useScheduledJobs` / `useTaskHistory`）
- **与桌面端通信**：通过宿主 HTTP API 访问桌面端插件端点（`/api/plugin/com.bedcode.auto-task/…`），队列调度与 hooks 安装等核心逻辑在桌面端

## 目录结构

```
auto-task/
├── plugin.json          # 插件清单（权限、命令、toolbox 视图、生命周期钩子）
├── rust/
│   └── src/
│       └── lib.rs       # WASM 入口（极简：激活/停用日志）
├── src/
│   ├── components/      # AutoTaskToolboxView / AutoTaskPanelHost / ScheduledJobsTab / TaskHistoryTab
│   ├── composables/     # useScheduledJobs / useTaskHistory
│   ├── state.ts         # 插件前端共享状态
│   └── i18n.ts          # 插件翻译表（zh-CN / en）
└── vite.config.ts       # Vite 配置
```

## 构建

```bash
cd bedcode-mobile
node scripts/plugin-build.js --plugin com.bedcode.auto-task
```

产物复制到 `src-tauri/resources/plugins/mobile/com.bedcode.auto-task/`（进 APK 资源）。

## 生命周期钩子

| 钩子 | 触发时机 | 行为 |
|------|----------|------|
| `onAuthSuccess` | 连接桌面端认证成功 | 初始化插件状态 |
| `onDisconnect` | 与桌面端断开 | 清理会话状态 |
| `onSessionCreated` | 桌面端会话创建 | 挂接会话队列状态 |
| `onSessionStopped` | 会话停止 | 清理会话映射 |

## 插件权限

| 权限 | 用途 |
|------|------|
| `session:read` | 读取会话信息（目标会话选择） |
| `storage` | 插件独立数据库（队列 / 定时任务 / 历史） |
| `ui:input` | 终端工具栏按钮（打开队列面板） |
| `ui:toolbox` | 工具箱「自动任务」入口 |

## 命令（WASM invoke_command）

| 命令 | 用途 |
|------|------|
| `auto-task.list-queue` | 查询会话任务队列 |
| `auto-task.add-task` | 添加任务到队列 |
| `auto-task.remove-task` | 从队列移除任务 |
| `auto-task.clear-queue` | 清空任务队列 |
