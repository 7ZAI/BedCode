//! 任务状态机与持久化
//!
//! 传输任务的生命周期管理：状态迁移规则（spec §7.1）、持久化策略（spec §7.3）。
//! 状态机迁移函数为纯函数，可独立单测。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 任务状态（spec §7.1 + v2 §14.3）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    /// 排队等待槽位
    Queued,
    /// v2：等待对方同意（仅 ask 模式上传任务，批上下文内）
    #[serde(rename = "waiting-approval")]
    WaitingApproval,
    /// v2.1：intent 已发送（等待手机回执）瞬时态
    ///
    /// 服务器归零后桌面不再直连手机：发 intent → 收 ACK → 收 progress 三步，
    /// ACK 到达即转 Transferring/Rejected。任务五态（v2）不变，此为瞬时态，
    /// 不持久化（TaskStore::load 过滤规则不保留，重启等价于未发）
    #[serde(rename = "waiting-reply")]
    WaitingReply,
    /// 传输进行中
    Transferring,
    /// 用户手动暂停
    Paused,
    /// 断线/对端下线自动暂停（重连自动续传）
    Resumable,
    /// 传输完成（终态）
    Completed,
    /// 传输失败（终态）
    Failed,
    /// 同名被拒（终态）
    Rejected,
    /// 用户取消（终态）
    Cancelled,
}

impl TaskState {
    /// 是否为终态（completed / failed / rejected / cancelled）
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskState::Completed | TaskState::Failed | TaskState::Rejected | TaskState::Cancelled
        )
    }

    /// 是否正在传输
    pub fn is_active(self) -> bool {
        matches!(self, TaskState::Transferring)
    }

    /// 是否可被调度（queued 等待中 或 resumable 可恢复）
    pub fn is_schedulable(self) -> bool {
        matches!(self, TaskState::Queued | TaskState::Resumable)
    }
}

/// 任务终态原因（wire JSON 保持字符串形状不变，经 `#[serde(from/into = "String")]`）
///
/// 枚举化收益：已知原因编译期拼写检查 + 匹配；动态透传（宿主错误信息、
/// 对端任意 reason 字符串）经 `Other` 兜底保留原文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum TaskReason {
    /// 目标同名（对端拒绝 / complete 409）
    DuplicateName,
    /// 用户拒绝
    UserRejected,
    /// 审批超时未决
    Timeout,
    /// 策略拒绝（auto-reject）
    PolicyDenied,
    /// 对端离线自动挂起
    PeerOffline,
    /// 一般性传输失败
    TransferFailed,
    /// 对端中止（非用户取消路径）
    CancelledByPeer,
    /// 远端文件变化（指纹不符）
    RemoteChanged,
    /// 续传重建超限
    ResumeLimitExceeded,
    /// 其他/动态透传原因（错误信息原文、对端自定义字符串）
    Other(String),
}

impl TaskReason {
    /// wire 字符串值（与 `From<TaskReason> for String` 一致）
    pub fn as_str(&self) -> &str {
        match self {
            TaskReason::DuplicateName => "duplicate-name",
            TaskReason::UserRejected => "user-rejected",
            TaskReason::Timeout => "timeout",
            TaskReason::PolicyDenied => "policy-denied",
            TaskReason::PeerOffline => "peer-offline",
            TaskReason::TransferFailed => "transfer-failed",
            TaskReason::CancelledByPeer => "cancelled by peer",
            TaskReason::RemoteChanged => "remote-changed",
            TaskReason::ResumeLimitExceeded => "resume-limit-exceeded",
            TaskReason::Other(s) => s,
        }
    }

    /// 从 wire 字符串构造（未识别的值原样保留为 `Other`）
    pub fn from_str(s: &str) -> Self {
        match s {
            "duplicate-name" => TaskReason::DuplicateName,
            "user-rejected" => TaskReason::UserRejected,
            "timeout" => TaskReason::Timeout,
            "policy-denied" => TaskReason::PolicyDenied,
            "peer-offline" => TaskReason::PeerOffline,
            "transfer-failed" => TaskReason::TransferFailed,
            "cancelled by peer" => TaskReason::CancelledByPeer,
            "remote-changed" => TaskReason::RemoteChanged,
            "resume-limit-exceeded" => TaskReason::ResumeLimitExceeded,
            _ => TaskReason::Other(s.to_string()),
        }
    }
}

