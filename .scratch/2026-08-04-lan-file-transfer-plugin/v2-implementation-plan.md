# 文件传输 v2 实施方案（implementation plan）

> 依据：`.scratch/lan-file-transfer-plugin/spec.md` 第 14 节（14.1–14.7）、决策票据 `issues/10`、`issues/11`、`docs/adr/0016`、handoff（`bedcode-file-transfer-v2-handoff.md`）。
> 本方案是施工图纸：**先审批，后按第 16 节分工执行**。两端为"同构双写"，所有 wire 定义（事件名/载荷/命令签名/状态枚举）两端必须逐字一致。
> 与 v1 冲突处以 v2 为准。术语纪律：发送/接收 = 发起方区分；上传/下载 = 协议方向。

---

## 1. 架构总览（v2 数据流）

```
发送方插件(设备A)                      接收方宿主(设备B)
 用户选择文件 → enqueue(batchId)
 → 首个任务调度: POST /transfer-request ─────────→ 批钩子 onTransferRequest 三路分流
                                                      ask → 批 pending(202) + 本地事件
                                                      allow → 批 approved(200)
                                                      deny → 403(policy-denied)
 202 → 批内任务 waiting-approval
                                                用户应答 → 宿主命令 approve/reject
                                                 → 跨端 WS 推送 TransferApproval
 收 filesrv:transfer_approval ────────────────┘
  approved → 任务 queued → 逐个 POST /upload{batchId}
    宿主校验批 approved → 免钩子建 session → 数据流
  rejected → 任务 rejected(user-rejected/timeout)
```

**核心规则（spec 14.2）**：
1. 上传拆两段：先询问同意，批准后才开始数据流；批准后批内 session 创建**免钩子**。
2. ask 模式强制批上下文：无已批准批 ID 的 session 创建一律 403（防绕过 `/upload`）。
3. pending 批 TTL 扫描在宿主，超时自动拒绝；批准状态随批保留（批内 session 终态 + 24h TTL 兜底）。
4. 等待同意期间断线**不重发**，任务直接 rejected(timeout)。
5. 批内文件同名沿用 v1 per-file 同名即拒（complete 409 → 该文件 rejected，批内其他不受影响）。

---

## 2. 线协议（HTTP 数据面）

### 2.1 新端点 `POST /{plugin}/{mount}/transfer-request`（两端服务器）

请求体（camelCase）：
```json
{ "batchId": "uuid", "files": [ { "relativePath": "dir/a.mp4", "size": 123456 } ],
  "totalSize": 123456 }
```

宿主处理：`get_entry` + `require_op(Upload)` → 沙箱不需要（仅元数据）→ **批钩子三路分流**：

| 钩子决定 | 响应 | 宿主动作 |
|---|---|---|
| allow | `200 { batchId, decision: "approved" }` | 批记录 approved（可立即建 session） |
| ask | `202 { batchId, decision: "pending" }` | 批记录 pending + 本地事件 `filesrv:transfer_request` |
| deny | `403`（message 含 reason，如 `policy-denied`） | 不建批、无任务无记录 |

批钩子 fail-closed：超时/插件异常/挂载不存在一律 deny（沿用 v1 2s 超时语义，常量复用 `UPLOAD_HOOK_TIMEOUT`）。

### 2.2 `POST /{plugin}/{mount}/upload` 扩展（批 gating）

`CreateUploadRequest` 增加可选字段 `batchId`（serde default，camelCase）：

```rust
pub struct CreateUploadRequest {
    pub relative_path: String,
    pub size: u64,
    #[serde(default)]
    pub batch_id: Option<String>,   // v2 新增
}
```

创建逻辑（**顺序**：沙箱解析目标 → gating → 建 session → 本地事件）：

1. `batchId` 存在 → `registry.check_batch(plugin_id, mount, batch_id)`：
   - `Approved` → **跳过 per-file 钩子**，直接建 session；
   - `Pending` / `Rejected` / `NotFound` → 403（message: `batch-not-approved` / `batch-rejected` / `batch-not-found`）。
2. `batchId` 不存在 → 走 v1 per-file 钩子；钩子返回 `ask` → 403 `batch-context-required`（fail-closed，防绕过）；`allow` → 建 session；`deny` → 403（v1 语义，409 duplicate-name 特例保留）。
3. **session 创建成功后**（两条路径都要）发本地事件 `filesrv:receiving_started`（见 §5.1）。

### 2.3 响应码语义（发送方插件解析用）

| 场景 | 码 | 发送方处理 |
|---|---|---|
| transfer-request approved | 200 | 批内任务直接调度 |
| transfer-request pending | 202 | 批内任务 waiting-approval |
| transfer-request deny（policy-denied） | 403 | 批内任务 rejected(policy-denied) |
| transfer-request 网络错误/非预期 | - | 批内任务 failed |
| create session batch approved | 200 | 正常开始 |
| create session 403 batch-not-* | 403 | failed（不应发生，防呆） |
| complete 409 duplicate-name | 409 | 该文件 rejected(duplicate-name)（v1 已实现） |

---

## 3. 宿主批状态机（两端各自实现）

### 3.1 数据模型（新建 `file_service/transfer.rs`，两端同名模块）

```rust
/// 批状态（spec 14.2）
pub enum BatchState {
    Pending,                                    // ask 后等待用户应答
    Approved,                                   // 用户接受 / 钩子 allow
    Rejected { reason: RejectReason },          // 用户拒绝 / 超时
}

/// 拒绝原因枚举（wire snake_case，发送方据此映射文案）
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    UserRejected,   // 用户点了拒绝
    Timeout,        // 等待超时（宿主 TTL / 断线）
}

/// 传输批记录（宿主内存态，不持久化）
pub struct TransferBatch {
    pub batch_id: String,
    pub plugin_id: String,
    pub mount_path: String,
    pub files: Vec<UploadRequestMeta>,
    pub total_size: u64,
    pub state: BatchState,
    pub created_at: Instant,
    /// 批内 session 活动刷新时间（approved 批 24h TTL 依据；pending 超时也以此计时）
    pub last_active: Instant,
    pub approval_timeout: Duration,
}
```

纯函数（可单测）：`validate_batch_transition(Pending→Approved|Rejected)`、`is_approved` 等。

### 3.2 注册表持有与方法（registry.rs，两端）

