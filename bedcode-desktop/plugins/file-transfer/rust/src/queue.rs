//! 并发队列调度
//!
//! FIFO 队列 + 并发槽位（默认 3，上限 8）。
//! 槽位空出/入队/恢复时 schedule_next 调度下一个任务。

use std::collections::VecDeque;

/// 默认并发数
pub const DEFAULT_CONCURRENCY: usize = 3;
/// 并发上限
pub const MAX_CONCURRENCY: usize = 8;
/// 并发下限
pub const MIN_CONCURRENCY: usize = 1;

/// 调度结果：需要启动传输的任务 ID 列表
pub type ScheduleActions = Vec<String>;

/// 传输队列管理器
///
/// 维护 FIFO 等待队列和活跃任务集合。
/// schedule() 返回需要启动的任务 ID 列表（调用方在释放锁后执行宿主传输启动）。
pub struct Queue {
    /// FIFO 等待队列（任务 ID）
    pending: VecDeque<String>,
    /// 当前活跃传输任务 ID 集合
    active: Vec<String>,
    /// 最大并发数
    concurrency: usize,
}

impl Queue {
    pub fn new(concurrency: usize) -> Self {
        Self {
            pending: VecDeque::new(),
            active: Vec::new(),
            concurrency: concurrency.clamp(MIN_CONCURRENCY, MAX_CONCURRENCY),
        }
    }

    /// 入队
    pub fn enqueue(&mut self, task_id: &str) {
        if !self.pending.contains(&task_id.to_string()) {
            self.pending.push_back(task_id.to_string());
        }
    }

    /// 出队（移除等待或活跃中的任务）
    pub fn remove(&mut self, task_id: &str) {
        self.pending.retain(|id| id != task_id);
        self.active.retain(|id| id != task_id);
    }

    /// 调度：填充空余槽位，返回需要启动传输的任务 ID 列表
    ///
    /// 调用方在释放状态锁后，逐个启动返回的任务传输
    pub fn schedule(&mut self) -> ScheduleActions {
        let mut actions = Vec::new();
        while self.active.len() < self.concurrency {
            if let Some(task_id) = self.pending.pop_front() {
                self.active.push(task_id.clone());
                actions.push(task_id);
            } else {
                break;
            }
        }
        actions
    }

    /// 标记任务完成/取消（释放槽位）
    pub fn release(&mut self, task_id: &str) {
        self.active.retain(|id| id != task_id);
    }

    /// 设置并发数
    pub fn set_concurrency(&mut self, n: usize) {
        self.concurrency = n.clamp(MIN_CONCURRENCY, MAX_CONCURRENCY);
    }

    pub fn concurrency(&self) -> usize {
        self.concurrency
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn is_active(&self, task_id: &str) -> bool {
        self.active.contains(&task_id.to_string())
    }

    pub fn pending_ids(&self) -> &[String] {
        self.pending.as_slices().0
    }
}
