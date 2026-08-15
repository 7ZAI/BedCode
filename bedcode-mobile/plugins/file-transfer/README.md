# File Transfer Plugin (Mobile)

移动端内网文件传输插件：浏览桌面端共享目录，双向传输文件。作为远程控制端，传输任务由移动端发起、经桌面端插件执行。Rust WASM 层负责对端握手与传输状态，TS 前端负责 UI。

## 功能

- **远程浏览**：浏览桌面端共享目录，多选批量下载到手机
- **上传**：手机文件经共享目录上传到桌面端（系统文件选择器多选）
- **传输队列**：暂停 / 恢复 / 取消 / 重试，失败原因明示；全部 / 发送 / 接收 / 历史分页查看
- **接收策略**：逐批批准 / 自动接受 / 自动拒绝，可设批准超时；接收请求弹窗即时审批
- **保存位置**：长按文件「保存到…」系统对话框自选保存位置（SAF）
- **历史记录**：打开所在文件夹、清空记录

## 使用

两端连接后先在桌面端开启共享目录 → 在「文件传输」页浏览、下载或上传；队列面板分「全部 / 发送 / 接收 / 历史」查看进度与记录。设置页可配置共享目录、接收策略与保存位置。

## 架构

- **Rust WASM 层**：对端状态机（`peer.rs` / `handshake.rs`）、传输队列（`queue.rs`）、共享目录（`shared.rs`）、任务命令（`commands.rs`）、全局状态（`state.rs`）
- **TS 前端**：`FileTransferView`（主视图）、`ToolboxEntry`（工具箱入口）、`TaskQueueSheet`（队列面板）、`BatchRequestDialog`（接收审批）、`SharedDirSheet`（共享目录）、`SettingsPage` / `SettingsSection`（设置）
- **与桌面端通信**：经宿主 `fileservice` / `transfer` 权限访问桌面端文件服务；上传走系统 SAF 文件选择（`useSharedUpload`）

## 目录结构

```
file-transfer/
├── plugin.json          # 插件清单（权限、命令、toolbox 视图、设置页、路由）
├── rust/
│   └── src/
│       ├── lib.rs       # WASM 入口 + 命令路由
│       ├── commands.rs  # 传输任务命令
│       ├── handshake.rs # 对端握手与能力协商
│       ├── peer.rs      # 对端状态管理
│       ├── queue.rs     # 传输队列
│       ├── shared.rs    # 共享目录
│       └── state.rs     # 全局状态
├── src/
│   ├── components/      # FileTransferView / TaskQueueSheet / SharedDirSheet 等
│   ├── composables/     # useRemoteFs / useTasks / useSharedUpload / useSettings
│   └── i18n/            # 插件翻译表（zh-CN / en）
└── vite.config.ts       # Vite 配置
```

## 构建

```bash
cd bedcode-mobile
node scripts/plugin-build.js --plugin com.bedcode.file-transfer
```

产物复制到 `src-tauri/resources/plugins/mobile/com.bedcode.file-transfer/`（进 APK 资源）。

## 插件权限

| 权限 | 用途 |
|------|------|
| `fileservice` | 文件服务（浏览桌面端共享目录） |
| `transfer` | 传输通道（发送 / 接收任务） |
| `storage` | 插件独立数据库（任务与历史记录） |
| `fs:read` / `fs:write` | 本地文件读写（SAF 保存） |
| `network:http` | 与桌面端 HTTP 通信 |
| `system:open` | 打开保存目录 |
| `ui:toolbox` / `ui:route` / `ui:settings` / `ui:back` | 工具箱入口、设置页与返回导航 |
| `broadcast` / `bus` | 状态变更广播与插件间通信 |

## 命令（WASM invoke_command）

与桌面端一致：`file-transfer.list-tasks` / `query-peer` / `list-peers` / `set-active-peer` / `list-remote` / `enqueue` / `pause` / `resume` / `cancel` / `remove-task` / `resume-all` / `retry` / `set-concurrency` / `get-settings` / `set-settings` / `pick-download-dir` / `mount-local` / `update-roots` / `list-batches` / `approve-batch` / `reject-batch` / `list-receiving` / `cancel-receiving` / `list-history` / `clear-history`。
