//! 本地已写字节游标（v2.1 服务器归零：接收端即断点真源）
//!
//! 断点续传语义：续传点一律取「接收端（落盘方）已写字节」。
//! - 下载方向（push/自主下载）：手机本地游标 = cursor.rs（本模块）
//! - 上传方向（pull/自主上传）：桌面 session 已收偏移（upload.rs 内查询）
//!
//! 本模块提供纯函数游标（可单测）+ 每 transfer 分 key 的共享存储：
//! - 游标含已写字节 `position` 与文件总大小 `total`（total=0 表示未知，
//!   进度退化为偏移量）
//! - 回跳（rollback）：失败重试前游标回退到上次成功点，不允许向前回跳
//! - 越界拒绝：position 超过已声明 total（源文件变更/陈旧游标）直接拒绝，
//!   由调用方按「放弃旧游标重传」处理

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

use super::FileFingerprint;

/// 游标错误（纯函数返回，调用方据此决策）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorError {
    /// 推进后超过总大小（源文件变更/陈旧游标；total=0 未知时不判越界）
    Overrun { position: u64, total: u64 },
    /// 回跳目标超过当前已写字节（不允许向前回跳）
    InvalidRollback { from: u64, to: u64 },
}

/// 本地已写字节游标（纯函数，可独立单测）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cursor {
    /// 已落盘字节数（续传 Range 起点 / IntentAck.offset 真源）
    pub position: u64,
    /// 文件总大小（0 = 未知；来自 intent.size 或 HEAD 指纹）
    pub total: u64,
}

impl Cursor {
    /// 全新游标（position = 0）
    pub fn new(total: u64) -> Self {
        Self { position: 0, total }
    }

    /// 从持久化偏移恢复游标；position 超过非零 total 视为陈旧游标返回 Err
    pub fn restore(position: u64, total: u64) -> Result<Self, CursorError> {
        if total != 0 && position > total {
            return Err(CursorError::Overrun { position, total });
        }
        Ok(Self { position, total })
    }

    /// 推进 n 字节并返回新位置；超过非零 total 拒绝（防陈旧游标无限增长）
    pub fn advance(&mut self, n: u64) -> Result<u64, CursorError> {
        let next = self.position.saturating_add(n);
        if self.total != 0 && next > self.total {
            return Err(CursorError::Overrun {
                position: next,
                total: self.total,
            });
        }
        self.position = next;
        Ok(self.position)
    }

    /// 游标回退（重试前回退到上次成功点）；不允许向前回跳
    pub fn rollback(&mut self, target: u64) -> Result<(), CursorError> {
        if target > self.position {
            return Err(CursorError::InvalidRollback {
                from: self.position,
                to: target,
            });
        }
        self.position = target;
        Ok(())
    }

    /// 续传 Range 起点（position 钳到 total，无剩余字节时后续传将得到空流）
    pub fn effective_offset(&self) -> u64 {
        if self.total != 0 {
            self.position.min(self.total)
        } else {
            self.position
        }
    }

    /// 是否已全部接收
    pub fn is_complete(&self) -> bool {
        self.total != 0 && self.position >= self.total
    }
}

/// 存储内游标条目（含上次指纹，续传有效性比对用）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredCursor {
    /// 已写字节
    pub position: u64,
    /// 上次 HEAD 指纹（源文件变更 → 放弃旧游标，重新下载）
    pub fingerprint: Option<FileFingerprint>,
}

/// 每 transfer 分 key 的游标存储（并发/断点续传按 transfer_id 隔离，无互踩）
#[derive(Default)]
pub struct CursorStore {
    inner: Mutex<HashMap<String, StoredCursor>>,
}

impl CursorStore {
    /// 创建空存储
    pub fn new() -> Self {
        Self::default()
    }

