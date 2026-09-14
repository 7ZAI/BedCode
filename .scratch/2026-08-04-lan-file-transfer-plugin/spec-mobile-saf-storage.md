# 移动端文件传输 SAF 化改造规格

Status: ready-for-agent

## Problem Statement

个人开发应用无法通过 Google Play 政策获取 MANAGE_EXTERNAL_STORAGE（All Files）权限，且国产 ROM（MIUI/HyperOS）该权限入口不稳定、用户拒绝率高。当前移动端文件传输的整条链路依赖真实路径直读直写：上传源经 `_data` 列/路径解析，公共目录文件在无 All Files 时读取失败；下载默认落 app 私有目录（用户不可见）；桌面端浏览手机共享目录返回空列表。需求是：**零依赖 All Files 的前提下，移动端文件传输（上传 / 下载接收 / 共享浏览）全功能可用**，对齐国际网盘路径（SAF + MediaStore + 私有目录）。

## Solution

移动端存储访问全部基于 SAF（Storage Access Framework）+ MediaStore：

- **共享目录**（上传源 + 桌面端浏览源）：条目以 SAF URI（`content://tree/...`）存储，经系统目录选择器选择并持久化授权；App 内直接列出目录文件，不弹系统选择器；旧真实路径条目废除
- **中转复制**：SAF 源 → 私有 cache 的顺序流复制（不可续传），复制完成后走现有传输引擎（可续传）
- **下载目录**（接收统一落点）：默认 MediaStore.Downloads（公共下载、用户可见、API 29+ 零权限），写入失败回退 app 私有下载目录；移动端主动下载与桌面端推送均落此
- **file_service 三端点 SAF 化**：桌面端浏览/拉取手机共享目录、推送文件到手机，全部经 SAF（cache 中转模式）
- Rust 传输引擎、协议、挂载声明零改动

## User Stories

1. 作为移动端用户，我想用系统目录选择器添加共享目录，以便把手机目录授权给文件传输（一次选择、重启仍有效）
2. 作为移动端用户，我想在 App 内浏览共享目录的文件列表（含子目录），以便不弹系统选择器直接挑选文件
3. 作为移动端用户，我想选中共享目录中的文件上传到桌面端，以便把手机文件传给电脑
4. 作为移动端用户，我想在上传的复制准备阶段看到「准备中」进度并可取消，以便大文件上传前了解等待时间
   （M3 已移除该阶段：上传 SAF 流直传直接入队，无中转复制步骤；进度由任务队列展示）
5. 作为移动端用户，我想上传中断后从断点继续，以便不整文件重传（引擎段续传）
6. 作为移动端用户，我想复制阶段取消/失败后无残留文件，以便不占手机空间
7. 作为移动端用户，我想从桌面端下载文件后能在系统「下载」目录找到它，以便用系统文件管理器直接访问
8. 作为移动端用户，我想下载中断后从断点继续，以便大文件下载不重来（30%/60%/90% 中断验证）
9. 作为移动端用户，我想 MediaStore 写入失败时自动回退私有下载目录并提示，以便下载不静默失败
10. 作为移动端用户，我想撤销授权或删除目录后看到「共享目录失效」提示并可重新授权，以便及时修复
11. 作为移动端用户，我想继续把 app 私有下载目录作为免授权共享条目，以便无需授权即可共享该目录
12. 作为移动端用户，我想取消下载任务时清理本地 `.part` 残留，以便不残留半成品文件
13. 作为移动端用户，我想在无 All Files 授权时一切功能正常（无 EACCES、无空列表），以便不依赖特殊权限
14. 作为移动端用户，我想上传大文件时全程流式（不整文件加载内存），以便几十 GB 文件可传
15. 作为桌面端用户，我想浏览移动端共享目录的文件列表，以便查看手机上有哪些文件
16. 作为桌面端用户，我想从移动端共享目录拉取文件到桌面（支持断点续传），以便手机文件进电脑
17. 作为桌面端用户，我想把桌面文件推送到手机并让它落到系统下载目录，以便「发送到手机」后用户能直接找到文件
18. 作为桌面端用户，我想在移动端共享目录不可用时看到明确提示而非空列表，以便知道是权限问题
19. 作为桌面端用户，我想上传到移动端遇到同名文件时被明确拒绝，以便不静默覆盖（现有行为保留）
20. 作为移动端用户，我想文件传输界面明示内网明文传输风险，以便只在可信网络使用（现有行为保留）