```rust
pub struct FileServiceRegistry {
    // ...现有字段...
    batches: RwLock<HashMap<String, TransferBatch>>,          // batch_id → 批
    approval_timeouts: RwLock<HashMap<(String, String), Duration>>, // (plugin,mount) → 超时，默认 60s
}

impl FileServiceRegistry {
    /// POST /transfer-request 处理：批钩子三路分流 → 建批 / 202 / 403
    pub async fn create_transfer_request(&self, plugin_id, mount_path, req: &TransferRequestDto)
        -> Result<BatchDecision, BatchError>;   // BatchDecision: Approved | Pending
    /// 应答命令：pending → approved（校验归属 plugin）
    pub async fn approve_transfer(&self, plugin_id: &str, batch_id: &str) -> Result<(), BatchError>;
    /// 应答命令：pending → rejected(UserRejected)
    pub async fn reject_transfer(&self, plugin_id: &str, batch_id: &str) -> Result<(), BatchError>;
    /// session 创建 gating：Approved → Ok(批引用)；其他 → Err
    pub async fn check_batch(&self, plugin_id: &str, mount_path: &str, batch_id: &str)
        -> Result<TransferBatch, BatchError>;
    /// 批内 session 活动刷新（建 session 成功时调用）
    pub async fn touch_batch(&self, batch_id: &str);
    /// 设置 per-mount 批准超时（10–600s 校验，默认 60）
    pub async fn set_approval_timeout(&self, plugin_id: &str, mount_path: &str, secs: u64) -> Result<()>;
    /// sweeper 一次扫描：pending 超时 → rejected(Timeout)；approved 24h 无活动 → 清理
    pub async fn sweep_batches(&self) -> Vec<ExpiredBatch>;    // 返回本次超时/清理的批
    /// 本地发布 transfer_resolved + 跨端推送 TransferApproval
    pub async fn publish_batch_resolved(&self, batch_id: &str, decision: &str, reason: &str);
}
```

`sweep_batches` 返回后由调用方（sweeper 任务）对每个超时批执行：本地事件 `filesrv:transfer_resolved` + 跨端推送（§5.2）。pending 超时与 approved 清理共用一个 sweeper（间隔 1s，挂 `start_background_tasks`）。

**事件发布统一入口**（registry 内新增私有 helper，仿 `emit_peer_changed` 双通道）：
```rust
fn emit_filesrv_event(&self, event: &str, payload: serde_json::Value)
// 通道1: app_handle.emit(event, payload)     （Tauri 事件，前端/TS 插件）
// 通道2: plugin_host.message_bus().publish(event, "host", payload)  （WASM 插件）
```

**跨端推送入口**（分端实现，同一方法名）：
```rust
async fn push_transfer_approval(&self, batch_id: &str, decision: &str, reason: &str)
// 桌面: ctx.sync_tx().send(DesktopSyncEvent::TransferApproval { batch_id, decision, reason })
// 移动: ConnectionManager::send(Message::file_service(FileServicePayload::TransferApproval { ... }))
```

### 3.3 错误语义（BatchError → HTTP/命令错误）

| 场景 | 结果 |
|---|---|
| 批不存在 | `AppError::NotFound("transfer batch not found: {id}")` |
| 批非 pending（重复应答/已超时） | `AppError::InvalidInput("transfer batch {id} not pending")` |
| 批归属不匹配（其他插件应答） | `AppError::NotFound`（不泄露存在性） |
| 超时值越界 | `AppError::InvalidInput`（10–600） |

---

## 4. SDK 变更（精确类型，两端双写，改一处须同步四处）

### 4.1 Rust `types.rs`（desktop/mobile `packages/plugin-sdk-*/rust/src/types.rs`）

```rust
/// 上传策略钩子决定（插件 → 宿主）——v2 三路化：allow / ask / deny
/// fail-closed：任何异常一律拒绝。
/// wire 兼容：旧插件返回 { allow: false } → deny；{ allow: true } → allow。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadHookDecision {
    pub allow: bool,
    /// v2：true = 需要用户批准（批上下文）；与 allow 互斥
    #[serde(default)]
    pub ask: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
impl UploadHookDecision {
    pub fn allow() -> Self;
    pub fn deny(reason: impl Into<String>) -> Self;
    /// v2：请求用户批准
    pub fn ask() -> Self { Self { allow: false, ask: true, reason: None } }
}
// Default = deny（fail-closed）不变

/// 批量传输请求元信息（宿主 → 插件批钩子入参，camelCase）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequestMeta {
    pub batch_id: String,
    pub files: Vec<UploadRequestMeta>,
    pub total_size: u64,
}
```

### 4.2 Rust `wasm.rs`（WasmPlugin trait + wasm_entry! 宏 + 测试）

```rust
/// 批量传输请求钩子（v2，可选，默认 fail-closed 拒绝）
/// 宿主在 POST /transfer-request 时调用一次
fn on_transfer_request(_meta: &TransferRequestMeta) -> UploadHookDecision {
    UploadHookDecision::deny("plugin does not implement on_transfer_request")
}
```

`wasm_entry!` 宏新增导出 `__bedcode_on_transfer_request(meta_ptr, meta_len, out_ptr) -> i32`（签名与 `__bedcode_on_upload_request` 完全同构：写入 JSON、返回 i32 状态码、out_ptr 决定 JSON）。`abi.rs` export 常量新增 `ON_TRANSFER_REQUEST`。宏内解析失败/未实现 → 返回 `deny("invalid transfer request meta")`（与 on_upload_request 分支同构）。trait 测试补 `TestWasmPlugin` 默认拒绝断言 + ask 插件断言。

### 4.3 Rust `host/file_service.rs`（HostFileService trait）+ `wasm_host.rs` + `abi.rs` + 宿主 host_impl

trait 新增 4 个方法（WasmHost impl 经 `host_file_service::*` import 桥接，宿主 host_impl 新增同名 `pub(crate) fn host_filesrv_*`，须过 `check_permission`）：

```rust
/// 批准传输批（接收端用户应答「接受全部」）
fn filesrv_approve_transfer(&self, batch_id: &str) -> Result<(), HostError>;
/// 拒绝传输批（接收端用户应答「拒绝全部」）
fn filesrv_reject_transfer(&self, batch_id: &str) -> Result<(), HostError>;
/// 设置批准超时（秒，10–600；仅 ask 策略生效，宿主 TTL 扫描用）
fn filesrv_set_approval_timeout(&self, mount_path: &str, seconds: u64) -> Result<(), HostError>;
/// 取消接收中的上传会话（接收端本地取消，session 级）
fn filesrv_cancel_receiving(&self, session_id: &str) -> Result<(), HostError>;
```

ABI 桥接命名（两端 host_impl 一致）：`host_filesrv_approve_transfer(batch_id)` / `host_filesrv_reject_transfer(batch_id)` / `host_filesrv_set_approval_timeout(mount_path, seconds)` / `host_filesrv_cancel_receiving(session_id)`。参数字符串/数值编码与现有 host fn 同模式（subagent 探索 abi.rs 的 import 声明机制后照做）。

