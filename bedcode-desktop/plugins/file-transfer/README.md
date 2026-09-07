# File Transfer Plugin (Desktop)

桌面端内网文件传输插件：在线对端发现与切换、远程目录浏览、多任务并发传输（失败重试 / 取消），支持本地目录挂载供对端访问，并内置首连确认（consent）与信任对端（trusted peers）两层安全闸门。核心业务状态由 Rust WASM 层自持（issue 13 Phase 3，ADR 0022 v2/v3），TS 前端负责 UI 渲染。

## 功能

- **对端管理**：局域网设备发现（`mdns:*` 事件透传）、设备快照持久化（activate 首屏「最近可见」标注）、显式拨号与断开、活跃对端切换、入站 endpoint 记忆
- **远程浏览 / 拉取**：浏览对端共享目录（`list-remote`）、按文件清单拉取（`pull-files`），支持 retryMeta 回放
- **并发传输**：多任务并发（并发数可调），失败重试 / 取消 / 移除已完成条目；任务与历史由插件自有存储承载，引擎 `peer:transfer` / `peer:receive` 快照为进度真源
- **接收审批**：对端发起传输时按批次审批（逐批批准 / 自动接受 / 自动拒绝，`ask` 策略下前端弹窗 + 超时秒数可配）
- **首连确认 & 信任层**：未信任对端首次连接触发 consent 请求（前端弹窗 + 30s 倒计时，激活期常驻订阅，不在面板时经状态栏项跳转处理）；已信任对端可列表 / 撤销
- **目录挂载**：本地目录挂载共享供对端访问（`mount-local` / `update-roots`，注册表以路径 FNV-1a 为稳定 id），下载目录可自定义，`local-downloads` 内置只读
- **发送加密**：设置面板开关，同时作为 send 批参数默认值并推送引擎全局闸门兜底
- **历史记录**：传输历史可查询、清空；终态归档 + 200 条封顶滚动淘汰；插件重启时 running/pending 条目标注 `interrupted`
- **状态栏项**：有待确认 consent 请求时挂载状态栏项展示计数，点击经共享 router 跳回面板

## 使用

侧边栏「文件传输」→ 刷新发现设备 → 选择在线对端（首次需完成 consent 确认）→ 浏览远程目录拉取或本机选文件直发 → 加入传输队列；下载目录、并发数、接收策略与加密开关在设置面板调整。

## 架构
> 📊 架构图：[architecture.html](./docs/architecture.html)

- **Rust WASM 层**（业务自持，宿主仅提供无业务语义的引擎原语）：
  - `lib.rs` — WASM 入口 + 命令路由 + `on_message` 事件编排（`mdns:*` 透传、`peer:consent` / `peer:connection` / `peer:transfer` / `peer:receive` 派发）
  - `peer.rs` — 数据面命令编排（dial / disconnect / enqueue / cancel / retry / list-remote / pull-files / approve-batch / respond-consent / settings / mount-local 等）
  - `device_bridge.rs` — 设备快照持久化 + endpoint memo + session 句柄映射 + 停用清理
  - `transfer_store.rs` — 任务 / 历史自持存储纯函数（batchId merge、终态归档、200 封顶、interrupted 标注、retryMeta 回放）
  - `roots_registry.rs` — 共享目录注册表真源（plugin-database SQLite + `set-shared-roots` 全量推送 + 失败回滚）
  - `settings_store.rs` — 接收策略 / 下载目录 / 加密开关（插件 storage 真源，推送引擎 `set-receive-policy` / `set-download-dir` 闸门）
- **TS 前端**：`FileTransferView`（主视图）、`PeerDevicesPanel`（对端设备面板）、`TaskPanel`（传输队列）、`RemoteFileTable`（远程目录浏览）、`BatchRequestDialog`（接收批次审批）、`ConsentDialog`（首连确认弹窗）、`TrustedPeersSection`（信任对端管理）、`SettingsPanel`（设置）、`FileTypeIcon`（文件类型图标）
- **Composables**：`usePeerDevices` / `useTasks` / `useRemoteFs` / `useReceiving` / `useSettings` / `useConsent` / `useTrustedPeers` / `deviceState`（设备缓存状态机纯函数）
- **宿主能力**：`fileservice` / `transfer` 通道 + `mdns` 发现 + `peer` 连接/consent/transfer 原语 + `storage` / `fs:*` / `system:open` / `broadcast` / `bus` / `timer:schedule`

## 目录结构

