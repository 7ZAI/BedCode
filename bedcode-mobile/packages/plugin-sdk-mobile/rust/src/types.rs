//! Plugin Types (Mobile SDK)
//!
//! 移动端插件声明式描述类型 — 从宿主 types.rs 迁移的共享部分

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 插件类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    /// 纯 Rust 插件，无前端组件
    Rust,
    /// Rust + TypeScript 插件
    RustTs,
    /// 纯 TypeScript 插件
    TsOnly,
    /// WASM 插件，通过 wasmtime 动态加载
    Wasm,
}

impl Default for PluginType {
    fn default() -> Self {
        Self::TsOnly
    }
}

/// 插件运行时状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PluginState {
    Loaded,
    Activated,
    Deactivated,
    Error { error: String },
}

impl Default for PluginState {
    fn default() -> Self {
        Self::Loaded
    }
}

/// 插件清单
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// 前端入口模块路径（相对于 dist/）
    #[serde(default)]
    pub main: String,
    #[serde(default)]
    pub plugin_type: PluginType,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub contributes: PluginContributes,
    /// 插件图标：emoji、内联 <svg> 标记或相对插件目录的图片路径（如 "icon.png"/"icon.svg"）
    /// 缺省时前端按插件 id 生成字母头像回退
    #[serde(default)]
    pub icon: Option<String>,
    /// WASM 文件 SHA256 哈希，用于远程下载校验
    #[serde(default)]
    pub wasm_hash: String,
    /// Rust 库名（对应 WASM 文件名）
    #[serde(default)]
    pub rust_library: String,
}

/// 插件扩展点声明
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginContributes {
    #[serde(default)]
    pub commands: Vec<CommandContribution>,
    #[serde(default)]
    pub views: Vec<ViewContribution>,
    #[serde(default)]
    pub terminal: Option<TerminalContribution>,
    #[serde(default)]
    pub nav_tab: Option<NavTabContribution>,
    #[serde(default)]
    pub settings: Option<SettingsContribution>,
    #[serde(default)]
    pub configuration: Option<PluginConfiguration>,
    #[serde(default)]
    pub lifecycle: Option<LifecycleContribution>,
}

/// 命令扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandContribution {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub icon: Option<String>,
}

/// 视图扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewContribution {
    pub id: String,
    #[serde(rename = "type")]
    pub view_type: String,
    pub title: String,
    pub component: String,
}

/// 底部导航 Tab 扩展点（移动端特有）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavTabContribution {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub component: String,
    #[serde(default)]
    pub order: i32,
}

/// 设置页扩展点（移动端特有）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsContribution {
    pub section: String,
    pub component: String,
}

/// 终端扩展点
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalContribution {
    #[serde(default)]
    pub input_handlers: Vec<String>,
    #[serde(default)]
    pub output_parsers: Vec<String>,
    #[serde(default)]
    pub toolbar_items: Vec<TerminalToolbarItemContribution>,
}

/// 终端工具栏按钮
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalToolbarItemContribution {
    pub id: String,
    pub title: String,
    pub icon: String,
}

/// 插件配置声明
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfiguration {
    pub title: String,
    pub properties: HashMap<String, ConfigProperty>,
}

/// 配置属性
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigProperty {
    #[serde(rename = "type")]
    pub prop_type: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

/// 生命周期扩展点声明
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleContribution {
    #[serde(default)]
    pub on_startup: bool,
    #[serde(default)]
    pub on_shutdown: bool,
    #[serde(default)]
    pub on_auth_success: bool,
    #[serde(default)]
    pub on_disconnect: bool,
    #[serde(default)]
    pub on_session_created: bool,
    #[serde(default)]
    pub on_session_stopped: bool,
    #[serde(default)]
    pub on_terminal_input: bool,
    #[serde(default)]
    pub on_terminal_output: bool,
}

impl LifecycleContribution {
    /// 检查是否声明了指定事件
    pub fn is_declared(&self, event_name: &str) -> bool {
        match event_name {
            "onStartup" => self.on_startup,
            "onShutdown" => self.on_shutdown,
            "onAuthSuccess" => self.on_auth_success,
            "onDisconnect" => self.on_disconnect,
            "onSessionCreated" => self.on_session_created,
            "onSessionStopped" => self.on_session_stopped,
            "onTerminalInput" => self.on_terminal_input,
            "onTerminalOutput" => self.on_terminal_output,
            _ => false,
        }
    }

    /// 检查是否有任何声明
    pub fn has_any_declared(&self) -> bool {
        self.on_startup
            || self.on_shutdown
            || self.on_auth_success
            || self.on_disconnect
            || self.on_session_created
            || self.on_session_stopped
            || self.on_terminal_input
            || self.on_terminal_output
    }
}

// ==================== File Service & Transfer ====================
//
// 宿主通用文件服务能力的 SDK 契约类型（两端同构，见内网文件传输插件规格第 4 节）。
// serde camelCase 与线协议一致：宿主 HTTP 端点、WASM ABI JSON 均直接使用。
// 与桌面端 SDK `plugin-sdk-desktop/rust/src/types.rs` 同名段落保持逐字段一致。

/// 文件操作类型（挂载时声明支持的操作集合）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileOperation {
    /// 目录列举
    List,
    /// 文件下载（Range 续传）
    Download,
    /// 文件上传（upload session 模型）
    Upload,
}

