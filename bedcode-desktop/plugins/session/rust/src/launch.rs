//! 会话创建编排（票 09）：命名唯一化 + config→launch spec 映射 + 两阶段启动决策
//!
//! 职责边界（spec D3/D4）——「一个配置如何变成一个可启动的会话」这条决策线：
//! - **在插件**：命名唯一化策略（重名冲突改写 `base(N)` 递增）、config→launch
//!   spec 映射（发行版与 shell 分支、空命令兜底）、两阶段启动编排决策（`start`
//!   布尔）——这些都是产品语义，`host-session.create-with-spec` 不做业务解释
//! - **留宿主**：`host-session.create-with-spec` 执行（shell 包装 / WSL 转换 /
//!   尺寸缺省 / ID 预生成）；映射产生的 `environment` 与宿主 `ExecutionEnvironment`
//!   serde 同形（`{"type":"Wsl2","distro":...}` | `{"type":"Linux"}` |
//!   `{"type":"Windows","shell":"PowerShell"}`），宿主按它做发行版转换
//!
//! 模块构成：
//! - [`generate_unique_name`]：命名唯一化（复刻宿主 `DefaultNamingService`，
//!   行为逐字等价——票面「命名唯一化策略搬入插件」）
//! - [`resolve_environment`] / [`build_launch_spec`]：config→launch spec 映射
//!   （复刻宿主 `DefaultConfigMapper` 的分支决策）
//! - native 单测覆盖：重名冲突、多配置同名互不干扰、Stopped 不计数、映射分支、
//!   非法环境取值防御

use crate::config::model::{SessionConfig, VALID_ENVIRONMENTS};

// ==================== 会话引用视图（自 host-session.list-sessions JSON 解析） ====================

/// 命名唯一化关心的会话字段子集（自宿主 `SessionInfo` camelCase JSON 解析）
///
/// `status` 与宿主 `SessionStatus` 的 serde 变体名一致：`"Starting"` / `"Running"` /
/// `"WaitingInput"` / `"Stopped"` —— 仅 `Stopped` 不参与「活跃」计数（与宿主
/// `DefaultNamingService` 的 `status != SessionStatus::Stopped` 过滤等价）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRef {
    pub config_id: String,
    pub name: String,
    pub status: String,
}

/// 解析宿主会话列表 JSON（camelCase 数组）→ `SessionRef[]`
///
/// 宽容策略：字段缺失/类型不符的会话条目跳过（命名唯一化无需为坏数据整体失败——
/// 与会话列表的可展示性解耦；但合法条目的解析错误不静默）。
pub fn parse_sessions(json: &serde_json::Value) -> Vec<SessionRef> {
    let Some(arr) = json.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|v| {
            let get = |key: &str| v.get(key).and_then(|x| x.as_str()).map(str::to_string);
            Some(SessionRef {
                config_id: get("configId")?,
                name: get("name")?,
                status: get("status")?,
            })
        })
        .collect()
}

// ==================== 命名唯一化（复刻宿主 DefaultNamingService） ====================

/// 唯一的会话名（重名冲突改写）：同配置的活跃会话名里提取最大编号，避免删除后
/// 编号回退导致重名——与宿主 `DefaultNamingService::generate_unique_name` 逐字等价：
/// - 无匹配 → 原名
/// - 命中 `base`（编号 0）→ `base(1)`
/// - 命中 `base(N)` → `base(N+1)`
///
/// 多配置同名互不干扰：过滤条件要求 `config_id == 目标配置`（另一配置的同名会话
/// 不算冲突）。
pub fn generate_unique_name(config_id: &str, base_name: &str, sessions: &[SessionRef]) -> String {
    let max_index = sessions
        .iter()
        .filter(|s| s.config_id == config_id && s.status != "Stopped")
        .filter_map(|s| {
            let name = &s.name;
            if name == base_name {
                Some(0usize)
            } else if let Some(rest) = name.strip_prefix(base_name) {
                rest.strip_prefix('(')
                    .and_then(|r| r.strip_suffix(')'))
                    .and_then(|n| n.parse::<usize>().ok())
            } else {
                None
            }
        })
        .max();

    match max_index {
        None => base_name.to_string(),
        Some(0) => format!("{}(1)", base_name),
        Some(n) => format!("{}({})", base_name, n + 1),
    }
}