### 4.4 TS `types.ts` + `runtime.ts`（desktop/mobile `packages/plugin-sdk-*/src/`）

```ts
/** 上传策略钩子决定（v2 三路：allow / ask / deny） */
export interface UploadHookDecision {
  allow: boolean
  /** v2：true = 需要用户批准（批上下文） */
  ask?: boolean
  reason?: string
}

/** 批量传输请求元信息（批钩子入参） */
export interface TransferRequestMeta {
  batchId: string
  files: { relativePath: string; size: number }[]
  totalSize: number
}

export interface MountOptions {
  // ...现有...
  /** v2：批量传输请求钩子（可选；提供时以 Webview 批钩子目标注册） */
  onTransferRequest?: (meta: TransferRequestMeta) => Promise<UploadHookDecision>
}

export interface FileServiceAPI {
  // ...现有...
  /** v2：批准传输批（接收端应答） */
  approveTransferRequest(batchId: string): Promise<void>
  /** v2：拒绝传输批（接收端应答） */
  rejectTransferRequest(batchId: string): Promise<void>
  /** v2：设置批准超时（秒，10–600） */
  setApprovalTimeout(mountPath: string, seconds: number): Promise<void>
  /** v2：取消接收中的上传会话（本地取消） */
  cancelReceivingSession(sessionId: string): Promise<void>
}
```

`runtime.ts`：
- `mount()` 解析 `onTransferRequest`：与 `onUploadRequest` 相同模式（注册为 Webview 钩子目标）。
- 新命令调用封装：`plugin_filesrv_approve_transfer` / `plugin_filesrv_reject_transfer` / `plugin_filesrv_set_approval_timeout` / `plugin_filesrv_cancel_receiving`。
- 前端监听 `filesrv:transfer_request_hook` 事件 → 调 `onTransferRequest` 回调 → `plugin_filesrv_respond_transfer_request` 回填（与 `filesrv:upload_request` / `respond_upload_request` 同构）。

---

## 5. 宿主事件与控制面（两端逐字一致）

### 5.1 本地事件表（Tauri 事件 + MessageBus topic 双通道，emit 方 = 接收端宿主）

| 事件名 | 载荷（camelCase） | 触发 | 受众 |
|---|---|---|---|
| `filesrv:transfer_request` | `{ batchId, pluginId, mountPath, files: [{relativePath, size}], totalSize }` | ask 分流建 pending 批时 | 接收端：pending 卡 + 批级 toast |
| `filesrv:transfer_resolved` | `{ batchId, decision: "approved"\|"rejected", reason: ""\|"user-rejected"\|"timeout" }` | approve/reject 命令、TTL 超时 | 接收端：pending 卡消失；ask 批准后发批级 toast |
| `filesrv:receiving_started` | `{ sessionId, batchId?: string, relativePath, size }` | session 创建成功（钩子路径与批路径都发） | 接收端：正在接收任务 + accept 模式 toast |
| `filesrv:receiving_done` | `{ sessionId, state: "completed"\|"failed"\|"cancelled", reason?: string }` | complete 成功 / complete 409(duplicate-name→failed) / cancel（用户、发送方） | 接收端：接收任务终态 + 归档 |

### 5.2 跨端消息（两端双写）

**Mobile → Desktop**：`FileServicePayload`（`enums/file_service.rs`，两端同构双写）新增 variant：

```rust
/// 传输批应答推送（v2）：接收端批准/拒绝/超时 → 发送端
TransferApproval {
    batch_id: String,
    /// "approved" | "rejected"
    decision: String,
    /// "" | "user-rejected" | "timeout"
    reason: String,
},
```

- 发送方（= 接收端宿主，mobile）：`announce.rs` 同款 `ConnectionManager.send(Message::file_service(...))`。
- 接收方（= 发送端宿主，desktop）：`server/ws/terminal_ws.rs` `handle_file_service` 新增 match 分支 → `registry.publish_transfer_approval(batch_id, decision, reason)`（内部：Tauri 事件 + Bus 发布 `filesrv:transfer_approval`，载荷与上表 transfer_resolved 相同形状 `{ batchId, decision, reason }`）。

**Desktop → Mobile**：`SyncPayload`（`enums/sync.rs`，两端同构双写）新增 variant：

```rust
TransferApproval {
    batch_id: String,
    decision: String,   // "approved" | "rejected"
    reason: String,     // "" | "user-rejected" | "timeout"
},
```

- 桌面 `events/sync_event.rs` `DesktopSyncEvent` 新增同名 variant；`events/sync_handler.rs` 映射为 `SyncPayload::TransferApproval`。
- 移动 `handler/sync.rs` 新增 match 分支 → `registry.publish_transfer_approval(...)`（同样双通道发布 `filesrv:transfer_approval`）。

**发送端插件统一订阅**：bus topic `filesrv:transfer_approval`（WASM） / Tauri 事件同名（前端）→ 载荷 `{ batchId, decision, reason }`。

### 5.3 Tauri 命令表（两端同名，`commands/file_service.rs` / `plugin/commands.rs`，均过 `require_fileservice`）

| 命令 | 签名 | 说明 |
|---|---|---|
| `plugin_filesrv_approve_transfer` | `(plugin_id, batch_id) -> Result<()>` | 批 pending→approved + 本地 resolved 事件 + 跨端推送 |
| `plugin_filesrv_reject_transfer` | `(plugin_id, batch_id) -> Result<()>` | 批 pending→rejected(user-rejected) + 事件 + 推送 |
| `plugin_filesrv_set_approval_timeout` | `(plugin_id, mount_path, seconds: u64) -> Result<()>` | 10–600 校验，per-mount |
| `plugin_filesrv_cancel_receiving` | `(plugin_id, session_id) -> Result<()>` | 本地取消上传会话（删 .part）+ receiving_done(cancelled) |
| `plugin_filesrv_respond_transfer_request` | `(plugin_id, request_id, decision_json: String) -> Result<()>` | Webview 批钩子回填（decision_json 为 UploadHookDecision JSON） |

### 5.4 WASM 插件侧入口汇总

- 新 trait 方法：`on_transfer_request`（§4.2）
- 新 host fn：`filesrv_approve_transfer` / `filesrv_reject_transfer` / `filesrv_set_approval_timeout` / `filesrv_cancel_receiving`（§4.3）
- 新 bus topic 订阅：`filesrv:transfer_request`、`filesrv:transfer_resolved`、`filesrv:receiving_started`、`filesrv:receiving_done`（接收端）；`filesrv:transfer_approval`（发送端）

