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
        // waiting-approval（v2：ask 批等待同意）
        (TaskState::Queued, TaskState::Transferring) => Ok(()),
        (TaskState::Queued, TaskState::Cancelled) => Ok(()),
        (TaskState::Queued, TaskState::Resumable) => Ok(()),
        (TaskState::Queued, TaskState::WaitingApproval) => Ok(()),

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
    /// 失败/拒绝原因
    pub reason: Option<String>,
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
    pub reason: Option<String>,
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
        };
        assert!(task.transition(TaskState::Transferring).is_ok());
        assert_eq!(task.state, TaskState::Transferring);
        // 非法迁移保持原状态
        assert!(task.transition(TaskState::Queued).is_err());
        assert_eq!(task.state, TaskState::Transferring);
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
}
