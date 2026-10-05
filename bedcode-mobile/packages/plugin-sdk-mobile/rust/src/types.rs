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
    /// 激活进行中（auto-activation / 手动激活期间），列表可见的中间态
    Activating,
    Activated,
    /// activate 成功但 on_startup 失败：实例可用、启动初始化未完成
    Degraded { error: String },
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
    /// 启用前预授权目录（移动端镜像桌面 wasiPreopenDirs）
    ///
    /// 宿主在 `preauthorize_plugin` 收集并展开后统一弹窗，避免插件 WASM
    /// activate 内部 `fs_request_auth` 弹窗晚于前端 loading 遮罩（授权弹窗与
    /// loading 同现的根因）。支持 `${downloads}` 模板（展开为宿主 app 下载目录，
    /// 如 `{AppDownloadsDir}/ai-chatbox`）。
    #[serde(default)]
    pub preauth_dirs: Vec<String>,
    /// 外网访问预授权 URL 声明（Egress L2，glob：`[scheme://][*.]host[:port][/path-prefix]`）
    ///
    /// 宿主在插件加载时收集进 Egress Policy 静态声明层，声明命中直接放行；
    /// 未声明的外网 URL（如用户自定义 baseUrl）走 L3 授权弹窗（fail-closed）。
    /// 仿 `preauthDirs` 先例。
    #[serde(default)]
    pub preauth_urls: Vec<String>,
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

// ==================== Tests ====================

// 单元测试已全部迁至 `../tests/`（集成测试，只经 crate 公开 API 验证契约）。