---

## 6. 桌面宿主改动清单

| 文件 | 改动 |
|---|---|
| `src-tauri/src/plugin/file_service/transfer.rs` | **新建**：BatchState/RejectReason/TransferBatch + 纯函数 + 单测 |
| `src-tauri/src/plugin/file_service/registry.rs` | batches/approval_timeouts 字段、create_transfer_request/approve_transfer/reject_transfer/check_batch/touch_batch/set_approval_timeout/sweep_batches、`emit_filesrv_event` 双通道 helper、`publish_transfer_approval`、`push_transfer_approval`（sync_tx）、批钩子分发（Wasm→host.call_transfer_hook / Webview→call_webview_batch_hook / None→deny）、start_background_tasks 加 batch sweeper |
| `src-tauri/src/plugin/file_service/upload.rs` | 无改动（或仅在需要时加 session 快照导出） |
| `src-tauri/src/server/controllers/file_service_controller.rs` | 新端点 `POST /transfer-request`；`CreateUploadRequest.batch_id`；create_upload gating（批校验/ask→403）；session 创建后发 `filesrv:receiving_started`；complete/cancel 后发 `filesrv:receiving_done` |
| `src-tauri/src/commands/file_service.rs` | 5 个新命令（§5.3） |
| `src-tauri/src/plugin/host.rs` | `call_transfer_hook(plugin_id, meta_json)`（与 call_upload_hook 同构，2s 超时由 registry 包裹，fail-closed） |
| `src-tauri/src/plugin/wasm_runtime/host_impl/file_service.rs` | 4 个新 host fn impl（§4.3） |
| `src-tauri/src/enums/file_service.rs` | `FileServicePayload::TransferApproval` + wire 测试 |
| `src-tauri/src/enums/sync.rs` | `SyncPayload::TransferApproval`（与移动端同构） |
| `src-tauri/src/events/sync_event.rs` | `DesktopSyncEvent::TransferApproval` |
| `src-tauri/src/events/sync_handler.rs` | 新 variant 映射到 SyncPayload |
| `src-tauri/src/server/ws/terminal_ws.rs` | `handle_file_service` 新增 TransferApproval 分支 → registry.publish_transfer_approval |

## 7. 移动宿主改动清单（与桌面差异点标注）

| 文件 | 改动 |
|---|---|
| `src-tauri/src/file_service/transfer.rs` | **新建**（同桌面，crate 内类型用 `bedcode_plugin_api_mobile`） |
| `src-tauri/src/file_service/registry.rs` | 同桌面批方法；**差异**：无 downloads_dir 字段（接收落点走 `downloads_dir()`/SAF relay，批逻辑不涉及）；`push_transfer_approval` 经 ConnectionManager 发 WS；`publish_transfer_approval` 双通道发布 |
| `src-tauri/src/file_service/server.rs` | 新端点路由 `/{plugin}/{mount}/transfer-request`（仿现有 service 注册）；`CreateUploadRequest.batch_id`；create_upload gating；session 创建后 `receiving_started`；complete/cancel 后 `receiving_done`；批端点单测（仿现有 test mod） |
| `src-tauri/src/plugin/commands.rs` | 5 个新命令（§5.3） |
| `src-tauri/src/plugin/manager.rs` | `call_transfer_hook(plugin_id, meta_json) -> Option<String>`（仿 call_upload_hook） |
| `src-tauri/src/plugin/wasm_runtime.rs` | `call_transfer_request(meta_json) -> Result<String>`（仿 call_upload_hook，导出 `__bedcode_on_transfer_request`，无导出 → Err → 上层 fail-closed） |
| `src-tauri/src/handler/file_service.rs` | 新增 `FileServicePayload::TransferApproval` 分支 → registry.publish_transfer_approval |
| `src-tauri/src/handler/sync.rs` | 新增 `SyncPayload::TransferApproval` 分支 → registry.publish_transfer_approval |
| `src-tauri/src/enums/file_service.rs` / `enums/sync.rs` | 同桌面新增 variant |
| `src-tauri/src/file_service/announce.rs` | 无改动（发送 helper 复用其 `send()` 模式，可在 registry 内自行实现） |

> 移动端 wasm_runtime 无 `pub mod abi` 的 host import 宏？其 SDK ABI 常量位置由 agent 探索（`bedcode_plugin_api_mobile::abi::export::*` 已存在，新增 `ON_TRANSFER_REQUEST`）。

---

## 8. 插件 WASM 规范（发送方）

### 8.1 Settings 扩展（`commands.rs`，两端）

```rust
pub struct Settings {
    // ...现有 roots/download_dir/concurrency...
    /// v2 接收策略：ask(默认) | accept | reject
    #[serde(default = "default_receiving_policy")]
    pub receiving_policy: String,
    /// v2 同意超时秒（10–600，仅 ask 生效，默认 60）
    #[serde(default = "default_approval_timeout")]
    pub approval_timeout_sec: u64,
}
```

- `set-settings` 支持 `receivingPolicy` / `approvalTimeoutSec` 字段；策略或超时变化时（且已挂载）调用 `filesrv_set_approval_timeout(MOUNT_PATH, secs)`（每次挂载时也调一次，保证宿主侧同步）。
- 前端 `useSettings` / `SettingsPanel` 同步扩展（见 §12）。

### 8.2 任务模型扩展（`state.rs`，两端）

```rust
pub enum TaskState {
    // ...现有...
    /// v2：等待对方同意（仅 ask 模式上传任务）
    WaitingApproval,
}

pub struct Task {
    // ...现有...
    /// v2 运行时：所属批 ID（上传任务，一次「发送」动作一匹）
    #[serde(skip)]
    pub batch_id: Option<String>,
}
```

`validate_transition` 新增：`Queued→WaitingApproval`、`WaitingApproval→Queued`（批准后重新调度）、`WaitingApproval→Rejected`（拒绝/超时）、`WaitingApproval→Cancelled`（用户取消）、`WaitingApproval→Resumable`（对端下线兜底，见 8.4 实际采用 rejected(timeout)，此项仅防御）。终态判据不变。前端 `TaskStateName` 加 `'waiting-approval'`。

**持久化**：`TaskStore::load` 过滤规则追加：WaitingApproval 任务**丢弃**（批上下文不可恢复，等价于未发）；批记录不持久化。`save` 时 WaitingApproval 照常写入（退出前 flush 语义不变）。

### 8.3 批记录（插件侧，内存，`commands.rs` 或新模块）