// ==================== config→launch spec 映射（复刻宿主 DefaultConfigMapper） ====================

/// 环境取值 → 宿主 `ExecutionEnvironment` wire 形状（`create-with-spec` 的
/// `environment` 字段；宿主按它做发行版转换 / shell 分支）。
///
/// 分支决策与宿主 `DefaultConfigMapper::to_launch_config` 逐字等价：
/// - `wsl2` → `{"type":"Wsl2","distro":<wslDistro> || "Ubuntu"}`（发行版缺省值宿主同款）
/// - `linux` → `{"type":"Linux"}`
/// - `windows` → `{"type":"Windows","shell":"PowerShell"}`
/// - 其他取值 → 显性报错（防御：`normalize` 已把环境取值收紧到
///   [`VALID_ENVIRONMENTS`]，此处兜底非法输入不产生半成品 spec）
pub fn resolve_environment(config: &SessionConfig) -> Result<serde_json::Value, String> {
    match config.environment.as_str() {
        "wsl2" => Ok(serde_json::json!({
            "type": "Wsl2",
            "distro": config.wsl_distro.clone().unwrap_or_else(|| "Ubuntu".to_string()),
        })),
        "linux" => Ok(serde_json::json!({ "type": "Linux" })),
        "windows" => Ok(serde_json::json!({ "type": "Windows", "shell": "PowerShell" })),
        other => Err(format!(
            "非法环境取值：{}（合法：{}）",
            other,
            VALID_ENVIRONMENTS.join(" / ")
        )),
    }
}

/// launch spec（`host-session.create-with-spec` 入参，camelCase；字段语义见 WIT）
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSpec {
    /// 唯一化后的会话名（插件决策；宿主不再二次命名）
    pub name: String,
    /// 原始命令串（宿主嵌入 shell 包装）
    pub command: String,
    /// 追加参数（当前配置无 args 概念，恒为空数组）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// 工作目录
    pub cwd: String,
    /// 启动端预算网格（桌面命令带 cols/rows 时填入；缺省宿主兜底默认网格）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u16>,
    /// 环境变量（当前配置无 env 概念，恒为空）
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub env: std::collections::HashMap<String, String>,
    /// 执行环境（映射决策产物）
    pub environment: serde_json::Value,
    /// 来源配置 id（会话记录 configId 字段；命名唯一化按它过滤）
    pub config_id: String,
    /// 两阶段启动编排决策：false = 只创建不启动（第一阶段）
    pub start: bool,
}

/// 配置 → launch spec（映射决策层；校验已由 `config::ops::normalize` 完成，
/// 此处对非法输入做防御性显性报错）
pub fn build_launch_spec(
    config: &SessionConfig,
    cols: Option<u16>,
    rows: Option<u16>,
    start: bool,
) -> Result<LaunchSpec, String> {
    let environment = resolve_environment(config)?;
    // 空命令兜底（normalize 已保证非空，此处防御非法构造体）
    let command = if config.command.trim().is_empty() {
        default_command_for(&config.environment)
    } else {
        config.command.clone()
    };
    Ok(LaunchSpec {
        name: String::new(), // 占位：调用方先命名唯一化再填入
        command,
        args: Vec::new(),
        cwd: config.working_dir.clone(),
        cols,
        rows,
        env: std::collections::HashMap::new(),
        environment,
        config_id: config.id.clone(),
        start,
    })
}

/// 空命令兜底（与 `config::model::ConfigDraft::default_command_for` 同规则）
fn default_command_for(environment: &str) -> String {
    if environment.eq_ignore_ascii_case("windows") {
        "powershell".to_string()
    } else {
        "bash".to_string()
    }
}

// ==================== 编排入口 ====================

