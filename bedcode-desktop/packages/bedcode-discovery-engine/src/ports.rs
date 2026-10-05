//! 宿主端口：能力域与宿主之间**唯一**的边界
//!
//! ## 设计依据（`bedcode-server-base::ports` 先例）
//!
//! 消费方（本 crate）声明端口，宿主实现。本 crate **不依赖 tauri、不依赖宿主
//! bin crate**，因此移动端将来要复用时只需换一个端口实现。
//!
//! ## 四个方法的取舍
//!
//! | 方法 | 为什么需要它 |
//! | --- | --- |
//! | [`DiscoveryPorts::check_permission`] | 权限门（`network:mdns`）是**宿主安全闸门**（AGENTS §5.1.3 四类薄壳之二），不该可插拔 ⇒ 留宿主，本 crate 只问结果 |
//! | [`DiscoveryPorts::local_node_id`] | 自播回显过滤需要「本机节点 ID」；无宿主句柄时返回 `None` ⇒ **不拦截**（与原实现同口径：无法比对即不拦） |
//! | [`DiscoveryPorts::publish`] | 向已拼好的完整 topic 投递发现事件；总线订阅方隔离在宿主侧 |
//! | [`DiscoveryPorts::spawn`] | 浏览事件循环 / re-announce 续期都是后台任务，**必须挂在宿主运行时上**（本 crate 不假设 tokio runtime 上下文，见下） |
//!
//! ## 为什么 `spawn` 返回可取消句柄而不是丢弃
//!
//! `stop_advertise` 必须能**中止 re-announce 续期**——不中止的话，已注销的服务会被
//! 续期循环重新注册回去，属真实功能回归（原实现是 `entry.reannounce_task.abort()`）。
//! 故 [`DiscoveryTask`] 只暴露 `cancel()`，**不暴露具体句柄类型**（tauri 的
//! `JoinHandle` / tokio 的 `JoinHandle` 不外泄到本 crate）。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// 能力域可用的宿主类型别名（boxed future；`async fn` 在 trait 里会破坏 dyn 兼容）
pub type BoxedTask = Pin<Box<dyn Future<Output = ()> + Send>>;

/// 后台任务句柄（宿主实现提供取消能力）
///
/// 本 trait 只承诺「可取消」，不暴露宿主运行时类型。新增能力域若需要等待任务
/// 结束（join），另加方法并同步评估两端的实现——当前 mdns 域不需要。
pub trait DiscoveryTask: Send + Sync + 'static {
    /// 取消任务（幂等：重复调用不 panic）
    fn cancel(&self);
}

/// 宿主能力端口
///
/// 实现方是各端宿主的适配器（桌面端见 `wasm_core::host_api::mdns::HostDiscoveryPorts`）。
/// 实现应当尽量做成**零大小可克隆**类型——浏览器事件循环要持有本端口直到退出，
/// 而该端口必须是 `'static`（宿主上下文本身不满足）。
pub trait DiscoveryPorts: Send + Sync + 'static {
    /// 权限判定：`network:mdns`
    ///
    /// 返回 `false` 时**宿主侧**必须已按 AGENTS §8 落 `warn`（结构化字段
    /// `plugin_id` / `permission` / `api`）；本 crate 不重复落日志。
    fn check_permission(&self, plugin_id: &str, api: &str) -> bool;

    /// 本机对等节点 ID（自播回显过滤用）
    ///
    /// `None` = 无宿主句柄（无头 / 测试）或节点未启动 ⇒ **不拦截自播**，
    /// 与原实现 `is_self_broadcast` 的「无法比对即不拦」口径一致。
    fn local_node_id(&self) -> Option<String>;

    /// 向**已拼好的完整 topic** 投递事件
    ///
    /// topic 由本 crate 用 SDK 的 `owned_topic(owner, event)` 拼好（含 `<owner>::`
    /// 前缀与命名空间仲裁所需的 owner）——那是纯字符串逻辑，留在能力域侧；
    /// 宿主只负责「往这个 topic 上发」并做总线的订阅方隔离。
    fn publish(&self, topic: &str, payload: serde_json::Value);

    /// 在**宿主运行时**上派生后台任务，返回可取消句柄
    ///
    /// 为什么必须经宿主：宿主调用栈不保证处于 tokio runtime 上下文
    /// （wasmtime async store 的 fiber 内直接 `tokio::spawn` 会 panic）。
    fn spawn(&self, task: BoxedTask) -> Arc<dyn DiscoveryTask>;
}
// ==================== 端口装配（进程级单例） ====================

/// 进程级端口持有者
///
/// **为什么是全局**：本能力域的另一样状态（共享守护）本就是进程级单例
/// （`ServiceDaemon` 同绑 5353 播端口，全局唯一）；端口与它同生命周期。
/// 另一面：`impl Host for WasmPluginState` 住在本 crate（它用本 crate 的
/// `bindgen!` 生成的 `Host` trait），而**端口的具体实现属于宿主**，crate 之间
/// 无从互相指名——故用「宿主开机装一次、之后经本函数取」的单向装配，方向单一。
static PORTS: std::sync::OnceLock<Arc<dyn DiscoveryPorts>> = std::sync::OnceLock::new();

/// 装配宿主端口实现（宿主开机期调用一次，幂等：重复装配被忽略而非替换）
///
/// 重复装配必须被忽略而不是替换：运行期换掉端口实现会让已注册的浏览事件
/// 循环继续持旧端口，属难以归因的不一致。首次装配者胜出。
pub fn install_ports(ports: Arc<dyn DiscoveryPorts>) {
    let _ = PORTS.set(ports);
}

/// 取已装配的宿主端口
///
/// # Panics
/// 尚未装配时 panic 并点名——**fail-visible**：让一个未接宿主的能力域以
/// 「跑起来但什么都做不了」的面貌存在，比启动即炸难诊断得多。
pub fn ports() -> Arc<dyn DiscoveryPorts> {
    PORTS.get().cloned().expect(
        "discovery engine ports are not installed — the host must call `install_ports` during boot",
    )
}
