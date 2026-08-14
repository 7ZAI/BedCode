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
    /// 插件请求的权限尚未获得用户批准（需在插件管理页人工审批后才能激活）
    NeedsApproval,
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
/// fail-closed：任何异常（超时/解析失败/插件未实现）宿主一律视为拒绝。
/// v2 三路化：`allow` / `ask`（请求用户批准，批上下文）/ `deny`。
/// wire 兼容：旧插件返回 `{ allow: false }` → deny；`{ allow: true }` → allow。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadHookDecision {
    /// 是否允许上传
    pub allow: bool,
    /// v2：true = 需要用户批准（批上下文）；与 allow 互斥（ask 时 allow 必为 false）
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

    /// v2：请求用户批准（批上下文，宿主建 pending 批并等待应答）
    pub fn ask() -> Self {
        Self { allow: false, ask: true, reason: None }
    }
}

impl Default for UploadHookDecision {
    /// 默认拒绝（fail-closed）
    fn default() -> Self {
        Self::deny("no decision")
    }
}

/// 批量传输请求元信息（宿主 → 插件批钩子入参，camelCase 线协议）
///
/// v2：POST /transfer-request 时宿主调用一次批钩子（`on_transfer_request`），
/// 载荷为整批文件清单 + 总大小，插件按接收策略三路分流。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequestMeta {
    /// 批 ID（发送方生成，跨端唯一标识一次「发送」动作）
    pub batch_id: String,
    /// 批内文件清单（相对路径 + 大小）
    pub files: Vec<UploadRequestMeta>,
    /// 批内文件总大小（字节）
    pub total_size: u64,
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

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== PluginManifest ====================

    #[test]
    fn test_manifest_parse_with_defaults() {
        // 缺省字段（description/author/main/pluginType/permissions/contributes/
        // icon/wasmHash/rustLibrary）全部走 default，宿主加载最小化 plugin.json 不应失败
        let json = serde_json::json!({
            "id": "com.bedcode.demo",
            "name": "Demo",
            "version": "0.1.0",
            "permissions": ["storage", "terminal:input"]
        });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        assert_eq!(m.id, "com.bedcode.demo");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.description, "");
        assert_eq!(m.author, "");
        assert_eq!(m.main, "");
        assert_eq!(m.plugin_type, PluginType::TsOnly);
        assert_eq!(m.permissions, vec!["storage", "terminal:input"]);
        assert_eq!(m.icon, None);
        assert_eq!(m.wasm_hash, "");
        assert_eq!(m.rust_library, "");
    }

    #[test]
    fn test_manifest_round_trip_fills_defaults() {
        // 宿主加载最小化 plugin.json 后序列化回写：缺省字段应已填充默认值
        let json = serde_json::json!({ "id": "com.bedcode.x", "name": "X", "version": "1.0.0" });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        let back = serde_json::to_value(&m).unwrap();
        assert_eq!(back["pluginType"], serde_json::json!("ts-only"));
        assert_eq!(back["wasmHash"], serde_json::json!(""));
        // contributes 序列化时带全部字段（serde(default) 只影响反序列化）
        assert_eq!(back["contributes"]["commands"], serde_json::json!([]));
        assert_eq!(back["contributes"]["navTab"], serde_json::Value::Null);
        assert_eq!(back["contributes"]["settings"], serde_json::Value::Null);
    }

    // ==================== PluginType / PluginState ====================

    #[test]
    fn test_plugin_type_kebab_case() {
        // 线协议 kebab-case：宿主按字面量解析 plugin.json 的 pluginType 字段；
        // 移动端比桌面端多 Wasm 变体
        assert_eq!(serde_json::to_value(PluginType::Rust).unwrap(), serde_json::json!("rust"));
        assert_eq!(serde_json::to_value(PluginType::RustTs).unwrap(), serde_json::json!("rust-ts"));
        assert_eq!(serde_json::to_value(PluginType::TsOnly).unwrap(), serde_json::json!("ts-only"));
        assert_eq!(serde_json::to_value(PluginType::Wasm).unwrap(), serde_json::json!("wasm"));
        assert_eq!(
            serde_json::from_value::<PluginType>(serde_json::json!("wasm")).unwrap(),
            PluginType::Wasm
        );
        assert!(serde_json::from_value::<PluginType>(serde_json::json!("rust_ts")).is_err());
    }

    #[test]
    fn test_plugin_type_default() {
        assert_eq!(PluginType::default(), PluginType::TsOnly);
    }

    #[test]
    fn test_plugin_state_camel_case_tag() {
        // state 内部标签 + camelCase 变体名（与桌面端 PascalCase 不同，移动端线协议如此）
        assert_eq!(
            serde_json::to_value(PluginState::Loaded).unwrap(),
            serde_json::json!({ "state": "loaded" })
        );
        assert_eq!(
            serde_json::to_value(PluginState::Activated).unwrap(),
            serde_json::json!({ "state": "activated" })
        );
        assert_eq!(
            serde_json::to_value(PluginState::Error { error: "boom".into() }).unwrap(),
            serde_json::json!({ "state": "error", "error": "boom" })
        );
        let back: PluginState =
            serde_json::from_value(serde_json::json!({ "state": "error", "error": "x" })).unwrap();
        assert_eq!(back, PluginState::Error { error: "x".into() });
    }

    #[test]
    fn test_plugin_state_default() {
        assert_eq!(PluginState::default(), PluginState::Loaded);
    }

    // ==================== 移动端特有扩展点 ====================

    #[test]
    fn test_nav_tab_contribution_wire_format() {
        // 底部导航 Tab（移动端特有）：order 缺省为 0
        let t = NavTabContribution {
            id: "tab1".into(),
            title: "Tasks".into(),
            icon: "tasks.svg".into(),
            component: "TasksView".into(),
            order: 2,
        };
        assert_eq!(
            serde_json::to_value(&t).unwrap(),
            serde_json::json!({
                "id": "tab1",
                "title": "Tasks",
                "icon": "tasks.svg",
                "component": "TasksView",
                "order": 2
            })
        );
        let minimal =
            serde_json::json!({ "id": "t", "title": "T", "icon": "i", "component": "C" });
        let back: NavTabContribution = serde_json::from_value(minimal).unwrap();
        assert_eq!(back.order, 0);
    }

    #[test]
    fn test_settings_contribution_wire_format() {
        let s = SettingsContribution {
            section: "network".into(),
            component: "NetSettings".into(),
        };
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            serde_json::json!({ "section": "network", "component": "NetSettings" })
        );
    }

    #[test]
    fn test_view_contribution_type_field() {
        // view_type 序列化为 "type"（与前端 vscode 风格扩展点一致）
        let v = ViewContribution {
            id: "v1".into(),
            view_type: "toolbox".into(),
            title: "Toolbox".into(),
            component: "Panel".into(),
        };
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            serde_json::json!({
                "id": "v1",
                "type": "toolbox",
                "title": "Toolbox",
                "component": "Panel"
            })
        );
    }

    #[test]
    fn test_terminal_contribution_toolbar_items() {
        // 终端扩展点含工具栏按钮（ui:input 权限对应）
        let t = TerminalContribution {
            input_handlers: vec!["in1".into()],
            output_parsers: vec!["out1".into()],
            toolbar_items: vec![TerminalToolbarItemContribution {
                id: "tb1".into(),
                title: "Send".into(),
                icon: "send.svg".into(),
            }],
        };
        assert_eq!(
            serde_json::to_value(&t).unwrap(),
            serde_json::json!({
                "inputHandlers": ["in1"],
                "outputParsers": ["out1"],
                "toolbarItems": [{ "id": "tb1", "title": "Send", "icon": "send.svg" }]
            })
        );
    }

    #[test]
    fn test_contributes_defaults_and_full_parse() {
        // 全量贡献点解析：commands/views/terminal/navTab/settings/configuration/lifecycle
        let json = serde_json::json!({
            "commands": [{ "id": "c1", "title": "C1" }],
            "views": [{ "id": "v1", "type": "toolbox", "title": "V1", "component": "C" }],
            "terminal": { "inputHandlers": ["in1"], "outputParsers": ["out1"] },
            "navTab": { "id": "t1", "title": "T", "icon": "i.svg", "component": "C" },
            "settings": { "section": "net", "component": "S" },
            "configuration": {
                "title": "Config",
                "properties": { "key": { "type": "string", "title": "Key" } }
            },
            "lifecycle": { "onStartup": true, "onAuthSuccess": true }
        });
        let c: PluginContributes = serde_json::from_value(json).unwrap();
        assert_eq!(c.commands[0].id, "c1");
        assert_eq!(c.views[0].view_type, "toolbox");
        assert_eq!(c.terminal.as_ref().unwrap().input_handlers, vec!["in1"]);
        assert_eq!(c.nav_tab.as_ref().unwrap().id, "t1");
        assert_eq!(c.settings.as_ref().unwrap().section, "net");
        assert_eq!(c.configuration.as_ref().unwrap().properties.len(), 1);
        assert!(c.lifecycle.as_ref().unwrap().on_startup);
        assert!(c.lifecycle.as_ref().unwrap().on_auth_success);
        assert!(!c.lifecycle.as_ref().unwrap().on_disconnect);
    }

    #[test]
    fn test_config_property_type_field() {
        // 属性类型字段序列化为 "type"，缺省字段（description/default）为 null
        let p = ConfigProperty {
            prop_type: "string".into(),
            title: "API Key".into(),
            description: None,
            default: None,
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({
                "type": "string",
                "title": "API Key",
                "description": null,
                "default": null
            })
        );
    }

    // ==================== LifecycleContribution ====================

    #[test]
    fn test_lifecycle_is_declared_mapping() {
        // 宿主按 camelCase 事件名查询声明；未声明/未知事件一律 false
        let mut lc = LifecycleContribution::default();
        assert!(!lc.is_declared("onStartup"));
        lc.on_startup = true;
        lc.on_auth_success = true;
        lc.on_session_stopped = true;
        assert!(lc.is_declared("onStartup"));
        assert!(lc.is_declared("onAuthSuccess"));
        assert!(lc.is_declared("onSessionStopped"));
        assert!(!lc.is_declared("onShutdown"));
        assert!(!lc.is_declared("onDisconnect"));
        // 未知事件名拒绝，防止宿主拼写漂移静默通过
        assert!(!lc.is_declared("onPaused"));
        assert!(!lc.is_declared(""));
    }

    #[test]
    fn test_lifecycle_has_any_declared() {
        // 全空默认 = 无任何生命周期钩子声明
        let lc = LifecycleContribution::default();
        assert!(!lc.has_any_declared());
        // 任一钩子置位即视为有声明（宿主据此决定是否注册回调）
        let mut lc2 = LifecycleContribution::default();
        lc2.on_terminal_input = true;
        assert!(lc2.has_any_declared());
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
            base_path: "/com.bedcode.x/shared".into(),
        };
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::json!({
                "mountPath": "shared",
                "basePath": "/com.bedcode.x/shared"
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
        // v2 ask：请求用户批准（与 allow 互斥）
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
        assert_eq!(d.reason.as_deref(), Some("no decision"));
    }

    #[test]
    fn test_upload_hook_decision_wire_format() {
        // allow/deny 时 ask 被跳过（skip_serializing_if）：保持 v1 wire 形状
        // （旧对端/宿主按无 ask 字段解析），ask 时序列化 ask=true
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
        // 反序列化兼容：旧插件返回 { allow: false } → deny；缺省 ask 字段不报错
        let back: UploadHookDecision =
            serde_json::from_value(serde_json::json!({ "allow": true })).unwrap();
        assert!(back.allow);
        assert!(!back.ask);
        assert_eq!(back.reason, None);
        let ask_back: UploadHookDecision =
            serde_json::from_value(serde_json::json!({ "allow": false, "ask": true })).unwrap();
        assert!(ask_back.ask);
    }

    #[test]
    fn test_transfer_request_meta_wire_format() {
        let meta = TransferRequestMeta {
            batch_id: "b1".to_string(),
            files: vec![
                UploadRequestMeta { relative_path: "dir/a.mp4".into(), size: 123456 },
                UploadRequestMeta { relative_path: "b.txt".into(), size: 2 },
            ],
            total_size: 123458,
        };
        assert_eq!(
            serde_json::to_value(&meta).unwrap(),
            serde_json::json!({
                "batchId": "b1",
                "files": [
                    { "relativePath": "dir/a.mp4", "size": 123456 },
                    { "relativePath": "b.txt", "size": 2 }
                ],
                "totalSize": 123458
            })
        );
        let back: TransferRequestMeta =
            serde_json::from_value(serde_json::json!({
                "batchId": "b1",
                "files": [{ "relativePath": "a", "size": 1 }],
                "totalSize": 1
            }))
            .unwrap();
        assert_eq!(back.batch_id, "b1");
        assert_eq!(back.files.len(), 1);
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
            url: "http://peer:8899/com.bedcode.x/shared/file".into(),
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
                "url": "http://peer:8899/com.bedcode.x/shared/file",
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
