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
    /// WASM 模块内容 SHA-256（小写十六进制；缺省空串 = 不校验）
    ///
    /// 由**发布者**在打包时填入：宿主 `downloader` 安装期比对 zip 内 wasm 文件的
    /// 摘要，拦截「包内二进制与 manifest 声明不符」的替换（与移动端同形）。
    /// 缺省空串时跳过校验（既有插件零迁移）——但**空串不等于安全**：
    /// 插件内容仍受审批门禁的目录哈希钉扎约束（`plugin/security/approval.rs`）。
    #[serde(default)]
    pub wasm_hash: String,
    /// 插件图标：图片路径（相对插件目录）或内联 SVG 标记
    #[serde(default)]
    pub icon: Option<String>,
    /// WASI 预打开目录声明（wasm32-wasip2 插件 std::fs 直连文件访问）
    ///
    /// 宿主在实例化时逐项校验授权（is_granted，无弹窗）后挂载到 guest
    /// 路径 `/data`、`/data1`、…；未授权/展开失败的目录跳过（不阻断加载）。
    /// 支持 `${home}` 变量展开为主目录绝对路径。缺省空数组 = 无预打开
    /// （既有 wasm32-unknown-unknown 插件不受影响）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wasi_preopen_dirs: Vec<String>,
    /// 组件类型：`system`（系统组件）/ `application`（应用插件）
    ///
    /// 系统组件：内置、默认启用、只停不删、先于应用插件激活，其导出
    /// 的 host-* 同形接口注册进能力注册表作为能力提供者（host-side
    /// 转发装配，见 core-plugin-manager）。缺省 `application`，旧插件
    /// 零迁移。注意与 `pluginType`（产物形态 rust/rust-ts/ts-only）
    /// 正交——本字段描述装配角色。
    #[serde(rename = "type", default, skip_serializing_if = "PluginKind::is_application")]
    pub kind: PluginKind,
    /// 能力依赖声明（应用插件消费的能力名，WIT host-* 接口名，如
    /// `host-storage`）
    ///
    /// 宿主在激活时校验：每个依赖必须已有提供者（宿主原语或已激活的
    /// 系统组件实例），缺失即激活失败并指明能力名。缺省空数组 =
    /// 无依赖（现有插件不受影响）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    /// 单插件 Store 资源覆盖请求（core-config × core-security，wasm-core 票据 07）
    ///
    /// 重型插件（大 JSON 解析等）可请求更大的燃料预算/线性内存；`None`
    /// 字段继承内核配置。最终值由宿主安全模块仲裁：逐字段取 min（请求值、
    /// 内核配置值、编译期硬上限）——插件只能自我收紧，放宽请求被钳回上限。
    /// 缺省 None = 完全继承内核配置（现有插件零迁移）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_overrides: Option<ResourceOverrides>,
}

/// 单插件 Store 资源上限覆盖请求（manifest `resourceOverrides`）
///
/// 字段可选：`None` = 继承内核配置（`CoreConfig.store` 对应项）；
/// `Some(v)` = 请求值，宿主仲裁后生效（见 [`PluginManifest::resource_overrides`]）。
/// 字段语义与 [`crate`] 宿主侧 `StoreLimits` 同名项一致（燃料预算 / 线性
/// 内存字节 / 表元素 / 实例数 / 内存数 / 表数）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ResourceOverrides {
    /// 单次导出调用燃料预算（指令数）；None = 继承
    pub fuel_per_call: Option<u64>,
    /// 线性内存上限（字节）；None = 继承
    pub max_memory_bytes: Option<usize>,
    /// 表元素上限；None = 继承
    pub max_table_entries: Option<usize>,
    /// 单 Store 核心实例数上限；None = 继承
    pub max_instances: Option<usize>,
    /// 单 Store 线性内存数量上限；None = 继承
    pub max_memories: Option<usize>,
    /// 单 Store 表数量上限；None = 继承
    pub max_tables: Option<usize>,
}

fn default_sandbox() -> String {
    "inline".to_string()
}

fn default_plugin_type() -> PluginType {
    PluginType::TsOnly
}

/// 插件产物形态类型
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

