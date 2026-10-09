//! 端口 trait 集合——**双端差异的唯一落点**。
//!
//! 纪律：
//! 1. 核内任何模块只经本文件的端口与宿主交互，不得直接引用任一端的 SDK；
//! 2. **双端都有的原语**（`PeerPort` / `PlatformPort::pick_files` 等）不给默认实现——
//!    缺失是编译期错误，不留给运行期；
//! 3. **只在一端成立的能力**（节点电源 / 目录多选 / 打开所在目录 / 旧快照通道）给
//!    默认实现，且默认语义 = 「本端不支持」，不是「假装成功」。
//!    默认 `unsupported` 是 fail-visible：某端误接线到不存在的原语会在调用点显性报错。
//!
//! `PortError` 是轻量字符串包装：双端各自把它转成自己的 `anyhow::Error` / 宿主错误类型，
//! 核不引入任何错误框架依赖。

use serde_json::Value;

/// 端口调用失败（描述已带操作上下文，调用方直接上抛或转接）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortError(pub String);

impl PortError {
    pub fn new(msg: impl Into<String>) -> Self {
        PortError(msg.into())
    }

    /// 本端不支持的端口能力（默认实现用；名字带 `unsupported` 便于日志检索）
    pub fn unsupported(op: &str) -> Self {
        PortError(format!("unsupported on this end: {op}"))
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PortError {}

pub type PortResult<T> = Result<T, PortError>;

// ==================== 观测面 ====================

/// 日志（双端 target 相同，级别语义见 AGENTS §8）
pub trait LogPort {
    fn log_info(&self, msg: &str);
    fn log_error(&self, msg: &str);
}

/// 插件间消息总线（属主私有 topic 由 `identity` 生成）
pub trait BusPort {
    fn bus_publish(&self, topic: &str, payload: &Value) -> PortResult<()>;
    fn bus_subscribe(&self, topic: &str) -> PortResult<()>;
    fn bus_unsubscribe(&self, topic: &str) -> PortResult<()>;
}

/// 向前端广播插件事件
///
/// 无返回值：双端 SDK 的 `emit_event` 都是不可失败调用（返回 `()`），端口照抄该形态——
/// 给不可失败的调用套一层 `PortResult` 会诱使适配器写 `Ok(())` 式的假分支。
pub trait EventPort {
    fn emit_event(&self, name: &str, payload: &Value);
}

// ==================== 存储面 ====================

/// 插件键值存储（双端同名同形原语）
pub trait KvStore {
    fn storage_get(&self, key: &str) -> PortResult<Option<Value>>;
    fn storage_set(&self, key: &str, value: &Value) -> PortResult<()>;
    fn storage_delete(&self, key: &str) -> PortResult<()>;
}

/// 共享根注册表持久化——**差异面①**
///
/// 桌面 = `host-plugin-database` 独立表 `shared_roots`（有序列）；移动 = `host-storage`
/// 单键 JSON 数组（整读整写）。两侧语义必须一致：`load` 返回加入顺序，`save` 覆盖全量。
pub trait RootsStore {
    fn load_roots(&self) -> PortResult<Vec<super::domain::SharedRoot>>;
    fn save_roots(&self, roots: &[super::domain::SharedRoot]) -> PortResult<()>;
}

/// 共享根注册表的**引擎广播面** wire 形状——**差异面③**
///
/// 桌面元素 `{ id, name, path }`；移动元素 `{ id, name, safTreeUri }`（引擎按 SAF 树 URI
/// 解析目录）。字段名不同、语义同位，故只抽「映射」这一步，不抽字段名常量。
pub trait RootWireCodec {
    fn roots_to_push_payload(&self, roots: &[super::domain::SharedRoot]) -> Vec<Value>;
}

// ==================== 对等网络面 ====================

/// 对等网络原语（双端同集合；签名对齐双端 SDK `HostPeer`）
pub trait PeerPort {
    fn peer_dial(&self, endpoint: &Value) -> PortResult<String>;
    fn peer_close(&self, handle: &str) -> PortResult<bool>;
    fn peer_respond_consent(&self, request_id: &str, accepted: bool) -> PortResult<bool>;
    fn peer_list_trusted(&self) -> PortResult<Value>;
    fn peer_revoke_trusted(&self, node_id: &str) -> PortResult<bool>;
    fn peer_send_files(&self, session: &str, paths: &[Value]) -> PortResult<String>;
    fn peer_respond_transfer(&self, batch_id: &str, accept: bool) -> PortResult<()>;
    fn peer_set_receive_policy(&self, mode: &str, timeout_secs: u64) -> PortResult<()>;
    fn peer_pause_transfer(&self, batch_id: &str) -> PortResult<()>;
    fn peer_resume_transfer(&self, batch_id: &str) -> PortResult<()>;
    fn peer_set_shared_roots(&self, dirs: &[Value]) -> PortResult<()>;
    fn peer_list_shared_roots(&self, session: &str) -> PortResult<Value>;
    fn peer_browse_directory(
        &self,
        session: &str,
        dir_id: &str,
        rel_path: &str,
    ) -> PortResult<Value>;
    fn peer_pull_files(&self, session: &str, dir_id: &str, files: &[Value]) -> PortResult<u32>;
    fn peer_set_download_dir(&self, path: &str) -> PortResult<()>;
    fn peer_active_transfers(&self) -> PortResult<Value>;
    fn peer_collect_outgoing(&self, paths: &[Value]) -> PortResult<Value>;
}

/// 节点电源——**差异面④**
///
/// 桌面：插件在 activate / deactivate 显式请求（审计票 12 起宿主无按产品 id 的开关外壳）。
/// 移动：节点生命周期由宿主外壳驱动，插件不持有电源。默认 = 不请求（移动形态）。
pub trait NodePower {
    fn peer_start_node(&self) -> PortResult<bool> {
        Err(PortError::unsupported(
            "node power is driven by the host shell",
        ))
    }
    fn peer_stop_node(&self) -> PortResult<bool> {
        Err(PortError::unsupported(
            "node power is driven by the host shell",
        ))
    }
}

// ==================== 平台交互面 ====================

/// 平台文件选择与目录打开
///
/// `pick_files` / `pick_folder` 双端均有；`pick_folders`（多选）与 `reveal_in_dir`
/// （打开所在目录）**桌面独有**——差异面⑤⑥，默认显性 unsupported。
pub trait PlatformPort {
    fn platform_pick_files(&self) -> PortResult<Vec<String>>;
    fn platform_pick_folder(&self) -> PortResult<String>;

    fn platform_pick_folders(&self) -> PortResult<Vec<String>> {
        Err(PortError::unsupported("platform_pick_folders"))
    }

    fn platform_reveal_in_dir(&self, path: &str) -> PortResult<()> {
        let _ = path;
        Err(PortError::unsupported("platform_reveal_in_dir"))
    }
}

/// mDNS 自建浏览
pub trait MdnsPort {
    fn mdns_browse(&self, service_type: &str) -> PortResult<String>;
    fn mdns_stop_browse(&self, browser_id: &str) -> PortResult<bool>;
}

// ==================== 信任决策面 ====================

/// 首连确认与信任列表——**差异面⑦**
///
/// 桌面：经互调认证中心（ADR 0031/0033）决策，中心不可用时双轨降级直答宿主。
/// 移动：无认证中心接入，直答宿主原语。两者都实现本 trait，语义分歧留在各自适配器内。
pub trait ConsentGate {
    /// 用户对首连确认的裁决：返回是否命中待应答项
    fn decide_consent(&self, request_id: &str, accepted: bool) -> PortResult<bool>;
    /// 入站确认事件的自动预检：已信任则自动放行并应答，返回是否已由本端口消化
    fn evaluate_consent(&self, payload: &Value) -> PortResult<bool>;
    /// 信任设备列表
    fn list_trusted(&self) -> PortResult<Value>;
}

// ==================== 形态能力位 ====================

/// 本端形态能力位（**只放「有没有」，不放「怎么做」**）
///
/// 放这里的判据：该差异无法表达为端口方法的有无，而是**同一段编排是否该走**。
pub trait PluginProfile {
    /// 是否仍订阅旧快照 topic（`peer:devices` / `peer:transfer` / `peer:receive`）做对账——
    /// 差异面⑧：桌面处于双写期仍订阅；移动端这些 topic 已整条退役。
    fn uses_legacy_snapshot(&self) -> bool {
        false
    }

    /// 接收落点是否可自定义——差异面⑤：桌面 `downloadDir` 由 UI 选择并推送
    /// `set-download-dir`；移动端接收落点固定 `MediaStore.Downloads`、页面无此设置项。
    ///
    /// **不用「有没有 `peer_set_download_dir` 原语」表达**：移动 SDK 也有该原语（票 04
    /// 对齐），这里差的是**产品形态**（移动端不给用户选落点）而非能力缺失——写成原语有无
    /// 会把「移动端误推了落点」变成一条静默生效的路径。
    fn supports_custom_download_dir(&self) -> bool {
        false
    }
}