/// 单个创建请求（互调 api `session-create` 入参，camelCase）
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub config_id: String,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
    /// 两阶段启动编排决策：缺省 true（创建即启动；与宿主 start_session 语义一致）
    #[serde(default = "default_start")]
    pub start: bool,
}

fn default_start() -> bool {
    true
}

impl CreateSessionRequest {
    /// 从互调 api 入参 JSON 解析（字段缺省/非法显性报错）
    pub fn parse(value: &serde_json::Value) -> Result<Self, String> {
        serde_json::from_value(value.clone())
            .map_err(|e| format!("session-create: invalid request: {}", e))
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn session(config_id: &str, name: &str, status: &str) -> SessionRef {
        SessionRef {
            config_id: config_id.to_string(),
            name: name.to_string(),
            status: status.to_string(),
        }
    }

    fn config(
        id: &str,
        environment: &str,
        wsl_distro: Option<&str>,
        command: &str,
    ) -> SessionConfig {
        SessionConfig {
            id: id.to_string(),
            name: format!("cfg-{}", id),
            environment: environment.to_string(),
            wsl_distro: wsl_distro.map(str::to_string),
            working_dir: "/tmp".to_string(),
            command: command.to_string(),
            auto_start: false,
            created_at: "2026-09-20T00:00:00Z".to_string(),
            updated_at: "2026-09-20T00:00:00Z".to_string(),
        }
    }

    // ==================== 命名唯一化 ====================

    #[test]
    fn unique_name_first_is_base() {
        let got = generate_unique_name("c1", "dev", &[]);
        assert_eq!(got, "dev");
    }

    /// 重名冲突：活跃会话已用原名 → 递增为 base(1)
    #[test]
    fn unique_name_second_becomes_suffix_one() {
        let sessions = vec![session("c1", "dev", "Running")];
        assert_eq!(generate_unique_name("c1", "dev", &sessions), "dev(1)");
    }

    /// 重名冲突：活跃会话已用 base(1) → 递增为 base(2)；乱序列表取最大编号
    #[test]
    fn unique_name_increments_from_max_suffix() {
        let sessions = vec![
            session("c1", "dev", "Running"),
            session("c1", "dev(3)", "WaitingInput"),
            session("c1", "dev(1)", "Running"),
        ];
        assert_eq!(generate_unique_name("c1", "dev", &sessions), "dev(4)");
    }

    /// 删除后编号不回退：base(2) 已删，只剩 base(1) → 仍生成 base(2)（非 base(1)）
    #[test]
    fn unique_name_does_not_reuse_freed_suffix() {
        let sessions = vec![session("c1", "dev(1)", "Running")];
        assert_eq!(generate_unique_name("c1", "dev", &sessions), "dev(2)");
    }

    /// 多配置同名互不干扰：c2 的 dev 系列不计入 c1 的冲突
    #[test]
    fn unique_name_ignores_other_config_same_name() {
        let sessions = vec![
            session("c1", "dev", "Running"),
            session("c2", "dev", "Running"),
            session("c2", "dev(7)", "Running"),
        ];
        assert_eq!(generate_unique_name("c1", "dev", &sessions), "dev(1)");
        assert_eq!(generate_unique_name("c2", "dev", &sessions), "dev(8)");
    }

    /// Stopped 会话不参与计数（与宿主 `status != Stopped` 过滤等价）
    #[test]
    fn unique_name_ignores_stopped() {
        let sessions = vec![
            session("c1", "dev", "Stopped"),
            session("c1", "dev", "Running"),
        ];
        assert_eq!(generate_unique_name("c1", "dev", &sessions), "dev(1)");
        let only_stopped = vec![session("c1", "dev", "Stopped")];
        assert_eq!(generate_unique_name("c1", "dev", &only_stopped), "dev");
    }

    /// 前缀误匹配不计数：「dev-ops」不以 "dev(" 形式跟随编号，不构成冲突
    #[test]
    fn unique_name_only_matches_numbered_suffix() {
        let sessions = vec![session("c1", "dev-ops", "Running")];
        assert_eq!(generate_unique_name("c1", "dev", &sessions), "dev");
    }

    // ==================== parse_sessions ====================

    #[test]
    fn parse_sessions_from_host_json() {
        let json = serde_json::json!([
            {"id": "s1", "configId": "c1", "name": "dev", "status": "Running"},
            {"id": "s2", "configId": "c1", "name": "dev(1)", "status": "Stopped"},
            {"bad": "entry"}, // 缺字段跳过
        ]);
        let sessions = parse_sessions(&json);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].status, "Running");
        assert_eq!(sessions[1].status, "Stopped");
    }

    #[test]
    fn parse_sessions_non_array_returns_empty() {
        assert!(parse_sessions(&serde_json::Value::Null).is_empty());
        assert!(parse_sessions(&serde_json::json!({"a": 1})).is_empty());
    }

    // ==================== config→launch spec 映射 ====================

    /// wsl2 分支：distro 显式给定时透传
    #[test]
    fn environment_wsl2_with_distro() {
        let cfg = config("c1", "wsl2", Some("Ubuntu-22.04"), "bash");
        let env = resolve_environment(&cfg).expect("resolve");
        assert_eq!(
            env,
            serde_json::json!({"type": "Wsl2", "distro": "Ubuntu-22.04"})
        );
    }

    /// wsl2 分支：distro 缺省 → Ubuntu（宿主 DefaultConfigMapper 同款缺省）
    #[test]
    fn environment_wsl2_default_distro_ubuntu() {
        let cfg = config("c1", "wsl2", None, "bash");
        let env = resolve_environment(&cfg).expect("resolve");
        assert_eq!(env, serde_json::json!({"type": "Wsl2", "distro": "Ubuntu"}));
    }

    /// linux 分支
    #[test]
    fn environment_linux() {
        let cfg = config("c1", "linux", None, "bash");
        assert_eq!(
            resolve_environment(&cfg).expect("resolve"),
            serde_json::json!({"type": "Linux"})
        );
    }

    /// windows 分支 → PowerShell（宿主 DefaultConfigMapper 默认 shell）
    #[test]
    fn environment_windows_powershell() {
        let cfg = config("c1", "windows", None, "powershell");
        assert_eq!(
            resolve_environment(&cfg).expect("resolve"),
            serde_json::json!({"type": "Windows", "shell": "PowerShell"})
        );
    }

    /// 非法环境取值：显性报错（防御；normalize 已拦）
    #[test]
    fn environment_invalid_value_is_explicit_error() {
        let cfg = config("c1", "macos", None, "bash");
        let err = resolve_environment(&cfg).expect_err("must reject");
        assert!(err.contains("非法环境取值"), "unexpected: {err}");
    }

    /// build_launch_spec：完整 spec 形状（camelCase wire 与宿主契约一致）
    #[test]
    fn build_launch_spec_shape() {
        let cfg = config("cfg-x", "wsl2", Some("Ubuntu"), "bash");
        let spec = build_launch_spec(&cfg, Some(100), Some(30), false).expect("spec");
        assert_eq!(spec.name, "", "命名由调用方先置入（占位空串）");
        assert_eq!(spec.command, "bash");
        assert_eq!(spec.cwd, "/tmp");
        assert_eq!(spec.cols, Some(100));
        assert_eq!(spec.rows, Some(30));
        assert!(!spec.start, "两阶段第一阶段");
        assert_eq!(spec.config_id, "cfg-x");
        assert!(spec.args.is_empty());
        assert!(spec.env.is_empty());
        let json = serde_json::to_value(&spec).expect("serialize");
        assert_eq!(
            json["environment"],
            serde_json::json!({"type": "Wsl2", "distro": "Ubuntu"})
        );
        // wire 键名 camelCase 锁定
        assert!(json.get("configId").is_some(), "configId 键名锁定");
        assert!(json.get("cwd").is_some(), "cwd 键名锁定");
    }

    /// 空命令兜底：config.command 空 → 按环境分支给默认 shell
    #[test]
    fn build_launch_spec_falls_back_empty_command() {
        let linux = config("c1", "linux", None, "");
        assert_eq!(
            build_launch_spec(&linux, None, None, true)
                .expect("spec")
                .command,
            "bash"
        );
        let windows = config("c2", "windows", None, "  ");
        assert_eq!(
            build_launch_spec(&windows, None, None, true)
                .expect("spec")
                .command,
            "powershell"
        );
    }

    // ==================== CreateSessionRequest ====================

    #[test]
    fn create_request_parse_defaults_start_true() {
        let req =
            CreateSessionRequest::parse(&serde_json::json!({"configId": "c1"})).expect("parse");
        assert_eq!(req.config_id, "c1");
        assert!(req.start, "start 缺省 true");
        assert_eq!(req.cols, None);
        assert_eq!(req.rows, None);
    }

    #[test]
    fn create_request_parse_start_false_and_size() {
        let req = CreateSessionRequest::parse(&serde_json::json!({
            "configId": "c1", "cols": 120, "rows": 40, "start": false
        }))
        .expect("parse");
        assert!(!req.start);
        assert_eq!(req.cols, Some(120));
        assert_eq!(req.rows, Some(40));
    }

    #[test]
    fn create_request_parse_missing_config_id_is_explicit_error() {
        let err = CreateSessionRequest::parse(&serde_json::json!({"start": true}))
            .expect_err("must reject");
        assert!(err.contains("invalid request"), "unexpected: {err}");
    }
}