```rust
struct BatchRecord {
    batch_id: String,
    peer_id: String,
    state: BatchRecordState,   // Pending | Approved | Rejected { reason: String }
}
```

- 位置：`PluginState` 新字段 `batches: HashMap<String, BatchRecord>`（不持久化）。
- 入队（`enqueue`/`enqueue_upload`）扩展参数 `batchId?: string`：非空 → 任务.batch_id 赋值；批内首个任务启动时若批记录不存在 → 发起 `POST /transfer-request`（HTTP，经 host http，base+auth 同 handshake 模式）：
  - 200 → 批记录 Approved，批内任务全部可调度；
  - 202 → 批记录 Pending，批内任务 `WaitingApproval`；
  - 403 → 批记录 Rejected(policy-denied)，批内任务 rejected(`policy-denied`)；
  - 网络错误/超时/非预期 → 批记录不建，批内任务 failed（reason 原文）。
- `start_single_task` Upload 分支前置：任务有 `batch_id` 且批记录 Pending → `WaitingApproval` 不入队启动（`queue.release` 后由 `schedule_and_start` 自然等待）；批记录不存在 → 先发批请求再决定（请求期间该任务保持 queued）。
- 批内 session 创建：`handshake::create_session` 扩展传 `batchId`（见 §8.5）。

### 8.4 发送方消息处理（`on_message`）

| topic | 处理 |
|---|---|
| `filesrv:transfer_approval` | `{ batchId, decision, reason }`：approved → 批记录 Approved，批内 WaitingApproval 任务 → Queued + enqueue + schedule；rejected → 批记录 Rejected，批内 WaitingApproval 任务 → rejected（reason 映射：`user-rejected` / `timeout`） |
| `filesrv:peer_changed` offline | 追加：该对端 **WaitingApproval** 任务 → rejected(`timeout`)（等待同意期间断线不重发，spec 14.2 边界 1）；批记录保留 Pending（接收端自然超时） |
| `filesrv:peer_changed` online | 不自动重发批请求（批已 rejected，用户手动 retry） |

- **retry 语义**：rejected 任务 retry → 转 Queued；若带 batch_id 且批记录已 rejected → 清除任务.batch_id 重新入队（新批上下文，重新发起 transfer-request，即"重新询问"）。实现：retry 时若 `reason ∈ {user-rejected, timeout, policy-denied}` 则清 batch_id。
- **拒绝文案映射**（前端按 reason 显示，i18n §13）：`duplicate-name`（已有）→ duplicateName；`user-rejected` → rejectedByUser；`timeout` → noResponse；`policy-denied` → policyDenied。

### 8.5 handshake 扩展（`handshake.rs`，两端）

```rust
/// POST /transfer-request（v2）
pub fn request_transfer(host, base, auth, batch_id, files: &[UploadRequestMeta], total_size)
    -> Result<TransferRequestOutcome, TransferRequestError>
// TransferRequestOutcome: Approved | Pending
// TransferRequestError: Denied(reason) | Network(e)

/// create_session 增加 batch_id 参数（Option<String>，v1 调用传 None）
pub fn create_session(host, base, auth, relative_path, size, batch_id: Option<&str>)
```

---

## 9. 插件 WASM 规范（接收方）

### 9.1 批钩子（`on_transfer_request`）

```rust
fn on_transfer_request(meta: &TransferRequestMeta) -> UploadHookDecision {
    // 按 settings.receiving_policy 分流：
    // "accept" → allow；"reject" → deny("policy-denied")；"ask"(默认) → ask
}
```

reject 策略：钩子 deny → 宿主 403 → 发送方 rejected(policy-denied)，接收方无任务无记录（零打扰）。**注意**：钩子函数不可异步——策略是同步读 settings，无 IO，满足。

### 9.2 接收状态（内存，`PluginState` 新字段，不持久化）

```rust
/// pending 批（接收端应答卡）
struct PendingBatch { batch_id, peer_id, files: Vec<UploadRequestMeta>, total_size, created_at }
/// 接收中任务（正在接收 tab；仅 session 级取消，无暂停/恢复）
struct ReceivingTask {
    session_id: String, batch_id: Option<String>, remote_path: String,
    size: u64, state: TaskState /* Transferring|Completed|Failed|Rejected|Cancelled */,
    reason: Option<String>, peer_id: String, created_at: u64, updated_at: u64,
}
```

### 9.3 消息处理（`on_message`，接收端订阅）

| topic | 处理 |
|---|---|
| `filesrv:transfer_request` | 建 PendingBatch（peer_id = 激活对端）→ emit `plugin:file-transfer:batches-changed`（全量批快照）→ **ask 模式**：不发 toast（等待应答） |
| `filesrv:transfer_resolved` | 移除 PendingBatch → emit batches-changed；decision=approved → emit toast 事件（批级一条：`{ name, count, totalSize, mode: "batch" }`） |
| `filesrv:receiving_started` | 建 ReceivingTask（Transferring）→ emit `plugin:file-transfer:receiving-changed`（全量接收快照）；**accept 模式**：emit toast 事件（`{ name, count, mode: "per-file" }`，前端 3s 窗口合并去重） |
| `filesrv:receiving_done` | ReceivingTask 终态（completed/failed/cancelled，409 竞态 → rejected duplicate-name）→ 归档历史（§10）→ emit receiving-changed + history-changed |

### 9.4 接收端命令

| 命令 | 行为 |
|---|---|
| `file-transfer.list-batches` | pending 批快照（前端应答卡数据源） |
| `file-transfer.approve-batch` `{ batchId }` | `filesrv_approve_transfer(batch_id)` |
| `file-transfer.reject-batch` `{ batchId }` | `filesrv_reject_transfer(batch_id)` |
| `file-transfer.list-receiving` | 接收任务快照 |
| `file-transfer.cancel-receiving` `{ sessionId }` | `filesrv_cancel_receiving(session_id)` |

### 9.5 接收端生命周期

- activate：订阅 4 个接收 topic + `filesrv:transfer_approval`；deactivate：退订；接收状态清空（不持久化）。
- **接收端不跨重启持久化**（宿主孤儿清理 + 发送方 session 丢失自动重建，v1 语义兜底）。

---

## 10. 传输历史（两端插件，对称各自记录）