impl From<String> for TaskReason {
    fn from(s: String) -> Self {
        TaskReason::from_str(&s)
    }
}

impl From<TaskReason> for String {
    fn from(r: TaskReason) -> Self {
        r.as_str().to_string()
    }
}

/// 传输决策（wire JSON 保持字符串形状，经 `#[serde(from/into = "String")]`）
///
/// 两端 wire 值分两组：intent ACK 用 "accepted"/"rejected"，transfer approval/
/// resolved 用 "approved"/"rejected"——同一 `TransferDecision` 枚举双值映射，
/// 内部不再散布字符串字面量。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum TransferDecision {
    /// 同意（wire "accepted"：intent ACK）
    Accepted,
    /// 同意（wire "approved"：transfer approval / resolved）
    Approved,
    /// 拒绝（wire "rejected"）
    Rejected,
}

impl TransferDecision {
    /// wire 字符串值
    pub fn as_str(&self) -> &str {
        match self {
            TransferDecision::Accepted => "accepted",
            TransferDecision::Approved => "approved",
            TransferDecision::Rejected => "rejected",
        }
    }

    /// 从 wire 字符串构造（未识别的值 None）
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "accepted" => Some(TransferDecision::Accepted),
            "approved" => Some(TransferDecision::Approved),
            "rejected" => Some(TransferDecision::Rejected),
            _ => None,
        }
    }

    /// 是否同意（Accepted / Approved）
    pub fn is_accept(self) -> bool {
        matches!(self, TransferDecision::Accepted | TransferDecision::Approved)
    }

    /// 是否拒绝
    pub fn is_reject(self) -> bool {
        self == TransferDecision::Rejected
    }
}

impl From<String> for TransferDecision {
    fn from(s: String) -> Self {
        TransferDecision::from_str(&s).unwrap_or(TransferDecision::Rejected)
    }
}

impl From<TransferDecision> for String {
    fn from(d: TransferDecision) -> Self {
        d.as_str().to_string()
    }
}

/// 意图方向（wire JSON 保持字符串："pull"/"push"）
///
/// 服务器归零后桌面为协调者：pull = 手机→桌面方向链路（手机发起 HTTP），
/// push = 桌面→手机。与任务内 `Direction`（download/upload 业务语义）正交。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentDirection {
    /// 拉取（wire "pull"）
    Pull,
    /// 推送（wire "push"）
    Push,
}

impl IntentDirection {
    /// wire 字符串值
    pub fn as_str(&self) -> &str {
        match self {
            IntentDirection::Pull => "pull",
            IntentDirection::Push => "push",
        }
    }
}

impl From<IntentDirection> for String {
    fn from(d: IntentDirection) -> Self {
        d.as_str().to_string()
    }
}