// ==================== 编排入口（wasm 运行时薄包装，native 显性失败） ====================
//
// lib.rs 的互调 api 面的 `session-create` 只调这个入口；native（cargo test）下
// `WasmHost` 没有 `ConfigStore` / `HostSession` impl（wasm 专属 import 符号不在
// native 链接），因此这里的 wasm 专属逻辑与 `config/mod.rs` 同模式隔离。
// 命名唯一化 / 映射 / 决策的纯逻辑已在上面 native 单测全量覆盖，此处只是
// 「读真源 → 算 spec → 调宿主原语」的调用编排。

#[cfg(target_arch = "wasm32")]
use crate::config::store::ConfigStore;

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::HostSession;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// `session-create` 编排：读配置真源（插件私有库）→ 命名唯一化（读宿主会话列表）
/// → config→launch spec 映射 → 两阶段启动决策 → 宿主 `create-with-spec` 执行
/// → `{sessionId}`（预生成 id 立即返回，实际创建宿主异步执行）。
#[cfg(target_arch = "wasm32")]
pub fn create_via_host(draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request = CreateSessionRequest::parse(draft_json)?;
    // 读配置真源（票 08 起配置真源在本插件私有库；找不到显性报错）
    let config = WasmHost
        .get(&request.config_id)
        .map_err(|e| format!("config read failed: {}", e))
        .and_then(|c| c.ok_or_else(|| format!("会话配置不存在：{}", request.config_id)))?;
    // 现有会话列表 → 命名唯一化（同配置活跃会话递增，Stopped 不计数）
    let sessions_json = WasmHost
        .session_list()
        .map_err(|e| format!("session list failed: {}", e.message))?;
    let sessions = parse_sessions(sessions_json.as_ref().unwrap_or(&serde_json::Value::Null));
    let unique_name = generate_unique_name(&request.config_id, &config.name, &sessions);
    // config→launch spec 映射 + 两阶段启动决策
    let mut spec = build_launch_spec(&config, request.cols, request.rows, request.start)?;
    spec.name = unique_name;
    // 宿主 create-with-spec 执行（shell 包装 / WSL 转换 / 尺寸缺省 / ID 预生成）
    let spec_json =
        serde_json::to_value(&spec).map_err(|e| format!("launch spec serialize failed: {}", e))?;
    let session_id = WasmHost
        .session_create_with_spec(&spec_json)
        .map_err(|e| format!("host create-with-spec failed: {}", e.message))?;
    Ok(serde_json::json!({ "sessionId": session_id }))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn create_via_host(_draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    Err("launch create unavailable outside wasm runtime".to_string())
}
