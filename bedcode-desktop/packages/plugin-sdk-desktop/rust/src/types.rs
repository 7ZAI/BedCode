//! Plugin Types
//!
//! 插件声明式描述类型、状态枚举 — 从桌面端 types.rs 迁移的共享部分

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 插件描述文件 (plugin.json) 的完整结构
///
/// `Default` 的用途是**测试夹具与合成 manifest 的构造基线**：除 `id` / `name` / `version`
/// 三个必填项外，每个字段都带 `#[serde(default)]`，因此 `Default` 与「一份只写了必填项的
/// plugin.json 解析结果」逐字段等价。调用点写成 `PluginManifest { id, name, version, ..Default::default() }`
/// 之后，本类型**追加可选字段不再连带测试编译红**——此前六处结构体字面量把「逐字段列全」
/// 当隐式契约用，`pty_quota` 追加时就漏修了 `plugin-wasip3-test` 一处（会话引擎下沉票 14）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    ///
    /// 条目两形态（[`WasiPreopenDir`]）：裸路径 = 可写挂载，`{path, readonly}`
    /// = 只读挂载。只读档只收紧 guest 对该目录的写能力，**不放宽授权**——
    /// 未授权目录无论哪一档都建不出 preopen（审计票 07 裁决 3）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wasi_preopen_dirs: Vec<WasiPreopenDir>,
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
    /// 单插件在册 `host-pty` 句柄数上限声明（会话引擎下沉 P1 / H1）
    ///
    /// 缺省 `None` = 取内核默认 `PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN`（8 条，迁移前
    /// 行为）。`Some(n)` 只在 `1..=PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN` 内合法，
    /// 越界与 0 在**加载期**即拒绝（宿主 `manager/validation.rs`），不夹取——静默降级
    /// 会让插件按自己声明的并发数规划业务、实际却少得多，与 `ringBytes` 同一分级口径。
    ///
    /// 存在理由：业务会话改由插件经 `host-pty.spawn` 自持 PTY 后，「用户可开多少终端」
    /// 变成该插件的配额；默认 8 条是「多 shell 并发」型插件的档位，不是会话产品档位。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pty_quota: Option<usize>,
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

/// 缺省产物形态取自 [`default_plugin_type`]，与 manifest `pluginType` 的 serde 缺省共用
/// 同一真源（不另立 `#[default]` 造成两处漂移）
impl Default for PluginType {
    fn default() -> Self {
        default_plugin_type()
    }
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
    /// 插件 HTTP 端点清单（`_http_endpoint` 的路径白名单 + 认证档位，票 16 / 票 08）
    ///
    /// 条目是**相对路径段**（不含 `/api/plugin/<插件 id>/` 前缀，前缀由宿主补），
    /// 与请求里 `path` 字段逐字一致。宿主 registry 据此生成完整路径登记，路由侧
    /// **只按声明精确匹配**：未声明的端点 404（票 08 起「未声明清单 → 前缀内 ANY
    /// 放行」的票据 03 过渡策略已退役，未声明插件的 HTTP 面整体不可达）。
    /// 每条可同时声明认证档位（[`HttpEndpointContribution::Declared`]），缺省即最严
    /// 档 `jwt`——免凭证必须逐条显式写 `auth: "none"`。
    #[serde(default)]
    pub http_endpoints: Vec<HttpEndpointContribution>,
    /// 插件 WS 端点清单（`contributes.wsEndpoints`，票 09a WS 动作词表声明式化 · expand）
    ///
    /// 与 `httpEndpoints` 同语义：插件在 manifest 声明它要挂到宿主 WS 服务器的端点
    /// 相对路径段（宿主注入完整路径 `/ws/plugin/<插件 id>/<path>`）。宿主按声明
    /// 静态登记进 WS 端点注册表，路由侧**只按声明精确匹配**：未声明的路径不可达。
    /// 每条可同时声明认证档位（[`WsEndpointContribution::Declared`]），缺省按
    /// WS 面默认档（`none`）处理。声明式路由与既有硬编码分发（终端/会话动作
    /// switch）**并存**，存量动作走旧路径仍工作（expand–contract 的 expand 阶段）。
    #[serde(default)]
    pub ws_endpoints: Vec<WsEndpointContribution>,
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

/// 端点认证档位（WS 端点注册与 HTTP 端点声明共用这一张词汇表）
///
/// 只有 `none | jwt` 两档。**缺省档位由各传输面自己决定**，不在这里表达：
/// WS 首消息认证的历史缺省是 `none`（插件自管认证），HTTP 声明缺省是 `jwt`
/// （票 08 裁决 1「未声明即最严」）。未知取值一律 Err，绝不静默降级为较宽档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EndpointAuth {
    /// 免凭证：宿主不校验 JWT（环回 hook、配对 / QR 这类「拿 token 之前」的入口）
    None,
    /// 必须通过宿主 JWT 验签
    Jwt,
}

