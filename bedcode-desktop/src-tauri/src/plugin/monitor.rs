//! 监控模块（core-monitor）
//!
//! 内核运行时指标埋点与观测：
//! - 每插件 Store 线性内存当前值/峰值（记账挂在 ResourceLimiter 增长回调）
//! - 导出调用聚合：次数 / 燃料消耗总量 / 耗时分布（直方图桶）
//! - 插件生命周期事件计数（实例化/激活成功/激活失败/停用/trap/降级）+ 最近发生时间
//! - 快照导出：一次调用拿全量 JSON（前端诊断页、CLI、未来 sink 扩展的统一数据源）
//!
//! 红线：热路径埋点为纯原子操作，零日志（日志级别语义见 AGENTS.md §8）；
//! 指标标签只用 `plugin_id` 等结构化维度，禁止拼消息字符串。
//!
//! 可扩展性：所有写入收敛于 [`MetricsRegistry::plugin`] 返回的 [`PluginMetrics`]
//! 句柄，未来新增指标只需在 [`PluginMetrics`] 加字段；sink 扩展点（文件/远程
//! 上报）以消费 [`MetricsRegistry::snapshot`] 的方式接入，不改埋点侧。

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// 授权决策类别（core-monitor 埋点维度；core-security 的 AuthDecision 映射到此）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthzDecisionKind {
    Allow,
    Deny,
    RequireApproval,
}

// ==================== 生命周期事件 ====================

/// 插件生命周期事件（计数维度）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    /// WASM 实例创建（Store 建立完成）
    Instantiate,
    /// activate() 成功
    ActivateOk,
    /// activate() 失败（含 guest 自报失败与 trap）
    ActivateFail,
    /// deactivate() 成功
    Deactivate,
    /// 导出调用 trap（panic/栈溢出/燃料耗尽/内存越界）
    Trap,
    /// 插件被标记为 Degraded（带病运行）
    Degraded,
}

impl LifecycleEvent {
    /// 数组索引（事件计数存储为定长数组，避免 map 开销）
    fn idx(self) -> usize {
        match self {
            Self::Instantiate => 0,
            Self::ActivateOk => 1,
            Self::ActivateFail => 2,
            Self::Deactivate => 3,
            Self::Trap => 4,
            Self::Degraded => 5,
        }
    }

    /// 快照导出的稳定键名（JSON 字段，面向诊断消费方）
    fn key(self) -> &'static str {
        match self {
            Self::Instantiate => "instantiate",
            Self::ActivateOk => "activate_ok",
            Self::ActivateFail => "activate_fail",
            Self::Deactivate => "deactivate",
            Self::Trap => "trap",
            Self::Degraded => "degraded",
        }
    }

    const ALL: [Self; 6] = [
        Self::Instantiate,
        Self::ActivateOk,
        Self::ActivateFail,
        Self::Deactivate,
        Self::Trap,
        Self::Degraded,
    ];
}

// ==================== 耗时直方图 ====================

/// 直方图桶上界（微秒）：1ms / 10ms / 100ms / 1s / 10s / +Inf
///
/// 覆盖插件调用的典型分布：存储读写 ~ms 内，HTTP/进程调用可至秒级；
/// 桶界为 10 的幂，诊断时肉眼可读
const DURATION_BUCKET_BOUNDS_US: [u64; 5] = [1_000, 10_000, 100_000, 1_000_000, 10_000_000];
const DURATION_BUCKET_COUNT: usize = DURATION_BUCKET_BOUNDS_US.len() + 1;

// ==================== 插件维度指标 ====================

/// 单插件指标集（全原子字段，埋点热路径零分配零锁）
pub struct PluginMetrics {
    /// 线性内存当前值（字节，ResourceLimiter 增长回调记账）
    memory_current_bytes: AtomicU64,
    /// 线性内存峰值（字节）
    memory_peak_bytes: AtomicU64,
    /// 导出调用总次数
    calls_total: AtomicU64,
    /// 燃料消耗总量（指令数；口径：两次燃料注入之间的消耗，首次含实例化与 ABI 协商）
    fuel_consumed_total: AtomicU64,
    /// 导出调用耗时总量（微秒）
    call_duration_us_total: AtomicU64,
    /// 导出调用单次耗时最大值（微秒）
    call_duration_us_max: AtomicU64,
    /// 耗时直方图桶计数（桶界见 [`DURATION_BUCKET_BOUNDS_US`]）
    call_duration_buckets: [AtomicU64; DURATION_BUCKET_COUNT],
    /// 生命周期事件计数（按 [`LifecycleEvent::idx`] 索引）
    lifecycle_counts: [AtomicU64; 6],
    /// 生命周期事件最近发生时间（unix 秒）
    lifecycle_last_unix: [AtomicU64; 6],
    /// 授权决策计数（按 [`AuthzDecisionKind`] 索引：allow/deny/require_approval）
    authz_decisions: [AtomicU64; 3],
    /// 消息总线丢弃计数（v11）：订阅者队列满时丢弃（背压保护）
    bus_dropped_total: AtomicU64,
    /// 消息总线格式不匹配拒绝计数（v11）：订阅方格式偏好与消息格式不符
    bus_format_rejected_total: AtomicU64,
}

