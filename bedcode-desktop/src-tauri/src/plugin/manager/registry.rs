//! Plugin Registry
//!
//! 扩展点注册表 — 管理 commands/views/terminal/http/file_handlers 的注册与查询
//! 前端 PluginContext 的注册调用通过 Tauri invoke 到达此注册表

use bedcode_plugin_api::{
    CommandContribution, EndpointAuth, FileHandlerContribution, HttpEndpointContribution, ToolProviderContribution,
    ViewContribution,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 已注册的命令条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEntry {
    pub plugin_id: String,
    pub command_id: String,
    pub title: String,
    pub icon: Option<String>,
}

/// 已注册的视图条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewEntry {
    pub plugin_id: String,
    pub view_id: String,
    pub view_type: String,
    pub title: String,
    pub component: String,
}

/// 已注册的终端处理器
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalHandlers {
    pub plugin_id: String,
    pub input_handlers: Vec<String>,
    pub output_parsers: Vec<String>,
}

/// 已注册的 HTTP 端点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpEndpointEntry {
    pub plugin_id: String,
    pub path: String,
    /// 认证档位（票 08）：路由侧转发前按它决定是否要求宿主已验签
    ///
    /// 缺省即 [`EndpointAuth::Jwt`]（manifest 未声明 `auth` 的条目），免凭证必须
    /// 由插件逐条显式声明 `auth: "none"`。
    pub auth: EndpointAuth,
}

/// 已注册的文件处理器
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHandlerEntry {
    pub plugin_id: String,
    pub handler_id: String,
    pub extensions: Vec<String>,
    pub viewer: String,
    pub icon: Option<String>,
}

/// 扩展点注册表
pub struct PluginRegistry {
    commands: Arc<RwLock<HashMap<String, CommandEntry>>>,
    views: Arc<RwLock<HashMap<String, ViewEntry>>>,
    terminal_handlers: Arc<RwLock<HashMap<String, TerminalHandlers>>>,
    http_endpoints: Arc<RwLock<HashMap<String, HttpEndpointEntry>>>,
    file_handlers: Arc<RwLock<HashMap<String, FileHandlerEntry>>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            commands: Arc::new(RwLock::new(HashMap::new())),
            views: Arc::new(RwLock::new(HashMap::new())),
            terminal_handlers: Arc::new(RwLock::new(HashMap::new())),
            http_endpoints: Arc::new(RwLock::new(HashMap::new())),
            file_handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // ==================== Commands ====================

    /// 注册命令（从 manifest contributes.commands 批量注册）
    pub async fn register_commands(&self, plugin_id: &str, commands: &[CommandContribution]) {
        let mut map = self.commands.write().await;
        for cmd in commands {
            let entry = CommandEntry {
                plugin_id: plugin_id.to_string(),
                command_id: cmd.id.clone(),
                title: cmd.title.clone(),
                icon: cmd.icon.clone(),
            };
            map.insert(cmd.id.clone(), entry);
        }
    }

    /// 获取所有已注册的命令
    pub async fn list_commands(&self) -> Vec<CommandEntry> {
        self.commands.read().await.values().cloned().collect()
    }

    /// 获取指定插件的命令
    pub async fn get_plugin_commands(&self, plugin_id: &str) -> Vec<CommandEntry> {
        self.commands
            .read()
            .await
            .values()
            .filter(|e| e.plugin_id == plugin_id)
            .cloned()
            .collect()
    }

    // ==================== Views ====================

    /// 注册视图（从 manifest contributes.views 批量注册）
    pub async fn register_views(&self, plugin_id: &str, views: &[ViewContribution]) {
        let mut map = self.views.write().await;
        for view in views {
            let entry = ViewEntry {
                plugin_id: plugin_id.to_string(),
                view_id: view.id.clone(),
                view_type: view.view_type.clone(),
                title: view.title.clone(),
                component: view.component.clone(),
            };
            map.insert(view.id.clone(), entry);
        }
    }

    /// 获取指定类型的视图（sidebar/toolbox/statusbar）
    pub async fn get_views_by_type(&self, view_type: &str) -> Vec<ViewEntry> {
        self.views
            .read()
            .await
            .values()
            .filter(|e| e.view_type == view_type)
            .cloned()
            .collect()
    }

    /// 获取所有已注册的视图
    pub async fn list_views(&self) -> Vec<ViewEntry> {
        self.views.read().await.values().cloned().collect()
    }

    // ==================== Terminal ====================