/// 校验状态迁移合法性（spec §7.1 + v2 §14.3）
///
/// 返回 `Ok(())` 表示迁移合法，`Err(reason)` 表示非法迁移。
/// 纯函数，无副作用，可独立单测。
/// v2 新增边：Queued→WaitingApproval（ask 批等待同意）、
/// WaitingApproval→Queued（批准后重新调度）/Rejected（拒绝/超时）/
/// Cancelled（用户取消）/Resumable（对端下线兜底，防御性）
pub fn validate_transition(from: TaskState, to: TaskState) -> Result<(), &'static str> {
    match (from, to) {
        // queued → transferring（槽位空出）/ cancelled / resumable（对端下线）/
        // waiting-approval（v2：ask 批等待同意）/ waiting-reply（v2.1：intent 已发待回执）
        (TaskState::Queued, TaskState::Transferring) => Ok(()),
        (TaskState::Queued, TaskState::Cancelled) => Ok(()),
        (TaskState::Queued, TaskState::Resumable) => Ok(()),
        (TaskState::Queued, TaskState::WaitingApproval) => Ok(()),
        (TaskState::Queued, TaskState::WaitingReply) => Ok(()),

        // waiting-reply（v2.1）：ACK accepted → transferring / rejected → rejected；
        // 用户取消 → cancelled；心跳超时/对端下线 → resumable（暂停-待续传）
        (TaskState::WaitingReply, TaskState::Transferring) => Ok(()),
        (TaskState::WaitingReply, TaskState::Rejected) => Ok(()),
        (TaskState::WaitingReply, TaskState::Cancelled) => Ok(()),
        (TaskState::WaitingReply, TaskState::Resumable) => Ok(()),
        (TaskState::WaitingReply, TaskState::Failed) => Ok(()),

        // waiting-approval（v2）：批准 → queued 重新调度；拒绝/超时 → rejected；
        // 用户取消 → cancelled；对端下线兜底 → resumable（实际采用 rejected(timeout)）
        (TaskState::WaitingApproval, TaskState::Queued) => Ok(()),
        (TaskState::WaitingApproval, TaskState::Rejected) => Ok(()),
        (TaskState::WaitingApproval, TaskState::Cancelled) => Ok(()),
        (TaskState::WaitingApproval, TaskState::Resumable) => Ok(()),

        // transferring → paused（用户）/ resumable（断线）/ completed / failed / rejected / cancelled
        (TaskState::Transferring, TaskState::Paused) => Ok(()),
        (TaskState::Transferring, TaskState::Resumable) => Ok(()),
        (TaskState::Transferring, TaskState::Completed) => Ok(()),
        (TaskState::Transferring, TaskState::Failed) => Ok(()),
        (TaskState::Transferring, TaskState::Rejected) => Ok(()),
        (TaskState::Transferring, TaskState::Cancelled) => Ok(()),

        // paused → queued（用户恢复，重新入队调度）/ transferring（恢复）/ cancelled
        (TaskState::Paused, TaskState::Queued) => Ok(()),
        (TaskState::Paused, TaskState::Transferring) => Ok(()),
        (TaskState::Paused, TaskState::Cancelled) => Ok(()),

        // resumable → queued（用户恢复 / 重连后自动恢复，重新入队）/ transferring（恢复）/ cancelled
        (TaskState::Resumable, TaskState::Queued) => Ok(()),
        (TaskState::Resumable, TaskState::Transferring) => Ok(()),
        (TaskState::Resumable, TaskState::Cancelled) => Ok(()),

        // failed → queued（重试）/ cancelled
        (TaskState::Failed, TaskState::Queued) => Ok(()),
        (TaskState::Failed, TaskState::Cancelled) => Ok(()),

        // 终态不可迁出
        (TaskState::Completed, _) => Err("cannot transition from completed"),
        (TaskState::Rejected, _) => Err("cannot transition from rejected"),
        (TaskState::Cancelled, _) => Err("cannot transition from cancelled"),

        // 自迁移无意义
        (from, to) if from == to => Err("self-transition"),

        // 其他均为非法
        _ => Err("invalid state transition"),
    }
}

/// 传输方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// 从对端下载到本地
    Download,
    /// 从本地上传到对端
    Upload,
}

impl Direction {
    /// wire 字符串值（与 serde lowercase 一致）
    pub fn as_str(&self) -> &str {
        match self {
            Direction::Download => "download",
            Direction::Upload => "upload",
        }
    }
}

/// 对端设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// 对端设备 ID
    pub device_id: String,
    /// 对端设备名称（展示用）
    pub name: String,
}

/// 文件指纹（续传有效性校验，spec §7.4）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fingerprint {
    /// 文件大小（字节）
    pub size: u64,
    /// 修改时间（Unix 秒）
    pub mtime: u64,
}

/// v2：默认发起方（wire 值 "me"，桌面端任务均为本端发起）
fn default_initiator() -> String {
    "me".to_string()
}