impl Default for PluginMetrics {
    fn default() -> Self {
        Self {
            memory_current_bytes: AtomicU64::new(0),
            memory_peak_bytes: AtomicU64::new(0),
            calls_total: AtomicU64::new(0),
            fuel_consumed_total: AtomicU64::new(0),
            call_duration_us_total: AtomicU64::new(0),
            call_duration_us_max: AtomicU64::new(0),
            call_duration_buckets: std::array::from_fn(|_| AtomicU64::new(0)),
            lifecycle_counts: std::array::from_fn(|_| AtomicU64::new(0)),
            lifecycle_last_unix: std::array::from_fn(|_| AtomicU64::new(0)),
            authz_decisions: std::array::from_fn(|_| AtomicU64::new(0)),
            bus_dropped_total: AtomicU64::new(0),
            bus_format_rejected_total: AtomicU64::new(0),
        }
    }
}

impl PluginMetrics {
    /// 内存增长记账（limiter 批准路径调用）：current = desired，峰值取大
    pub(crate) fn record_memory_growth(&self, desired: usize) {
        let desired = desired as u64;
        self.memory_current_bytes.store(desired, Ordering::Relaxed);
        self.memory_peak_bytes.fetch_max(desired, Ordering::Relaxed);
    }

    /// 燃料消耗记账（exports() 续费前调用：consumed = 预算 - 剩余）
    pub(crate) fn record_fuel_consumed(&self, consumed: u64) {
        self.fuel_consumed_total.fetch_add(consumed, Ordering::Relaxed);
    }

    /// 一次导出调用计时开始（RAII：drop 时记录耗时并计数）
    ///
    /// 计时器持有 `Arc<Self>`（owning），不借用调用方上下文——
    /// 插件方法拿到计时器后还需 `&mut self` 驱动导出调用
    pub(crate) fn start_call(self: &Arc<Self>) -> CallTimer {
        CallTimer {
            metrics: self.clone(),
            start: Instant::now(),
        }
    }

    fn record_call_duration(&self, duration_us: u64) {
        self.calls_total.fetch_add(1, Ordering::Relaxed);
        self.call_duration_us_total.fetch_add(duration_us, Ordering::Relaxed);
        self.call_duration_us_max.fetch_max(duration_us, Ordering::Relaxed);
        let bucket = DURATION_BUCKET_BOUNDS_US
            .iter()
            .position(|&bound| duration_us < bound)
            .unwrap_or(DURATION_BUCKET_COUNT - 1);
        self.call_duration_buckets[bucket].fetch_add(1, Ordering::Relaxed);
    }

