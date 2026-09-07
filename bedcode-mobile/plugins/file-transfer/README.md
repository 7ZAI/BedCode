# File Transfer Plugin (Mobile)

移动端内网文件传输插件：浏览桌面端共享目录，双向传输文件。作为远程控制端，传输任务由移动端发起、经桌面端插件执行。Rust WASM 层接管业务状态（设备缓存出口、共享根注册表、任务存储、接收策略），宿主只提供无业务语义的引擎原语；TS 前端负责 UI 与设备缓存状态机。

## 功能

- **远程浏览**：浏览桌面端共享目录，多选批量下载到手机
- **上传**：手机文件经共享目录上传到桌面端（系统文件选择器多选）
- **传输队列**：取消 / 重试，失败原因明示；全部 / 发送 / 接收 / 历史分页查看（队列生命周期由插件自有存储管理，`pause` / `resume` / `resume-all` 已下线）
- **接收策略**：逐批批准 / 自动接受 / 自动拒绝，可设批准超时；接收请求弹窗即时审批
- **首连确认**：终端配对迁移规则 + 全局确认弹窗，确认后写入可信对端（含撤销）
- **保存位置**：长按文件「保存到…」系统对话框自选保存位置（SAF）；下载默认落 `MediaStore.Downloads`，不支持自定义下载目录
- **历史记录**：打开所在文件夹、清空记录；终态归档 + 200 条封顶滚动

## 使用

两端连接后先在桌面端开启共享目录 → 在「文件传输」页浏览、下载或上传；队列面板分「全部 / 发送 / 接收 / 历史」查看进度与记录。设置页可配置共享目录（SAF 树授权）与接收策略；移动端下载目录固定为系统下载目录，不支持自定义。

## 架构

> 📊 架构图：[architecture.html](./docs/architecture.html)

- **Rust WASM 层**：WASM 入口与命令路由（`lib.rs`）、设备发现桥接与快照持久化（`device_bridge.rs`）、共享根注册表（`roots_registry.rs`）、任务/历史自持存储（`transfer_store.rs`）、接收策略设置（`settings_store.rs`）、数据面命令编排（`peer.rs`）
- **TS 前端**：`FileTransferView`（三段式主视图：传输/浏览/设备）+ Tab 子组件（`TransfersTab` / `BrowseTab` / `DevicesTab`）、卡片与工具（`TaskCard` / `PeerHeader` / `SummaryBar` / `TabBar` / `FileTypeIcon` / `EmptyState`）、审批与信任（`BatchRequestDialog` / `TrustedPeersSection`）、设置（`SettingsPage` / `SettingsSection`）、工具箱入口（`ToolboxEntry`）
- **设备缓存**：`deviceState.ts` 纯函数状态机（去重 / TTL / 能力位）自持于前端，Rust 侧只透传 `mdns:*` 事件与持久化快照
- **与桌面端通信**：经宿主 `fileservice` / `transfer` / `peer` 权限访问桌面端文件服务；上传走系统 SAF 文件选择（`platform_pick_files` → `enqueue`）；`storage` 承载插件自有 KV 真源（设备快照 / 共享根 / 接收策略）

## 目录结构

```
file-transfer/
├── plugin.json          # 插件清单（权限、命令、toolbox 视图、设置页、路由）
├── rust/
│   └── src/
│       ├── lib.rs              # WASM 入口 + 命令路由 + 总线消息分发
│       ├── device_bridge.rs    # 设备发现桥接 + 快照持久化 + endpoint memo
│       ├── peer.rs             # 数据面命令编排 + 存储运行时
│       ├── roots_registry.rs   # 共享根注册表真源（storage KV + 引擎推送）
│       ├── settings_store.rs   # 接收策略设置自持（storage KV + 闸门推送）
│       └── transfer_store.rs   # 任务/历史自持存储（快照 merge + 归档）
├── src/
│   ├── index.ts               # 插件入口：注册 i18n / 视图 / 路由 / 设置区 / 首连确认
│   ├── types.ts               # 业务类型（camelCase 干净类型，宿主 DTO 由 Rust 翻译）
│   ├── devMock.ts             # dev-shell 领域种子数据（SDK PluginDevMock 协议）
│   ├── styles.css             # 插件全局样式（运行时注入）
│   ├── components/            # 15 个 .vue：FileTransferView + 3 Tab + TaskCard/PeerHeader/
│   │                          #   SummaryBar/TabBar/FileTypeIcon/EmptyState + BatchRequestDialog/
│   │                          #   TrustedPeersSection + SettingsPage/SettingsSection + ToolboxEntry
│   ├── composables/           # useRemoteFs / useTasks / usePeerDevices / useSettings /
│   │                          #   useConsent / useTrustedPeers + deviceState（纯函数）
│   ├── utils/                 # format.ts（文件大小 / 时间等格式化）
│   └── i18n/                  # 插件翻译表（zh-CN / en + messages 类型）
├── package.json
├── tsconfig.json
└── vite.config.ts             # Vite 配置
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
| `peer` | 对端原语（拨号、会话、可信对端、同意请求） |
| `mdns` | 局域网设备发现（`mdns:found` / `mdns:lost` 透传） |
| `storage` | 插件独立 KV（设备快照 / 共享根 / 接收策略） |
| `fs:read` / `fs:write` | 本地文件读写（SAF 保存） |
| `network:http` | 与桌面端 HTTP 通信 |
| `system:open` | 打开保存目录 |
| `ui:toolbox` / `ui:route` / `ui:settings` / `ui:back` | 工具箱入口、设置页与返回导航 |
| `broadcast` / `bus` | 状态变更广播与插件间通信 |

## 命令（WASM invoke_command）

按功能分组，均经 `lib.rs::invoke_command` 路由（清单以 `plugin.json` 的 `contributes.commands` 为准）：

| 分组 | 命令 |
|------|------|
| 设备 | `refresh-devices`（触发宿主即时重查）· `get-device-snapshot` / `save-device-snapshot`（快照持久化） |
| 连接 | `dial-peer` · `disconnect-peer` · `set-active-peer` · `remember-peer-endpoint` |
| 发送 | `pick-files`（SAF 多选）· `enqueue` · `list-tasks` · `cancel` · `retry` |
| 接收 | `list-batches` · `approve-batch` / `reject-batch` · `list-receiving` · `cancel-receiving` |
| 历史 | `list-history` · `clear-history` |
| 信任 | `respond-consent`（首连确认）· `list-trusted` · `revoke-trusted` |
| 远端浏览 | `list-remote` · `pull-files` |
| 设置 | `get-settings` · `set-settings` · `mount-local` · `update-roots` |

全部前缀 `file-transfer.`。以下命令在移动端返回 `unsupported`（`plugin.json` 已声明以保持双端清单一致，但 `lib.rs` 显式拒绝）：`remove-task`（生命周期由插件自有存储管理）、`pick-download-dir`（移动端下载固定 `MediaStore.Downloads`）、`pause` / `resume` / `resume-all`（未声明、亦由 `lib.rs` 兜底拒绝）。桌面端专用命令（如 `set-concurrency`）在移动端清单中不存在。