```
file-transfer/
├── plugin.json                # 插件清单（权限、命令、侧边栏 + 状态栏视图）
├── rust/
│   └── src/
│       ├── lib.rs             # WASM 入口 + 命令路由 + 事件派发
│       ├── peer.rs            # 数据面命令编排与存储运行时
│       ├── device_bridge.rs   # 设备快照 / endpoint memo / session 句柄
│       ├── transfer_store.rs  # 任务与历史自持存储（纯函数 + I/O）
│       ├── roots_registry.rs  # 共享目录注册表真源
│       └── settings_store.rs  # 接收策略 / 下载目录 / 加密设置
├── scripts/
│   └── build.js               # 统一构建脚本（Vite + Cargo WASM + Component Model + 复制产物；支持 --watch / --frontend-only / --rust-only）
├── src/
│   ├── components/            # FileTransferView / PeerDevicesPanel / TaskPanel / RemoteFileTable / BatchRequestDialog / ConsentDialog / TrustedPeersSection / SettingsPanel / FileTypeIcon
│   ├── composables/           # usePeerDevices / useTasks / useRemoteFs / useReceiving / useSettings / useConsent / useTrustedPeers / deviceState
│   ├── i18n/                  # 插件翻译表（zh-CN / en + schema）
│   ├── utils/                 # 通用工具（format 等）
│   ├── devMock.ts             # dev-shell 领域种子数据（真实宿主忽略）
│   ├── types.ts               # 业务类型（Task / Peer / Batch 等）
│   └── index.ts               # 插件入口（UI 注册、consent 编排、样式注入）
└── vite.config.ts             # Vite 配置
```

## 构建

```bash
cd bedcode-desktop/plugins/file-transfer
node scripts/build.js              # 完整构建（前端 + Rust + Component Model + 复制）
node scripts/build.js --watch      # 前端 watch（配合宿主 PluginDevWatcher 热重载）
node scripts/build.js --frontend-only
node scripts/build.js --rust-only
```

构建脚本串联：`vite build` → `cargo build --target wasm32-unknown-unknown --features wasm` → Component Model 编码（`packages/plugin-sdk-desktop/rust/tools/componentize`）→ 复制 `index.js` / `plugin.json` / `icon.svg` / `*.wasm` 到 `src-tauri/resources/plugins/desktop/com.bedcode.file-transfer/`。

> 产物目录（`**/src-tauri/resources/plugins/`）已加入 .gitignore，打包/运行前需先执行构建。

## 插件权限

| 权限 | 用途 |
|------|------|
| `mdns` | 局域网设备发现（`mdns:found` / `mdns:lost` 事件） |
| `peer` | 对等连接 / consent / transfer / trusted 原语 |
| `fileservice` | 文件服务（目录挂载 / 远程浏览） |
| `transfer` | 传输通道（send / receive / pull 任务） |
| `storage` | 插件独立 storage（设备快照、设置、预授权路径） |
| `fs:read` / `fs:write` | 本地文件读写 |
| `network:http` | 对端 HTTP 通信 |
| `system:open` | 打开下载目录 / 传输文件 |
| `broadcast` / `bus` | 状态变更广播与插件间通信 |
| `timer:schedule` | consent 弹窗倒计时 |
| `ui:sidebar` | 侧边栏「文件传输」视图 |
| `ui:statusbar` | 状态栏项（待确认 consent 计数） |

## 命令（WASM invoke_command）

| 命令 | 用途 |
|------|------|
| `file-transfer.refresh-devices` | 触发宿主即时重查设备缓存 |
| `file-transfer.get-device-snapshot` / `save-device-snapshot` | 设备快照读取 / 持久化 |
| `file-transfer.dial-peer` / `disconnect-peer` / `set-active-peer` / `remember-peer-endpoint` | 拨号、断开、活跃对端切换、入站 endpoint 登记 |
| `file-transfer.pick-files` | 系统多选文件选择器 |
| `file-transfer.enqueue` / `list-tasks` / `cancel` / `retry` / `remove-task` | 传输任务控制与查询 |
| `file-transfer.list-batches` / `approve-batch` / `reject-batch` | 接收批次审批 |
| `file-transfer.list-receiving` / `cancel-receiving` | 接收任务查询与取消 |
| `file-transfer.list-history` / `clear-history` | 传输历史 |
| `file-transfer.respond-consent` / `list-trusted` / `revoke-trusted` | 首连确认应答与信任对端管理 |
| `file-transfer.list-remote` / `pull-files` | 远程目录浏览与按文件清单拉取 |
| `file-transfer.get-settings` / `set-settings` / `pick-download-dir` | 设置读写与下载目录选择 |
| `file-transfer.mount-local` / `update-roots` | 本地目录挂载与注册表更新 |
