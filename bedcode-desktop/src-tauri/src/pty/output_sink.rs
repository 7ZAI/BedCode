//! PTY Output Sink
//!
//! PTY 输出读取线程（`pty_reader`）只负责「read → 有序队列 → 单消费者顺序投递」，
//! **投递到哪里由调用方注入**：
//!
//! - 业务终端会话 → `session::SessionOutputSink`（全局输出总线；**业务实现在业务层**
//!   `session/session_output.rs`，本引擎模块只保留 [`PtyOutputSink`] 抽象——
//!   pty 引擎不认识会话输出总线，这是「pty 只提供基础服务接口」的边界）
//! - 插件私有 PTY（host-pty）→ 插件自备的环形缓冲，绝不灌进业务会话链路
//!
//! **源零等待契约**：实现方只允许做「写入自己的缓冲 + 通告水位」这类 O(1) 均摊
//! 操作，不得等待任何下游消费者——慢消费者只能节流自己，背压不得回传到读取线程
//! （历史教训见 `.scratch/2026-09-17-pty-pull-subscribers/spec.md`）。

use async_trait::async_trait;

/// PTY 输出投递目标
#[async_trait]
pub trait PtyOutputSink: Send + Sync + 'static {
    /// 按读线程产出顺序投递一段原始字节（未解码，可能是非 UTF-8）
    async fn on_bytes(&self, bytes: Vec<u8>, timestamp_ms: i64);
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;

    /// 自备输出汇（测试替身）：原样累积投递到的字节，用于验证读线程可投递到
    /// 业务总线之外的目标；同时记录投递顺序
    pub(crate) struct CollectingSink {
        chunks: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl CollectingSink {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(Self {
                chunks: Arc::new(Mutex::new(Vec::new())),
            })
        }

        /// 已投递字节（按到达顺序拼接）
        pub(crate) fn collected(&self) -> Vec<u8> {
            let chunks = self.chunks.lock().unwrap();
            chunks.concat()
        }

        /// 单次投递的字节块序列（顺序断言用，验证读线程未乱序）
        pub(crate) fn chunks(&self) -> Vec<Vec<u8>> {
            self.chunks.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl PtyOutputSink for CollectingSink {
        async fn on_bytes(&self, bytes: Vec<u8>, _timestamp_ms: i64) {
            self.chunks.lock().unwrap().push(bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::CollectingSink;
    use super::*;
    use crate::session::GlobalOutputManager;

    fn unique_id(prefix: &str) -> String {
        format!(
            "{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    /// C-002 正例：自备 sink 只收自己的字节，业务总线不留痕
    #[tokio::test]
    async fn collecting_sink_receives_bytes_without_touching_session_bus() {
        let sid = unique_id("sink-private");
        let global = GlobalOutputManager::global();
        let session_manager = global.register_session(&sid).await;

        let sink = CollectingSink::new();
        sink.on_bytes(b"alpha".to_vec(), 1).await;
        sink.on_bytes(b"beta".to_vec(), 2).await;

        assert_eq!(sink.collected(), b"alphabeta".to_vec());
        assert_eq!(sink.chunks(), vec![b"alpha".to_vec(), b"beta".to_vec()]);

        let ring_arc = session_manager.ring();
        let ring = ring_arc.read().await;
        let (min, max) = ring.watermarks();
        assert_eq!(max - min, 0, "自备 sink 不得把字节写进业务会话环");
        drop(ring);

        global.unregister_session(&sid).await;
    }
}
