# File Transfer Plugin (Desktop)

桌面端内网文件传输插件：在线对端发现与切换、远程目录浏览、多任务并发传输（暂停 / 恢复 / 断点续传 / 失败重试），支持本地目录挂载供对端访问。核心业务逻辑在 Rust WASM 层实现，TS 前端负责 UI 渲染。

## 功能

- **对端管理**：在线对端发现与切换（局域网 + 移动端经 WebSocket 直连）
- **远程浏览**：浏览对端共享目录，多选批量加入传输队列
- **并发传输**：多任务并发（并发数可调），暂停 / 恢复 / 断点续传 / 失败重试，失败原因明示
- **接收审批**：对端发起传输时按批次审批（逐批批准 / 自动接受 / 自动拒绝，可设批准超时）
- **目录挂载**：本地目录挂载共享供对端访问，下载目录可自定义
- **历史记录**：传输历史可查询、清空；完成后可打开所在文件夹

## 使用

侧边栏「文件传输」→ 选择在线对端 → 浏览远程文件 → 加入传输队列；下载目录与并发数在设置面板调整。

## 架构

- **Rust WASM 层**：对端状态机（`peer.rs` / `handshake.rs`）、传输队列与调度（`queue.rs`）、任务命令（`commands.rs`）、全局状态（`state.rs`）
- **TS 前端**：`FileTransferView`（主视图）、`RemoteFileTable`（远程目录浏览）、`TaskPanel`（传输队列）、`BatchRequestDialog`（接收审批）、`SettingsPanel`
- **宿主能力**：基于宿主 `fileservice` / `transfer` 权限实现文件服务与传输通道，`system:open` 打开下载目录

## 目录结构

```
file-transfer/
├── plugin.json          # 插件清单（权限、命令、侧边栏视图）
├── rust/
│   └── src/
│       ├── lib.rs       # WASM 入口 + 命令路由
│       ├── commands.rs  # 传输任务命令（列表/暂停/恢复/取消/重试…）
│       ├── handshake.rs # 对端握手与能力协商
│       ├── peer.rs      # 对端状态管理
│       ├── queue.rs     # 传输队列与并发调度
│       └── state.rs     # 全局状态
├── scripts/
│   └── build.js         # 统一构建脚本（Vite + Cargo WASM + 复制产物）
├── src/
│   ├── components/      # FileTransferView / TaskPanel / RemoteFileTable 等
│   ├── composables/     # usePeer / useRemoteFs / useTasks / useReceiving / useSettings
│   └── i18n/            # 插件翻译表（zh-CN / en）
└── vite.config.ts       # Vite 配置
```

## 构建

```bash
cd bedcode-desktop/plugins/file-transfer
node scripts/build.js
```

构建脚本串联：`vite build` → `cargo build`（WASM，Component Model 编码）→ 复制产物到 `src-tauri/resources/plugins/desktop/com.bedcode.file-transfer/`。

> 产物目录（`**/src-tauri/resources/plugins/`）已加入 .gitignore，打包/运行前需先执行构建。

## 插件权限

| 权限 | 用途 |
|------|------|
| `fileservice` | 文件服务（目录挂载 / 远程浏览） |
| `transfer` | 传输通道（发送 / 接收任务） |
| `storage` | 插件独立数据库（任务与历史记录） |
| `fs:read` / `fs:write` | 本地文件读写 |
| `network:http` | 对端 HTTP 通信 |
| `system:open` | 打开下载目录 / 传输文件 |
| `broadcast` / `bus` | 状态变更广播与插件间通信 |
| `ui:sidebar` | 侧边栏「文件传输」视图 |

## 命令（WASM invoke_command）

| 命令 | 用途 |
|------|------|
| `file-transfer.list-peers` / `query-peer` / `set-active-peer` | 对端发现、状态查询、切换 |
| `file-transfer.list-remote` | 浏览远程文件 |
| `file-transfer.enqueue` / `pause` / `resume` / `cancel` / `retry` / `resume-all` | 传输任务控制 |
| `file-transfer.list-tasks` / `remove-task` | 任务列表与移除 |
| `file-transfer.set-concurrency` | 设置并发数 |
| `file-transfer.get-settings` / `set-settings` / `pick-download-dir` | 设置读写与下载目录选择 |
| `file-transfer.mount-local` / `update-roots` | 本地目录挂载 |
| `file-transfer.list-batches` / `approve-batch` / `reject-batch` | 接收批次审批 |
| `file-transfer.list-receiving` / `cancel-receiving` | 接收任务查询与取消 |
| `file-transfer.list-history` / `clear-history` | 传输历史 |