    /// 生命周期事件记账（计数 + 最近发生时间）
    pub fn record_lifecycle(&self, event: LifecycleEvent) {
        self.lifecycle_counts[event.idx()].fetch_add(1, Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.lifecycle_last_unix[event.idx()].store(now, Ordering::Relaxed);
    }

    /// 授权决策记账（core-security 决策管线的唯一埋点点）
    pub fn record_authz_decision(&self, kind: AuthzDecisionKind) {
        let idx = match kind {
            AuthzDecisionKind::Allow => 0,
            AuthzDecisionKind::Deny => 1,
            AuthzDecisionKind::RequireApproval => 2,
        };
        self.authz_decisions[idx].fetch_add(1, Ordering::Relaxed);
    }

    /// 消息总线队列满丢弃记账（v11，背压保护）
    pub fn record_bus_dropped(&self) {
        self.bus_dropped_total.fetch_add(1, Ordering::Relaxed);
    }

    /// 消息总线格式不匹配拒绝记账（v11）
    pub fn record_bus_format_rejected(&self) {
        self.bus_format_rejected_total.fetch_add(1, Ordering::Relaxed);
    }
}

/// 导出调用计时器（RAII）：drop 时把耗时记入指标
pub(crate) struct CallTimer {
    metrics: Arc<PluginMetrics>,
    start: Instant,
}

impl Drop for CallTimer {
    fn drop(&mut self) {
        self.metrics
            .record_call_duration(self.start.elapsed().as_micros() as u64);
    }
}

// ==================== 快照（导出形状） ====================

/// 单插件指标快照（serde 字段名即对外契约，保持稳定）
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PluginMetricsSnapshot {
    pub memory_current_bytes: u64,
    pub memory_peak_bytes: u64,
    pub calls_total: u64,
    pub fuel_consumed_total: u64,
    pub call_duration_us_total: u64,
    pub call_duration_us_max: u64,
    /// 耗时直方图：长度恒为 6，桶界见 DURATION_BUCKET_BOUNDS_US
    pub call_duration_buckets: Vec<u64>,
    /// 生命周期事件计数（key 见 LifecycleEvent::key）
    pub lifecycle: HashMap<String, u64>,
    /// 生命周期事件最近发生时间 unix 秒（仅发生过的事件出现）
    pub lifecycle_last_unix: HashMap<String, u64>,
    /// 授权决策计数（allow / deny / require_approval）
    pub authz: HashMap<String, u64>,
    /// 消息总线指标（v11）：dropped = 队列满丢弃；format_rejected = 格式不匹配拒绝
    pub bus: HashMap<String, u64>,
}

impl PluginMetrics {
    fn snapshot(&self) -> PluginMetricsSnapshot {
        let mut lifecycle = HashMap::new();
        let mut lifecycle_last_unix = HashMap::new();
        for event in LifecycleEvent::ALL {
            let count = self.lifecycle_counts[event.idx()].load(Ordering::Relaxed);
            lifecycle.insert(event.key().to_string(), count);
            let last = self.lifecycle_last_unix[event.idx()].load(Ordering::Relaxed);
            if last > 0 {
                lifecycle_last_unix.insert(event.key().to_string(), last);
            }
        }
        PluginMetricsSnapshot {
            memory_current_bytes: self.memory_current_bytes.load(Ordering::Relaxed),
            memory_peak_bytes: self.memory_peak_bytes.load(Ordering::Relaxed),
            calls_total: self.calls_total.load(Ordering::Relaxed),
            fuel_consumed_total: self.fuel_consumed_total.load(Ordering::Relaxed),
            call_duration_us_total: self.call_duration_us_total.load(Ordering::Relaxed),
            call_duration_us_max: self.call_duration_us_max.load(Ordering::Relaxed),
            call_duration_buckets: self
                .call_duration_buckets
                .iter()
                .map(|b| b.load(Ordering::Relaxed))
                .collect(),
            lifecycle,
            lifecycle_last_unix,
            authz: HashMap::from([
                ("allow".to_string(), self.authz_decisions[0].load(Ordering::Relaxed)),
                ("deny".to_string(), self.authz_decisions[1].load(Ordering::Relaxed)),
                (
                    "require_approval".to_string(),
                    self.authz_decisions[2].load(Ordering::Relaxed),
                ),
            ]),
            bus: HashMap::from([
                ("dropped".to_string(), self.bus_dropped_total.load(Ordering::Relaxed)),
                (
                    "format_rejected".to_string(),
                    self.bus_format_rejected_total.load(Ordering::Relaxed),
                ),
            ]),
        }
    }
}

// ==================== 指标注册表 ====================

/// 指标注册表（内核级共享，线程安全）
///
/// 经 [`crate::plugin::manager::wasm_runtime::WasmRuntime::monitor`] 获取；
/// 所有埋点方（limiter / 导出调用 / 生命周期）先取插件句柄再记账
pub struct MetricsRegistry {
    /// plugin_id → 指标集
    plugins: RwLock<HashMap<String, Arc<PluginMetrics>>>,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
        }
    }

    /// 获取（或创建）插件指标句柄
    pub fn plugin(&self, plugin_id: &str) -> Arc<PluginMetrics> {
        // 快路径：读锁命中
        if let Some(m) = self.plugins.read().expect("metrics lock poisoned").get(plugin_id) {
            return m.clone();
        }
        let mut map = self.plugins.write().expect("metrics lock poisoned");
        map.entry(plugin_id.to_string())
            .or_insert_with(|| Arc::new(PluginMetrics::default()))
            .clone()
    }

    /// 全量快照（JSON）：`{ "plugins": { "<plugin_id>": { ... } } }`
    pub fn snapshot(&self) -> serde_json::Value {
        let map = self.plugins.read().expect("metrics lock poisoned");
        let plugins: serde_json::Map<String, serde_json::Value> = map
            .iter()
            .map(|(id, m)| {
                (
                    id.clone(),
                    serde_json::to_value(m.snapshot()).unwrap_or(serde_json::Value::Null),
                )
            })
            .collect();
        serde_json::json!({ "plugins": plugins })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_get_or_create_is_idempotent_per_plugin() {
        let reg = MetricsRegistry::new();
        let a1 = reg.plugin("com.bedcode.a");
        let a2 = reg.plugin("com.bedcode.a");
        let b = reg.plugin("com.bedcode.b");
        assert!(Arc::ptr_eq(&a1, &a2), "同插件必须返回同一句柄");
        assert!(!Arc::ptr_eq(&a1, &b));
    }

    #[test]
    fn memory_growth_tracks_current_and_peak() {
        let m = PluginMetrics::default();
        m.record_memory_growth(1024);
        m.record_memory_growth(4096);
        m.record_memory_growth(2048); // 回落不动峰值
        let snap = m.snapshot();
        assert_eq!(snap.memory_current_bytes, 2048);
        assert_eq!(snap.memory_peak_bytes, 4096);
    }

    #[test]
    fn call_aggregation_counts_fuel_and_duration() {
        let m = Arc::new(PluginMetrics::default());
        for _ in 0..3 {
            let _t = m.start_call();
            m.record_fuel_consumed(100);
        } // 3 次调用在此落账（drop 计时）
        m.record_fuel_consumed(50);
        let snap = m.snapshot();
        assert_eq!(snap.calls_total, 3);
        assert_eq!(snap.fuel_consumed_total, 350);
        assert_eq!(
            snap.call_duration_buckets.iter().sum::<u64>(),
            3,
            "直方图桶计数必须等于调用数"
        );
    }

    #[test]
    fn duration_histogram_bucket_boundaries() {
        let m = PluginMetrics::default();
        m.record_call_duration(500); // < 1ms → 桶 0
        m.record_call_duration(1_000); // ≥ 1ms → 桶 1
        m.record_call_duration(20_000_000); // ≥ 10s → 最后桶
        let snap = m.snapshot();
        assert_eq!(snap.call_duration_buckets[0], 1);
        assert_eq!(snap.call_duration_buckets[1], 1);
        assert_eq!(snap.call_duration_buckets[5], 1);
        assert_eq!(snap.call_duration_us_max, 20_000_000);
    }

    #[test]
    fn lifecycle_counts_and_last_timestamp() {
        let m = PluginMetrics::default();
        m.record_lifecycle(LifecycleEvent::Instantiate);
        m.record_lifecycle(LifecycleEvent::ActivateOk);
        m.record_lifecycle(LifecycleEvent::Trap);
        m.record_lifecycle(LifecycleEvent::Trap);
        let snap = m.snapshot();
        assert_eq!(snap.lifecycle["instantiate"], 1);
        assert_eq!(snap.lifecycle["activate_ok"], 1);
        assert_eq!(snap.lifecycle["trap"], 2);
        assert_eq!(snap.lifecycle["deactivate"], 0);
        assert!(
            snap.lifecycle_last_unix.contains_key("trap"),
            "发生过的事件须有最近时间戳"
        );
        assert!(
            !snap.lifecycle_last_unix.contains_key("deactivate"),
            "未发生的事件不出现时间戳"
        );
    }

    #[test]
    fn snapshot_shape_is_stable_json_with_plugin_dimension() {
        let reg = MetricsRegistry::new();
        reg.plugin("com.bedcode.a")
            .record_lifecycle(LifecycleEvent::Instantiate);
        let snap = reg.snapshot();
        let plugin = &snap["plugins"]["com.bedcode.a"];
        assert!(plugin.is_object(), "快照必须按 plugin_id 维度组织: {snap}");
        for key in [
            "memory_current_bytes",
            "memory_peak_bytes",
            "calls_total",
            "fuel_consumed_total",
            "call_duration_us_total",
            "call_duration_us_max",
            "call_duration_buckets",
            "lifecycle",
            "lifecycle_last_unix",
            "authz",
            "bus",
        ] {
            assert!(plugin.get(key).is_some(), "快照缺字段 {key}");
        }
        assert_eq!(plugin["lifecycle"]["instantiate"], 1);
    }
}
