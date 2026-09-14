# 12 — 切换 + 删除旧管线（contract）

**What to build:** expand–contract 的收口票：新对等链路通过功能对齐验收后，同一版本内删除旧宿主中心链路——终端 WS 上的文件服务控制面消息族、intent 协调层、旧 announce 机制及其宿主命令与 host functions 一并移除；file-transfer 插件仅保留对等链路路径。验证升级兼容：既有传输历史与本机配置在新版保留；双端真机全链路回归（发现/首连/互信迁移/推送/扇出/续传/浏览拉取/历史）。词汇表与 spec 中被删除机制的引用同步清理。

**Blocked by:** 09, 10, 11

**Status:** done（代码与测试收口完成；真机回归待用户执行）

- [x] 代码库中不再存在旧链路符号（编译级消失，非注释弃用）
- [x] 全部相关测试清理或迁移后 `cargo test` 与两端 `test:run` 绿
- [ ] 真机回归清单（spec Further Notes 场景）逐项通过：含手机↔手机、桌面↔桌面、已配对自动互信
- [ ] 升级安装验证：历史与配置保留
- [x] CONTEXT.md / ADR 引用一致性检查通过

## Comments

### 实现记录（2026-08-23，切换收口）

**架构纠偏落地**：issues 08–11 曾把对等传输 UI 做成宿主原生页面（方向错误）。本票最终形态 = **file-transfer 插件是对等传输的功能载体，宿主只提供基础能力原语**。宿主原生对等 UI（08–11 产物，全部为未跟踪文件）暂留双轨、后续另立票删除。

**已删除（编译级消失）**：
- 终端 WS `Message::FileService` 消息族、intent 协调层（`SyncPayload::FileTransferIntent` 族 + responder）、announce 机制
- filesrv mount 栈：registry/sandbox/cipher/upload/transfer/list_pending/client HTTP 栈、`FileServicePayload`、`plugin_filesrv_*` 命令族
- SDK：`FileServiceAPI`/`HostFileService`/`HostTransfer` trait 与 TS 类型、upload-hook/transfer-request-hook（WIT + 宏 + 权限表）

**新增**：
- 两端 WIT `interface host-peer`（24 fn，JSON 载荷过界）+ SDK `HostPeer` trait + 权限 `peer`
- 宿主 `host_impl/peer.rs` 桥接（桌面 28 fn；移动 block_on 驱动），复用 `peer_net/peer_transfer/peer_receive/peer_remote` 的 pub async 命令实现（零复制）
- peer 事件桥接到插件消息总线：两端 `emit_json` 内按名映射 `peer:devices/connection/consent/transfer/receive` topic 同步发布

**file-transfer 插件改造 ×2（HostPeer 薄代理）**：
- Rust 后端重写为单文件翻译层：删 handshake/queue/state/mount/intent 全部自管状态机，`invoke_command("file-transfer.*")` → `host.peer_*` 并把宿主批级 `PeerTransferDto` 翻译成前端既有 wire 形状；`activate` 订阅 `peer:*` 总线 topic，`on_bus_message` 翻译后经 emit_event 推给前端既有事件名
- 前端语义变化：任务/历史均为**批粒度**（一批 = 一条记录）；发送 = 系统文件选择器直发（移动端 SAF 选图换算真实路径，中转复制废除）；接收应答卡 = 宿主 pending 批；暂停/恢复/删除单任务随宿主托管生命周期移除；移动端共享目录添加走宿主 SAF 目录树选择器
- 两端 `npm run build`（vite + wasm32 + componentize）通过

**双轨期说明**：宿主原生对等 UI 的 composables/views（`usePeer*.ts`、`Peer*.vue` 等，未入库）与插件链路并存，互不引用；后续票删除宿主侧。

**验证矩阵**：mobile `cargo test --lib` 296 ✓；desktop `cargo test --lib` 507 ✓；`packages/peer-net` 95 ✓；mobile vitest 259 ✓；desktop vitest 497 ✓；根目录 eslint 改动面 0 error。无 Kotlin 改动，gradle 不适用。

**文档**：CONTEXT.md 文件传输词条按批粒度模型更新（删「中转复制」）；ADR-0021 头部标注「已取代」；ADR-0001/0002 为传输栈/信任层决策记录仍然有效未动；spec.md 决策属历史记录保留。
