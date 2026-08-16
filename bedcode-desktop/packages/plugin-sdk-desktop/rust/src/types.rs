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
    /// 对外可互调 api 清单（ADR-0017，插件互调机制）
    ///
    /// 全限定名数组（如 `com.bedcode.scheduler.add`）；宿主在插件激活时
    /// 登记到 api 注册表，`bedcode.api.*` 请求 topic 的目标 api 必须命中
    /// 某已激活插件的声明清单，否则被总线门禁拒绝。缺省空数组 = 不对外
    /// 提供互调 api（现有插件不受影响）。
    #[serde(default)]
    pub api: Vec<String>,
    /// 扩展点声明
    #[serde(default)]
    pub contributes: PluginContributes,
    /// 插件类型：rust / rust-ts / ts-only
    #[serde(default = "default_plugin_type")]
    pub plugin_type: PluginType,
    /// WASM 库文件名（不含路径，相对于插件目录）
    /// 仅 rust-ts 类型插件使用，宿主根据平台自动添加后缀
    #[serde(default)]
    pub rust_library: String,
    /// 插件图标：图片路径（相对插件目录）或内联 SVG 标记
    #[serde(default)]
    pub icon: Option<String>,
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
    /// 插件请求的权限尚未获得用户批准（需在插件管理页人工审批后才能激活）
    NeedsApproval,
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
    /// 允许目录根（绝对路径，来自插件 storage 的用户配置）；供对端浏览/下载（只读暴露），
    /// 声明 Upload 操作时同时作为接收落点的兼容回退（旧语义）。
    pub roots: Vec<String>,
    /// 允许的操作集合（未声明的操作端点返回 403）
    pub operations: Vec<FileOperation>,
    /// 接收落点（接收对端 upload 的目录，spec 方向模型：“下载目录 = 接收落点”，
    /// 不落共享 roots）。存在时 POST /upload 创建 session 的目标名解析以此为准，
    /// 跳 roots 沙箱；为 None（旧插件）时回退到 roots 语义保后兼容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads_dir: Option<String>,
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

/// 上传策略钩子决定（插件 → 宿主）—— v2 三路化：allow / deny / ask
///
/// fail-closed：任何异常（超时/解析失败/插件未实现）宿主一律视为拒绝。
/// wire 兼容：旧插件返回 `{ allow: false }` → deny；`{ allow: true }` → allow。
/// ask = 请求用户批准（批上下文，spec 14.2），与 allow 互斥。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadHookDecision {
    /// 是否允许上传
    pub allow: bool,
    /// v2：true = 需要用户批准（批上下文）；与 allow 互斥
    /// skip_serializing_if：false 时不序列化，保持 v1 wire 形状（与移动端 SDK 逐字一致）
    #[serde(default, skip_serializing_if = "is_false")]
    pub ask: bool,
    /// 拒绝原因（如 duplicate-name / policy-denied），允许时为空
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// serde skip 辅助：false 时不序列化（保持 v1 wire 形状，两端字节一致）
fn is_false(b: &bool) -> bool {
    !*b
}

impl UploadHookDecision {
    /// 允许上传
    pub fn allow() -> Self {
        Self { allow: true, ask: false, reason: None }
    }

    /// 拒绝上传（fail-closed 语义）
    pub fn deny(reason: impl Into<String>) -> Self {
        Self { allow: false, ask: false, reason: Some(reason.into()) }
    }

    /// v2：请求用户批准（异步批准协议，宿主将批置 pending 并等待用户应答）
    pub fn ask() -> Self {
        Self { allow: false, ask: true, reason: None }
    }
}