impl EndpointAuth {
    /// manifest / 线协议取值
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Jwt => "jwt",
        }
    }

    /// 解析声明值：缺省 / 空串 → `default`（由调用方给档位）；未知取值 → Err
    ///
    /// 大小写敏感——与 WS 注册面既有行为一致，`"JWT"` 视为拼写错误而非合法档位。
    pub fn parse_with(raw: Option<&str>, default: Self) -> Result<Self, String> {
        match raw.map(str::trim).unwrap_or("") {
            "" => Ok(default),
            "none" => Ok(Self::None),
            "jwt" => Ok(Self::Jwt),
            other => Err(format!(
                "unknown auth '{}' (expected \"none\" or \"jwt\")",
                other
            )),
        }
    }
}

/// 一条 HTTP 端点声明（票 08：两种形态并存）
///
/// - `"configs"` —— 只声明路径段，认证档位取宿主默认（最严档 `jwt`）
/// - `{ "path": "task-status", "auth": "none" }` —— 显式声明档位
///
/// 两形态并存是为了零迁移：既有 manifest 的 `string[]` 不必重写，收紧只作用在
/// 「未声明 auth」的端点上（默认变严）。`auth` 保留原始字符串，档位仲裁在宿主
/// registry（[`EndpointAuth::parse_with`]）——声明面负责表达，判定面负责解释。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HttpEndpointContribution {
    /// 仅路径段（认证档位 = 宿主默认）
    Path(String),
    /// 路径段 + 显式认证档位
    Declared {
        /// 相对路径段，与宿主传入 `_http_endpoint` 的 `path` 字段逐字一致
        path: String,
        /// `"none"` | `"jwt"`；缺省 = 宿主默认档位
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auth: Option<String>,
    },
}

impl HttpEndpointContribution {
    /// 声明的相对路径段
    pub fn path(&self) -> &str {
        match self {
            Self::Path(p) => p,
            Self::Declared { path, .. } => path,
        }
    }

    /// 声明的认证档位原始值（`None` = 未声明，由宿主按默认档处理）
    pub fn auth_raw(&self) -> Option<&str> {
        match self {
            Self::Path(_) => None,
            Self::Declared { auth, .. } => auth.as_deref(),
        }
    }
}

/// 一条 WS 端点声明（票 09a：`contributes.wsEndpoints`，WS 动作词表声明式化 · expand）
///
/// - `"echo"` —— 只声明路径段，认证档位取宿主默认
/// - `{ "path": "echo", "auth": "jwt" }` —— 显式声明档位
///
/// 与 [`HttpEndpointContribution`] 同形态（两形态并存 = 零迁移）。`auth` 保留原始
/// 字符串，档位仲裁在宿主（[`EndpointAuth::parse_with`]）——声明面负责表达，
/// 判定面负责解释。**缺省档由各传输面自定**：WS 缺省 = `none`（与 `host-websocket`
/// `register-endpoint` 一致，插件自管首消息认证），HTTP 缺省 = `jwt`（最严，票 08）。
/// 声明面只负责表达「路径 + 是否显式给档位」，不在此强求任何一面默认。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WsEndpointContribution {
    /// 仅路径段（认证档位 = 宿主默认）
    Path(String),
    /// 路径段 + 显式认证档位
    Declared {
        /// 相对路径段，与插件挂载到 `/ws/plugin/<id>/<path>` 的 `path` 逐字一致
        path: String,
        /// `"none"` | `"jwt"`；缺省 = 宿主默认档位
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auth: Option<String>,
    },
}