/// 组件装配角色（manifest `type` 字段，core-plugin-manager）
///
/// 与 [`PluginType`]（产物形态）正交：本枚举描述插件在能力装配中的角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PluginKind {
    /// 应用插件（缺省）：消费能力，经 `dependencies` 声明依赖
    #[default]
    Application,
    /// 系统组件：内置、默认启用、只停不删、先于应用插件激活，
    /// 向能力注册表提供 host-* 同形接口能力
    System,
}

impl PluginKind {
    /// serde skip_serializing_if 钩子：缺省角色不写入序列化输出
    pub fn is_application(&self) -> bool {
        matches!(self, PluginKind::Application)
    }
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
    /// 插件 HTTP 端点清单（`_http_endpoint` 的路径白名单，票 16）
    ///
    /// 条目是**相对路径段**（不含 `/api/plugin/<插件 id>/` 前缀，前缀由宿主补），
    /// 与请求里 `path` 字段逐字一致。宿主 registry 据此生成完整路径登记；路由侧
    /// 「已声明 → 精确匹配、未声明 → 前缀内 ANY 放行」是票据 03 的过渡策略，
    /// 本字段让新插件第一次能走「声明命中」那一轨（审计面：清单即端点真源）。
    #[serde(default)]
    pub http_endpoints: Vec<String>,
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
    /// 静态标题；statusbar 项由运行时注册动态 label，声明中可缺省
    #[serde(default)]
    pub title: String,
    /// 视图组件名；statusbar 项无独立视图组件（点击行为运行时注册），声明中可缺省
    #[serde(default)]
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
    /// 激活进行中（auto-activation / 手动激活期间的瞬时中间态，列表查询可见）
    Activating,
    Activated,
    /// 激活成功但启动初始化失败（v8 契约）：WASM 实例可用、扩展点已注册，
    /// 但插件内部启动流程未完成。可重试激活回到 Activated
    Degraded(String),
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

    #[test]
    fn test_manifest_parse_system_component_kind_and_dependencies() {
        // core-plugin-manager：系统组件 manifest 声明 type=system + 能力依赖
        let json = serde_json::json!({
            "id": "com.bedcode.sys-store",
            "name": "Sys Store",
            "version": "1.0.0",
            "type": "system",
            "dependencies": ["host-storage", "host-log"]
        });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        assert_eq!(m.kind, PluginKind::System);
        assert_eq!(m.dependencies, vec!["host-storage", "host-log"]);
        // 序列化回写：system 角色与依赖保留
        let back = serde_json::to_value(&m).unwrap();
        assert_eq!(back["type"], serde_json::json!("system"));
        assert_eq!(back["dependencies"], serde_json::json!(["host-storage", "host-log"]));
    }

    #[test]
    fn test_manifest_kind_and_dependencies_default_for_legacy() {
        // 缺省兼容：旧插件 manifest 无 type/dependencies 字段 → application + 空依赖，
        // 序列化回写时缺省值不落盘（skip_serializing_if）
        let json = serde_json::json!({ "id": "com.bedcode.legacy", "name": "L", "version": "0.1.0" });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        assert_eq!(m.kind, PluginKind::Application);
        assert!(m.dependencies.is_empty());
        let back = serde_json::to_value(&m).unwrap();
        assert!(back.get("type").is_none(), "application 缺省角色不序列化");
        assert!(back.get("dependencies").is_none(), "空依赖不序列化");
    }

    // ==================== PluginType / PluginState ====================

    /// core-config × core-security（票据 07）：manifest `resourceOverrides`
    /// 解析 + 未声明字段为 None（继承内核配置）+ 序列化回写
    #[test]
    fn test_manifest_parse_resource_overrides() {
        let json = serde_json::json!({
            "id": "com.bedcode.heavy",
            "name": "Heavy",
            "version": "1.0.0",
            "resourceOverrides": {
                "fuelPerCall": 128_000_000_000u64,
                "maxMemoryBytes": 512 * 1024 * 1024
            }
        });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        let overrides = m.resource_overrides.expect("resourceOverrides must be parsed");
        assert_eq!(overrides.fuel_per_call, Some(128_000_000_000));
        assert_eq!(overrides.max_memory_bytes, Some(512 * 1024 * 1024));
        // 未声明字段为 None → 宿主侧继承内核配置
        assert_eq!(overrides.max_table_entries, None);
        // 序列化回写：camelCase 键名保留
        let back = serde_json::to_value(&m).unwrap();
        assert_eq!(back["resourceOverrides"]["fuelPerCall"], serde_json::json!(128_000_000_000u64));
        assert_eq!(
            back["resourceOverrides"]["maxMemoryBytes"],
            serde_json::json!(512 * 1024 * 1024)
        );
    }

