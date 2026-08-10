//! 任务状态机与持久化
//!
//! 传输任务的生命周期管理：状态迁移规则（spec §7.1）、持久化策略（spec §7.3）。
//! 状态机迁移函数为纯函数，可独立单测。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 任务状态（spec §7.1）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    /// 排队等待槽位
    Queued,
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

/// 校验状态迁移合法性（spec §7.1）
///
/// 返回 `Ok(())` 表示迁移合法，`Err(reason)` 表示非法迁移。
/// 纯函数，无副作用，可独立单测。
pub fn validate_transition(from: TaskState, to: TaskState) -> Result<(), &'static str> {
    match (from, to) {
        // queued → transferring（槽位空出）/ cancelled / resumable（对端下线）
        (TaskState::Queued, TaskState::Transferring) => Ok(()),
        (TaskState::Queued, TaskState::Cancelled) => Ok(()),
        (TaskState::Queued, TaskState::Resumable) => Ok(()),

        // transferring → paused（用户）/ resumable（断线）/ completed / failed / rejected / cancelled
        (TaskState::Transferring, TaskState::Paused) => Ok(()),
        (TaskState::Transferring, TaskState::Resumable) => Ok(()),
        (TaskState::Transferring, TaskState::Completed) => Ok(()),
        (TaskState::Transferring, TaskState::Failed) => Ok(()),
        (TaskState::Transferring, TaskState::Rejected) => Ok(()),
        (TaskState::Transferring, TaskState::Cancelled) => Ok(()),

        // paused → transferring（恢复）/ cancelled
        (TaskState::Paused, TaskState::Transferring) => Ok(()),
        (TaskState::Paused, TaskState::Cancelled) => Ok(()),

        // resumable → transferring（恢复）/ cancelled
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

    /// 从宿主 storage 加载（保留 paused/resumable，传输中残留降级为 resumable，spec §7.3）
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
        // transferring → all valid targets
        assert!(validate_transition(TaskState::Transferring, TaskState::Paused).is_ok());
        assert!(validate_transition(TaskState::Transferring, TaskState::Resumable).is_ok());
        assert!(validate_transition(TaskState::Transferring, TaskState::Completed).is_ok());
        assert!(validate_transition(TaskState::Transferring, TaskState::Failed).is_ok());
        assert!(validate_transition(TaskState::Transferring, TaskState::Rejected).is_ok());
        assert!(validate_transition(TaskState::Transferring, TaskState::Cancelled).is_ok());
        // paused → transferring / cancelled
        assert!(validate_transition(TaskState::Paused, TaskState::Transferring).is_ok());
        assert!(validate_transition(TaskState::Paused, TaskState::Cancelled).is_ok());
        // resumable → transferring / cancelled
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
        // 非法迁移
        assert!(validate_transition(TaskState::Queued, TaskState::Paused).is_err());
        assert!(validate_transition(TaskState::Paused, TaskState::Completed).is_err());
        assert!(validate_transition(TaskState::Resumable, TaskState::Paused).is_err());
        // 自迁移
        assert!(validate_transition(TaskState::Queued, TaskState::Queued).is_err());
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
}
