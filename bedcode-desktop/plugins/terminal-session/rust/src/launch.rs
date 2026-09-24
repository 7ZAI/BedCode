//! 会话创建编排（票 09）：命名唯一化 + config→launch spec 映射 + 两阶段启动决策
//!
//! 职责边界（spec D3/D4）——「一个配置如何变成一个可启动的会话」这条决策线：
//! - **在插件**：命名唯一化策略（重名冲突改写 `base(N)` 递增）、config→launch
//!   spec 映射（发行版与 shell 分支、空命令兜底）、两阶段启动编排决策（`start`
//!   布尔）、**shell 包装与 WSL 路径转换**（[`build_argv`]）——这些都是产品语义，
//!   `host-session.create-with-spec` 不做业务解释
//! - **留宿主**：`host-session.create-with-spec` 执行（**只做 argv 原样 exec** /
//!   尺寸缺省 / ID 预生成）。宿主旧 shell 包装路径（`pty/command.rs::build_command`、
//!   `pty/wsl.rs::windows_to_wsl_path`）已随 2026-09-23 PTY 解耦票退役，故本模块
//!   必须**始终**送 `commandArgs`（缺省即被宿主显性拒绝，不再有旧路径回退）；
//!   映射产生的 `environment` 与宿主 `ExecutionEnvironment` serde 同形
//!   （`{"type":"Wsl2","distro":...}` | `{"type":"Linux"}` |
//!   `{"type":"Windows","shell":"PowerShell"}`），宿主仅用它决定 cwd 是否显式设置
//!
//! 模块构成：
//! - [`generate_unique_name`]：命名唯一化（复刻宿主 `DefaultNamingService`，
//!   行为逐字等价——票面「命名唯一化策略搬入插件」）
//! - [`resolve_environment`] / [`build_launch_spec`]：config→launch spec 映射
//!   （复刻宿主 `DefaultConfigMapper` 的分支决策）
//! - [`build_argv`]：<本模块的核心安全面> argv 拼装与转义（`cd '<dir>' && pwd && <cmd>`
//!   的 Linux/WSL 单引号闭合、PowerShell `''`、WSL 路径转换）
//! - native 单测覆盖：重名冲突、多配置同名互不干扰、Stopped 不计数、映射分支、
//!   非法环境取值防御、三环境 argv 与转义反例

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
    /// 原始命令串（**仅诊断/日志**：宿主不解释它，实际 exec 的是 `command_args`）
    pub command: String,
    /// 完整 argv（**必填**）：`build_argv` 算好的 argv（shell 包装/转义/WSL 路径
    /// 转换已在插件侧完成），宿主 raw exec 不做二次解释。宿主旧 shell 包装路径
    /// 已随 2026-09-23 PTY 解耦票退役——**缺省即被宿主显性拒绝**，故本字段恒为
    /// `Some`（保留 `Option` 只为省略 `null` 序列化）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_args: Option<Vec<String>>,
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
    /// 启动端设备名（可选）：桌面本地启动缺省；移动端经 HTTP/WS 启动时带设备名，
    /// 内核据此把「正统渲染端」初始归属固定为启动端（移动端单独启动的会话不会被
    /// 桌面端首次 resize 误判为需要覆盖确认）。纯事实透传，不含业务解释。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_device: Option<String>,
    /// 指定会话 id（可选）：重启编排用（先 remove 旧会话，再以同一 id 重建）。
    /// 普通创建缺省 → 宿主预生成 UUID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
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
    // shell 包装在插件侧：完整 argv 由本层算好（转义/WSL 转换在此），宿主经
    // `commandArgs` raw exec。2026-09-23 起宿主包装路径已退役 → 本字段必送
    // （缺省会被宿主 create-with-spec 显性拒绝）；command 字符串仍随 spec 透传，
    // 但只作诊断字段（宿主不解释）。先借 command 算 argv，再 move 进 spec。
    let command_args = Some(build_argv(
        &config.environment,
        config.wsl_distro.as_deref(),
        &config.working_dir,
        &command,
    )?);
    Ok(LaunchSpec {
        name: String::new(), // 占位：调用方先命名唯一化再填入
        command,
        command_args,
        args: Vec::new(),
        cwd: config.working_dir.clone(),
        cols,
        rows,
        env: std::collections::HashMap::new(),
        environment,
        config_id: config.id.clone(),
        start,
        // 启动端归属由调用方（编排入口）按请求来源填入，纯映射层不猜
        source_device: None,
        // 会话 id 由调用方（重启编排）按需填入，普通创建交给宿主预生成
        session_id: None,
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

// ==================== shell 包装下沉（pty 票 1）：build_argv ====================

/// Windows 路径 → WSL 内路径（复刻宿主 `pty/wsl.rs::windows_to_wsl_path`，下行
/// 语义逐字等价：正斜杠/反斜杠两种输入形态都归一化，WSL2 新旧前缀 + 盘符分支）
pub fn windows_to_wsl_path(path: &str) -> String {
    // 先统一分隔符，使 `/` 与 `\` 两种写法走同一套解析（盘符分支再转回）
    let path = path.replace('/', "\\");

    if path.starts_with("\\\\wsl.localhost\\") {
        // 新格式: \\wsl.localhost\Ubuntu\home\user -> /home/user（WSL2 1903+）
        let rest = path.trim_start_matches('\\').trim_start_matches("wsl.localhost\\");
        let parts: Vec<&str> = rest.splitn(2, '\\').collect();
        if parts.len() >= 2 {
            return format!("/{}", parts[1].replace('\\', "/"));
        }
        return rest.replace('\\', "/");
    }

    if path.starts_with("\\\\wsl$") {
        // 旧格式: \\wsl$\Ubuntu\home\user -> /home/user
        let rest = path.trim_start_matches('\\');
        let parts: Vec<&str> = rest.splitn(3, '\\').collect();
        if parts.len() >= 3 {
            return format!("/{}", parts[2].replace('\\', "/"));
        }
        return rest.replace('\\', "/");
    }

    // 盘符路径: C:\Users\test -> /mnt/c/Users/test
    if path.len() >= 2 && path.chars().nth(1) == Some(':') {
        let drive = path.chars().next().unwrap().to_ascii_lowercase();
        let rest = &path[2..].replace('\\', "/");
        return format!("/mnt/{}{}", drive, rest);
    }

    // 类 Unix 路径透传
    path.replace('\\', "/")
}

/// shell 包装 → argv（本模块的核心安全面）
///
/// 由插件计算**完整 argv**（含 bash -lic 包装、wsl.exe 前缀、转义），经
/// `create-with-spec.commandArgs` 交宿主 raw exec（宿主不做任何 shell 解释——
/// 宿主 `pty/command.rs::build_command` 已随 2026-09-23 PTY 解耦票删除，本函数
/// 是 shell 包装的唯一实现）。分支决策与转义规则与宿主旧实现逐语义等价：
/// - linux：`bash -lic "cd '<esc>' && pwd && <cmd>"`（-i 让 .bashrc 交互守卫通过）
/// - wsl2：`wsl.exe -d <distro> -- bash -lic <脚本>`（路径先转 WSL 形态）
/// - windows：PowerShell（config 域三值之一，无 CMD 分支；与 `resolve_environment`
///   的 `{"type":"Windows","shell":"PowerShell"}` 输出一致）
///
/// **安全边界（working_dir 来自配置 wire，不可信）**：
/// - PowerShell：单引号字面量内 `'` → `''`（`&;$"` 在单引号串内为字面，无需转义）
/// - Linux/WSL：bash 单引号内 `'` → `'\''`（闭合注入防护）
/// - CMD 不产生：插件侧无 cmd 环境分支（`VALID_ENVIRONMENTS` 三值不含 cmd），
///   且宿主旧路径（含 CMD 危险字符拒绝）已退役 → CMD 语义在本产品中不可达
pub fn build_argv(
    environment: &str,
    wsl_distro: Option<&str>,
    working_dir: &str,
    command: &str,
) -> Result<Vec<String>, String> {
    match environment.to_ascii_lowercase().as_str() {
        "linux" => {
            // bash 单引号转义：working_dir 含 `'` 会闭合 `cd '…'` 字面量执行任意命令
            let escaped = working_dir.replace('\'', "'\\''");
            let script = format!("cd '{}' && pwd && {}", escaped, command);
            Ok(vec!["bash".to_string(), "-lic".to_string(), script])
        }
        "wsl2" => {
            let distro = wsl_distro
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| "Ubuntu".to_string());
            let wsl_path = windows_to_wsl_path(working_dir);
            let escaped = wsl_path.replace('\'', "'\\''");
            let script = format!("cd '{}' && pwd && {}", escaped, command);
            Ok(vec![
                "wsl.exe".to_string(),
                "-d".to_string(),
                distro,
                "--".to_string(),
                "bash".to_string(),
                "-lic".to_string(),
                script,
            ])
        }
        "windows" => {
            // PowerShell 单引号内 `'` → `''`；其余字符（`&;$"`）在单引号串内为字面
            let escaped = working_dir.replace('\'', "''");
            let script = format!(
                "chcp 65001 > $null; [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new(); Set-Location '{}'; Write-Host 'Working directory:' $PWD.Path; {}",
                escaped, command
            );
            Ok(vec![
                "powershell.exe".to_string(),
                "-NoLogo".to_string(),
                "-NoExit".to_string(),
                "-Command".to_string(),
                script,
            ])
        }
        other => Err(format!("build_argv: unsupported environment: {other}")),
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
    /// 启动端设备名（可选；移动端 HTTP/WS 启动路径携带 → 正统端初始归属该端）
    #[serde(default)]
    pub source_device: Option<String>,
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

    // ==================== build_argv（shell 包装唯一实现） ====================

    /// linux：bash -lic 包装 + 单引号转义（working_dir 含 `'` → `'\''`，闭合注入防护）
    #[test]
    fn build_argv_linux_bash_lic_with_quote_escape() {
        let argv = build_argv("linux", None, "/home/usr/o'brien", "pnpm dev").expect("argv");
        assert_eq!(argv[0], "bash");
        assert_eq!(argv[1], "-lic");
        assert_eq!(
            argv[2],
            "cd '/home/usr/o'\\''brien' && pwd && pnpm dev",
            "工作目录含单引号必须闭合转义（闭合注入防护）"
        );
    }

    /// wsl2：wsl.exe 前缀 + distro 透传 + WSL 路径转换 + bash 脚本
    #[test]
    fn build_argv_wsl2_prefix_and_path_conversion() {
        let argv = build_argv("wsl2", Some("Ubuntu-22.04"), r"\\wsl.localhost\Ubuntu/home/user", "zsh")
            .expect("argv");
        assert_eq!(
            argv,
            vec![
                "wsl.exe", "-d", "Ubuntu-22.04", "--", "bash", "-lic",
                "cd '/home/user' && pwd && zsh",
            ]
        );
        // distro 缺省 → Ubuntu（与 resolve_environment 同款缺省）
        let argv = build_argv("wsl2", None, "/home/u", "bash").expect("argv");
        assert_eq!(argv[2], "Ubuntu", "distro 缺省 Ubuntu");
        // 正斜杠 WSL 前缀形态与反斜杠等价（宿主 wsl.rs 已随 PTY 解耦票退役，
        // 本模块自持该语义，故这里保留其票据 01 修复的回归锁）
        assert_eq!(windows_to_wsl_path("//wsl.localhost/Ubuntu/home/user"), "/home/user");
        assert_eq!(windows_to_wsl_path("//wsl$/Ubuntu/home/user"), "/home/user");
    }

    /// wsl2：路径转换**之后**仍做单引号闭合转义
    ///
    /// 回归锁：宿主旧实现曾遗漏 WSL 一路（票据 02 只补了 PowerShell/CMD/Linux），
    /// 而 WSL 分支同样是 `cd '<path>' && …` 结构——路径含 `'` 会闭合字面量执行
    /// 任意命令。本函数是 shell 包装的唯一实现，故该防护必须自带用例。
    #[test]
    fn build_argv_wsl2_escapes_single_quote_in_working_dir() {
        let argv = build_argv("wsl2", Some("Ubuntu"), r"C:\work\o'brien", "zsh").expect("argv");
        assert_eq!(argv[0], "wsl.exe");
        assert_eq!(
            argv.last().unwrap(),
            "cd '/mnt/c/work/o'\\''brien' && pwd && zsh",
            "WSL 分支必须转义单引号（防 cd 字面量被闭合注入）"
        );
    }

    /// windows：PowerShell 包装（chcp 65001 UTF-8 + Set-Location）+ `'` → `''` 转义
    #[test]
    fn build_argv_windows_powershell_utf8_and_escape() {
        let argv = build_argv("windows", None, r"D:\work\it's", "dir").expect("argv");
        assert_eq!(argv[0], "powershell.exe");
        assert!(argv.iter().any(|a| a == "-NoExit"));
        let script = argv.last().unwrap();
        assert!(script.contains("chcp 65001 > $null"), "UTF-8 输出编码必须设置");
        assert!(script.contains("Set-Location 'D:\\work\\it''s'"), "单引号 → '' 转义");
        assert!(script.contains("; dir"));
    }

    /// 未知环境：显性报错（不静默兜底）
    #[test]
    fn build_argv_unsupported_environment_errors() {
        let err = build_argv("bogus", None, "/tmp", "bash").unwrap_err();
        assert!(err.contains("unsupported environment"), "got: {err}");
    }

    /// Windows 路径转换全形态（复刻宿主 wsl.rs 测试：盘符 / 新旧 WSL 前缀 / 类 Unix 透传）
    #[test]
    fn windows_to_wsl_path_all_forms() {
        assert_eq!(windows_to_wsl_path("C:\\Users\\test"), "/mnt/c/Users/test");
        assert_eq!(windows_to_wsl_path("D:/Projects/my-app"), "/mnt/d/Projects/my-app");
        assert_eq!(windows_to_wsl_path(r"\\wsl$\Ubuntu\home\user"), "/home/user");
        assert_eq!(windows_to_wsl_path("/home/user"), "/home/user");
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
// `WasmHost` 没有 `ConfigStore` / `HostPty` impl（wasm 专属 import 符号不在
// native 链接），因此这里的 wasm 专属逻辑与 `config/mod.rs` 同模式隔离。
// 命名唯一化 / 映射 / 决策的纯逻辑已在上面 native 单测全量覆盖，此处只是
// 「读真源 → 算 spec → host-pty.spawn 执行」的调用编排。

#[cfg(target_arch = "wasm32")]
use crate::config::store::ConfigStore;

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::{HostApp, HostConfig, HostEvents, HostLog, HostPty};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

/// 注入子进程环境的业务会话标识环境变量（与宿主 `system/constants.rs` 同值；
/// agent 集成 hook 靠它把任务状态回投到对应会话）
#[cfg(target_arch = "wasm32")]
const ENV_BEDCODE_SESSION_ID: &str = "BEDCODE_SESSION_ID";

/// `session-create` 编排：读配置真源（插件私有库）→ 命名唯一化（读本域会话真源）
/// → config→launch spec 映射 → 会话 id 由本插件自产 → `host-pty.spawn` 执行
/// → 登记（含 `pty_id`）→ 广播 `SessionCreated` → `{sessionId}`。
///
/// P1-b 起**同步可见**：`host-pty.spawn` 同步执行（旧 create-with-spec 是宿主
/// 异步 fire-and-forget，回执语义因此变强——spawn 失败直接 Err，不产生幽灵会话）。
#[cfg(target_arch = "wasm32")]
pub fn create_via_host(draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request = CreateSessionRequest::parse(draft_json)?;
    // 读配置真源（票 08 起配置真源在本插件私有库；找不到显性报错）
    let config = WasmHost
        .get(&request.config_id)
        .map_err(|e| format!("config read failed: {e}"))?
        .ok_or_else(|| format!("会话配置不存在：{}", request.config_id))?;
    // 现有会话列表 → 命名唯一化（同配置活跃会话递增，Stopped 不计数）；
    // 真源已在本域（宿主不再持会话登记）
    let sessions_json = crate::session::internal_records_json()?;
    let sessions = parse_sessions(&sessions_json);
    let unique_name = generate_unique_name(&request.config_id, &config.name, &sessions);
    // config→launch spec 映射 + 两阶段启动决策（P1-b 起恒即起，start 保留为
    // wire 兼容字段，不再有「只建不启」的双态——host-pty 无 create-without-spawn）
    let spec = build_launch_spec(&config, request.cols, request.rows, true)?;
    // 会话 id 由插件自产（宿主不再预生成，`BEDCODE_SESSION_ID` 随 env 注入）
    let session_id = crate::session::ops::new_session_id()?;
    spawn_session(&request.config_id, &spec, &session_id, &unique_name, &request.source_device)
        .map_err(|e| format!("会话创建失败：{e}"))?;
    Ok(serde_json::json!({ "sessionId": session_id }))
}

/// 共用的「spawn → 登记 → Created 逻辑 → 广播」核心（普通创建与重启共走）
///
/// 顺序：
/// 1. **Creating（agent 集成）先于 spawn**（与内核旧时序一致：hook 就位后 shell
///    才启动，集成脚本在子进程首帧就绪）；
/// 2. `host-pty.spawn`（env 注入 `BEDCODE_SESSION_ID`；working_dir 仅
///    Windows/Linux 原生环境显式设置——WSL 路径已内嵌进 argv 脚本）；
/// 3. 登记（真源写入含 `pty_id`；失败 → kill 已 spawn 的 pty 再报错，不留孤儿
///    进程）；
/// 4. Created 逻辑（重启补发 `session-restarted` + 定时任务域就绪信号）；
/// 5. 广播 `SessionCreated`（概要自本域视图，宿主不回查内核）。
#[cfg(target_arch = "wasm32")]
pub fn spawn_session(
    config_id: &str,
    spec: &LaunchSpec,
    session_id: &str,
    name: &str,
    source_device: &Option<String>,
) -> Result<(), String> {
    // 1. Creating：agent 集成（hook 就位先于 spawn；取不到资源目录 → 跳过注入）
    run_creating_integration(spec.command.as_str(), spec.cwd.as_str());

    // 2. host-pty.spawn（裸 argv / env 追加 / 尺寸）
    let argv = spec
        .command_args
        .as_deref()
        .ok_or_else(|| "launch spec missing commandArgs".to_string())?;
    let Some((head, rest)) = argv.split_first() else {
        return Err("launch spec commandArgs is empty".to_string());
    };
    let mut env = spec.env.clone();
    env.insert(ENV_BEDCODE_SESSION_ID.to_string(), session_id.to_string());
    let mut builder = bedcode_plugin_api::host::PtySpawnConfig::new(head);
    if !rest.is_empty() {
        builder = builder.args(rest.iter());
    }
    if !env.is_empty() {
        builder = builder.env(env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }
    // working_dir 仅原生环境显式设置（WSL 路径在 build_argv 的脚本里 cd）
    if spec.environment.get("type").and_then(|v| v.as_str()) != Some("Wsl2") {
        if !spec.cwd.is_empty() {
            builder = builder.working_dir(&spec.cwd);
        }
    }
    if let Some(cols) = spec.cols {
        builder = builder.cols(cols);
    }
    if let Some(rows) = spec.rows {
        builder = builder.rows(rows);
    }
    // 宿主广播声明（会话语义下沉票 05）：本会话输出允许宿主 server 只读订阅——
    // 「业务会话也是插件 PTY，能不能被宿主读走由插件决定」的落地。宿主据此登记
    // 「会话 id → pty 句柄」只读映射；移动端输出面（票 06）经它直读同进程环。
    builder = builder.host_broadcast_session_id(session_id);
    let pty_id = WasmHost
        .pty_spawn(&builder.to_json())
        .map_err(|e| format!("host-pty.spawn failed: {}", e.message))?;

    // 3. 登记（真源写入含 pty_id；失败 → 回收已 spawn 的 pty，不留孤儿进程）
    if let Err(e) = crate::session::note_created(
        session_id,
        &pty_id,
        config_id,
        name,
        true,
        source_device.as_deref(),
    ) {
        let _ = WasmHost.pty_kill(&pty_id);
        return Err(format!("session record failed: {e}"));
    }

    // 4. Created 逻辑（与宿主旧生命周期回调逐字一致：重启补发 + 定时任务就绪）
    crate::actions::flush_pending_restart(&WasmHost, session_id);
    crate::task::scheduled::handle_session_created(&WasmHost, session_id, config_id);

    // 5. 发布 SessionCreated（概要自本域视图；载荷自足——取不到概要就跳过并
    //    留痕，不伪造半成品通知）。websocket 业务下沉票 05：会话生命周期事件
    //    由本插件定义载荷并经 emit+bus 发布，不再走宿主 broadcast-sync。
    match crate::session::summary_for(session_id) {
        Ok(session) => {
            match serde_json::to_value(&session) {
                Ok(summary_json) => crate::session::events::publish_created(
                    &summary_json,
                    source_device.as_deref().unwrap_or_default(),
                ),
                Err(e) => WasmHost.log_warn(&format!(
                    "session created 概要序列化失败，跳过发布（不伪造半成品通知）: {e}"
                )),
            }
        }
        Err(e) => WasmHost.log_warn(&format!(
            "session created 发布跳过（载荷不自足，生产者侧兜底）: {e}"
        )),
    }
    WasmHost.log_info(&format!(
        "session created via host-pty (session_id={session_id}, pty_id={pty_id}, config_id={config_id})"
    ));
    Ok(())
}

/// Creating 语义的 agent 集成注入（hook 就位先于 spawn，与内核旧生命周期时序一致）
///
/// 原 `on_session_lifecycle(Creating)` 分支迁入：detect_agent → 无集成则跳过 →
/// 资源目录经 `host-app.plugin-resource-dir` 自取（v25 原语）→ hooks 安装。
/// 失败只记日志（集成缺失不阻断会话创建，与旧行为一致）。
#[cfg(target_arch = "wasm32")]
fn run_creating_integration(command: &str, working_dir: &str) {
    use bedcode_plugin_api::host::ConfigKey;
    let host = WasmHost;
    let agent_name = crate::task::agent::detect_agent(command);
    if crate::task::agent::session_integration_for(agent_name)
        == crate::task::agent::SessionIntegration::None
    {
        host.log_debug(&format!(
            "create: agent '{}' has no session integration, skip setup",
            agent_name
        ));
        return;
    }
    let resource_dir = match host.plugin_resource_dir() {
        Ok(dir) => dir,
        Err(e) => {
            host.log_warn(&format!(
                "create: plugin resource dir unavailable, skip agent integration: {}",
                e.message
            ));
            return;
        }
    };
    // 集成脚本经 HTTP 推送任务状态，端点由网关中间件本地放行，无需 token；
    // 端口读不到时与旧插件同退化为 8765（行为等价，不因配置缺失停摆）
    let port = host
        .config_get(ConfigKey::NetworkPort)
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8765);
    let result = crate::task::hooks::ensure_agent_integration(&host, agent_name, working_dir, port, &resource_dir);
    if result.success {
        host.log_info(&format!(
            "create: integration setup for agent '{}' in {}",
            agent_name, working_dir
        ));
    } else if result.skipped {
        host.log_debug(&format!(
            "create: integration skipped for agent '{}' in {}",
            agent_name, working_dir
        ));
    } else {
        host.log_warn(&format!(
            "create: integration setup failed for agent '{}' in {}: {}",
            agent_name, working_dir, result.message
        ));
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn create_via_host(_draft_json: &serde_json::Value) -> Result<serde_json::Value, String> {
    Err("launch create unavailable outside wasm runtime".to_string())
}
