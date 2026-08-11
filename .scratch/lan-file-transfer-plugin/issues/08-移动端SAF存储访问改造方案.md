# 移动端存储访问 SAF 化改造方案

Type: research
Status: resolved
Blocked by: 02, 03

## Question

移动端文件传输在 **MANAGE_EXTERNAL_STORAGE（All Files）不可申请**（Google Play 政策：应用类别不符，核心功能是远程终端/文件传输而非本地文件管理）的前提下，如何对齐国际网盘路径（SAF + MediaStore + 私有目录），让上传/下载/浏览在无 All Files 时全功能可用？

结论先行：**需求可以实现，且不需要 All Files**。网盘应用的权限页只有「照片和视频 / 音乐和音频」媒体权限，验证了手动上传（SAF 选择器）、保存下载（MediaStore / SAF 目标）、目录授权（SAF 目录树 + 持久化）全部零运行时权限。All Files 仅作为体验优化（真实路径直读、免选目录）。

## 现状检测（2026-08 核查）

### 链路现状

```
SafPickerPlugin 选路（pickFile / pickDirectory + takePersistableUriPermission 持久化）
  → resolve_saf_path 解析真实路径（仅 externalstorage / downloads 两个 provider）
    → 传输引擎 tokio::fs 直操作（transfer.rs download/upload）
    → 失败（云盘等）→ 放弃
```

### 缺口矩阵

| 场景 | 现状 | 缺口原因 |
|------|------|---------|
| 下载到 app 私有目录（`getExternalFilesDir(Download)`） | ✅ 全功能 | 免授权真实路径，续传/Range/.part 完整 |
| 下载到 SAF 自定义目录 | ❌ | 解析成功也受分区存储 FUSE 过滤：无 All Files 时直写 EACCES；云盘 URI 解析失败 |
| 上传本地公共目录文件 | ❌ | `_data` 列/解析成功 → 直读同样 EACCES（无 All Files） |
| 上传云盘文件（Drive 等） | ❌ | `resolve_saf_path` 返回 None，无路径可走 |
| 桌面端浏览手机公共目录（file_service `list_dir`） | ⚠️ | `std::fs::read_dir` + `needs_all_files_access` 仅提示，无 All Files 时顶层空列表 |
| 断点续传（引擎层） | ✅ | offset + Range + `.part` + 进度/取消/队列，与路径形态无关 |

### 关键陷阱（必须理解）

**SAF 授权 ≠ 真实路径直读**。Android 11+ 分区存储的 FUSE 过滤在挂载层，SAF 授权（`takePersistableUriPermission`）只放行 ContentResolver（content:// URI）访问，不豁免 `/storage/emulated/0/...` 路径直读。因此"授权目录 → 解析真实路径 → tokio::fs 直读直写"这条捷径在无 All Files 时**不成立**，IO 必须走 ContentResolver 流。

### 现有资产（方案复用，不动）

- 传输引擎 `transfer.rs`：Range 续传、`.part` 原子落位、500ms 进度、取消令牌、任务表
- `TransferRequest`：`local_path` 为字符串，可承载 content:// URI
- `SafPickerPlugin`：选择器 + `takePersistableUriPermission` 持久化授权（地基已具备）
- `DownloadsDirPlugin`：私有下载目录（默认目标）
- `AllFilesAccessPlugin` + `needs_all_files_access` notice 链路：保留为"有 All Files 则增强"的体验优化，不作为依赖
- file-transfer 插件任务状态机/队列/偏移持久化：与路径形态无关

## 方案设计

### v1 主方案：SAF 授权目录 + 中转复制（收缩版，Rust 引擎零改动）

**上传方向**（缺口最大，优先）：

```
pickDirectory 选共享目录（一次，已有）→ 条目存 SAF URI（content://tree/... + 持久化授权）
  → App 内 DocumentsContract 遍历共享目录列文件（免每次弹系统选择器）
    → 选中 → safToCache：ContentResolver 顺序流复制到私有 cache（带大小预检 + 进度 + 取消）
      → 现有 enqueue_upload（local_path = cache 真实路径）→ 完成 → 删除 cache 副本
```

- 上传源 = 共享目录（术语见 CONTEXT.md「文件传输」分组），UI 为普通文件列表，不弹系统选择器；不支持目录树选择的 provider 不在范围内（内网传输场景不涉及云盘）
- 私有 cache 是免授权真实路径，`fs_auth` 检查天然通过，引擎续传/进度/取消全部复用

**接收方向（下载 + 桌面端推送统一落点）**：

- 默认落点 **MediaStore.Downloads**（API 29+ 零权限、系统文件管理器可见）：session 临时文件落 cache（续传点保留）→ 完成时 MediaStore 插入 + 流写入（缓存段可续、拷贝段不可续）
- 写入失败回退 app 私有下载目录（`getExternalFilesDir(Download)`，现有机制保留）
- 桌面端「发送到手机」与移动端主动下载均落此目录（不落共享目录——共享目录只读暴露）
- 「保存到…」`ACTION_CREATE_DOCUMENT` 单文件目标（后置，见 M3）

**技术要点**：

- `safToCache` 顺序流：`openFileDescriptor(uri, "r")` → 512KB 缓冲 `Os.read` → 写 cache 文件；`OpenableColumns.SIZE` 预检（云盘流可能为 -1，按未知大小处理）
- 复制阶段**不可续传**（顺序流无 offset 语义），中断重来——v1 接受的代价
- 双倍 IO + 临时空间（传完即删），对大文件在 UI 展示「准备中」进度

### v2 可选增强（按需推进，非阻塞）