## Implementation Decisions

**架构决策**（详见 `docs/adr/0009`，术语见 `CONTEXT.md`「文件传输」分组）：
- 存储访问完全基于 SAF + MediaStore，对 MANAGE_EXTERNAL_STORAGE 零依赖；`AllFilesAccessPlugin` 引导仅作可选体验增强，不进入本改造
- 共享目录 = 只读暴露语义（桌面端浏览/拉取）；下载目录 = 接收落点语义（移动端下载 + 桌面端推送统一落此，不落共享目录）；上传源 = 共享目录

**主 seam：Rust `SafIo` trait 抽象**（新增，唯一新 seam）：
- trait 方法集（M1 落地版）：`list_tree(tree_uri, document_id)` 列目录树子条目、`read_to_cache(uri, dest_name)` 启动中转复制并立即返回 `{copy_id, dest_path}` 句柄（顺序流、512KB 缓冲、`OpenableColumns.SIZE` 预检，未知大小按流处理）、`copy_status(copy_id)` 轮询进度与终态（done/total/finished/cancelled/error）、`cancel_copy(copy_id)` 取消（复制方删除半成品后结束）、`cleanup_stale_copies()` 清扫 cache 残留、`check_authorized(tree_uri)` 授权有效性检测
- 与 spec 初版的差异（落地选择，M1 合入时回写）：进度/取消由回调通道改为轮询式 `copy_status`/`cancel_copy`——WASM host fn 为同步上下文，无法承载回调通道，轮询是跨桥最简编码（前端 400ms 轮询）；`write_media_downloads`（MediaStore 落点）属 M2，不在 M1 trait 落地，M2 消费者（file_service 三端点）以本落地版签名为准
- Kotlin 插件（`SafTransferPlugin`）实现 trait 的 ContentResolver/DocumentsContract 后端——不可测薄壳，仅系统调用转发
- Rust 侧全部编排逻辑依赖 trait，测试注入 fake（含命令层 `saf_*_impl` 错误上下文包装，见 Testing Decisions）

**共享目录存储**（file-transfer 插件）：
- 条目存 `content://tree/...` URI + 持久化授权（`takePersistableUriPermission`）；旧真实路径条目直接废除（开发阶段无兼容负担）
- app 私有下载目录保留为免授权特殊条目（真实路径直读直传）
- 失效检测：授权回收/目录删除 → 条目标记失效、UI 提示、可重新授权

**file_service 三端点**（cache 中转模式）：
- `list_dir`：content:// 根 → `list_tree` 遍历（替代 `std::fs::read_dir`；`needs_all_files_access` notice 链路在共享场景不再触发）
- `download`：桌面端拉取 → `read_to_cache` 预复制（不可续）→ 现有 Range 响应从 cache 服务（可续）→ 完成删副本
- `upload`：桌面端推送 → session 临时文件落私有下载目录（实现偏差：spec 初版写 cache，实际落私有目录——`.part` 跨重启可续传且免授权，优于 cache；孤儿清理已覆盖下载目录）→ 完成时 `write_media_downloads` 写入下载目录（拷贝段不可续；同名即拒：私有目录同名经插件钩子预检，公共 Download 目录同名经 Kotlin MediaStore 预检返回 duplicate-name，不回退私有覆盖）

**接收方向**：
- 默认落点 MediaStore.Downloads；写入失败回退私有下载目录；「保存到…」SAF 单文件目标（M3 已落地）

