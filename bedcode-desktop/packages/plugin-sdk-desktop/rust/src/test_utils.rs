//! 测试共享工具（仅 cfg(test) 编译）
//!
//! - [`block_on`]：极简自旋执行器，驱动测试中的 async 代码
//!   （mock 实现均为纯内存操作、无 IO 等待点，首次 poll 即 Ready）
//! - 宿主能力 mock：`PluginStorageAccess` / `SessionQuery` / `EventEmitter`
//!   的最小实现，供 `context.rs` / `traits.rs` 测试构造 `RustPluginContext`

use crate::context::{EventEmitter, PluginStorageAccess, SessionQuery};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

/// 极简阻塞执行器：反复 poll 直至 Ready
///
/// 上限 10000 次 poll，超出视为死循环（测试中的 future 无 await 点，
/// 正常一次 poll 即完成；上限仅防 mock 行为改动后测试无限挂起）
pub(crate) fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = Box::pin(fut);
    let waker: Waker = Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    for _ in 0..10_000 {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
    panic!("future did not complete within 10000 polls");
}

// ==================== 宿主能力 mock ====================

/// 键值存储 mock：记录 `(plugin_id, key)` 调用，get 固定返回一个含 key 的对象
pub(crate) struct MockStorage {
    pub calls: Arc<Mutex<Vec<(String, String)>>>,
}

impl Default for MockStorage {
    fn default() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl PluginStorageAccess for MockStorage {
    fn get(
        &self,
        plugin_id: &str,
        key: &str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<serde_json::Value>>> + Send>> {
        self.calls.lock().unwrap().push((plugin_id.to_string(), key.to_string()));
        let key = key.to_string();
        Box::pin(async move { Ok(Some(serde_json::json!({ "stored": key }))) })
    }

    fn set(
        &self,
        plugin_id: &str,
        key: &str,
        _value: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        self.calls.lock().unwrap().push((plugin_id.to_string(), key.to_string()));
        Box::pin(async { Ok(()) })
    }

    fn delete(
        &self,
        plugin_id: &str,
        key: &str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        self.calls.lock().unwrap().push((plugin_id.to_string(), key.to_string()));
        Box::pin(async { Ok(()) })
    }
}

/// 会话查询 mock：固定返回两个会话；`s1` 可查，其余返回 None
pub(crate) struct MockSessionQuery;

impl SessionQuery for MockSessionQuery {
    fn list_sessions(
        &self,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<serde_json::Value>>> + Send>> {
        Box::pin(async { Ok(vec![serde_json::json!({ "id": "s1" }), serde_json::json!({ "id": "s2" })]) })
    }

    fn get_session(
        &self,
        session_id: &str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<serde_json::Value>>> + Send>> {
        let session_id = session_id.to_string();
        Box::pin(async move {
            if session_id == "s1" {
                Ok(Some(serde_json::json!({ "id": "s1" })))
            } else {
                Ok(None)
            }
        })
    }
}

/// 事件发射 mock：记录 `(event, payload)` 调用
pub(crate) struct MockEventEmitter {
    pub emitted: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    pub emit_count: Arc<AtomicUsize>,
}

impl Default for MockEventEmitter {
    fn default() -> Self {
        Self {
            emitted: Arc::new(Mutex::new(Vec::new())),
            emit_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl EventEmitter for MockEventEmitter {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        self.emit_count.fetch_add(1, Ordering::SeqCst);
        self.emitted.lock().unwrap().push((event.to_string(), payload));
    }
}