impl WsEndpointContribution {
    /// 声明的相对路径段
    pub fn path(&self) -> &str {
        match self {
            Self::Path(p) => p,
            Self::Declared { path, .. } => path,
        }
    }

    /// 声明的认证档位原始值（`None` = 未声明，由宿主按默认档处理）
    pub fn auth_raw(&self) -> Option<&str> {
        match self {
            Self::Path(_) => None,
            Self::Declared { auth, .. } => auth.as_deref(),
        }
    }
}

/// 一条 WASI 预打开目录声明（manifest `wasiPreopenDirs`，审计票 07 增只读档）
///
/// - `"/data/x"` —— 可写挂载（既有 manifest 的唯一形态，零迁移）
/// - `{ "path": "/data/x", "readonly": true }` —— 只读挂载
///
/// **缺省档 = 可写**，与改造前的行为逐字一致：本字段收紧的是「插件能声明什么」，
/// 不是「已声明的插件失去什么」。两形态共用同一默认，避免同一个列表里
/// 「写成对象就悄悄变只读、写成字符串就可写」这种反向意外。
///
/// 只读档只作用在 guest 侧写能力（宿主按 [`Self::readonly`] 选 `FsPerms`），
/// **不构成更松的授权**：两种形态都要先过 `is_granted` 才建得出 preopen
/// （票 07 裁决 3——「声明即免弹窗」会让插件声明 `~/.ssh` 就能预打开）。
///
/// `readonly` 用 `Option<bool>` 而非 `bool`：三态里 `None` = 未声明，序列化该
/// 条目时不回写此键，产物与源清单保持逐字一致（票 14 口径）。
///
/// 反序列化走手写实现（[`WasiPreopenDir::from_json`]）而非 serde derive：
/// derive 的 `untagged` 只会给出「data did not match any variant」这种不点名的
/// 错误，而这条声明是第三方 zip 绕开构建 CLI 时唯一的仲裁点——错误必须点名到键
/// 与非法取值（与 [`EndpointAuth::parse_with`] 同口径），且未知键一律拒绝。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum WasiPreopenDir {
    /// 仅路径（挂载档 = 可写）
    Path(String),
    /// 路径 + 显式挂载档
    Declared {
        /// 主机路径，支持 `${home}` 展开
        path: String,
        /// `true` = 只读挂载；缺省 / `false` = 可写（与既有行为一致）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        readonly: Option<bool>,
    },
}

impl WasiPreopenDir {
    /// 构造可写档条目（宿主内部与测试用）
    pub fn writable(path: impl Into<String>) -> Self {
        Self::Path(path.into())
    }

    /// 构造只读档条目
    pub fn read_only(path: impl Into<String>) -> Self {
        Self::Declared {
            path: path.into(),
            readonly: Some(true),
        }
    }

    /// 声明的主机路径（未展开 `${home}`）
    pub fn path(&self) -> &str {
        match self {
            Self::Path(p) => p,
            Self::Declared { path, .. } => path,
        }
    }

    /// 挂载档：`true` = 只读。未声明即可写（缺省档见类型注释）
    pub fn readonly(&self) -> bool {
        match self {
            Self::Path(_) => false,
            Self::Declared { readonly, .. } => readonly.unwrap_or(false),
        }
    }

    /// 以展开后的主机路径替换自身路径，挂载档保持不变
    ///
    /// `${home}` 展开与尾分隔符清理走这一步，避免各消费方自己重建条目时丢掉档位。
    pub fn with_path(self, path: impl Into<String>) -> Self {
        let readonly = self.readonly();
        Self::Declared {
            path: path.into(),
            readonly: readonly.then_some(true),
        }
    }