/// 传输任务（spec §7.3 字段 + 前端便利字段）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// 任务唯一 ID（插件生成，UUID）
    pub id: String,
    /// 传输方向
    pub direction: Direction,
    /// 对端信息
    pub peer: PeerInfo,
    /// 远端路径（相对挂载点）
    pub remote_path: String,
    /// 本地路径（下载 = .part 写入路径，上传 = 源文件路径）
    pub local_path: String,
    /// 文件总大小（字节，0 = 未知）
    pub size: u64,
    /// 已传输偏移（字节）
    pub offset: u64,
    /// 上传会话 ID（仅上传方向）
    pub upload_session_id: Option<String>,
    /// 文件指纹（续传校验用）
    pub fingerprint: Option<Fingerprint>,
    /// 当前状态
    pub state: TaskState,
    /// 失败/拒绝原因（wire 字符串形状不变，见 TaskReason）
    pub reason: Option<TaskReason>,
    /// 创建时间（Unix 毫秒）
    pub created_at: u64,
    /// 更新时间（Unix 毫秒）
    pub updated_at: u64,
    /// v2：所属批 ID（上传任务，一次「发送」动作一匹；wire snake_case）
    ///
    /// 批上下文只在批记录（内存）存在时有效；重启后批记录丢失，
    /// 带批 ID 的排队任务会在启动时重新发起 transfer-request（新批）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    /// v2：发起方（队列分类依据；桌面端任务均为本端发起，固定 "me"）
    #[serde(default = "default_initiator")]
    pub initiator: String,

    // ---- 运行时字段（不持久化） ----
    /// 宿主传输引擎 task_id（关联进度回调）
    #[serde(skip)]
    pub host_task_id: Option<String>,
    /// 是否因断线自动转为 resumable（重连自动续传标记）
    #[serde(skip)]
    pub auto_resumable: bool,
    /// 上次持久化时间戳（毫秒，用于 1s 节流）
    #[serde(skip)]
    pub last_flush: u64,
    /// v2.1：intent 驱动任务的意图 ID（关联 ACK/进度/心跳/取消）
    #[serde(skip)]
    pub intent_id: Option<String>,
    /// v2.1：intent 驱动任务最近一次活跃时间（毫秒，30s 无回传判失联）
    #[serde(skip)]
    pub last_activity: u64,
}

impl Task {
    /// 尝试状态迁移，合法则更新 state 并返回 Ok(())
    pub fn transition(&mut self, new_state: TaskState) -> Result<(), String> {
        validate_transition(self.state, new_state)
            .map_err(|e| format!("task {} transition {:?}→{:?}: {}", self.id, self.state, new_state, e))?;
        self.state = new_state;
        Ok(())
    }

    /// 距上次持久化是否已超过 1s（进度节流判断，纯逻辑）
    pub fn should_flush(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_flush) >= 1000
    }

    /// 标记已持久化（记录当前时间戳）
    pub fn mark_flushed(&mut self, now_ms: u64) {
        self.last_flush = now_ms;
    }
}

// ==================== TaskStore ====================

/// Storage key（插件 KV 存储）
const STORAGE_KEY: &str = "transfer-tasks";

/// 任务持久化存储
///
/// 写入策略（spec §7.3）：
/// - 传输中：每 1s 节流（记录 last_flush，超期才写）
/// - 状态迁移：立即写
/// - deactivate：强制 flush
///
/// 重启加载：保留 paused/resumable，传输中残留（App 被杀）降级为 resumable（其余丢弃）
pub struct TaskStore {
    tasks: HashMap<String, Task>,
    /// 待持久化标记（dirty = 需要写入 storage）
    dirty: bool,
}

