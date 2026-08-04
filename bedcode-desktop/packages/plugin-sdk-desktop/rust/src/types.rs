//! Plugin Types
//!
//! 插件声明式描述类型、状态枚举 — 从桌面端 types.rs 迁移的共享部分

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 插件描述文件 (plugin.json) 的完整结构
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    /// 唯一标识（反向域名格式，如 com.bedcode.quick-snippets）
    pub id: String,
    /// 显示名称
    pub name: String,
    /// 语义化版本号
    pub version: String,
    /// 插件描述
    #[serde(default)]
    pub description: String,
    /// 作者
    #[serde(default)]
    pub author: String,
    /// 入口文件路径（相对于插件根目录，TS-only 插件使用）
    #[serde(default)]
    pub main: String,
    /// 沙箱模式：MVP 仅支持 "inline"
    #[serde(default = "default_sandbox")]
    pub sandbox: String,
    /// 请求的权限列表
    #[serde(default)]
    pub permissions: Vec<String>,
    /// 扩展点声明
    #[serde(default)]
    pub contributes: PluginContributes,
    /// 插件类型：rust / rust-ts / ts-only
    #[serde(default = "default_plugin_type")]
    pub plugin_type: PluginType,
    /// cdylib 动态库文件名（不含路径，相对于插件目录）
    /// 仅 rust-ts 类型插件使用，宿主根据平台自动添加后缀
    #[serde(default)]
    pub rust_library: String,
}

fn default_sandbox() -> String {
    "inline".to_string()
}

fn default_plugin_type() -> PluginType {
    PluginType::TsOnly
}

/// 插件类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    /// 纯 Rust 插件，无前端组件
    Rust,
    /// Rust + TypeScript 插件，Rust 提供后端能力，TS 提供 UI
    RustTs,
    /// 纯 TypeScript 插件，仅前端组件
    TsOnly,
}

/// 插件配置声明
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfiguration {
    /// 配置区域标题
    pub title: String,
    /// 配置属性映射（key → 属性定义）
    pub properties: HashMap<String, ConfigProperty>,
}

/// 配置属性定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigProperty {
    /// 属性类型：string / number / boolean
    #[serde(rename = "type")]
    pub prop_type: String,
    /// 显示标题
    pub title: String,
    /// 帮助描述
    #[serde(default)]
    pub description: Option<String>,
    /// 默认值
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    /// 枚举选项（type 为 string 时使用）
    #[serde(default)]
    pub enum_values: Option<Vec<String>>,
}

/// 插件扩展点声明
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PluginContributes {
    #[serde(default)]
    pub commands: Vec<CommandContribution>,
    #[serde(default)]
    pub views: Vec<ViewContribution>,
    #[serde(default)]
    pub terminal: Option<TerminalContribution>,
    #[serde(default)]
    pub tool_providers: Vec<ToolProviderContribution>,
    #[serde(default)]
    pub file_handlers: Vec<FileHandlerContribution>,
    /// 配置声明
    #[serde(default)]
    pub configuration: Option<PluginConfiguration>,
    /// 生命周期钩子声明
    #[serde(default)]
    pub lifecycle: Option<LifecycleContribution>,
    /// 声明此插件会发布的消息 topic（文档性质，不做强制校验）
    #[serde(default)]
    pub provides: Vec<String>,
    /// 声明此插件感兴趣的消息 topic（宿主据此路由消息）
    #[serde(default)]
    pub subscribes: Vec<String>,
}

/// 生命周期扩展点声明
///
/// 插件通过此声明告知宿主它需要接收应用启动/关闭事件。
/// Rust 插件通过 `BedcodePlugin` trait 的 `on_startup`/`on_shutdown` 方法实现回调；
/// TS-only 插件通过前端事件 `lifecycle:startup`/`lifecycle:shutdown` 接收。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleContribution {
    /// 是否注册 onStartup 回调
    #[serde(default)]
    pub on_startup: bool,
    /// 是否注册 onShutdown 回调
    #[serde(default)]
    pub on_shutdown: bool,
}

/// 命令扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// "sidebar" | "toolbox" | "statusbar"
    #[serde(rename = "type")]
    pub view_type: String,
    pub title: String,
    pub component: String,
}

/// 终端扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalContribution {
    #[serde(default)]
    pub input_handlers: Vec<String>,
    #[serde(default)]
    pub output_parsers: Vec<String>,
}

/// 外部工具扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolProviderContribution {
    pub id: String,
    pub name: String,
    pub endpoint: String,
}

/// 文件处理扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHandlerContribution {
    pub id: String,
    pub extensions: Vec<String>,
    pub viewer: String,
    #[serde(default)]
    pub icon: Option<String>,
}

/// 插件运行时状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "error")]
pub enum PluginState {
    Loaded,
    Activated,
    Error(String),
    Deactivated,
}

/// 插件信息（返回给前端的精简版本）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub main: String,
    pub sandbox: String,
    pub plugin_type: PluginType,
    pub permissions: Vec<String>,
    pub state: PluginState,
    pub extension_path: String,
    pub contributes: PluginContributes,
}

// ==================== File Service & Transfer ====================
//
// 宿主通用文件服务能力的 SDK 契约类型（两端同构，见内网文件传输插件规格第 4 节）。
// serde camelCase 与线协议一致：宿主 HTTP 端点、WASM ABI JSON 均直接使用。

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
    /// 挂载点名称（小写字母数字 `-_`，暴露为 /plugins/{pluginId}/{mountPath}/**）
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
    /// 服务端基础路径（相对主机地址，如 /api/plugins/{pluginId}/{mountPath}）
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
    /// 宿主生成的任务 ID
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

/// 对端挂载点信息（控制面公告的单个挂载，阶段 2 起）
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

/// 对端文件服务信息（控制面公告，阶段 2 由 WS 公告填充）
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
    /// 对端挂载点列表
    #[serde(default)]
    pub mounts: Vec<PeerMountAnnouncement>,
}