    /// 解析一条 manifest 声明：`"路径"` 字符串 或 `{path, readonly}` 对象
    ///
    /// 只管形态，不管语义：空串与 `${home}` 不可用留给展开阶段处理（既有口径）。
    /// 未知键报错而非忽略——拼成 `read_only` 若被静默吞掉，条目会退化成可写
    /// 挂载，插件的自我收紧声明就此消失且无人报错。
    pub fn from_json(value: &serde_json::Value) -> Result<Self, String> {
        match value {
            serde_json::Value::String(s) => Ok(Self::Path(s.clone())),
            serde_json::Value::Object(map) => {
                for key in map.keys() {
                    if key != "path" && key != "readonly" {
                        return Err(format!(
                            "wasiPreopenDirs 条目含未知字段（只允许 path / readonly）: {key} → {value}"
                        ));
                    }
                }
                let path = map
                    .get("path")
                    .and_then(|p| p.as_str())
                    .ok_or_else(|| format!("wasiPreopenDirs 条目缺 path 或 path 不是字符串: {value}"))?;
                let readonly = match map.get("readonly") {
                    None => None,
                    Some(v) => Some(v.as_bool().ok_or_else(|| {
                        format!("wasiPreopenDirs 条目 readonly 必须是布尔（true / false）: {value}")
                    })?),
                };
                Ok(Self::Declared {
                    path: path.to_string(),
                    readonly,
                })
            }
            other => Err(format!(
                "wasiPreopenDirs 条目形态非法（须为路径字符串或 {{path, readonly}} 对象）: {other}"
            )),
        }
    }
}

impl<'de> serde::Deserialize<'de> for WasiPreopenDir {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        Self::from_json(&value).map_err(serde::de::Error::custom)
    }
}

impl From<&str> for HttpEndpointContribution {
    fn from(path: &str) -> Self {
        Self::Path(path.to_string())
    }
}

impl From<String> for HttpEndpointContribution {
    fn from(path: String) -> Self {
        Self::Path(path)
    }
}

impl From<&str> for WsEndpointContribution {
    fn from(path: &str) -> Self {
        Self::Path(path.to_string())
    }
}

impl From<String> for WsEndpointContribution {
    fn from(path: String) -> Self {
        Self::Path(path)
    }
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