1. **上传 SAF 流直传**：Kotlin `saf_open(uri, "r", offset)` + 任务内保 fd 流式续传（`Os.lseek` 检测 `getStatSize()==-1` 判断 pipe 流），Rust 侧仅抽象 upload 半边 IO。消除 v1 的双倍 IO，且 pipe 流场景任务内续传可用
2. **.part 语义在 SAF 写目标的处理**：`renameDocument` 仅 DocumentsProvider 有效，兜底 `createDocument` + 拷贝 + 删除——仅在「保存到…」/SAF 写目标场景需要

## 改动清单

| 层 | 改动 | v1 | v2 |
|----|------|----|----|
| Kotlin | 新插件 `SafTransferPlugin`（或扩展 `SafPickerPlugin`）：`listTreeChildren(treeUri)` 遍历、`safToCache(uri, dest)` 流复制（进度回调/取消） | ✅ | — |
| Kotlin | MediaStore.Downloads 写入（插入 + 流拷贝，私有目录失败回退） | ✅ | — |
| Kotlin | `saf_open/read/seek/close` 句柄服务（`Os.lseek/read`，`getStatSize()` 探测） | — | ✅ |
| Kotlin | `saveToDocument(uri, src)` 单文件保存（「保存到…」） | — | ✅ |
| 前端 | 上传页：共享目录文件列表 + 「准备中」复制进度 + 完成后自动入队 | ✅ | — |
| 前端 | 下载页：MediaStore 落点状态 + 失败回退提示 | ✅ | — |
| 前端 | 下载页：「保存到…」入口 | — | ✅ |
| Rust 传输引擎 | 零改动 | ✅ | — |
| Rust 传输引擎 | upload 半边 IO 抽象（Android 走命令桥接） | — | ✅ |
| Rust file_service | `list_dir`/download/upload 端点 content:// 分支（cache 中转模式：listTreeChildren 遍历、safToCache 预复制、copyToDocument 写目标） | ✅ | — |
| Rust 共享目录存储 | 条目改存 SAF URI（`content://tree/...`），旧真实路径条目废除；私有下载目录保留为免授权特殊条目 | ✅ | — |
| 配置 | `gen/android` 重建恢复清单（AGENTS.md「Android」节）追加新插件 | ✅ | — |

## 风险与边界

- 上传源严格限于共享目录（目录树授权）：不支持目录树选择的来源（如部分第三方 provider）不在范围内——内网传输场景不涉及云盘，无降级路径
- v1 复制阶段不可续传、双倍 IO：接受（收缩需求的取舍），大文件占比升高后由 v2-1 消除
- cache 目录系统可清理：副本生命周期短（复制→上传→删除），中断残留由下个任务重试时清理
- 不引入 `WRITE_EXTERNAL_STORAGE` / `READ_MEDIA_*`：本方案与媒体权限无关（BedCode 传任意文件，非媒体扫描场景）

## 实施顺序

- **M1（v1 上传）**：共享目录存储改 SAF URI → Kotlin `SafTransferPlugin`（遍历 + 流复制）→ 前端上传页共享目录文件列表 + 准备中进度 → 真机验证
- **M2（v1 接收 + 共享）**：MediaStore.Downloads 默认落点（私有回退）→ 桌面端「发送到手机」落下载目录 → file_service 三端点 SAF 化（cache 中转）→ 真机验证
- **M3（v2 按需）**：上传 SAF 流直传 → 「保存到…」单文件目标 + `.part` SAF 写语义

## 验证要点（真机，无 All Files 授权）

1. 上传共享目录文件（含公共 Download 目录，无 All Files）到桌面端，哈希一致
2. 上传目录树授权后重启 App 仍可列出并上传（持久化授权生效）
3. 下载到「保存到…」目标，断点续传回归（30%/60%/90% 中断恢复哈希一致）
4. 全流程无 All Files 时无 EACCES，`needs_all_files_access` 提示仅在浏览场景出现
5. 取消/失败路径：复制阶段取消无残留；上传失败重试正常

## Comments

### 2026-08-11 grilling 定稿（决策树 Q1–Q7 全部确认，领域术语见 CONTEXT.md「文件传输」分组，架构决策见 docs/adr/0009）

- **需求收紧**：移动端上传源仅限授权过的目录（共享目录）中的文件，不做任意文件获取
- **All Files 注定不可得**（个人应用 Play 政策 + 国产 ROM 入口不稳定），方案对其零依赖；`AllFilesAccessPlugin` 引导仅作可选体验增强
- **共享目录全量 SAF URI 存储**（`content://tree/...` + `takePersistableUriPermission` 持久化），旧真实路径条目直接废除（开发阶段无兼容负担）；私有下载目录保留为免授权特殊条目
- **v1 起步**：中转复制（SAF → cache，不可续）+ 现有引擎（cache → 桌面，可续）；v2 增强（上传 SAF 流直传等）留作后续——file_service SAF 挂载与 MediaStore 已因 Q5/Q4 并入 v1
- **下载默认落点改 MediaStore.Downloads**（API 29+ 零权限、用户可见），私有目录回退备用，「保存到…」SAF 单文件目标后置
- **方向模型**：共享目录 = 只读暴露（桌面端浏览 + 拉取）；下载目录 = 接收落点（移动端下载 + 桌面端「发送到手机」统一落此，不落共享目录）；上传源 = 共享目录
- **file_service 三端点全 SAF 化**（list_dir / download / upload），统一 cache 中转模式：listTreeChildren 遍历、safToCache 预复制、copyToDocument 写目标
- **复制桥进度/取消**：UI 层「准备中」进度 + 取消按钮，不入任务状态机；残留清理 = 取消/失败立即删 + 插件激活时扫描清理 cache 残留
- **单文件导入降级已砍**（2026-08-12 确认）：内网传输场景无云盘概念，上传源严格限于共享目录（目录树授权），不支持目录树选择的 provider 不在范围内