- 存储 key：`transfer-history`，JSON 数组，**封顶 200 条**滚动淘汰最旧。
- 记录范围：全部终态任务（发送 + 接收、completed/failed/rejected/cancelled），终态即归档（**替代 v1「重启清除」**——v1 TaskStore 终态任务仅当次会话可见，v2 归档进历史）。
- 字段：`{ id, direction, initiator: "me"|"peer", fileName, size, state, reason?, peerName, localPath?, createdAt, updatedAt }`（localPath 仅 completed 且本地有文件时，供打开所在文件夹；接收任务无 localPath——移动端 MediaStore 场景无路径，桌面接收有私有下载路径）。
- 命令：`file-transfer.list-history` / `file-transfer.clear-history`；事件 `plugin:file-transfer:history-changed`（全量快照）。
- 归档时机：发送任务在 `handle_transfer_progress` 终态分支归档（并**从 TaskStore 移除**——终态不留在当前队列）；接收任务在 `receiving_done` 归档。remove_task 语义不变（仅移除当前列表）。
- **v1 兼容**：现有 `transfer-tasks` 存储中无终态任务（v1 不持久化终态），无迁移负担；v1 终态任务当次会话可见 → v2 归档后仍在历史 tab 可见，行为升级。

> 实现建议：`state.rs` 新增 `HistoryStore`（同 TaskStore 模式：load/save/insert/clear/snapshot + 封顶 200 纯函数 `trim_to_cap`，可单测）。

---

## 11. 插件命令/事件全表（两端一致）

### 命令（plugin.json 声明 + invoke_command 路由）

| 命令 | 变更 |
|---|---|
| `file-transfer.enqueue` | 新增参数 `batchId?: string` |
| `file-transfer.set-settings` | 新增 `receivingPolicy` / `approvalTimeoutSec` |
| `file-transfer.get-settings` | 返回新字段 |
| `file-transfer.list-batches` | **新** |
| `file-transfer.approve-batch` | **新** |
| `file-transfer.reject-batch` | **新** |
| `file-transfer.list-receiving` | **新** |
| `file-transfer.cancel-receiving` | **新** |
| `file-transfer.list-history` | **新** |
| `file-transfer.clear-history` | **新** |

### 前端事件（WASM emit，`plugin:file-transfer:*`）

| 事件 | 载荷 | 说明 |
|---|---|---|
| `plugin:file-transfer:tasks-changed` | 任务数组（含 waiting-approval / initiator 字段） | 现有，扩展字段 |
| `plugin:file-transfer:batches-changed` | 批数组 `{ batchId, peerName, files, totalSize, createdAt }` | **新**（接收端） |
| `plugin:file-transfer:receiving-changed` | 接收任务数组 | **新**（接收端） |
| `plugin:file-transfer:history-changed` | 历史数组 | **新** |
| `plugin:file-transfer:toast` | `{ name, count, totalSize?, mode: "batch"\|"per-file" }` | **新**（接收端 toast 请求，前端展示 + 3s 窗口合并去重 per-file） |

---

## 12. 前端 UI 规范（两端）

> 所有 UI 改动必须先加载 `frontend-styles` skill（token-bound、禁原生 select、反模式清单）。

### 12.1 Task 类型扩展（两端 `src/types.ts`）

```ts
export type TaskStateName = /* 现有 */ | 'waiting-approval'
export type TaskInitiator = 'me' | 'peer'
export interface Task {
  // ...现有...
  /** v2：发起方（队列分类依据；wire snake_case，默认 me） */
  initiator: TaskInitiator
  /** v2：所属批 ID（发送方上传任务） */
  batchId?: string | null
}
```

### 12.2 队列 4 tab（两端 TaskPanel / TaskQueueSheet）

- Tabs：**全部 | 正在发送 | 正在接收 | 历史**。
- 全部 = 非终态任务（tasks-changed）；正在发送 = initiator==='me'；正在接收 = initiator==='peer'；历史 = history-changed 快照。
- 状态 chips 汇总保持；waiting-approval 计入"排队"类 chip（或单独颜色）。
- 发送 tab：任务卡操作不变（暂停/恢复/取消/重试），waiting-approval 显示"等待对方同意"文案 + 可取消。
- 接收 tab：任务卡**只可取消**（cancel-receiving），无暂停/恢复。
- 历史 tab：只读条目（时间、方向 ↑/↓、文件名、大小、结果）+「清空历史」按钮 + 完成且有 localPath 的条目「打开所在文件夹」（桌面 `context.system.revealInDir`；移动复用现有「系统查看器打开文件」，无 localPath 不显示）。

### 12.3 pending 批卡与应答

- 桌面：**应用内横幅**（FileTransferView 顶部，批卡：对端名 + N 个文件 + 总大小 + 接受全部/拒绝全部）；最小化时系统通知仅「打开应用」（**不新增通知应答**）。
- 移动：**前台应用内对话框**（Material dialog：标题 transfer.request.title、正文 transfer.request.body、两按钮）；后台/锁屏 = 系统通知 action 按钮（接受全部/拒绝全部，见 §14）。
- 数据源：`batches-changed` 事件 + `list-batches` 初始拉取。
- 应答：调 `approve-batch` / `reject-batch`；宿主推送后批卡经 `batches-changed` 消失。

### 12.4 toast（接收端）

- 订阅 `plugin:file-transfer:toast`：`mode==='batch'` → 立即一条（转移 toast key：`transfer.toast.receiving`）；`mode==='per-file'` → **3 秒窗口合并去重**（窗口内只更新计数不重复弹）。
- toast 组件：桌面宿主提供？两端各自用宿主/插件现有 toast 机制（subagent 探索宿主 toast API，如 context.ui / notifications；无则自绘轻量 toast 组件，token-bound）。

### 12.5 设置 UI（两端 SettingsPanel / SettingsPage）

- 新增「接收策略」分段控件（**自绘 segmented，禁原生 select**）：每次询问 / 直接接收 / 直接拒绝。
- 「同意超时（秒）」数字输入：仅策略为「每次询问」时显示；范围 10–600。
- hint 文案：`transfer.settings.receivingPolicyHint`。

### 12.6 状态文案扩展（TASK_STATE_KEYS / 移动端等价 map）

- `waiting-approval` → `transfer.task.waitingApproval`
- 接收中（initiator peer && transferring）→ `transfer.task.receiving`
- rejected 原因映射（§8.4）三文案。

---

## 13. i18n key 表（zh-CN / en 两端同步，spec 14.7 全量 + 扩展）

spec 14.7 的 20 条（照抄表值，见 spec.md §14.7）+ 新增：

