//! Server Metrics
//!
//! 服务器性能指标数据结构，由主进程内 MetricsCollector 采集
//! Supervisor 定时采样并通过 Tauri command 提供给前端

use serde::{Deserialize, Serialize};

/// 服务器性能指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMetrics {
    /// 运行时长（秒）
    pub uptime_secs: u64,
    /// 当前连接数
    pub connections: usize,
    /// HTTP 总请求数
    pub total_http_requests: u64,
    /// 每秒请求数
    pub http_requests_per_sec: f64,
    /// WS 发送消息总数
    pub ws_messages_sent: u64,
    /// WS 接收消息总数
    pub ws_messages_received: u64,
    /// WS 发送速率 (msg/s)
    pub ws_sent_rate: f64,
    /// WS 接收速率 (msg/s)
    pub ws_recv_rate: f64,
    /// CPU 占用百分比
    pub cpu_usage_percent: f64,
    /// 内存占用（字节）
    pub memory_usage_bytes: u64,
}

impl Default for ServerMetrics {
    fn default() -> Self {
        Self {
            uptime_secs: 0,
            connections: 0,
            total_http_requests: 0,
            http_requests_per_sec: 0.0,
            ws_messages_sent: 0,
            ws_messages_received: 0,
            ws_sent_rate: 0.0,
            ws_recv_rate: 0.0,
            cpu_usage_percent: 0.0,
            memory_usage_bytes: 0,
        }
    }
}

/// 全局指标采集器 — 主进程内使用
///
/// Actix Web 中间件和 WS actor 通过此单例递增计数器，
/// Supervisor 定时读取并计算速率
pub struct MetricsCollector {
    inner: std::sync::Arc<MetricsInner>,
}

struct MetricsInner {
    /// 启动时间（Mutex 保护以支持 reset）
    start_time: std::sync::Mutex<std::time::Instant>,
    /// HTTP 请求计数
    http_requests: std::sync::atomic::AtomicU64,
    /// WS 发送消息计数
    ws_sent: std::sync::atomic::AtomicU64,
    /// WS 接收消息计数
    ws_received: std::sync::atomic::AtomicU64,
    /// 上次采样时的 HTTP 请求数
    last_http_requests: std::sync::atomic::AtomicU64,
    /// 上次采样时的 WS 发送数
    last_ws_sent: std::sync::atomic::AtomicU64,
    /// 上次采样时的 WS 接收数
    last_ws_received: std::sync::atomic::AtomicU64,
    /// 上次采样时间
    last_sample_time: std::sync::Mutex<std::time::Instant>,
}

impl MetricsCollector {
    /// 获取全局单例
    pub fn global() -> &'static Self {
        static INSTANCE: std::sync::LazyLock<MetricsCollector> =
            std::sync::LazyLock::new(|| MetricsCollector {
                inner: std::sync::Arc::new(MetricsInner {
                    start_time: std::sync::Mutex::new(std::time::Instant::now()),
                    http_requests: std::sync::atomic::AtomicU64::new(0),
                    ws_sent: std::sync::atomic::AtomicU64::new(0),
                    ws_received: std::sync::atomic::AtomicU64::new(0),
                    last_http_requests: std::sync::atomic::AtomicU64::new(0),
                    last_ws_sent: std::sync::atomic::AtomicU64::new(0),
                    last_ws_received: std::sync::atomic::AtomicU64::new(0),
                    last_sample_time: std::sync::Mutex::new(std::time::Instant::now()),
                }),
            });
        &INSTANCE
    }

    /// 递增 HTTP 请求计数
    pub fn inc_http_request(&self) {
        self.inner.http_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 递增 WS 发送计数
    pub fn inc_ws_sent(&self) {
        self.inner.ws_sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 递增 WS 接收计数
    pub fn inc_ws_received(&self) {
        self.inner.ws_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 重置所有计数器和计时器（服务器重启时调用）
    pub fn reset(&self) {
        if let Ok(mut start_time) = self.inner.start_time.lock() {
            *start_time = std::time::Instant::now();
        }
        self.inner.http_requests.store(0, std::sync::atomic::Ordering::Relaxed);
        self.inner.ws_sent.store(0, std::sync::atomic::Ordering::Relaxed);
        self.inner.ws_received.store(0, std::sync::atomic::Ordering::Relaxed);
        self.inner.last_http_requests.store(0, std::sync::atomic::Ordering::Relaxed);
        self.inner.last_ws_sent.store(0, std::sync::atomic::Ordering::Relaxed);
        self.inner.last_ws_received.store(0, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut last_time) = self.inner.last_sample_time.lock() {
            *last_time = std::time::Instant::now();
        }
    }

    /// 采样并计算当前指标快照
    ///
    /// 使用滑动窗口计算速率：当前值 - 上次采样值 / 时间间隔
    pub fn sample(&self, connections: usize, cpu_percent: f64, memory_bytes: u64) -> ServerMetrics {
        let now = std::time::Instant::now();
        let uptime = self.inner.start_time.lock().map(|t| t.elapsed().as_secs()).unwrap_or(0);

        let http_total = self.inner.http_requests.load(std::sync::atomic::Ordering::Relaxed);
        let ws_sent_total = self.inner.ws_sent.load(std::sync::atomic::Ordering::Relaxed);
        let ws_recv_total = self.inner.ws_received.load(std::sync::atomic::Ordering::Relaxed);

        let mut rate_http = 0.0f64;
        let mut rate_ws_sent = 0.0f64;
        let mut rate_ws_recv = 0.0f64;

        if let Ok(mut last_time) = self.inner.last_sample_time.lock() {
            let elapsed = now.duration_since(*last_time).as_secs_f64();
            if elapsed > 0.0 {
                let last_http = self.inner.last_http_requests.load(std::sync::atomic::Ordering::Relaxed);
                let last_sent = self.inner.last_ws_sent.load(std::sync::atomic::Ordering::Relaxed);
                let last_recv = self.inner.last_ws_received.load(std::sync::atomic::Ordering::Relaxed);

                rate_http = (http_total - last_http) as f64 / elapsed;
                rate_ws_sent = (ws_sent_total - last_sent) as f64 / elapsed;
                rate_ws_recv = (ws_recv_total - last_recv) as f64 / elapsed;
            }

            // 更新采样基准
            self.inner.last_http_requests.store(http_total, std::sync::atomic::Ordering::Relaxed);
            self.inner.last_ws_sent.store(ws_sent_total, std::sync::atomic::Ordering::Relaxed);
            self.inner.last_ws_received.store(ws_recv_total, std::sync::atomic::Ordering::Relaxed);
            *last_time = now;
        }

        ServerMetrics {
            uptime_secs: uptime,
            connections,
            total_http_requests: http_total,
            http_requests_per_sec: rate_http,
            ws_messages_sent: ws_sent_total,
            ws_messages_received: ws_recv_total,
            ws_sent_rate: rate_ws_sent,
            ws_recv_rate: rate_ws_recv,
            cpu_usage_percent: cpu_percent,
            memory_usage_bytes: memory_bytes,
        }
    }
}