impl TaskStore {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            dirty: false,
        }
    }

    /// 从宿主 storage 加载（保留 paused/resumable，传输中残留降级为 resumable，spec §7.3；
    /// v2：WaitingApproval 任务丢弃——批上下文不可恢复，等价于未发，spec §8.2）
    pub fn load(&mut self, host: &impl bedcode_plugin_api::host::HostStorage) {
        match host.storage_get(STORAGE_KEY) {
            Ok(Some(value)) => {
                let all: Vec<Task> = serde_json::from_value(value).unwrap_or_default();
                self.tasks = all
                    .into_iter()
                    .filter(|t| {
                        matches!(
                            t.state,
                            TaskState::Paused | TaskState::Resumable | TaskState::Transferring
                        )
                    })
                    .map(|mut t| {
                        // App 被杀残留的 transferring → 降级为 resumable，保留 offset/fingerprint
                        if t.state == TaskState::Transferring {
                            t.state = TaskState::Resumable;
                        }
                        // 重启恢复不置 auto_resumable（spec §7.2：需手动「全部继续」，不自动传）
                        t.auto_resumable = false;
                        (t.id.clone(), t)
                    })
                    .collect();
            }
            _ => {
                self.tasks = HashMap::new();
            }
        }
        self.dirty = false;
    }

    /// 全量持久化到宿主 storage
    pub fn save(&self, host: &impl bedcode_plugin_api::host::HostStorage) {
        let values: Vec<&Task> = self.tasks.values().collect();
        if let Ok(json) = serde_json::to_value(&values) {
            let _ = host.storage_set(STORAGE_KEY, &json);
        }
    }

    pub fn get(&self, id: &str) -> Option<&Task> {
        self.tasks.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Task> {
        self.dirty = true;
        self.tasks.get_mut(id)
    }

    pub fn insert(&mut self, task: Task) {
        self.dirty = true;
        self.tasks.insert(task.id.clone(), task);
    }

    pub fn remove(&mut self, id: &str) -> Option<Task> {
        self.dirty = true;
        self.tasks.remove(id)
    }

    pub fn values(&self) -> impl Iterator<Item = &Task> {
        self.tasks.values()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Task> {
        self.dirty = true;
        self.tasks.values_mut()
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// 按 host_task_id 查找本地任务 ID
    pub fn find_by_host_task_id(&self, host_task_id: &str) -> Option<String> {
        self.tasks
            .values()
            .find(|t| t.host_task_id.as_deref() == Some(host_task_id))
            .map(|t| t.id.clone())
    }

    /// 返回所有任务的快照（供前端渲染）
    pub fn snapshot(&self) -> Vec<Task> {
        let mut tasks: Vec<Task> = self.tasks.values().cloned().collect();
        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        tasks
    }
}

// ==================== HistoryStore（v2 传输历史，spec §14.5） ====================

/// 历史存储 key（插件 KV 存储）
const HISTORY_KEY: &str = "transfer-history";
/// 历史封顶条数（超出滚动淘汰最旧）
const HISTORY_CAP: usize = 200;

/// 传输历史条目（终态任务归档，per-file 记录，批维度不记）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// 任务 ID（发送任务 = 原任务 ID；接收任务 = 接收 session_id）
    pub id: String,
    /// 协议方向：upload（我发送）/ download（我下载）
    pub direction: Direction,
    /// 发起方："me" | "peer"（队列分类依据）
    #[serde(default = "default_initiator")]
    pub initiator: String,
    /// 文件名（展示用）
    pub file_name: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 终态（completed / failed / rejected / cancelled）
    pub state: TaskState,
    /// 失败/拒绝原因（如 duplicate-name / user-rejected / timeout / policy-denied）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<TaskReason>,
    /// 对端设备名（展示用）
    #[serde(default)]
    pub peer_name: String,
    /// 本地路径（仅 completed 且本地有文件时，供「打开所在文件夹」；
    /// 接收任务无 localPath——桌面接收落点在私有下载目录，路径对端不可知）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    /// 创建时间（Unix 毫秒）
    pub created_at: u64,
    /// 终态时间（Unix 毫秒）
    pub updated_at: u64,
}

/// 滚动淘汰最旧条目（纯函数，可单测）：返回被淘汰的条数
///
/// 封顶 200 条；超出部分从头部（最旧）开始淘汰
pub fn trim_to_cap(entries: &mut Vec<HistoryEntry>, cap: usize) -> usize {
    if entries.len() <= cap {
        return 0;
    }
    let removed = entries.len() - cap;
    entries.drain(0..removed);
    removed
}

/// 传输历史存储（同 TaskStore 模式：load/save/insert/clear/snapshot + 封顶滚动）
///
/// 写入策略：终态任务归档时立即写；deactivate 强制 flush。
/// 记录范围（spec §14.5）：全部终态任务（发送 + 接收），直接拒绝模式无任务不补记。
pub struct HistoryStore {
    /// 历史条目（头部最旧，尾部最新）
    entries: Vec<HistoryEntry>,
    /// 待持久化标记
    dirty: bool,
}

impl HistoryStore {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            dirty: false,
        }
    }

    /// 从宿主 storage 加载（损坏数据静默重置为空）
    pub fn load(&mut self, host: &impl bedcode_plugin_api::host::HostStorage) {
        match host.storage_get(HISTORY_KEY) {
            Ok(Some(value)) => {
                let mut entries: Vec<HistoryEntry> =
                    serde_json::from_value(value).unwrap_or_default();
                // 加载时同样执行封顶（防御：旧数据或手工修改超出上限）
                trim_to_cap(&mut entries, HISTORY_CAP);
                self.entries = entries;
            }
            _ => {
                self.entries = Vec::new();
            }
        }
        self.dirty = false;
    }

    /// 全量持久化到宿主 storage
    pub fn save(&self, host: &impl bedcode_plugin_api::host::HostStorage) {
        if let Ok(json) = serde_json::to_value(&self.entries) {
            let _ = host.storage_set(HISTORY_KEY, &json);
        }
    }

    /// 归档一条终态记录（封顶滚动淘汰 + 立即持久化），返回是否成功
    pub fn insert(&mut self, host: &impl bedcode_plugin_api::host::HostStorage, entry: HistoryEntry) -> bool {
        self.dirty = true;
        self.entries.push(entry);
        trim_to_cap(&mut self.entries, HISTORY_CAP);
        self.save(host);
        self.dirty = false;
        true
    }

    /// 清空历史（立即持久化）
    pub fn clear(&mut self, host: &impl bedcode_plugin_api::host::HostStorage) {
        self.entries.clear();
        self.dirty = true;
        self.save(host);
        self.dirty = false;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 返回全部条目快照（最新在前，供前端渲染）
    pub fn snapshot(&self) -> Vec<HistoryEntry> {
        let mut entries = self.entries.clone();
        entries.reverse();
        entries
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        // queued → transferring
        assert!(validate_transition(TaskState::Queued, TaskState::Transferring).is_ok());
        // queued → cancelled
        assert!(validate_transition(TaskState::Queued, TaskState::Cancelled).is_ok());
        // v2：queued → waiting-approval（ask 批等待同意）
        assert!(validate_transition(TaskState::Queued, TaskState::WaitingApproval).is_ok());
        // v2：waiting-approval → queued（批准后重新调度）/ rejected（拒绝/超时）/ cancelled
        assert!(validate_transition(TaskState::WaitingApproval, TaskState::Queued).is_ok());
        assert!(validate_transition(TaskState::WaitingApproval, TaskState::Rejected).is_ok());
        assert!(validate_transition(TaskState::WaitingApproval, TaskState::Cancelled).is_ok());
        assert!(validate_transition(TaskState::WaitingApproval, TaskState::Resumable).is_ok());
        // v2.1：queued → waiting-reply（intent 已发待回执）；
        // waiting-reply → transferring/rejected/cancelled/resumable/failed
        assert!(validate_transition(TaskState::Queued, TaskState::WaitingReply).is_ok());
        assert!(validate_transition(TaskState::WaitingReply, TaskState::Transferring).is_ok());
        assert!(validate_transition(TaskState::WaitingReply, TaskState::Rejected).is_ok());
        assert!(validate_transition(TaskState::WaitingReply, TaskState::Cancelled).is_ok());
        assert!(validate_transition(TaskState::WaitingReply, TaskState::Resumable).is_ok());
        assert!(validate_transition(TaskState::WaitingReply, TaskState::Failed).is_ok());
        // transferring → all valid targets
        assert!(validate_transition(TaskState::Transferring, TaskState::Paused).is_ok());
        assert!(validate_transition(TaskState::Transferring, TaskState::Resumable).is_ok());
        assert!(validate_transition(TaskState::Transferring, TaskState::Completed).is_ok());
        assert!(validate_transition(TaskState::Transferring, TaskState::Failed).is_ok());
        assert!(validate_transition(TaskState::Transferring, TaskState::Rejected).is_ok());
        assert!(validate_transition(TaskState::Transferring, TaskState::Cancelled).is_ok());
        // paused → queued（用户恢复） / transferring / cancelled
        assert!(validate_transition(TaskState::Paused, TaskState::Queued).is_ok());
        assert!(validate_transition(TaskState::Paused, TaskState::Transferring).is_ok());
        assert!(validate_transition(TaskState::Paused, TaskState::Cancelled).is_ok());
        // resumable → queued（重连自动恢复） / transferring / cancelled
        assert!(validate_transition(TaskState::Resumable, TaskState::Queued).is_ok());
        assert!(validate_transition(TaskState::Resumable, TaskState::Transferring).is_ok());
        assert!(validate_transition(TaskState::Resumable, TaskState::Cancelled).is_ok());
        // failed → queued (retry) / cancelled
        assert!(validate_transition(TaskState::Failed, TaskState::Queued).is_ok());
        assert!(validate_transition(TaskState::Failed, TaskState::Cancelled).is_ok());
    }

    #[test]
    fn test_invalid_transitions() {
        // 终态不可迁出
        assert!(validate_transition(TaskState::Completed, TaskState::Queued).is_err());
        assert!(validate_transition(TaskState::Rejected, TaskState::Queued).is_err());
        assert!(validate_transition(TaskState::Cancelled, TaskState::Queued).is_err());
        // v2：waiting-approval 不可直接转入 transferring（必须经 queued 重新调度）
        assert!(validate_transition(TaskState::WaitingApproval, TaskState::Transferring).is_err());
        // v2.1：waiting-reply 不可直接入 queued（重发 intent 经 resumable→queued 路径）
        assert!(validate_transition(TaskState::WaitingReply, TaskState::Queued).is_err());
        // 非法迁移
        assert!(validate_transition(TaskState::Queued, TaskState::Paused).is_err());
        assert!(validate_transition(TaskState::Paused, TaskState::Completed).is_err());
        assert!(validate_transition(TaskState::Resumable, TaskState::Paused).is_err());
        // 自迁移
        assert!(validate_transition(TaskState::Queued, TaskState::Queued).is_err());
        assert!(validate_transition(TaskState::WaitingApproval, TaskState::WaitingApproval).is_err());
    }

    #[test]
    fn test_terminal_states() {
        assert!(TaskState::Completed.is_terminal());
        assert!(TaskState::Failed.is_terminal());
        assert!(TaskState::Rejected.is_terminal());
        assert!(TaskState::Cancelled.is_terminal());
        assert!(!TaskState::Queued.is_terminal());
        assert!(!TaskState::Transferring.is_terminal());
        assert!(!TaskState::Paused.is_terminal());
        assert!(!TaskState::Resumable.is_terminal());
        assert!(!TaskState::WaitingApproval.is_terminal());
        assert!(!TaskState::WaitingReply.is_terminal());

        // v2.1：waiting-reply 是瞬时态，不参与调度
        assert!(!TaskState::WaitingReply.is_schedulable());
    }

    #[test]
    fn test_task_transition() {
        let mut task = Task {
            id: "test".to_string(),
            direction: Direction::Download,
            peer: PeerInfo { device_id: "d".to_string(), name: "n".to_string() },
            remote_path: "file.txt".to_string(),
            local_path: "/tmp/file.txt.part".to_string(),
            size: 1000,
            offset: 0,
            upload_session_id: None,
            fingerprint: None,
            state: TaskState::Queued,
            reason: None,
            created_at: 0,
            updated_at: 0,
            batch_id: None,
            initiator: "me".to_string(),
            host_task_id: None,
            auto_resumable: false,
            last_flush: 0,
            intent_id: None,
            last_activity: 0,
        };
        assert!(task.transition(TaskState::Transferring).is_ok());
        assert_eq!(task.state, TaskState::Transferring);
        // 非法迁移保持原状态
        assert!(task.transition(TaskState::Queued).is_err());
        assert_eq!(task.state, TaskState::Transferring);
    }

    #[test]
    fn test_waiting_reply_wire_name() {
        // wire lowercase：前端按字面量展示「等待回执」；与 v2 waiting-approval 同风格
        assert_eq!(
            serde_json::to_value(TaskState::WaitingReply).unwrap(),
            serde_json::json!("waiting-reply")
        );
        let back: TaskState = serde_json::from_value(serde_json::json!("waiting-reply")).unwrap();
        assert_eq!(back, TaskState::WaitingReply);
    }

    #[test]
    fn test_waiting_approval_wire_name() {
        // wire lowercase：前端按字面量展示「等待对方同意」
        assert_eq!(
            serde_json::to_value(TaskState::WaitingApproval).unwrap(),
            serde_json::json!("waiting-approval")
        );
        let back: TaskState =
            serde_json::from_value(serde_json::json!("waiting-approval")).unwrap();
        assert_eq!(back, TaskState::WaitingApproval);
    }

    // ==================== HistoryStore（v2） ====================

    fn sample_entry(id: &str) -> HistoryEntry {
        HistoryEntry {
            id: id.to_string(),
            direction: Direction::Upload,
            initiator: "me".to_string(),
            file_name: format!("{}.bin", id),
            size: 1024,
            state: TaskState::Completed,
            reason: None,
            peer_name: "phone".to_string(),
            local_path: Some("/tmp/a.bin".to_string()),
            created_at: 1,
            updated_at: 2,
        }
    }

    #[test]
    fn trim_to_cap_removes_oldest_only() {
        let mut entries: Vec<HistoryEntry> =
            (0..250).map(|i| sample_entry(&format!("t{}", i))).collect();
        let removed = trim_to_cap(&mut entries, 200);
        assert_eq!(removed, 50);
        assert_eq!(entries.len(), 200);
        // 最旧 50 条被淘汰（t0..t49），最新 200 条保留
        assert_eq!(entries[0].id, "t50");
        assert_eq!(entries[199].id, "t249");
        // 未超上限：不动
        let removed = trim_to_cap(&mut entries, 200);
        assert_eq!(removed, 0);
        assert_eq!(entries.len(), 200);
    }

    #[test]
    fn history_snapshot_newest_first() {
        let mut store = HistoryStore::new();
        store.entries = vec![sample_entry("old"), sample_entry("new")];
        let snap = store.snapshot();
        assert_eq!(snap[0].id, "new");
        assert_eq!(snap[1].id, "old");
    }

    #[test]
    fn task_reason_keeps_wire_string_shape() {
        // 枚举化不得改变 wire JSON 形状：Task / HistoryEntry 的 reason 必须是字符串
        let json = serde_json::json!("duplicate-name");
        let r: TaskReason = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(r, TaskReason::DuplicateName);
        assert_eq!(serde_json::to_value(r).unwrap(), json);
        // 未知字符串 → Other 原样保留，往返形状不变
        let unknown = serde_json::json!("custom-wire-reason");
        let r2: TaskReason = serde_json::from_value(unknown.clone()).unwrap();
        assert_eq!(r2, TaskReason::Other("custom-wire-reason".into()));
        assert_eq!(serde_json::to_value(r2).unwrap(), unknown);
        // 历史条目整体序列化（批内字段名 camelCase，reason 保持字符串）
        let entry = HistoryEntry {
            id: "s1".into(),
            direction: Direction::Download,
            initiator: "me".into(),
            file_name: "a.bin".into(),
            size: 10,
            state: TaskState::Rejected,
            reason: Some(TaskReason::DuplicateName),
            peer_name: "phone".into(),
            local_path: None,
            created_at: 1,
            updated_at: 2,
        };
        let v = serde_json::to_value(&entry).unwrap();
        assert_eq!(v["state"], "rejected");
        assert_eq!(v["reason"], "duplicate-name");
    }

    #[test]
    fn transfer_decision_wire_shape() {
        assert_eq!(
            serde_json::to_string(&TransferDecision::Accepted).unwrap(),
            "\"accepted\""
        );
        assert_eq!(
            serde_json::to_string(&TransferDecision::Approved).unwrap(),
            "\"approved\""
        );
        assert_eq!(
            serde_json::to_string(&TransferDecision::Rejected).unwrap(),
            "\"rejected\""
        );
        assert!(TransferDecision::from_str("accepted") == Some(TransferDecision::Accepted));
        assert!(TransferDecision::from_str("rejected") == Some(TransferDecision::Rejected));
        assert!(TransferDecision::from_str("approved") == Some(TransferDecision::Approved));
        assert!(TransferDecision::from_str("unknown") == None);
    }

    #[test]
    fn intent_direction_wire_shape() {
        assert_eq!(IntentDirection::Pull.as_str(), "pull");
        assert_eq!(IntentDirection::Push.as_str(), "push");
        let s: String = IntentDirection::Pull.into();
        assert_eq!(s, "pull");
    }
}