/// 文件服务挂载选项（插件 → 宿主）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountOptions {
    /// 挂载点名称（小写字母数字 `-_`，暴露为 /{pluginId}/{mountPath}/**）
    pub mount_path: String,
    /// 允许目录根（绝对路径，来自插件 storage 的用户配置）
    pub roots: Vec<String>,
    /// 允许的操作集合（未声明的操作端点返回 403）
    pub operations: Vec<FileOperation>,
}

/// 挂载结果（宿主 → 插件）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountResult {
    /// 挂载点名称
    pub mount_path: String,
    /// 服务端基础路径（相对主机地址，移动端为 /{pluginId}/{mountPath}，无 /api 前缀）
    pub base_path: String,
}

/// 上传策略钩子入参（宿主 → 插件）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadRequestMeta {
    /// 目标相对路径（相对挂载根）
    pub relative_path: String,
    /// 声明的文件大小（字节）
    pub size: u64,
}

/// 上传策略钩子决定（插件 → 宿主）
///
/// fail-closed：任何异常（超时/解析失败/插件未实现）宿主一律视为拒绝
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadHookDecision {
    /// 是否允许上传
    pub allow: bool,
    /// 拒绝原因（如 duplicate-name），允许时为空
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl UploadHookDecision {
    /// 允许上传
    pub fn allow() -> Self {
        Self { allow: true, reason: None }
    }

    /// 拒绝上传（fail-closed 语义）
    pub fn deny(reason: impl Into<String>) -> Self {
        Self { allow: false, reason: Some(reason.into()) }
    }
}

impl Default for UploadHookDecision {
    /// 默认拒绝（fail-closed）
    fn default() -> Self {
        Self::deny("no decision")
    }
}

/// 传输方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferDirection {
    /// 从本地读文件 PUT 到对端
    Upload,
    /// 从对端 GET 文件写到本地
    Download,
}

/// 传输任务请求（插件 → 宿主传输引擎）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequest {
    /// 任务 ID（插件预生成）。
    ///
    /// 宿主以它为进度总线 topic（`transfer:{task_id}`）与 Tauri 事件
    /// `plugin:transfer:progress` 的 taskId，不再自生成 UUID ——
    /// 插件可在 `transfer_start` 前订阅 `transfer:{task_id}` 收到全部
    /// 进度/终态消息，避免「宿主传输先完成、插件后订阅」的竞态丢消息
    pub task_id: String,
    /// 传输方向
    pub direction: TransferDirection,
    /// 对端 URL（下载 = 文件 URL；上传 = upload session 的 append URL）
    pub url: String,
    /// 附加请求头（如 Authorization、Range 由插件控制）
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// 本地文件路径（下载 = 写入目标，上传 = 读取源）
    pub local_path: String,
    /// 续传偏移（字节，0 = 从头）
    #[serde(default)]
    pub offset: u64,
    /// 预期总大小（字节，用于进度计算；0 = 未知）
    #[serde(default)]
    pub expected_size: u64,
    /// 下载完成后的最终落位路径（原子 rename 目标）。
    /// 仅 Download 方向生效：local_path 写 .part 临时文件，完成后 rename 到此路径；
    /// 目标已存在 → Failed("duplicate-name") 且保留临时文件。Upload 方向忽略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_path: Option<String>,
}

/// 传输任务状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "reason")]
pub enum TransferState {
    /// 传输进行中
    #[serde(rename = "running")]
    Running,
    /// 传输完成（终态）
    #[serde(rename = "completed")]
    Completed,
    /// 传输失败（终态，携带原因）
    #[serde(rename = "failed")]
    Failed(String),
    /// 已取消（终态，宿主已回报最终偏移）
    #[serde(rename = "cancelled")]
    Cancelled,
}

/// 传输进度（宿主 → 插件/前端，经 Tauri 事件与消息总线双通道推送）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    /// 任务 ID（= 插件预生成的 task_id，与任务快照同一命名空间）
    pub task_id: String,
    /// 已传输字节数（含续传偏移）
    pub transferred: u64,
    /// 总字节数（0 = 未知）
    pub total: u64,
    /// 瞬时速率（字节/秒）
    pub bytes_per_sec: u64,
    /// 当前状态
    pub state: TransferState,
}

/// 对端挂载点信息（控制面公告的单个挂载）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerMountAnnouncement {
    /// 挂载所属插件 ID（URL 第一段）
    pub plugin_id: String,
    /// 挂载点名称（URL 第二段）
    pub mount_path: String,
    /// 该挂载支持的操作集合
    pub operations: Vec<FileOperation>,
}

/// 对端文件服务信息（控制面公告，由 WS Announce 填充）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerFileService {
    /// 对端 IP
    pub ip: String,
    /// 对端文件服务端口
    pub port: u16,
    /// 鉴权 Token（移动端服务为 Bearer Token；桌面端走 JWT 时可为空）
    #[serde(default)]
    pub token: String,
    /// 对端真实设备名（用户设置名，获取不到时为兜底名）
    #[serde(default)]
    pub device_name: String,
    /// 对端挂载点列表
    #[serde(default)]
    pub mounts: Vec<PeerMountAnnouncement>,
}