    /// 注册终端处理器
    pub async fn register_terminal_handlers(
        &self,
        plugin_id: &str,
        input_handlers: &[String],
        output_parsers: &[String],
    ) {
        let mut map = self.terminal_handlers.write().await;
        map.insert(
            plugin_id.to_string(),
            TerminalHandlers {
                plugin_id: plugin_id.to_string(),
                input_handlers: input_handlers.to_vec(),
                output_parsers: output_parsers.to_vec(),
            },
        );
    }

    /// 获取所有终端处理器
    pub async fn list_terminal_handlers(&self) -> Vec<TerminalHandlers> {
        self.terminal_handlers.read().await.values().cloned().collect()
    }

    // ==================== HTTP Endpoints ====================

    /// 注册 HTTP 端点（票据 03 接线治理面：路径冲突检测 + 票 08 认证档位）
    ///
    /// 同路径被其他插件占用 → Err（携带占用者）；同插件重复注册 → Ok（幂等，
    /// reload/重复 activate 不产生歧义）。注册表此前只登记不仲裁（死代码），
    /// 路由 `plugin_http_endpoint` 查表后按声明匹配。
    pub async fn register_http_endpoint(&self, plugin_id: &str, path: &str, auth: EndpointAuth) -> Result<(), String> {
        let mut map = self.http_endpoints.write().await;
        if let Some(existing) = map.get(path) {
            if existing.plugin_id != plugin_id {
                return Err(format!(
                    "http endpoint path '{}' is already registered by plugin '{}'",
                    path, existing.plugin_id
                ));
            }
            return Ok(()); // 同插件重复注册幂等
        }
        map.insert(
            path.to_string(),
            HttpEndpointEntry {
                plugin_id: plugin_id.to_string(),
                path: path.to_string(),
                auth,
            },
        );
        Ok(())
    }

    /// 查找注册的 HTTP 端点（路由侧据此取属主与认证档位）
    pub async fn find_http_endpoint(&self, path: &str) -> Option<HttpEndpointEntry> {
        self.http_endpoints.read().await.get(path).cloned()
    }

    /// 列出指定插件声明的 HTTP 端点完整路径（空 = 未声明）
    ///
    /// 路由侧的判据是「**只认声明**」：未声明的路径一律 404，未声明清单等于没有
    /// HTTP 面（票 08 起，票据 03 的「未声明 → 前缀内 ANY 放行」过渡策略退役）。
    /// 认证档位不在本函数返回面上——按全路径 [`Self::find_http_endpoint`] 取。
    pub async fn list_http_endpoint_paths(&self, plugin_id: &str) -> Vec<String> {
        self.http_endpoints
            .read()
            .await
            .values()
            .filter(|e| e.plugin_id == plugin_id)
            .map(|e| e.path.clone())
            .collect()
    }

    /// 注册外部工具端点（从 manifest contributes.toolProviders）
    ///
    /// 声明冲突（同路径被其他插件占用）记 warn! 而非致命——toolProviders 是 manifest
    /// 声明面，冲突时该声明不生效，插件本身仍可激活（票据 03）。
    ///
    /// 档位取缺省 [`EndpointAuth::Jwt`]：toolProviders 的 `endpoint` 是外部 URL 而非
    /// 路径段，宿主没有免凭证派发点（`find_http_endpoint` 无生产调用者），保持最严档
    /// 只为「将来有人接这条面」留一个不会默认敞开的形状。
    pub async fn register_tool_providers(&self, plugin_id: &str, providers: &[ToolProviderContribution]) {
        for provider in providers {
            self.register_declared_http_endpoint(plugin_id, &provider.endpoint, None, "tool provider")
                .await;
        }
    }

    /// 注册插件 HTTP 端点清单（从 manifest contributes.httpEndpoints，票 16 / 票 08）
    ///
    /// 与 toolProviders 同一登记面（都进 `http_endpoints` 表，因此跨插件路径冲突照样
    /// 仲裁），区别只在语义：`httpEndpoints` 是 `_http_endpoint` 的**路径白名单声明**，
    /// 不带工具提供者的产品含义，因此不会在插件管理页被读成「工具提供者」。
    /// 声明了清单的插件在路由侧走「精确匹配、未注册路径 404」（见
    /// `plugin_controller::plugin_http_path_allowed`）；空清单 = 未声明 = 没有 HTTP 面。
    ///
    /// 每条自带认证档位：`{path, auth}` 显式声明，纯 `path` 条目落最严缺省档（票 08
    /// 裁决 1）。`auth` 取值非法（`none|jwt` 之外）→ 该条**不登记** + warn 留痕：
    /// 宁可使该端点不可达（404，插件作者立刻看到），也不把它静默放宽成免凭证。
    pub async fn register_http_endpoints(&self, plugin_id: &str, endpoints: &[HttpEndpointContribution]) {
        for endpoint in endpoints {
            self.register_declared_http_endpoint(plugin_id, endpoint.path(), endpoint.auth_raw(), "http endpoint")
                .await;
        }
    }