| key | zh-CN | en |
|---|---|---|
| `transfer.queue.all` | 全部 | All |
| `transfer.queue.sending` | 正在发送 | Sending |
| `transfer.queue.receiving` | 正在接收 | Receiving |
| `transfer.queue.history` | 历史 | History |
| `transfer.batch.acceptAll` | 接受全部 | Accept all |
| `transfer.batch.rejectAll` | 拒绝全部 | Reject all |
| `transfer.batch.pendingTitle` | 文件传输请求 | File transfer request |
| `transfer.history.results.completed` | 已完成 | Completed |
| `transfer.history.results.failed` | 失败 | Failed |
| `transfer.history.results.rejected` | 已拒绝 | Rejected |
| `transfer.history.results.cancelled` | 已取消 | Cancelled |
| `transfer.toast.receiving` | {name} 正在向你上传 {count} 个文件 | {name} is sending you {count} files |

> 注意：spec 14.7 已含 `transfer.task.waitingApproval`、`transfer.task.receiving`、`transfer.request.*`、`transfer.history.*`、`transfer.error.*`、`transfer.settings.*` 等 20 条——两端 i18n 文件（桌面 `plugins/file-transfer/src/i18n/{zh-CN,en}.ts`、移动对应文件）按上表 + spec 表合并补齐；现有状态 key（`transfer.task.state.*`）保留不动。

---

## 14. Kotlin（仅移动端）：通知 action 按钮

- 目标文件：`bedcode-mobile/src-tauri/gen/android/app/src/main/java/com/bedcode/mobile/TaskNotificationManager.kt`（+ 按需 `TaskNotificationPlugin.kt`）。**`android-backup/app-java/` 下旧副本勿改。**
- 需求：批量请求通知（后台/锁屏）带 **接受全部 / 拒绝全部** 两个 action 按钮 → 路由回宿主命令 `plugin_filesrv_approve_transfer` / `plugin_filesrv_reject_transfer`。
- 实现路径（agent 探索 TaskNotificationPlugin 现有 PendingIntent 模式后照做）：
  1. 宿主（Rust）在批 pending 且 App 在后台时，经现有通知通道（`task_notification`）发通知，附加 `batchId`；
  2. Kotlin `addAction(接受全部)` / `addAction(拒绝全部)`，PendingIntent 携带 action 类型 + batchId，点击 → 经 TaskNotificationPlugin（或新 NotificationActionPlugin）调 Tauri command 桥 → Rust 命令执行 approve/reject → 宿主发 `filesrv:transfer_resolved` + 跨端推送。
  3. action 响应式更新：通知点击后 dismiss；批已解决（resolved）后如通知仍在，宿主取消该通知（`cancel`）。
- 若 TaskNotificationPlugin 现有模式是"通知点击 → 打开 App"，则 v2 增加 action 分支（打开 App 的 PendingIntent 保留为通知默认点击）。
- **验证**：`cd bedcode-mobile/src-tauri/gen/android && ./gradlew :app:compileUniversalDebugKotlin`（必跑）。

---

## 15. 测试与验收

### 15.1 单元测试要求（两端）

| 层 | 必测点 |
|---|---|
| 宿主 transfer.rs | 状态迁移纯函数（pending→approved/rejected 合法、非法迁移拒绝）；RejectReason serde |
| 宿主 registry | create_transfer_request 三路分流（Wasm/Webview/None hook → allow/ask/deny）；approve/reject 归属校验（他插件应答 → NotFound）；check_batch gating（approved Ok / pending 403 / rejected 403 / not-found 403）；TTL：set_approval_timeout 边界（9/10/600/601）；sweep_batches 超时与 24h 清理 |
| 宿主 controller/server | 批端点：200/202/403；create_upload 带 batchId 免钩子 + 无 batchId ask → 403；receiving_started/done 事件发出 |
| 插件 state.rs | validate_transition 新增边（WaitingApproval 相关合法/非法）；HistoryStore trim_to_cap(200) |
| 插件批流 | 批记录状态机 + 拒绝 reason 映射纯函数 |
| 既有测试 | 全部保持通过（UploadHookDecision 三路化后 `deny` 构造不变，`allow` 断言兼容） |

### 15.2 命令验证（AGENTS.md 硬性）

- 两端 Rust：`cargo test`（桌面全量；移动端按现有可运行范围，至少 `cargo check` + 新增模块单测通过）。
- 前端：`npm run test:run`（**禁止** `npm run test` watch）。
- Kotlin：`./gradlew :app:compileUniversalDebugKotlin`。
- i18n：zh-CN 与 en key 一一对应。

### 15.3 手动联调验收清单（两端编译产物联调）

- [ ] ask 模式：发送方点发送 → 接收方出现批卡（桌面横幅/移动对话框）→ 接受全部 → 发送方任务转传输中 → 文件落盘 → 双方历史出现 completed。
- [ ] ask 拒绝：拒绝全部 → 发送方 rejected（文案"对方拒绝了传输"）→ 接收方无任务记录、无历史。
- [ ] ask 超时：不操作 60s → 发送方 rejected（"对方未响应"）→ 接收方批卡消失。
- [ ] accept 模式：直接传输，接收方 toast 3s 窗口合并；正在接收 tab 出现任务。
- [ ] reject 模式：发送方 403 → rejected（"对方设置了直接拒绝"）→ 接收方零打扰。
- [ ] 等待同意期间断线（关接收方 App）：发送方任务 → rejected(timeout)，重连不重发。
- [ ] 批准后断线重连续传：免问、免钩子续传。
- [ ] 接收方取消单个接收任务：发送方任务终态正确。
- [ ] 队列 4 tab 分类正确；历史封顶 200；清空历史；打开所在文件夹（完成且本地文件）。
- [ ] 设置：策略切换即时生效；超时 10–600 校验。
- [ ] 移动端后台/锁屏通知 action：接受全部/拒绝全部可应答。
- [ ] v1 回归：下载、续传、pause/resume、duplicate-name、peers 在线状态。

---

## 16. 任务划分与执行顺序

### Step 0（主 agent）
1. 本方案审批通过后，先向两个 subagent 交付：本方案 + spec.md §14 + issues/10 + issues/11。
2. 明确约束：**wire 定义逐字一致**（§2/§4/§5 表格）；每端 agent 开工前先用 codegraph 探索自己宿主侧的钩子分发/WS/命令桥结构，不照抄另一端的实现细节。

### Step 1（并行，两个 worker subagent，agentScope: both）

**Agent A — 桌面端全栈**（cwd: `D:/tauriProject/BedCode/bedcode-desktop`）：
- SDK：`packages/plugin-sdk-desktop/rust/src/{types.rs, wasm.rs, abi.rs, host/file_service.rs, wasm_host.rs}` + `packages/plugin-sdk-desktop/src/{types.ts, runtime.ts}`
- 宿主：§6 全部文件（含 transfer.rs 新建）
- 插件：`plugins/file-transfer/rust/src/*` + `plugins/file-transfer/src/*` + `plugin.json`
- 测试：桌面 `cargo test`、`npm run test:run`（前端有测试则跑）

