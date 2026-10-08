# Terminal Session Plugin (Mobile)

移动端内置「远程终端控制端」wasm app：与桌面端 `com.bedcode.terminal-session` **同名但职责不同**（spec D6 选项 A）——桌面端是会话/任务/终端的**权威**（持有 PTY、WS 服务端、认证中心桥接），本端只做订阅消费、会话控制调用与任务面板的只读投影（ADR 0012「手机看、桌面管」）。两端契约独立（ADR 0018），不因同名互相约束。

## 域划分

| 域 | 位置 | 内容 |
| --- | --- | --- |
| 终端域 | `rust/src/`（WASM 后端） | 终端订阅协议客户端：subscribe 门控 / ack 节流 / ring_resync 重锚 / special-key 翻译 / 退避重连随迁（票 12 自宿主 `terminal_link.rs` 迁入）；认证 / 配对编排（票 14 阶段 B） |
| 任务域 | `src/task/`（TS 前端） | 原 `com.bedcode.auto-task` 插件整体并入（票 16）：任务队列面板 / 工具箱「任务记录 + 定时任务」/ 桌面任务域 HTTP 客户端 |
| 终端消费 UI | 待票 15 迁入（`TerminalView` / `terminalBuffer` 等） | 终端输出消费与渲染 |

## 功能

- **终端流**：订阅回放 + 实时输出，断线由宿主自动重连（`ws:open` 新句柄重新订阅 + `terminal-resync` 重锚）
- **输入**：可打印文本与特殊键（Ctrl+C / 方向键 / 功能键）直达桌面 PTY；流控 ack 保证渲染背压
- **自动任务**：任务队列面板（终端工具栏入口，仅对适配 agent 的会话显示）；添加 / 移除 / 清空 / 重排序 / 编辑，执行中可取消；自动执行 / 自动应答开关
- **预设任务**：常用任务一键预存入队（宿主预设任务 composable）
- **定时任务**：按时间点自动触发（桌面任务域实体，本端只读 + 创建）
- **任务记录**：状态筛选、下拉刷新、分页加载（桌面任务域实体，本端只读）

## 目录结构

```
terminal-session/
├── plugin.json          # 插件清单（权限并集：终端域 + 任务域）
├── src/
│   ├── index.ts         # 插件入口：组合各域 activate/deactivate + devMock 导出
│   └── task/            # 任务域（票 16 合并，按域重组）
│       ├── activate.ts  # 任务域激活编排（i18n / 面板挂载 / 工具箱注册 / 工具栏可见性）
│       ├── api.ts       # 桌面端 REST API 强类型封装（基址 /api/plugin/com.bedcode.terminal-session）
│       ├── state.ts     # 面板可见性共享状态
│       ├── i18n.ts      # 任务域翻译表（zh-CN / en）
│       ├── devMock.ts   # dev-shell 队列种子（PluginDevMock 协议）
│       ├── panel.css / toolbox.css
│       ├── components/  # AutoTaskPanelHost / AutoTaskToolboxView / ScheduledJobsTab / TaskHistoryTab
│       └── composables/ # useScheduledJobs / useTaskHistory
└── rust/                # WASM 后端：终端订阅协议 + 认证编排（见上表）
```

## 构建

```bash
cd bedcode-mobile
node scripts/plugin-build.js --plugin com.bedcode.terminal-session
```

产物复制到 `src-tauri/resources/plugins/mobile/com.bedcode.terminal-session/`（进 APK 资源）。

## 插件权限

| 权限 | 用途 |
|------|------|
| `auth` | 认证 / 配对编排（host-auth 5 原语，凭据零过境） |
| `bus` | ws 属主 topic 订阅（状态事件 + 二进制帧信封） |
| `ws:client` | 终端 WS 出站连接（host-websocket 客户端域） |
| `terminal:output` | 输出字节窄转发（`host-terminal-stream`） |
| `session:read` | 读取会话信息（任务域目标会话选择） |
| `storage` | 插件存储（任务域预留） |
| `ui:input` | 终端工具栏按钮（打开任务队列面板） |
| `ui:toolbox` | 工具箱「自动任务」入口 |

## 命令（plugin.json `contributes.commands`）

命令 id 统一 `terminal-session.` 前缀，经插件命令面（`plugin_invoke`）调用：

- 终端域：`subscribe` / `unsubscribe` / `unsubscribe-all` / `remove` / `send-input` / `ack-rendered` / `get-state`
- 认证域：`request-pairing` / `verify-pairing-code` / `authenticate-with-qr` / `authenticate-with-biometric`

任务域不注册 WASM 命令：业务全走 TS 前端 HTTP（桌面端任务端点），原 auto-task 的 `auto-task.*` 命令（rust 侧显式全拒、TS 从未调用）随合并退役，回接由 `src-tauri/tests/retired_mobile_auto_task_plugin_lock.rs` 拦截。