    /// 契约锁（会话引擎下沉票 14）：`PluginManifest::default()` 必须与「只写必填项的
    /// plugin.json 解析结果」逐字段等价。测试夹具因此可以 `..Default::default()` 构造，
    /// SDK 追加可选字段不再连带六处结构体字面量编译红。两侧任一失配都会让该锁红：
    /// 新字段漏了 `#[serde(default)]`，或 Rust 侧默认值与解析缺省不是同一个值。
    #[test]
    fn default_manifest_equals_minimal_json_manifest() {
        let from_json = serde_json::from_value::<PluginManifest>(serde_json::json!({
            "id": "com.bedcode.lock",
            "name": "Lock",
            "version": "0.0.0",
        }))
        .expect("只写必填项的 manifest 必须可解析");
        let from_default = PluginManifest {
            id: "com.bedcode.lock".to_string(),
            name: "Lock".to_string(),
            version: "0.0.0".to_string(),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&from_json).unwrap(),
            serde_json::to_value(&from_default).unwrap(),
            "Default 与 serde 缺省失配：新字段要么补 #[serde(default)]，要么夹具必须显式列出它"
        );
    }

    #[test]
    fn test_manifest_parse_with_defaults() {
        // 缺省字段（description/author/main/icon/contributes）全部走 default，
        // 宿主加载最小化 plugin.json 不应失败
        let json = serde_json::json!({
            "id": "com.bedcode.demo",
            "name": "Demo",
            "version": "0.1.0",
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
        assert_eq!(back["description"], serde_json::json!(""));
        // contributes 序列化时带全部字段（serde(default) 只影响反序列化）
        assert_eq!(back["contributes"]["commands"], serde_json::json!([]));
        assert_eq!(back["contributes"]["subscribes"], serde_json::json!([]));
    }

    /// 退役字段兼容（审计票 06 裁决 2）：`sandbox` 已从 manifest 退役（前端不做隔离，
    /// 安全边界在 Rust 端与 WASM 端），旧 plugin.json 仍带该字段时必须照常解析，
    /// 且不得再被序列化回产物（避免把退役字段继续传播给下游）
    #[test]
    fn test_manifest_ignores_retired_sandbox_field() {
        let json = serde_json::json!({
            "id": "com.bedcode.legacy",
            "name": "Legacy",
            "version": "1.0.0",
            "sandbox": "isolated"
        });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        assert_eq!(m.id, "com.bedcode.legacy");
        let back = serde_json::to_value(&m).unwrap();
        assert!(back.get("sandbox").is_none(), "退役字段不得再出现在序列化输出: {back}");
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
        assert_eq!(
            c.http_endpoints,
            vec![
                HttpEndpointContribution::Path("task-status".into()),
                HttpEndpointContribution::Path("task-queue/add".into()),
            ],
            "纯字符串条目解析为 Path 形态（未声明档位）"
        );
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

    /// 票 08：`httpEndpoints` 两形态并存——`string` 与 `{path, auth}` 必须都能解析，
    /// 否则既有插件的 manifest 会在解析期整表丢掉端点声明（表现为全部 404）。
    #[test]
    fn test_http_endpoints_parse_both_forms() {
        let c: PluginContributes = serde_json::from_value(serde_json::json!({
            "httpEndpoints": [
                "configs",
                { "path": "task-status", "auth": "none" },
                { "path": "task-queue/add", "auth": "jwt" },
                { "path": "session-mode" }
            ]
        }))
        .unwrap();
        assert_eq!(c.http_endpoints.len(), 4);
        assert_eq!(c.http_endpoints[0].path(), "configs");
        assert_eq!(
            c.http_endpoints[0].auth_raw(),
            None,
            "纯字符串条目 = 未声明档位"
        );
        assert_eq!(c.http_endpoints[1].path(), "task-status");
        assert_eq!(c.http_endpoints[1].auth_raw(), Some("none"));
        assert_eq!(c.http_endpoints[2].auth_raw(), Some("jwt"));
        // 对象条目缺 auth 键 → 同样落「未声明」，由宿主按最严默认档仲裁
        assert_eq!(c.http_endpoints[3].path(), "session-mode");
        assert_eq!(c.http_endpoints[3].auth_raw(), None, "对象条目不带 auth = 未声明档位");
    }

    /// 票 08：产物 manifest 与源逐字一致（构建链既有口径）——序列化必须原样保持
    /// 两形态，不得把 `"configs"` 改写成对象或反向（那会让 manifest-gen 的逐字节
    /// 比对每次构建即红）。
    #[test]
    fn test_http_endpoints_round_trip_preserves_form() {
        let src = serde_json::json!([
            "configs",
            { "path": "task-status", "auth": "none" },
            { "path": "session-mode" }
        ]);
        let parsed: Vec<HttpEndpointContribution> = serde_json::from_value(src.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), src);
    }

    /// 票 08：形态错误的条目不会「降级成未声明」——整表解析失败（宿主拒绝激活），
    /// 而不是静默收下少一项的清单（少一项即一个端点 404，静默失效最难查）。
    #[test]
    fn test_http_endpoints_reject_malformed_entries() {
        let parse = |raw: serde_json::Value| {
            serde_json::from_value::<Vec<HttpEndpointContribution>>(raw).is_err()
        };
        // path 非字符串 → 两形态都不匹配
        assert!(parse(serde_json::json!([{ "path": 1 }])));
        // 数字条目 → 解析失败（不得静默当成「未声明该端点」少收一项）
        assert!(parse(serde_json::json!([42])));
        // 数组条目 → 同上
        assert!(parse(serde_json::json!([["task-status"]])));
    }

    /// 票 08：认证档位词汇——两档之外的取值一律 Err（绝不静默降级为较宽档位）。
    /// 缺省档由调用方给：WS = none（历史行为）、HTTP = jwt（未声明即最严）。
    #[test]
    fn test_endpoint_auth_parse_with_default_and_unknown() {
        assert_eq!(
            EndpointAuth::parse_with(None, EndpointAuth::Jwt),
            Ok(EndpointAuth::Jwt)
        );
        assert_eq!(
            EndpointAuth::parse_with(Some(""), EndpointAuth::None),
            Ok(EndpointAuth::None),
            "空串 = 缺省档（WS 既有行为）"
        );
        assert_eq!(
            EndpointAuth::parse_with(Some("  none  "), EndpointAuth::Jwt),
            Ok(EndpointAuth::None),
            "前后空白容忍，档位取值仍须逐字匹配"
        );
        assert_eq!(
            EndpointAuth::parse_with(Some("jwt"), EndpointAuth::None),
            Ok(EndpointAuth::Jwt)
        );
        // 大小写敏感：JWT / Bearer / token 这些拼写都算未知取值（WS 既有口径）
        for bad in ["JWT", "token", "bearer", "local-only", "pairing-scope"] {
            let err = EndpointAuth::parse_with(Some(bad), EndpointAuth::Jwt)
                .expect_err("未知档位必须报错，不得回落到默认档");
            assert!(
                err.contains(bad) && err.contains("none") && err.contains("jwt"),
                "错误文案须点明非法取值与合法档位: {err}"
            );
        }
        assert_eq!(EndpointAuth::None.as_str(), "none");
        assert_eq!(EndpointAuth::Jwt.as_str(), "jwt");
    }

    /// 既有测试的补位：`From<&str>` 让老写法 `vec!["x".into()]` 继续可用，
    /// 但产出的是「未声明档位」条目——不能顺手把 auth 填成任何一档。
    #[test]
    fn test_http_endpoint_from_str_is_path_form() {
        let e = HttpEndpointContribution::from("task-status");
        assert_eq!(e, HttpEndpointContribution::Path("task-status".to_string()));
        assert_eq!(e.auth_raw(), None);
    }

    // ==================== WsEndpointContribution（contributes.wsEndpoints，票 09a） ====================

    /// `wsEndpoints` 缺省即空清单 = 未声明（与 httpEndpoints 同判据）：
    /// 未声明清单的插件 WS 端点整体不可达，宿主按「未声明即不解释」处理。
    #[test]
    fn test_ws_endpoints_default_is_undeclared() {
        let c: PluginContributes = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(c.ws_endpoints.is_empty(), "未声明 wsEndpoints 必须解析为空清单");
    }

    /// 票 09a：`wsEndpoints` 两形态并存——`string` 与 `{path, auth}` 都能解析，
    /// 与 `httpEndpoints` 同构，避免 WS 端点在解析期整表丢掉声明。
    #[test]
    fn test_ws_endpoints_parse_both_forms() {
        let c: PluginContributes = serde_json::from_value(serde_json::json!({
            "wsEndpoints": [
                "echo",
                { "path": "chat", "auth": "jwt" },
                { "path": "status" }
            ]
        }))
        .unwrap();
        assert_eq!(c.ws_endpoints.len(), 3);
        assert_eq!(c.ws_endpoints[0].path(), "echo");
        assert_eq!(c.ws_endpoints[0].auth_raw(), None, "纯字符串条目 = 未声明档位");
        assert_eq!(c.ws_endpoints[1].path(), "chat");
        assert_eq!(c.ws_endpoints[1].auth_raw(), Some("jwt"));
        // 对象条目缺 auth 键 → 未声明档位（由宿主按 WS 默认档 none 仲裁）
        assert_eq!(c.ws_endpoints[2].path(), "status");
        assert_eq!(c.ws_endpoints[2].auth_raw(), None);
    }

    /// 票 09a：序列化必须原样保持两形态（产物与源逐字一致，票 14 口径），
    /// 不得把 `"echo"` 改写成对象或反向。
    #[test]
    fn test_ws_endpoints_round_trip_preserves_form() {
        let src = serde_json::json!([
            "echo",
            { "path": "chat", "auth": "none" },
            { "path": "status" }
        ]);
        let parsed: Vec<WsEndpointContribution> = serde_json::from_value(src.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), src);
    }

    /// 票 09a：形态错误的条目不会「降级成未声明」——整表解析失败（宿主拒绝激活），
    /// 而不是静默收下少一项的清单。
    #[test]
    fn test_ws_endpoints_reject_malformed_entries() {
        let parse = |raw: serde_json::Value| {
            serde_json::from_value::<Vec<WsEndpointContribution>>(raw).is_err()
        };
        assert!(parse(serde_json::json!([{ "path": 1 }])));
        assert!(parse(serde_json::json!([42])));
        assert!(parse(serde_json::json!([["echo"]])));
    }

    /// `From<&str>` 让老写法 `vec!["x".into()]` 继续可用，产出「未声明档位」条目。
    #[test]
    fn test_ws_endpoint_from_str_is_path_form() {
        let e = WsEndpointContribution::from("echo");
        assert_eq!(e, WsEndpointContribution::Path("echo".to_string()));
        assert_eq!(e.auth_raw(), None);
    }

    // ==================== WasiPreopenDir（manifest wasiPreopenDirs，票 07 只读档） ====================

    /// 裸路径 = 既有条目形态，必须原样解析且档位在可写侧
    #[test]
    fn test_wasi_preopen_bare_string_is_writable_tier() {
        let d: WasiPreopenDir = serde_json::from_str(r#""${home}/.bedcode/ai-chatbox""#).unwrap();
        assert_eq!(d, WasiPreopenDir::Path("${home}/.bedcode/ai-chatbox".to_string()));
        assert_eq!(d.path(), "${home}/.bedcode/ai-chatbox");
        assert!(!d.readonly(), "裸路径档 = 可写（改造前的唯一形态）");
    }

    /// 对象形态但没写 readonly：同样落到可写档（缺省档 = 改造前行为，
    /// 收紧只作用在显式写 readonly 的条目上）
    #[test]
    fn test_wasi_preopen_object_without_readonly_defaults_to_writable() {
        let d: WasiPreopenDir = serde_json::from_str(r#"{"path":"/data/x"}"#).unwrap();
        assert_eq!(d.path(), "/data/x");
        assert!(!d.readonly());
    }

    /// 显式 readonly: true / false 各自生效（false 不得被当成「未声明」以外的东西）
    #[test]
    fn test_wasi_preopen_readonly_flag_both_values() {
        let ro: WasiPreopenDir = serde_json::from_str(r#"{"path":"/data/x","readonly":true}"#).unwrap();
        assert_eq!(ro.path(), "/data/x");
        assert!(ro.readonly(), "readonly:true 必须挂只读档");

        let rw: WasiPreopenDir = serde_json::from_str(r#"{"path":"/data/x","readonly":false}"#).unwrap();
        assert!(!rw.readonly(), "readonly:false 是可写档");
    }

    /// 未知键必须报错：`read_only` 这种拼写若被静默忽略，条目会退化成可写挂载
    /// ——插件的自我收紧声明被吞掉且无人报错（与 EndpointAuth「未知档位一律 Err，
    /// 绝不静默降级为较宽档位」同一条口径）
    #[test]
    fn test_wasi_preopen_rejects_unknown_key_in_object_entry() {
        let err = serde_json::from_str::<WasiPreopenDir>(r#"{"path":"/x","read_only":true}"#)
            .expect_err("read_only 是拼错的未知键，静默忽略等于把只读声明降级成可写")
            .to_string();
        assert!(
            err.contains("read_only") && err.contains("path") && err.contains("readonly"),
            "错误文案须点名未知键并给出允许字段: {err}"
        );
    }

    /// 非法形态与非法取值逐个拒绝，且错误文案点名到键——这条是第三方 zip 绕开
    /// 构建 CLI 时唯一能给出可定位信息的地方
    #[test]
    fn test_wasi_preopen_rejects_malformed_entries() {
        for (raw, needle) in [
            (r#"{"path":"/x","readonly":"true"}"#, "readonly 必须是布尔"),
            (r#"{"path":"/x","readonly":1}"#, "readonly 必须是布尔"),
            (r#"{"readonly":true}"#, "缺 path"),
            (r#"{"path":42}"#, "缺 path"),
            (r#"42"#, "条目形态非法"),
            (r#"[{"path":"/x"}]"#, "条目形态非法"),
            (r#"null"#, "条目形态非法"),
        ] {
            let err = serde_json::from_str::<WasiPreopenDir>(raw)
                .expect_err(&format!("非法声明必须解析失败: {raw}"))
                .to_string();
            assert!(err.contains(needle), "错误文案须点名问题（期望含「{needle}」）: {err}");
        }
    }

    /// 序列化回到声明形态（未写 readonly 的对象条目不得被回填 `"readonly":false`）——
    /// 产物与源清单逐字一致的口径（票 14）依赖这一条
    #[test]
    fn test_wasi_preopen_serializes_back_to_declared_form() {
        assert_eq!(
            serde_json::to_string(&WasiPreopenDir::Path("/x".to_string())).unwrap(),
            r#""/x""#
        );
        assert_eq!(serde_json::to_string(&WasiPreopenDir::writable("/x")).unwrap(), r#""/x""#);
        assert_eq!(
            serde_json::to_string(&WasiPreopenDir::read_only("/x")).unwrap(),
            r#"{"path":"/x","readonly":true}"#
        );
        // 只写了 path 的对象条目不得被回写成 `"readonly":false`
        assert_eq!(
            serde_json::to_string(&WasiPreopenDir::Declared {
                path: "/x".to_string(),
                readonly: None
            })
            .unwrap(),
            r#"{"path":"/x"}"#
        );
    }

    /// 空数组整键省略（既有 manifest 零迁移：不声明就不出现在产物里）
    #[test]
    fn test_wasi_preopen_dirs_omitted_when_empty() {
        let json = serde_json::json!({
            "id": "com.bedcode.demo",
            "name": "Demo",
            "version": "0.1.0",
            "pluginType": "rust-ts",
            "wasiPreopenDirs": []
        });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        assert!(m.wasi_preopen_dirs.is_empty());
        let back = serde_json::to_value(&m).unwrap();
        assert!(back.get("wasiPreopenDirs").is_none(), "空声明不得回写成键: {back}");
    }

    /// 两形态混列：档位逐条独立，顺序即 guest 挂载顺序（/data、/data1…）
    #[test]
    fn test_wasi_preopen_dirs_mixed_forms_parse_in_order() {
        let json = serde_json::json!({
            "id": "com.bedcode.demo",
            "name": "Demo",
            "version": "0.1.0",
            "pluginType": "rust-ts",
            "wasiPreopenDirs": [
                "${home}/.bedcode/ai-chatbox",
                { "path": "${home}/.ssh", "readonly": true },
                { "path": "${home}/write-me" }
            ]
        });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        assert_eq!(m.wasi_preopen_dirs.len(), 3);
        let tiers: Vec<(&str, bool)> = m
            .wasi_preopen_dirs
            .iter()
            .map(|d| (d.path(), d.readonly()))
            .collect();
        assert_eq!(
            tiers,
            vec![
                ("${home}/.bedcode/ai-chatbox", false),
                ("${home}/.ssh", true),
                ("${home}/write-me", false),
            ]
        );
    }

    /// with_path 只换路径、不换档位：宿主展开 `${home}` 靠它，
    /// 若实现改成重建条目而丢掉档位，只读声明会在到达 preopen 前退化成可写
    #[test]
    fn test_wasi_preopen_with_path_keeps_tier() {
        let ro = WasiPreopenDir::read_only("${home}/.ssh").with_path("/home/u/.ssh");
        assert_eq!(ro.path(), "/home/u/.ssh");
        assert!(ro.readonly(), "展开后仍须是只读档");

        let rw = WasiPreopenDir::writable("${home}/x").with_path("/home/u/x");
        assert_eq!(rw.path(), "/home/u/x");
        assert!(!rw.readonly(), "可写档展开后不得被串成只读");
    }
}