**M3 落地（上传 SAF 流直传 + 「保存到…」）**：
- **上传链路改造**：上传源（共享目录 SAF 条目）不再中转复制——local_path 直接传 content:// URI，宿主 `transfer.rs` upload 半边经 SafIo 新方法（`open_stream`/`read_stream`/`close_stream`，Kotlin `safOpen`/`safRead`/`safClose` 句柄服务，base64 跨桥传输）流式读，消除 v1 双倍 IO；免授权特殊条目（真实路径）走原 tokio::fs 分支，引擎/协议/TransferRequest 零改动
- **续传策略**：可 seek（`getStatSize()!=-1`）→ `safOpen(uri, offset)` 打开即 seek 真续传；pipe 流（不可 seek）→ 句柄表以 uri 为 key，任务内断线重连重复 `safOpen` 复用 fd 顺序续读（不重读）；跨任务（无活跃句柄）`effective_offset=0 ≠ 请求 offset` → 宿主回报 `not-seekable-resume`，插件重建 session 全量重传（Kotlin offset=0 重开强制从头）
- **句柄生命周期**：上传成功宿主显式 close；失败/取消保留 fd 供任务内续读；Kotlin 超时（10min）/表上限（16）清扫兜底
- **「保存到…」**：下载页长按文件 → 入队（`saveTo` 标记 + 中转唯一名，跳过 duplicate-name 预检）→ 完成时插件调 `fs_save_to_document` → 宿主 SafIo `save_to_document` → Kotlin `saveToDocument` 单命令（ACTION_CREATE_DOCUMENT 对话框 → ContentResolver 流拷贝写完即达 → 删私有副本）；失败/取消保留私有副本（place=save-failed 前端提示）；`.part` 语义 = createDocument + 拷贝 + 删除（不依赖 renameDocument，DocumentsProvider 专属能力不假设）
- **前端上传页**：选中文件直接入队，无「准备中」步骤（中转复制 UI 移除）；进度由任务队列展示
- **中转复制保留语义**：`safToCache`/`read_to_cache` 仍供 file_service `download` 端点（桌面端拉取手机共享目录文件）使用，不删除

**复制桥语义**：
- 中转复制不可断点续传（顺序流无 offset），中断重来——v1 接受的代价（现仅 file_service download 端点使用）
- 残留清理：取消/失败立即删除 + 插件激活时扫描清理 cache 残留；前端设置加载时兜底重试一次（插件激活可能早于宿主置 Activated 导致门控拒绝，双保险）
- 前端上传页 UI：共享目录文件列表直接入队（M3 后无「准备中」进度条）

**实施顺序**：M1（共享目录 SAF URI 存储 + `SafIo` Kotlin 后端 + 上传页）→ M2（MediaStore 落点 + file_service 三端点）→ M3（上传 SAF 流直传消除双倍 IO、「保存到…」+ `.part` SAF 写语义）——M3 已落地（2026-08，见上「M3 落地」）

## Testing Decisions

- **测试原则**：只测外部行为（条目增删改查、端点返回、残留清理、落点选择），不测实现细节
- **主 seam 测试**：`SafIo` fake 注入——覆盖共享目录条目管理（URI 存储/失效标记，复用插件 `state.rs` 测试模式）、命令层 `saf_*_impl` 错误上下文包装（宿主编译单元测试，见 commands.rs）、file_service 端点 cache 中转编排（复用 `server.rs` 现有 `mod tests` 模式）、MediaStore 落点失败回退路径
- **现有测试不动**：传输引擎 mock HTTP 测试（`transfer.rs`）、`saf_path.rs` 解析测试、协议序列化测试
- **Kotlin 薄壳**：不做单元测试（无基建、强系统依赖），由真机验证清单覆盖（见 08 号方案票「验证要点」：无 All Files 下上传/下载/续传/取消残留全链路）

## Out of Scope

- 任意文件获取机制（系统文件选择器主入口）——上传源严格限于共享目录
- 云盘/第三方 provider 支持——内网传输场景不涉及
- MANAGE_EXTERNAL_STORAGE 获取与 `AllFilesAccessPlugin` 跳转修复
- 文件删除/重命名/移动/覆盖操作、访问审计（主 spec 既有约束）
- 传输加密（主 spec 已有 TransportCipher 缝，与本改造正交）

## Further Notes

- 本规格为移动端存储访问改造的独立 PRD；与主 spec（`spec.md`，协议/能力契约）正交——实施稳定后把稳定条款回写主 spec
- 依赖项：`gen/android` 重建恢复清单（`AGENTS.md`「Android」节）追加 `SafTransferPlugin`；AndroidManifest 无需新增权限（SAF/MediaStore 零权限，MediaStore 写入 API 29+）
- 遗留约束：cache 目录系统可清理，副本生命周期短（复制→上传→删除），中断残留由启动扫描兜底
- 验证基准则：真机无 All Files 授权全链路（上传哈希一致、续传 30/60/90% 恢复、取消无残留）
