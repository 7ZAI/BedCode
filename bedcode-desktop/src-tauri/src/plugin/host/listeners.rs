//! 插件专属的会话生命周期 / 输入监听器
//!
//! 从 `host.rs` 拆出：每个 WASM 插件在 activate 时注册，收到宿主事件后
//! 序列化 payload 调用插件导出函数（SDK 类型化枚举，serde 表示即线协议）。

use bedcode_plugin_api::events::{
    InputSubmittedEvent,
    SessionLifecycleEvent as SdkLifecycleEvent,
};
use serde_json;

use super::PluginHost;
use crate::session::{SessionInputListener, SessionLifecycleEvent, SessionLifecycleListener};

// ==================== PluginLifecycleListener ====================

/// 插件专属的会话生命周期监听器
///
/// 每个 WASM 插件在 activate 时通过 host_session_lifecycle_register 注册。
/// 收到生命周期事件后，将事件序列化为 JSON payload，调用插件的
/// __bedcode_on_session_lifecycle 导出函数。
pub struct PluginLifecycleListener {
    /// 插件 ID
    plugin_id: String,
    /// 插件宿主（Arc 内部，Clone 成本低）
    plugin_host: PluginHost,
}

impl PluginLifecycleListener {
    /// 创建插件生命周期监听器
    pub fn new(plugin_id: String, plugin_host: PluginHost) -> Self {
        Self { plugin_id, plugin_host }
    }

    /// 获取插件 ID
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }
}

impl SessionLifecycleListener for PluginLifecycleListener {
    fn on_session_lifecycle(&self, event: &SessionLifecycleEvent) {
        // 获取插件的 extension_path 作为 resource_dir 注入 payload
        // （剥离 verbatim 前缀，保证插件侧正斜杠拼接可用，见 loader.rs strip_verbatim_prefix）
        let plugins = self.plugin_host.plugins.clone();
        let plugin_id = self.plugin_id.clone();
        let resource_dir = crate::plugin::wasm_runtime::block_on_async(async move {
            let plugins = plugins.read().await;
            plugins
                .get(&plugin_id)
                .map(|p| crate::plugin::loader::strip_verbatim_prefix(&p.extension_path))
                .unwrap_or_default()
        });

        // 宿主事件 → SDK 类型化枚举（与插件侧 on_session_lifecycle 接收的类型一致），
        // 穷尽 match：任一端新增变体时编译失败，强制同步
        use bedcode_plugin_api::events::SessionLifecycleEvent as SdkLifecycleEvent;

        let sdk_event = match event {
            SessionLifecycleEvent::Creating { config_id, command, working_dir, source_device } => {
                SdkLifecycleEvent::Creating {
                    config_id: config_id.clone(),
                    command: command.clone(),
                    working_dir: working_dir.clone(),
                    source_device: source_device.clone(),
                    resource_dir: resource_dir.clone(),
                }
            }
            SessionLifecycleEvent::Created { session_id, config_id, name, working_dir } => {
                SdkLifecycleEvent::Created {
                    session_id: session_id.clone(),
                    config_id: config_id.clone(),
                    name: name.clone(),
                    working_dir: working_dir.clone(),
                    resource_dir: resource_dir.clone(),
                }
            }
            SessionLifecycleEvent::Stopping { session_id, source_device } => {
                SdkLifecycleEvent::Stopping {
                    session_id: session_id.clone(),
                    source_device: source_device.clone(),
                    resource_dir: resource_dir.clone(),
                }
            }
            SessionLifecycleEvent::Stopped { session_id, source_device } => {
                SdkLifecycleEvent::Stopped {
                    session_id: session_id.clone(),
                    source_device: source_device.clone(),
                    resource_dir: resource_dir.clone(),
                }
            }
        };

        // serde 表示即线协议；序列化失败（理论上不可能）退化为空对象，插件侧按协议错误处理
        let payload = serde_json::to_value(&sdk_event).unwrap_or_else(|_| serde_json::json!({}));

        self.plugin_host.dispatch_lifecycle_to_plugin(&self.plugin_id, &payload);
    }

    fn plugin_id(&self) -> Option<&str> {
        Some(&self.plugin_id)
    }
}

// ==================== PluginInputListener ====================

/// 插件专属的提交输入行监听器（见 ADR 0001）
///
/// 每个 WASM 插件在 activate 时通过 host_session_input_register 注册
///（需 `terminal:observe` 权限）。收到提交输入行后，构造 SDK 类型化
/// `InputSubmittedEvent`（serde 表示即线协议），调用插件的
/// `__bedcode_on_input_submitted` 导出函数。
pub struct PluginInputListener {
    /// 插件 ID
    plugin_id: String,
    /// 插件宿主（Arc 内部，Clone 成本低）
    plugin_host: PluginHost,
}

impl PluginInputListener {
    /// 创建插件输入监听器
    pub fn new(plugin_id: String, plugin_host: PluginHost) -> Self {
        Self { plugin_id, plugin_host }
    }
}

impl SessionInputListener for PluginInputListener {
    fn on_input_submitted(&self, session_id: &str, text: &str) {
        tracing::debug!(
            "PluginInputListener on_input_submitted plugin_id={}, session_id={}, text_len={}",
            self.plugin_id,
            session_id,
            text.len()
        );
        // 宿主事件 → SDK 类型化结构体（与插件侧 on_input_submitted 接收的类型一致），
        // serde 表示即线协议；序列化失败（理论上不可能）退化为空对象
        let event = bedcode_plugin_api::events::InputSubmittedEvent {
            session_id: session_id.to_string(),
            text: text.to_string(),
        };
        let payload = serde_json::to_value(&event).unwrap_or_else(|_| serde_json::json!({}));

        self.plugin_host.dispatch_input_to_plugin(&self.plugin_id, &payload);
    }

    fn plugin_id(&self) -> Option<&str> {
        Some(&self.plugin_id)
    }
}