    /// manifest 声明面 → 注册表条目（toolProviders 与 httpEndpoints 共用）
    ///
    /// 空段（缺省/纯空白）不登记：登记出来即 `/api/plugin/<id>/` 本身，会把插件前缀
    /// 当成端点匹配上。冲突与非法档位记 warn! 不致命——声明面出问题时该条不生效，
    /// 插件仍可激活。
    async fn register_declared_http_endpoint(
        &self,
        plugin_id: &str,
        endpoint: &str,
        auth_raw: Option<&str>,
        kind: &str,
    ) {
        let endpoint = endpoint.trim().trim_start_matches('/');
        if endpoint.is_empty() {
            tracing::warn!(
                plugin_id = %plugin_id,
                "manifest declared {} path is empty, entry skipped",
                kind
            );
            return;
        }
        // HTTP 声明面的缺省档 = jwt（票 08 裁决 1「未声明即最严」）
        let auth = match EndpointAuth::parse_with(auth_raw, EndpointAuth::Jwt) {
            Ok(auth) => auth,
            Err(e) => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    declared = %endpoint,
                    error = %e,
                    "manifest declared {} has invalid auth, endpoint not registered (unreachable by design)",
                    kind
                );
                return;
            }
        };
        let full_path = format!("/api/plugin/{}/{}", plugin_id, endpoint);
        if let Err(e) = self.register_http_endpoint(plugin_id, &full_path, auth).await {
            tracing::warn!(
                plugin_id = %plugin_id,
                path = %full_path,
                error = %e,
                "{} registration conflict",
                kind
            );
        }
    }

    // ==================== File Handlers ====================

    /// 注册文件处理器（从 manifest contributes.fileHandlers 批量注册）
    pub async fn register_file_handlers(&self, plugin_id: &str, handlers: &[FileHandlerContribution]) {
        let mut map = self.file_handlers.write().await;
        for handler in handlers {
            let entry = FileHandlerEntry {
                plugin_id: plugin_id.to_string(),
                handler_id: handler.id.clone(),
                extensions: handler.extensions.clone(),
                viewer: handler.viewer.clone(),
                icon: handler.icon.clone(),
            };
            map.insert(handler.id.clone(), entry);
        }
    }

    /// 根据文件扩展名查找匹配的处理器
    pub async fn find_file_handler(&self, extension: &str) -> Option<FileHandlerEntry> {
        let map = self.file_handlers.read().await;
        for entry in map.values() {
            if entry.extensions.iter().any(|e| e == extension) {
                return Some(entry.clone());
            }
        }
        None
    }

    /// 获取所有已注册的文件处理器
    pub async fn list_file_handlers(&self) -> Vec<FileHandlerEntry> {
        self.file_handlers.read().await.values().cloned().collect()
    }

    // ==================== Cleanup ====================

    /// 移除插件的所有注册（停用时调用）
    pub async fn unregister_plugin(&self, plugin_id: &str) {
        {
            let mut map = self.commands.write().await;
            map.retain(|_, v| v.plugin_id != plugin_id);
        }
        {
            let mut map = self.views.write().await;
            map.retain(|_, v| v.plugin_id != plugin_id);
        }
        {
            let mut map = self.terminal_handlers.write().await;
            map.remove(plugin_id);
        }
        {
            let mut map = self.http_endpoints.write().await;
            map.retain(|_, v| v.plugin_id != plugin_id);
        }
        {
            let mut map = self.file_handlers.write().await;
            map.retain(|_, v| v.plugin_id != plugin_id);
        }
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(id: &str, title: &str, icon: Option<&str>) -> CommandContribution {
        CommandContribution {
            id: id.to_string(),
            title: title.to_string(),
            icon: icon.map(|s| s.to_string()),
        }
    }

    fn view(id: &str, view_type: &str, title: &str) -> ViewContribution {
        ViewContribution {
            id: id.to_string(),
            view_type: view_type.to_string(),
            title: title.to_string(),
            component: format!("{}.vue", id),
        }
    }

    fn file_handler(id: &str, extensions: &[&str], viewer: &str) -> FileHandlerContribution {
        FileHandlerContribution {
            id: id.to_string(),
            extensions: extensions.iter().map(|s| s.to_string()).collect(),
            viewer: viewer.to_string(),
            icon: None,
        }
    }

    fn tool_provider(id: &str, endpoint: &str) -> ToolProviderContribution {
        ToolProviderContribution {
            id: id.to_string(),
            name: id.to_string(),
            endpoint: endpoint.to_string(),
        }
    }

    /// manifest `httpEndpoints` 条目：纯路径形态（未声明认证档位）
    fn ep(path: &str) -> HttpEndpointContribution {
        HttpEndpointContribution::Path(path.to_string())
    }

    /// manifest `httpEndpoints` 条目：`{path, auth}` 形态（显式声明认证档位）
    fn ep_auth(path: &str, auth: &str) -> HttpEndpointContribution {
        HttpEndpointContribution::Declared {
            path: path.to_string(),
            auth: Some(auth.to_string()),
        }
    }

    /// 某全路径登记出来的认证档位（`None` = 该路径未登记，路由侧即 404）
    async fn auth_of(registry: &PluginRegistry, path: &str) -> Option<EndpointAuth> {
        registry.find_http_endpoint(path).await.map(|e| e.auth)
    }

    /// 从列表按 command_id 取条目（HashMap 遍历无序，断言时先定位）
    fn find_command<'a>(entries: &'a [CommandEntry], command_id: &str) -> Option<&'a CommandEntry> {
        entries.iter().find(|e| e.command_id == command_id)
    }

    /// 新建注册表默认状态：所有扩展点均为空，查询不存在条目返回空/None
    #[tokio::test(flavor = "multi_thread")]
    async fn test_new_registry_is_empty() {
        let registry = PluginRegistry::new();
        assert!(registry.list_commands().await.is_empty());
        assert!(registry.get_plugin_commands("any").await.is_empty());
        assert!(registry.list_views().await.is_empty());
        assert!(registry.get_views_by_type("sidebar").await.is_empty());
        assert!(registry.list_terminal_handlers().await.is_empty());
        assert!(registry.find_http_endpoint("/api/x").await.is_none());
        assert!(registry.find_file_handler("md").await.is_none());
        assert!(registry.list_file_handlers().await.is_empty());
    }

    /// 命令批量注册后可全量/按插件查询，字段原样保存
    #[tokio::test(flavor = "multi_thread")]
    async fn test_register_commands_and_query_by_plugin() {
        let registry = PluginRegistry::new();
        registry
            .register_commands(
                "plugin-a",
                &[
                    cmd("cmd-1", "Command One", Some("icon1")),
                    cmd("cmd-2", "Command Two", None),
                ],
            )
            .await;
        registry
            .register_commands("plugin-b", &[cmd("cmd-3", "Command Three", None)])
            .await;

        assert_eq!(registry.list_commands().await.len(), 3);
        let plugin_a_cmds = registry.get_plugin_commands("plugin-a").await;
        assert_eq!(plugin_a_cmds.len(), 2);
        let c1 = find_command(&plugin_a_cmds, "cmd-1").expect("cmd-1 应存在");
        assert_eq!(c1.plugin_id, "plugin-a");
        assert_eq!(c1.title, "Command One");
        assert_eq!(c1.icon.as_deref(), Some("icon1"));
        assert_eq!(registry.get_plugin_commands("no-such-plugin").await.len(), 0);
    }

    /// 相同 command_id 重复注册以最后一次为准（覆盖语义）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_register_commands_same_id_overwrites() {
        let registry = PluginRegistry::new();
        registry
            .register_commands("plugin-a", &[cmd("dup", "First Title", None)])
            .await;
        registry
            .register_commands("plugin-b", &[cmd("dup", "Second Title", None)])
            .await;

        let entries = registry.list_commands().await;
        assert_eq!(entries.len(), 1);
        let dup = find_command(&entries, "dup").expect("dup 应存在");
        assert_eq!(dup.title, "Second Title");
        assert_eq!(dup.plugin_id, "plugin-b");
    }

    /// 视图按 type 过滤与全量列举
    #[tokio::test(flavor = "multi_thread")]
    async fn test_views_get_by_type_and_list() {
        let registry = PluginRegistry::new();
        registry
            .register_views(
                "plugin-a",
                &[
                    view("v-side", "sidebar", "Side View"),
                    view("v-tool", "toolbox", "Tool View"),
                ],
            )
            .await;
        registry
            .register_views("plugin-b", &[view("v-status", "statusbar", "Status View")])
            .await;

        let sidebars = registry.get_views_by_type("sidebar").await;
        assert_eq!(sidebars.len(), 1);
        assert_eq!(sidebars[0].view_id, "v-side");
        assert_eq!(sidebars[0].plugin_id, "plugin-a");
        assert_eq!(sidebars[0].component, "v-side.vue");
        assert!(registry.get_views_by_type("unknown-type").await.is_empty());
        assert_eq!(registry.list_views().await.len(), 3);
    }

    /// 终端处理器按插件 ID 存储，重复注册覆盖旧值
    #[tokio::test(flavor = "multi_thread")]
    async fn test_terminal_handlers_reregister_overwrites() {
        let registry = PluginRegistry::new();
        registry
            .register_terminal_handlers("plugin-a", &["h1".to_string()], &["p1".to_string()])
            .await;
        // 同一插件再次注册 → 覆盖而非追加
        registry
            .register_terminal_handlers("plugin-a", &["h1".to_string(), "h2".to_string()], &[])
            .await;
        registry
            .register_terminal_handlers("plugin-b", &[], &["p9".to_string()])
            .await;

        let all = registry.list_terminal_handlers().await;
        assert_eq!(all.len(), 2);
        let a = all.iter().find(|t| t.plugin_id == "plugin-a").expect("plugin-a 应存在");
        assert_eq!(a.input_handlers, vec!["h1", "h2"]);
        assert!(a.output_parsers.is_empty());
    }

    /// HTTP 端点注册后可按 path 精确查找；未注册的 path 返回 None
    #[tokio::test(flavor = "multi_thread")]
    async fn test_http_endpoint_find_and_missing() {
        let registry = PluginRegistry::new();
        registry
            .register_http_endpoint("plugin-a", "/api/files", EndpointAuth::None)
            .await
            .unwrap();

        let found = registry.find_http_endpoint("/api/files").await.expect("应找到端点");
        assert_eq!(found.plugin_id, "plugin-a");
        assert_eq!(found.path, "/api/files");
        assert_eq!(
            found.auth,
            EndpointAuth::None,
            "登记必须带上档位（路由侧按它判要不要验签）"
        );
        assert!(registry.find_http_endpoint("/api/other").await.is_none());
    }

    /// tool provider 注册路径规范化为 /api/plugin/{pluginId}/{endpoint}（去除前导斜杠）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_tool_providers_register_with_prefixed_path() {
        let registry = PluginRegistry::new();
        // endpoint 带前导斜杠与不带两种写法应归一为同一路径
        registry
            .register_tool_providers("plugin-a", &[tool_provider("tp-1", "/chat")])
            .await;
        registry
            .register_tool_providers("plugin-b", &[tool_provider("tp-2", "mcp")])
            .await;

        let a = registry
            .find_http_endpoint("/api/plugin/plugin-a/chat")
            .await
            .expect("应找到 tool provider 端点");
        assert_eq!(a.plugin_id, "plugin-a");
        let b = registry
            .find_http_endpoint("/api/plugin/plugin-b/mcp")
            .await
            .expect("应找到 tool provider 端点");
        assert_eq!(b.plugin_id, "plugin-b");
        // 未归一化的原始路径不应命中
        assert!(registry.find_http_endpoint("/chat").await.is_none());
    }

    /// 文件处理器按扩展名匹配；不支持的扩展名返回 None
    #[tokio::test(flavor = "multi_thread")]
    async fn test_file_handlers_find_by_extension_and_missing() {
        let registry = PluginRegistry::new();
        registry
            .register_file_handlers(
                "plugin-a",
                &[file_handler("md-viewer", &["md", "markdown"], "MarkdownPreview")],
            )
            .await;
        registry
            .register_file_handlers("plugin-b", &[file_handler("json-viewer", &["json"], "JsonView")])
            .await;

        let md = registry.find_file_handler("md").await.expect("md 应命中");
        assert_eq!(md.plugin_id, "plugin-a");
        assert_eq!(md.handler_id, "md-viewer");
        assert_eq!(md.viewer, "MarkdownPreview");
        assert_eq!(md.extensions, vec!["md", "markdown"]);
        assert_eq!(
            registry
                .find_file_handler("json")
                .await
                .expect("json 应命中")
                .handler_id,
            "json-viewer"
        );
        assert!(registry.find_file_handler("rs").await.is_none());
        assert_eq!(registry.list_file_handlers().await.len(), 2);
    }

    /// unregister_plugin 移除该插件的全部扩展点注册，其他插件不受影响
    #[tokio::test(flavor = "multi_thread")]
    async fn test_unregister_plugin_removes_all_entries() {
        let registry = PluginRegistry::new();
        registry.register_commands("plugin-a", &[cmd("a1", "A1", None)]).await;
        registry.register_commands("plugin-b", &[cmd("b1", "B1", None)]).await;
        registry
            .register_views("plugin-a", &[view("va", "sidebar", "VA")])
            .await;
        registry
            .register_terminal_handlers("plugin-a", &["h".to_string()], &[])
            .await;
        registry
            .register_http_endpoint("plugin-a", "/api/a", EndpointAuth::Jwt)
            .await
            .unwrap();
        registry
            .register_file_handlers("plugin-a", &[file_handler("fa", &["a"], "A")])
            .await;

        registry.unregister_plugin("plugin-a").await;

        assert_eq!(registry.get_plugin_commands("plugin-a").await.len(), 0);
        assert_eq!(registry.get_plugin_commands("plugin-b").await.len(), 1);
        assert!(registry.get_views_by_type("sidebar").await.is_empty());
        assert!(registry.list_terminal_handlers().await.is_empty());
        assert!(registry.find_http_endpoint("/api/a").await.is_none());
        assert!(registry.find_file_handler("a").await.is_none());
        // 未注册过任何内容的插件调用 unregister 不应 panic
        registry.unregister_plugin("never-registered").await;
    }

    /// CommandEntry 序列化为 camelCase（前端 invoke 返回约定）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_command_entry_serializes_camel_case() {
        let registry = PluginRegistry::new();
        registry
            .register_commands("plugin-a", &[cmd("c1", "T", Some("i")), cmd("c2", "T2", None)])
            .await;
        let json = serde_json::to_value(registry.list_commands().await).unwrap();
        let arr = json.as_array().expect("应为数组");
        assert_eq!(arr.len(), 2);
        for entry in arr {
            assert!(entry.get("commandId").is_some(), "字段应为 commandId 驼峰命名");
            assert!(entry.get("pluginId").is_some());
            assert!(entry.get("command_id").is_none());
        }
    }

    // ==================== HTTP 端点注册治理（票据 03） ====================

    /// 路径冲突：同路径被其他插件占用 → Err（携带占用者）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_http_endpoint_conflict_rejected() {
        let registry = PluginRegistry::new();
        registry
            .register_http_endpoint("p1", "/api/plugin/p1/x", EndpointAuth::Jwt)
            .await
            .expect("first registration");
        let err = registry
            .register_http_endpoint("p2", "/api/plugin/p1/x", EndpointAuth::Jwt)
            .await
            .expect_err("same path by another plugin must be rejected");
        assert!(err.contains("p1"), "conflict error should name the owner, got: {}", err);
        // 冲突后仍属首个注册者
        let found = registry.find_http_endpoint("/api/plugin/p1/x").await.expect("kept");
        assert_eq!(found.plugin_id, "p1");
    }

    /// 同插件重复注册路径：幂等 Ok，不产生重复条目
    #[tokio::test(flavor = "multi_thread")]
    async fn test_http_endpoint_same_plugin_repeat_is_idempotent() {
        let registry = PluginRegistry::new();
        registry
            .register_http_endpoint("p1", "/api/plugin/p1/x", EndpointAuth::Jwt)
            .await
            .expect("first");
        registry
            .register_http_endpoint("p1", "/api/plugin/p1/x", EndpointAuth::Jwt)
            .await
            .expect("repeat by same plugin is idempotent");
    }

    /// list_http_endpoint_paths 按插件过滤；unregister 后清空（属主回收）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_http_endpoint_list_and_owner_reclaim() {
        // 直接写 map（模拟 toolProviders 声明路径）
        let registry = PluginRegistry::new();
        registry
            .register_http_endpoint("p1", "/api/plugin/p1/a", EndpointAuth::Jwt)
            .await
            .unwrap();
        registry
            .register_http_endpoint("p1", "/api/plugin/p1/b", EndpointAuth::Jwt)
            .await
            .unwrap();
        registry
            .register_http_endpoint("p2", "/api/plugin/p2/c", EndpointAuth::Jwt)
            .await
            .unwrap();

        let p1 = registry.list_http_endpoint_paths("p1").await;
        assert_eq!(p1.len(), 2);
        assert!(p1.contains(&"/api/plugin/p1/a".to_string()));
        assert!(p1.contains(&"/api/plugin/p1/b".to_string()));
        assert_eq!(registry.list_http_endpoint_paths("no-such").await.len(), 0);

        // 属主回收：unregister 后 p1 路径全部消失
        registry.unregister_plugin("p1").await;
        assert_eq!(registry.list_http_endpoint_paths("p1").await.len(), 0);
        assert_eq!(registry.list_http_endpoint_paths("p2").await.len(), 1);
    }

    /// toolProviders 批量登记：生成 /api/plugin/{id}/{endpoint} 全路径（自家命名空间），
    /// 正常登记不冲突；跨命名空间声明（防御性冲突路径）时 p1 的注册保留
    #[tokio::test(flavor = "multi_thread")]
    async fn test_tool_provider_registration_namespaced() {
        let registry = PluginRegistry::new();
        registry
            .register_tool_providers("p1", &[tool_provider("t1", "/tools/a")])
            .await;
        // p1 的端点登记在自家命名空间
        let found = registry
            .find_http_endpoint("/api/plugin/p1/tools/a")
            .await
            .expect("p1 endpoint registered");
        assert_eq!(found.plugin_id, "p1");
        // 其他插件不共享 p1 的路径（命名空间隔离）
        assert!(registry.find_http_endpoint("/api/plugin/p2/tools/a").await.is_none());
    }

    /// 票 16：`contributes.httpEndpoints` 批量登记——相对段补前缀、带前导斜杠归一、
    /// 空段跳过（登记成 `/api/plugin/<id>/` 就是把前缀本身当端点）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_http_endpoints_declared_list_registered_with_prefix() {
        let registry = PluginRegistry::new();
        registry
            .register_http_endpoints("p1", &[ep("task-status"), ep("/task-queue/add"), ep("   ")])
            .await;

        let paths = registry.list_http_endpoint_paths("p1").await;
        assert_eq!(paths.len(), 2, "空段不得登记, got: {:?}", paths);
        assert!(paths.contains(&"/api/plugin/p1/task-status".to_string()));
        assert!(paths.contains(&"/api/plugin/p1/task-queue/add".to_string()));
        assert!(
            !paths.contains(&"/api/plugin/p1/".to_string()),
            "前缀本身不得成为端点, got: {:?}",
            paths
        );
    }

    /// 票 16：声明面与既有 toolProviders 共用同一张表 —— 同插件两路声明都在清单里
    /// （路由侧只看 `list_http_endpoint_paths` 是否为空，不看它来自哪个声明面）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_http_endpoints_and_tool_providers_share_registry() {
        let registry = PluginRegistry::new();
        registry
            .register_tool_providers("p1", &[tool_provider("tp", "/mcp")])
            .await;
        registry.register_http_endpoints("p1", &[ep("task-status")]).await;

        let paths = registry.list_http_endpoint_paths("p1").await;
        assert_eq!(paths.len(), 2, "两路声明合并可见, got: {:?}", paths);
        assert!(paths.contains(&"/api/plugin/p1/mcp".to_string()));
    }

    /// 票 16：`httpEndpoints` 的相对段按插件各自命名 —— 两个插件声明同名相对段
    /// （如都做 `task-status`）互不占用，因此跨插件不会因端点重名而失去声明。
    /// 真正的属主冲突只可能出现在同一插件的两路声明之间，那一路由
    /// `register_http_endpoint` 仲裁（见 `test_http_endpoint_conflict_rejected`）。
    #[tokio::test(flavor = "multi_thread")]
    async fn test_http_endpoints_are_namespaced_per_plugin() {
        let registry = PluginRegistry::new();
        registry.register_http_endpoints("p1", &[ep("shared")]).await;
        registry.register_http_endpoints("p2", &[ep("shared")]).await;

        assert_eq!(
            registry
                .find_http_endpoint("/api/plugin/p1/shared")
                .await
                .expect("p1 声明生效")
                .plugin_id,
            "p1"
        );
        assert_eq!(
            registry
                .find_http_endpoint("/api/plugin/p2/shared")
                .await
                .expect("同名相对段各自进自家命名空间，不被判冲突")
                .plugin_id,
            "p2"
        );

        // 同插件重复声明幂等（reload / 重复 activate 不产生第二条，也不清空属主）
        registry
            .register_http_endpoints("p1", &[ep("shared"), ep("shared")])
            .await;
        assert_eq!(registry.list_http_endpoint_paths("p1").await.len(), 1);
        assert_eq!(registry.list_http_endpoint_paths("p2").await.len(), 1);
    }

    // ==================== 票 08：HTTP 端点认证档位 ====================

    /// HTTP 声明面的缺省档 = **jwt（未声明即最严）**。它与 WS 注册面的缺省 `none`
    /// 共用同一张档位表却各走各的缺省——缺省由 `EndpointAuth::parse_with` 的参数
    /// 承载，不在词汇表里写死；这条差异是两票裁决的落点，回归即安全边界漂移。
    #[tokio::test(flavor = "multi_thread")]
    async fn test_http_endpoint_undeclared_auth_defaults_to_jwt() {
        let registry = PluginRegistry::new();
        registry.register_http_endpoints("p1", &[ep("task-status")]).await;
        assert_eq!(
            auth_of(&registry, "/api/plugin/p1/task-status").await,
            Some(EndpointAuth::Jwt),
            "纯路径条目必须落最严档，不得沿用 WS 的 none 缺省"
        );
    }

    /// 免凭证必须逐条显式声明 `auth: "none"`；显式 `"jwt"` 与缺省同档
    #[tokio::test(flavor = "multi_thread")]
    async fn test_http_endpoint_declared_auth_tiers_registered() {
        let registry = PluginRegistry::new();
        registry
            .register_http_endpoints(
                "p1",
                &[ep_auth("task-status", "none"), ep_auth("task-queue/add", "jwt")],
            )
            .await;
        assert_eq!(
            auth_of(&registry, "/api/plugin/p1/task-status").await,
            Some(EndpointAuth::None),
            "声明 none 的端点才免凭证"
        );
        assert_eq!(
            auth_of(&registry, "/api/plugin/p1/task-queue/add").await,
            Some(EndpointAuth::Jwt)
        );
    }

    /// 档位词汇外的取值（`local-only` / 空串以外的大小写变体等）→ 该条**不登记**，
    /// 其余条目照常。方向必须是「不可达（404 可见）」而非「静默放宽成 none」。
    #[tokio::test(flavor = "multi_thread")]
    async fn test_http_endpoint_invalid_auth_is_not_registered() {
        let registry = PluginRegistry::new();
        registry
            .register_http_endpoints("p1", &[ep_auth("task-status", "local-only"), ep("session-mode")])
            .await;
        assert_eq!(
            auth_of(&registry, "/api/plugin/p1/task-status").await,
            None,
            "非法档位的条目必须落空（该端点不可达），不得登记成任一档"
        );
        assert_eq!(
            auth_of(&registry, "/api/plugin/p1/session-mode").await,
            Some(EndpointAuth::Jwt)
        );
        assert_eq!(
            registry.list_http_endpoint_paths("p1").await,
            vec!["/api/plugin/p1/session-mode".to_string()],
            "未登记的非法条目不得出现在声明清单里（否则路由侧当成已声明）"
        );
    }

    /// 档位随属主登记：两个插件声明同名相对段时各保各的档，不共享一条记录
    #[tokio::test(flavor = "multi_thread")]
    async fn test_http_endpoint_auth_is_per_owner() {
        let registry = PluginRegistry::new();
        registry
            .register_http_endpoints("p1", &[ep_auth("task-status", "none")])
            .await;
        registry.register_http_endpoints("p2", &[ep("task-status")]).await;
        assert_eq!(
            auth_of(&registry, "/api/plugin/p1/task-status").await,
            Some(EndpointAuth::None)
        );
        assert_eq!(
            auth_of(&registry, "/api/plugin/p2/task-status").await,
            Some(EndpointAuth::Jwt)
        );
    }

    /// toolProviders 声明面没有 auth 字段 → 一律最严缺省档（不为它单独开口子）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_tool_providers_get_strictest_auth() {
        let registry = PluginRegistry::new();
        registry
            .register_tool_providers("p1", &[tool_provider("tp", "/mcp")])
            .await;
        assert_eq!(auth_of(&registry, "/api/plugin/p1/mcp").await, Some(EndpointAuth::Jwt));
    }
}