/// 批量传输请求元信息（宿主 → 插件批钩子入参，v2）
///
/// POST /transfer-request 时宿主调用一次批级钩子；files 为批内全部
/// 文件的元信息清单，total_size 为批总大小（字节）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequestMeta {
    /// 批 ID（UUID，发送方生成，批上下文标识）
    pub batch_id: String,
    /// 批内文件清单（相对路径 + 大小）
    pub files: Vec<UploadRequestMeta>,
    /// 批总大小（字节）
    pub total_size: u64,
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
    /// 对端真实设备名（用户设置名，获取不到时为兜底名）
    #[serde(default)]
    pub device_name: String,
    /// 对端挂载点列表
    #[serde(default)]
    pub mounts: Vec<PeerMountAnnouncement>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== PluginManifest ====================

    #[test]
    fn test_manifest_parse_with_defaults() {
        // 缺省字段（description/author/main/icon/contributes）全部走 default，
        // 宿主加载最小化 plugin.json 不应失败
        let json = serde_json::json!({
            "id": "com.bedcode.demo",
            "name": "Demo",
            "version": "0.1.0",
            "sandbox": "inline",
            "permissions": ["storage", "terminal:input"],
            "pluginType": "rust-ts",
            "contributes": {
                "commands": [{ "id": "run", "title": "Run", "icon": "run.svg" }],
                "subscribes": ["task:status-changed"]
            }
        });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        assert_eq!(m.id, "com.bedcode.demo");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.description, "");
        assert_eq!(m.sandbox, "inline");
        assert_eq!(m.plugin_type, PluginType::RustTs);
        assert_eq!(m.permissions, vec!["storage", "terminal:input"]);
        assert_eq!(m.contributes.commands.len(), 1);
        assert_eq!(m.contributes.commands[0].id, "run");
        assert_eq!(m.contributes.commands[0].title, "Run");
        assert_eq!(m.contributes.commands[0].icon.as_deref(), Some("run.svg"));
        assert_eq!(m.contributes.subscribes, vec!["task:status-changed"]);
    }

    #[test]
    fn test_manifest_round_trip_fills_defaults() {
        // 宿主加载最小化 plugin.json 后序列化回写：缺省字段应已填充默认值
        // （serde default 在反序列化时生效，序列化反映内存中的实际值）
        let json = serde_json::json!({ "id": "com.bedcode.x", "name": "X", "version": "1.0.0" });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        let back = serde_json::to_value(&m).unwrap();
        assert_eq!(back["pluginType"], serde_json::json!("ts-only"));
        assert_eq!(back["sandbox"], serde_json::json!("inline"));
        assert_eq!(back["description"], serde_json::json!(""));
        // contributes 序列化时带全部字段（serde(default) 只影响反序列化）
        assert_eq!(back["contributes"]["commands"], serde_json::json!([]));
        assert_eq!(back["contributes"]["subscribes"], serde_json::json!([]));
    }

    // ==================== PluginType / PluginState ====================

    #[test]
    fn test_plugin_type_kebab_case() {
        // 线协议 kebab-case：宿主按字面量解析 plugin.json 的 pluginType 字段
        assert_eq!(serde_json::to_value(PluginType::Rust).unwrap(), serde_json::json!("rust"));
        assert_eq!(serde_json::to_value(PluginType::RustTs).unwrap(), serde_json::json!("rust-ts"));
        assert_eq!(serde_json::to_value(PluginType::TsOnly).unwrap(), serde_json::json!("ts-only"));
        assert_eq!(serde_json::from_value::<PluginType>(serde_json::json!("rust-ts")).unwrap(), PluginType::RustTs);
        assert!(serde_json::from_value::<PluginType>(serde_json::json!("rust_ts")).is_err());
    }

    #[test]
    fn test_plugin_state_adjacent_tagging() {
        // state/error 相邻标签：unit 变体无 error 字段，Error 携带消息
        assert_eq!(
            serde_json::to_value(PluginState::Activated).unwrap(),
            serde_json::json!({ "state": "Activated" })
        );
        assert_eq!(
            serde_json::to_value(PluginState::Error("boom".into())).unwrap(),
            serde_json::json!({ "state": "Error", "error": "boom" })
        );
        let back: PluginState =
            serde_json::from_value(serde_json::json!({ "state": "Error", "error": "x" })).unwrap();
        assert_eq!(back, PluginState::Error("x".into()));
    }

    // ==================== 扩展点声明 ====================

    #[test]
    fn test_view_contribution_type_field() {
        // view_type 序列化为 "type"（与前端 vscode 风格扩展点一致）
        let v = ViewContribution {
            id: "v1".into(),
            view_type: "sidebar".into(),
            title: "Side".into(),
            component: "SidePanel".into(),
        };
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            serde_json::json!({
                "id": "v1",
                "type": "sidebar",
                "title": "Side",
                "component": "SidePanel"
            })
        );
    }

    #[test]
    fn test_config_property_type_field() {
        // 属性类型字段序列化为 "type"，缺省字段（description/default/enumValues）为 null
        let p = ConfigProperty {
            prop_type: "string".into(),
            title: "API Key".into(),
            description: None,
            default: None,
            enum_values: None,
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({
                "type": "string",
                "title": "API Key",
                "description": null,
                "default": null,
                "enumValues": null
            })
        );
    }

    #[test]
    fn test_contributes_defaults_and_full_parse() {
        // 全量贡献点解析：terminal/toolProviders/fileHandlers/configuration/lifecycle
        let json = serde_json::json!({
            "commands": [{ "id": "c1", "title": "C1" }],
            "views": [{ "id": "v1", "type": "toolbox", "title": "V1", "component": "C" }],
            "terminal": {
                "inputHandlers": ["in1"],
                "outputParsers": ["out1"]
            },
            "toolProviders": [{ "id": "tp1", "name": "N", "endpoint": "http://x" }],
            "fileHandlers": [{ "id": "fh1", "extensions": ["md"], "viewer": "V" }],
            "configuration": {
                "title": "Config",
                "properties": {
                    "key": { "type": "string", "title": "Key" }
                }
            },
            "lifecycle": { "onStartup": true, "onShutdown": false },
            "provides": ["topic:a"],
            "subscribes": ["topic:b"]
        });
        let c: PluginContributes = serde_json::from_value(json).unwrap();
        assert_eq!(c.terminal.as_ref().unwrap().input_handlers, vec!["in1"]);
        assert_eq!(c.tool_providers[0].endpoint, "http://x");
        assert_eq!(c.file_handlers[0].extensions, vec!["md"]);
        assert_eq!(c.configuration.as_ref().unwrap().properties.len(), 1);
        assert!(c.lifecycle.as_ref().unwrap().on_startup);
        assert!(!c.lifecycle.as_ref().unwrap().on_shutdown);
        assert_eq!(c.provides, vec!["topic:a"]);
    }

    // ==================== FileOperation / Mount ====================

    #[test]
    fn test_file_operation_lowercase() {
        // 线协议 lowercase：HTTP 端点与 WASM ABI JSON 直接使用
        assert_eq!(serde_json::to_value(FileOperation::List).unwrap(), serde_json::json!("list"));
        assert_eq!(serde_json::to_value(FileOperation::Download).unwrap(), serde_json::json!("download"));
        assert_eq!(serde_json::to_value(FileOperation::Upload).unwrap(), serde_json::json!("upload"));
        assert_eq!(
            serde_json::from_value::<FileOperation>(serde_json::json!("download")).unwrap(),
            FileOperation::Download
        );
        assert!(serde_json::from_value::<FileOperation>(serde_json::json!("Download")).is_err());
    }

    #[test]
    fn test_mount_options_and_result_camel_case() {
        let opts = MountOptions {
            mount_path: "shared".into(),
            roots: vec!["/data".into()],
            operations: vec![FileOperation::List, FileOperation::Download],
            downloads_dir: None,
        };
        assert_eq!(
            serde_json::to_value(&opts).unwrap(),
            serde_json::json!({
                "mountPath": "shared",
                "roots": ["/data"],
                "operations": ["list", "download"]
            })
        );
        let result = MountResult {
            mount_path: "shared".into(),
            base_path: "/api/plugins/com.bedcode.x/shared".into(),
        };
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::json!({
                "mountPath": "shared",
                "basePath": "/api/plugins/com.bedcode.x/shared"
            })
        );
    }

    // ==================== UploadHookDecision ====================

    #[test]
    fn test_upload_hook_decision_constructors() {
        let allow = UploadHookDecision::allow();
        assert!(allow.allow);
        assert!(!allow.ask);
        assert_eq!(allow.reason, None);
        let deny = UploadHookDecision::deny("duplicate-name");
        assert!(!deny.allow);
        assert!(!deny.ask);
        assert_eq!(deny.reason.as_deref(), Some("duplicate-name"));
        // v2：ask 与 allow 互斥
        let ask = UploadHookDecision::ask();
        assert!(!ask.allow);
        assert!(ask.ask);
        assert_eq!(ask.reason, None);
    }

    #[test]
    fn test_upload_hook_decision_fail_closed_default() {
        // fail-closed：Default 必须是拒绝，且携带 "no decision" 原因
        let d = UploadHookDecision::default();
        assert!(!d.allow);
        assert!(!d.ask);
        assert_eq!(d.reason.as_deref(), Some("no decision"));
    }

    #[test]
    fn test_upload_hook_decision_wire_format() {
        // allow/deny 时 ask 被跳过（skip_serializing_if）：保持 v1 wire 形状
        // （旧对端/宿主按无 ask 字段解析，与移动端 SDK 逐字一致）；ask 时序列化 ask=true
        assert_eq!(
            serde_json::to_value(UploadHookDecision::allow()).unwrap(),
            serde_json::json!({ "allow": true })
        );
        assert_eq!(
            serde_json::to_value(UploadHookDecision::deny("duplicate-name")).unwrap(),
            serde_json::json!({ "allow": false, "reason": "duplicate-name" })
        );
        assert_eq!(
            serde_json::to_value(UploadHookDecision::ask()).unwrap(),
            serde_json::json!({ "allow": false, "ask": true })
        );
        // v1 旧插件载荷（无 ask 字段）→ ask=false（deny 语义）
        let old: UploadHookDecision =
            serde_json::from_value(serde_json::json!({ "allow": false, "reason": "x" })).unwrap();
        assert!(!old.ask);
        // 旧 allow 载荷 → 仍 allow
        let old_allow: UploadHookDecision =
            serde_json::from_value(serde_json::json!({ "allow": true })).unwrap();
        assert!(old_allow.allow);
        assert!(!old_allow.ask);
    }

    #[test]
    fn test_transfer_request_meta_wire_format() {
        let meta = TransferRequestMeta {
            batch_id: "b1".into(),
            files: vec![UploadRequestMeta { relative_path: "a.txt".into(), size: 10 }],
            total_size: 10,
        };
        assert_eq!(
            serde_json::to_value(&meta).unwrap(),
            serde_json::json!({
                "batchId": "b1",
                "files": [{ "relativePath": "a.txt", "size": 10 }],
                "totalSize": 10
            })
        );
    }

    #[test]
    fn test_upload_request_meta_wire_format() {
        let meta = UploadRequestMeta { relative_path: "dir/a.txt".into(), size: 1024 };
        assert_eq!(
            serde_json::to_value(&meta).unwrap(),
            serde_json::json!({ "relativePath": "dir/a.txt", "size": 1024 })
        );
        let back: UploadRequestMeta =
            serde_json::from_value(serde_json::json!({ "relativePath": "a", "size": 1 })).unwrap();
        assert_eq!(back.relative_path, "a");
    }

    // ==================== Transfer ====================

    #[test]
    fn test_transfer_direction_lowercase() {
        assert_eq!(serde_json::to_value(TransferDirection::Upload).unwrap(), serde_json::json!("upload"));
        assert_eq!(serde_json::to_value(TransferDirection::Download).unwrap(), serde_json::json!("download"));
        assert!(serde_json::from_value::<TransferDirection>(serde_json::json!("UP")).is_err());
    }

    #[test]
    fn test_transfer_request_wire_format() {
        let req = TransferRequest {
            task_id: "t1".into(),
            direction: TransferDirection::Download,
            url: "http://peer:8899/api/plugins/x/m/file".into(),
            headers: {
                let mut h = HashMap::new();
                h.insert("Authorization".into(), "Bearer abc".into());
                h
            },
            local_path: "/tmp/t1.part".into(),
            offset: 4096,
            expected_size: 0,
            final_path: Some("/tmp/t1".into()),
        };
        assert_eq!(
            serde_json::to_value(&req).unwrap(),
            serde_json::json!({
                "taskId": "t1",
                "direction": "download",
                "url": "http://peer:8899/api/plugins/x/m/file",
                "headers": { "Authorization": "Bearer abc" },
                "localPath": "/tmp/t1.part",
                "offset": 4096,
                "expectedSize": 0,
                "finalPath": "/tmp/t1"
            })
        );
    }

    #[test]
    fn test_transfer_request_minimal_round_trip() {
        // 缺省字段（headers/offset/expectedSize/finalPath）不携带时按默认值解析
        let json = serde_json::json!({
            "taskId": "t2",
            "direction": "upload",
            "url": "http://peer/up",
            "localPath": "/data/f.bin"
        });
        let req: TransferRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.direction, TransferDirection::Upload);
        assert!(req.headers.is_empty());
        assert_eq!(req.offset, 0);
        assert_eq!(req.expected_size, 0);
        assert_eq!(req.final_path, None);
    }

    #[test]
    fn test_transfer_state_wire_format() {
        // state/reason 相邻标签，变体名显式锁定小写
        assert_eq!(
            serde_json::to_value(TransferState::Running).unwrap(),
            serde_json::json!({ "state": "running" })
        );
        assert_eq!(
            serde_json::to_value(TransferState::Failed("network".into())).unwrap(),
            serde_json::json!({ "state": "failed", "reason": "network" })
        );
        assert_eq!(
            serde_json::to_value(TransferState::Cancelled).unwrap(),
            serde_json::json!({ "state": "cancelled" })
        );
        let back: TransferState =
            serde_json::from_value(serde_json::json!({ "state": "completed" })).unwrap();
        assert_eq!(back, TransferState::Completed);
    }

    #[test]
    fn test_transfer_progress_wire_format() {
        let p = TransferProgress {
            task_id: "t1".into(),
            transferred: 2048,
            total: 8192,
            bytes_per_sec: 512,
            state: TransferState::Running,
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({
                "taskId": "t1",
                "transferred": 2048,
                "total": 8192,
                "bytesPerSec": 512,
                "state": { "state": "running" }
            })
        );
    }

    // ==================== Peer 公告 ====================

    #[test]
    fn test_peer_file_service_round_trip() {
        let peer = PeerFileService {
            ip: "192.168.1.5".into(),
            port: 8899,
            token: String::new(),
            device_name: "phone".into(),
            mounts: vec![PeerMountAnnouncement {
                plugin_id: "com.bedcode.x".into(),
                mount_path: "shared".into(),
                operations: vec![FileOperation::List],
            }],
        };
        let json = serde_json::to_value(&peer).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "ip": "192.168.1.5",
                "port": 8899,
                "token": "",
                "deviceName": "phone",
                "mounts": [{
                    "pluginId": "com.bedcode.x",
                    "mountPath": "shared",
                    "operations": ["list"]
                }]
            })
        );
        // 缺省字段（token/deviceName/mounts）解析为默认值
        let minimal = serde_json::json!({ "ip": "10.0.0.1", "port": 8899 });
        let back: PeerFileService = serde_json::from_value(minimal).unwrap();
        assert_eq!(back.token, "");
        assert_eq!(back.device_name, "");
        assert!(back.mounts.is_empty());
    }
}