**Agent B — 移动端全栈**（cwd: `D:/tauriProject/BedCode/bedcode-mobile`）：
- SDK：`packages/plugin-sdk-mobile/rust/src/*` + `packages/plugin-sdk-mobile/src/*`
- 宿主：§7 全部文件（含 transfer.rs 新建）
- 插件：`plugins/file-transfer/rust/src/*` + `plugins/file-transfer/src/*`
- Kotlin：§14
- 测试：移动 `cargo test`/`cargo check`、`./gradlew :app:compileUniversalDebugKotlin`、前端 `npm run test:run`

**每个 agent 的验收清单**：§15.1 对应侧 + 编译全绿；i18n 两端文件同步；文档注释中文；错误用 AppError/anyhow Context；无注释掉的代码。

### Step 2（主 agent，交叉验证与收尾）
1. diff 两端 wire 定义：`enums/file_service.rs`、`enums/sync.rs`、SDK `types.rs` 的 v2 部分、插件命令名/事件名/i18n key——不一致处修复（主 agent 直接改或交回对应 agent）。
2. 全量回归：两端 cargo test + npm run test:run + gradlew Kotlin 编译。
3. 更新 `CONTEXT.md`「文件传输」区术语（若实现引入新概念，如「批」）。
4. 手动联调验收（§15.3）——需要真实设备时产出联调记录到 `.scratch/lan-file-transfer-plugin/`。

### 风险与注意
- 两端 registry 已分化（移动 SAF 分流），**不要假设同构粘贴**；批逻辑共用但落点/钩子分发按各自实现。
- 批状态机是安全关键路径：ask 绕过（403）与 fail-closed 必须配测试。
- `UploadHookDecision` 三路化会触碰现有测试断言——保持 `allow()/deny()` 构造签名不变，只加 `ask()`。
- 历史归档改动 `useTasks.ts` 持久化（transfer-tasks key）→ v2 终态立即归档，前端「历史」tab 数据源切换为 `list-history`。
- 发送方批请求 HTTP 失败要能区分 403（策略拒绝）与网络错误（failed vs rejected）。
- `.scratch/`、`docs/` 受 git hooks 保护，提交时按 AGENTS.md 流程（dev/feature 分支正常跟踪）。

---

## 17. 需求覆盖矩阵（对照 handoff + spec 14 + issues 10/11）

| # | 需求（来源） | 方案落点 |
|---|---|---|
| 1 | 接收策略 receivingPolicy ask/accept/reject（默认 ask）+ approvalTimeoutSec 10–600（默认 60），存插件 storage 与 roots/download_dir/concurrency 并列 | §8.1 Settings 扩展 + §12.5 设置 UI + §13 i18n |
| 2 | 上传拆两段：POST /transfer-request（批 ID+清单+总大小）→ 钩子三路分流 → ask pending(202) → 宿主命令 approve/reject → WS 推送发送方 → 批准后批内免钩子 | §2.1、§2.2、§3.2、§5.2、§8.3 |
| 3 | 钩子三路化 allow/deny/ask + 批级钩子 onTransferRequest | §4.1–4.4（4 处 SDK 同步：桌面/移动 TS + 两端 Rust + 两端宿主 plugin/types） |
| 4 | ask 模式强制批上下文：无已批准批 ID 的 session 创建一律 403 | §2.2 gating + §15.1 测试 |
| 5 | pending 批 TTL 宿主扫描自动拒绝（默认 60s 可配） | §3.2 sweep_batches + set_approval_timeout |
| 6 | 批准状态随批保留至批内全部终态（+24h TTL） | §3.2 touch_batch + 24h 清理 |
| 7 | 等待同意期间断线不重发，任务直接 rejected(timeout) | §8.4 peer offline 处理 |
| 8 | 批内文件同名沿用 v1 per-file 同名即拒，该文件 rejected 批内其他不受影响 | §2.3 complete 409 路径（v1 保留） |
| 9 | 状态机扩展：发送方 waiting-approval；接收方 pending→transferring→completed/failed、pending→rejected、transferring→rejected（取消） | §8.2、§9.2 |
| 10 | rejected reason 扩展：duplicate-name/user-rejected/timeout/policy-denied + 发送方三种拒绝文案 | §8.4 映射 + §13 i18n |
| 11 | 接收方任务只可取消、不可暂停/恢复；不跨重启持久化 | §9.4 cancel-receiving + §9.5 |
| 12 | 队列 4 tab：全部/正在发送/正在接收/历史（发送接收以发起方区分） | §12.2 + §11 事件 |
| 13 | 批量请求应答：移动前台对话框/后台通知 action；桌面横幅/最小化仅打开应用 | §12.3、§14 |
| 14 | 接收中 toast：ask 批准后批级一条；accept 传输开始 3s 窗口合并去重；reject 无 | §9.3、§12.4 |
| 15 | pending 批卡（对端名+N 文件+总大小+两按钮）；批准后批卡消失文件逐条出现；拒绝/超时批卡消失且接收方不记历史 | §9.3、§12.3、§10 |
| 16 | 传输历史：两端各自全部终态任务、封顶 200、只读+清空+打开所在文件夹 | §10、§12.2 |
| 17 | 宿主/SDK 脚印：批状态机、/transfer-request、approve/reject 命令、批 ID 校验、WS 新消息、session 免钩子、UploadHookDecision ask、onTransferRequest、命令封装 | §3、§5、§6、§7 |
| 18 | Kotlin 通知 action 按钮 + PendingIntent 路由回插件命令 | §14 |
| 19 | 移动端前台服务通知带取消动作（复用 TaskNotification） | §14（取消动作若已有则保留；新增批应答 action） |
| 20 | i18n 20 条 key zh/en 同步 | §13 |
| 21 | 性能验收：v2 无新增验收项，回归 | §15.3 v1 回归清单 |

---

## 附：执行约束（AGENTS.md 强制）

- CodeGraph 优先于 grep 循环；⚠️ staleness banner 文件以 Read 为准。
- 改 UI 前加载 `frontend-styles` skill；禁止原生 select/checkbox 外观；token-bound。
- composable 禁中文硬编码；i18n key 双端同步；注释解释为什么。
- 禁 `unsafe impl Send/Sync`；错误上下文化；`tokio::spawn` 用 `spawn_with_error_boundary`。
- 前端测试用 `npm run test:run`；Kotlin 改动必跑 gradlew 编译。
- commit message 禁 `Co-Authored-By`。