    /// 写入/更新游标（含指纹）
    pub fn upsert(&self, key: &str, cursor: StoredCursor) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.insert(key.to_string(), cursor);
        }
    }

    /// 读取游标（不存在返回 None）
    pub fn get(&self, key: &str) -> Option<StoredCursor> {
        self.inner.lock().ok().and_then(|inner| inner.get(key).cloned())
    }

    /// 移除游标（任务终态清理）
    pub fn remove(&self, key: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.remove(key);
        }
    }

    /// 推进已存游标 n 字节（分 key 原子写回）
    ///
    /// 总大小由游标条目独立管理（download 用 HEAD 指纹维护 total），
    /// 此处仅做无锁语义下的原子增量，越界防护在调用层游标上完成
    pub fn advance(&self, key: &str, n: u64) -> u64 {
        let Ok(mut inner) = self.inner.lock() else {
            return 0;
        };
        let new_pos = inner
            .get_mut(key)
            .map(|e| {
                e.position = e.position.saturating_add(n);
                e.position
            })
            .unwrap_or(0);
        new_pos
    }

    /// 已存游标数量（诊断/测试用）
    pub fn len(&self) -> usize {
        self.inner.lock().map(|inner| inner.len()).unwrap_or(0)
    }

    /// 是否为空（测试用）
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for Cursor {
    fn default() -> Self {
        Self::new(0)
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_increments_positively() {
        let mut c = Cursor::new(100);
        assert_eq!(c.position, 0);
        assert_eq!(c.advance(10).unwrap(), 10);
        assert_eq!(c.advance(25).unwrap(), 35);
        assert_eq!(c.position, 35);
    }

    #[test]
    fn advance_rejects_overrun() {
        let mut c = Cursor::new(100);
        c.advance(90).unwrap();
        // 再推 20 会超 100 → 拒绝，游标保持 90
        let err = c.advance(20).unwrap_err();
        assert_eq!(
            err,
            CursorError::Overrun {
                position: 110,
                total: 100
            }
        );
        assert_eq!(c.position, 90);
    }

    #[test]
    fn advance_with_unknown_total_allows_unbounded() {
        // total=0（未知）：进度退化为偏移量，允许任意推进
        let mut c = Cursor::new(0);
        c.advance(64 * 1024).unwrap();
        assert_eq!(c.position, 64 * 1024);
    }

    #[test]
    fn restore_rejects_stale_cursor() {
        // 陈旧游标（position > total）→ Err，调用方放弃旧游标重传
        assert!(Cursor::restore(150, 100).is_err());
        assert_eq!(
            Cursor::restore(150, 100).unwrap_err(),
            CursorError::Overrun {
                position: 150,
                total: 100
            }
        );
        // position == total 合法（已完整接收）
        assert_eq!(Cursor::restore(100, 100).unwrap().position, 100);
        // total=0 未知时任意 position 合法
        assert_eq!(Cursor::restore(999, 0).unwrap().position, 999);
    }

    #[test]
    fn rollback_moves_back_target() {
        let mut c = Cursor::new(100);
        c.advance(50).unwrap();
        c.rollback(30).unwrap();
        assert_eq!(c.position, 30);
    }

    #[test]
    fn rollback_forward_rejected() {
        let mut c = Cursor::new(100);
        c.advance(30).unwrap();
        assert_eq!(
            c.rollback(40).unwrap_err(),
            CursorError::InvalidRollback { from: 30, to: 40 }
        );
        assert_eq!(c.position, 30);
    }

    #[test]
    fn effective_offset_clamps_to_total() {
        // position 越过 total（不可能经 advance 产生，防御性钳制）
        let c = Cursor {
            position: 120,
            total: 100,
        };
        assert_eq!(c.effective_offset(), 100);
        assert!(c.is_complete());
        let c2 = Cursor::new(100);
        assert_eq!(c2.effective_offset(), 0);
        assert!(!c2.is_complete());
    }

    #[test]
    fn store_isolates_per_key_and_persists_fingerprint() {
        let store = CursorStore::new();
        let fp_a = FileFingerprint { size: 10, mtime: 100 };
        let fp_b = FileFingerprint { size: 20, mtime: 200 };
        store.upsert(
            "t1",
            StoredCursor {
                position: 10,
                fingerprint: Some(fp_a),
            },
        );
        store.upsert(
            "t2",
            StoredCursor {
                position: 99,
                fingerprint: Some(fp_b),
            },
        );
        // 分 key 互不干扰
        assert_eq!(store.get("t1").unwrap().position, 10);
        assert_eq!(store.get("t1").unwrap().fingerprint, Some(fp_a));
        assert_eq!(store.get("t2").unwrap().position, 99);
        assert_eq!(store.len(), 2);

        store.advance("t1", 5);
        assert_eq!(store.get("t1").unwrap().position, 15);
        // t2 不受影响
        assert_eq!(store.get("t2").unwrap().position, 99);

        store.remove("t1");
        assert!(store.get("t1").is_none());
        assert_eq!(store.len(), 1);
    }
}