    /// 票据 07 缺省兼容：旧 manifest 无 resourceOverrides → None，且序列化不落盘
    #[test]
    fn test_manifest_resource_overrides_default_none_for_legacy() {
        let json = serde_json::json!({
            "id": "com.bedcode.legacy",
            "name": "Legacy",
            "version": "1.0.0"
        });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        assert!(
            m.resource_overrides.is_none(),
            "旧 manifest 无该字段 → None（继承内核配置，零迁移）"
        );
        let back = serde_json::to_value(&m).unwrap();
        assert!(
            back.get("resourceOverrides").is_none(),
            "缺省值不得写入序列化输出: {back}"
        );
    }

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

    #[test]
    fn test_plugin_state_activating_and_degraded() {
        // v8 契约新增变体：Activating 瞬时中间态 + Degraded 携带降级原因，
        // serde 形状与 Error 一致（tag=state / content=error）
        assert_eq!(
            serde_json::to_value(PluginState::Activating).unwrap(),
            serde_json::json!({ "state": "Activating" })
        );
        assert_eq!(
            serde_json::to_value(PluginState::Degraded("init failed".into())).unwrap(),
            serde_json::json!({ "state": "Degraded", "error": "init failed" })
        );
        let back: PluginState = serde_json::from_value(
            serde_json::json!({ "state": "Degraded", "error": "hooks install failed" }),
        )
        .unwrap();
        assert_eq!(back, PluginState::Degraded("hooks install failed".into()));
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
    fn test_view_contribution_statusbar_optional_fields() {
        // statusbar 项由 manifest-gen 扫描 registerStatusBarItem 生成，无静态 title/component；
        // 反序列化必须放行，否则整个插件加载失败（missing field `title`）
        let v: ViewContribution =
            serde_json::from_value(serde_json::json!({ "id": "v1", "type": "statusbar" }))
                .unwrap();
        assert_eq!(v.id, "v1");
        assert_eq!(v.view_type, "statusbar");
        assert_eq!(v.title, "");
        assert_eq!(v.component, "");
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
        // 全量贡献点解析：terminal/toolProviders/httpEndpoints/fileHandlers/
        // configuration/lifecycle
        let json = serde_json::json!({
            "commands": [{ "id": "c1", "title": "C1" }],
            "views": [{ "id": "v1", "type": "toolbox", "title": "V1", "component": "C" }],
            "terminal": {
                "inputHandlers": ["in1"],
                "outputParsers": ["out1"]
            },
            "toolProviders": [{ "id": "tp1", "name": "N", "endpoint": "http://x" }],
            "httpEndpoints": ["task-status", "task-queue/add"],
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
        assert_eq!(c.http_endpoints, vec!["task-status", "task-queue/add"]);
        assert_eq!(c.file_handlers[0].extensions, vec!["md"]);
        assert_eq!(c.configuration.as_ref().unwrap().properties.len(), 1);
        assert!(c.lifecycle.as_ref().unwrap().on_startup);
        assert!(!c.lifecycle.as_ref().unwrap().on_shutdown);
        assert_eq!(c.provides, vec!["topic:a"]);
    }

    /// `httpEndpoints` 缺省即空清单 = 未声明（宿主路由保持票据 03 的前缀 ANY 放行轨，
    /// 既有插件零迁移）。缺省语义被固化在此，避免后续把它改成 Option 引发歧义。
    #[test]
    fn test_http_endpoints_default_is_undeclared() {
        let c: PluginContributes = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(
            c.http_endpoints.is_empty(),
            "未声明 httpEndpoints 必须解析为空清单（未声明判据）"
        );
    }

}
