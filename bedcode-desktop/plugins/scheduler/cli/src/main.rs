//! bedtask — 计划任务 CLI（spec §8）
//!
//! 薄客户端：localhost HTTP → 桌面端网关 `/api/plugin/com.bedcode.scheduler/...`
//! 命令与 02 号 issue 的 HTTP 端点一一对应；人类可读默认输出 + `--json`。
//!
//! 用法：
//! ```text
//! bedtask add --cron "<6段>" (--script <path> | --exec "<cmd>")
//!             [--name <n>] [--cwd <dir>] [--env K=V,...] [--timeout <sec>] [--once]
//! bedtask list [--json]
//! bedtask show <id> [--json]
//! bedtask remove <id>
//! bedtask edit <id> [--cron <e>] [--exec <v>] [--name <n>] [--cwd <d>] [--env K=V] [--timeout <s>] [--once|--no-once]
//! bedtask enable <id> | bedtask disable <id>
//! bedtask run <id>
//! bedtask logs <id> [--limit N] [--json]
//! ```
//!
//! 端口：环境变量 `BEDCODE_PORT`，缺省 8765（与宿主 config_get(NetworkPort) 对齐）。

mod http;

use serde_json::Value;

/// 插件 ID（网关路由前缀）
const PLUGIN_ID: &str = "com.bedcode.scheduler";
/// 默认端口（与宿主配置缺省一致）
const DEFAULT_PORT: u16 = 8765;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match parse_args(&args) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("bedtask: {}", msg);
            eprintln!("{}", usage());
            return 1;
        }
    };
    if opts.help {
        println!("{}", usage());
        return 0;
    }

    let port = std::env::var("BEDCODE_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);

    match execute(&opts, port) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("bedtask: {}", e);
            1
        }
    }
}

// ==================== 参数解析 ====================

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Opts {
    pub help: bool,
    pub json: bool,
    pub command: String,
    pub positional: Vec<String>,
    pub cron: Option<String>,
    pub script: Option<String>,
    pub exec: Option<String>,
    pub name: Option<String>,
    pub cwd: Option<String>,
    pub env: Option<String>,
    pub timeout: Option<u64>,
    pub once: Option<bool>,
    pub limit: Option<u64>,
}

/// 解析命令行参数（flag 与位置参数混合；`--flag value` 取下一个 token）
pub fn parse_args(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts::default();
    let mut it = args.iter().peekable();
    // 第一个非 flag token 是命令
    let mut command_seen = false;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => o.help = true,
            "--json" => o.json = true,
            "--once" => o.once = Some(true),
            "--no-once" => o.once = Some(false),
            "--cron" | "--script" | "--exec" | "--name" | "--cwd" | "--env" => {
                let value = it
                    .next()
                    .ok_or_else(|| format!("{} requires a value", arg))?;
                match arg.as_str() {
                    "--cron" => o.cron = Some(value.clone()),
                    "--script" => o.script = Some(value.clone()),
                    "--exec" => o.exec = Some(value.clone()),
                    "--name" => o.name = Some(value.clone()),
                    "--cwd" => o.cwd = Some(value.clone()),
                    "--env" => o.env = Some(value.clone()),
                    _ => unreachable!(),
                }
            }
            "--timeout" => {
                let value = it
                    .next()
                    .ok_or_else(|| "--timeout requires a value".to_string())?;
                o.timeout = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| format!("--timeout must be a positive integer: {}", value))?,
                );
            }
            "--limit" => {
                let value = it
                    .next()
                    .ok_or_else(|| "--limit requires a value".to_string())?;
                o.limit = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| format!("--limit must be a positive integer: {}", value))?,
                );
            }
            _ if arg.starts_with('-') => {
                return Err(format!("unknown option: {}", arg));
            }
            _ => {
                if !command_seen {
                    o.command = arg.clone();
                    command_seen = true;
                } else {
                    o.positional.push(arg.clone());
                }
            }
        }
    }
    Ok(o)
}

/// 解析 `K=V,K2=V2` 环境变量列表为 JSON 对象
pub fn parse_env_list(s: &str) -> Result<Value, String> {
    let mut obj = serde_json::Map::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (k, v) = part
            .split_once('=')
            .ok_or_else(|| format!("env entry must be K=V: {}", part))?;
        obj.insert(k.trim().to_string(), Value::String(v.trim().to_string()));
    }
    Ok(Value::Object(obj))
}

/// 用法说明
pub fn usage() -> &'static str {
    "Usage:\n  \
     bedtask add --cron \"<6段 cron>\" (--script <path> | --exec \"<cmd>\") [--name <n>] [--cwd <dir>] [--env K=V,...] [--timeout <sec>] [--once]\n  \
     bedtask list [--json]\n  \
     bedtask show <id> [--json]\n  \
     bedtask remove <id>\n  \
     bedtask edit <id> [--cron <e>] [--exec <v>] [--name <n>] [--cwd <d>] [--env K=V] [--timeout <s>] [--once|--no-once]\n  \
     bedtask enable <id> | bedtask disable <id>\n  \
     bedtask run <id>\n  \
     bedtask logs <id> [--limit N] [--json]\n  \
     Env: BEDCODE_PORT (default 8765)"
}

// ==================== 命令执行 ====================

fn execute(opts: &Opts, port: u16) -> Result<(), String> {
    match opts.command.as_str() {
        "add" => cmd_add(opts, port),
        "list" => cmd_list(opts, port),
        "show" => cmd_show(opts, port),
        "remove" => cmd_remove(opts, port),
        "edit" => cmd_edit(opts, port),
        "enable" | "disable" => cmd_set_enabled(opts, port, opts.command == "enable"),
        "run" => cmd_run(opts, port),
        "logs" => cmd_logs(opts, port),
        "" => Err("missing command".to_string()),
        other => Err(format!("unknown command: {}", other)),
    }
}

/// 调用网关端点，返回 data（HTTP 非 200 或 body.code != 0 时报错退出）
fn call(port: u16, method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
    let body_str = body.map(|b| b.to_string());
    let resp = http::request(port, method, path, body_str.as_deref())?;
    let status = resp["status"].as_u64().unwrap_or(0);
    let code = resp["body"]["code"].as_i64().unwrap_or(0);
    if status != 200 || code != 0 {
        let message = resp["body"]["message"].as_str().unwrap_or("unknown error");
        return Err(format!("{} {}: {} (HTTP {})", method, path, message, status));
    }
    Ok(resp["body"]["data"].clone())
}

fn job_path(sub: &str, query: &[(&str, &str)]) -> String {
    let mut p = format!("/api/plugin/{}/task-scheduler/{}", PLUGIN_ID, sub);
    if !query.is_empty() {
        p.push('?');
        p.push_str(
            &query
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&"),
        );
    }
    p
}

fn cmd_add(opts: &Opts, port: u16) -> Result<(), String> {
    let schedule = opts
        .cron
        .as_deref()
        .ok_or_else(|| "add requires --cron \"<6段 cron>\"".to_string())?;
    let (exec_type, exec_value) = match (opts.script.as_deref(), opts.exec.as_deref()) {
        (Some(s), None) => ("script", s),
        (None, Some(e)) => ("inline", e),
        (Some(_), Some(_)) => {
            return Err("add: use either --script or --exec, not both".to_string());
        }
        (None, None) => {
            return Err("add requires --script <path> or --exec \"<cmd>\"".to_string());
        }
    };
    let mut body = serde_json::json!({
        "schedule": schedule,
        "exec_type": exec_type,
        "exec_value": exec_value,
    });
    if let Some(name) = &opts.name {
        body["name"] = Value::String(name.clone());
    }
    if let Some(cwd) = &opts.cwd {
        body["cwd"] = Value::String(cwd.clone());
    }
    if let Some(env) = &opts.env {
        body["env"] = parse_env_list(env)?;
    }
    if let Some(t) = opts.timeout {
        body["timeout_sec"] = Value::from(t as i64);
    }
    if let Some(once) = opts.once {
        body["once"] = Value::Bool(once);
    }

    let data = call(port, "POST", &job_path("add", &[]), Some(body))?;
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
    } else {
        let id = data["job_id"].as_str().unwrap_or("?");
        println!("Created job {}", id);
        println!("Schedule: {}  Next run: {}", schedule, data["next_at"].as_str().unwrap_or("-"));
    }
    Ok(())
}

fn cmd_list(opts: &Opts, port: u16) -> Result<(), String> {
    let data = call(port, "GET", &job_path("list", &[]), None)?;
    let jobs = data["jobs"].as_array().cloned().unwrap_or_default();
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
        return Ok(());
    }
    if jobs.is_empty() {
        println!("No scheduled jobs (use `bedtask add` to create one)");
        return Ok(());
    }
    for j in &jobs {
        let id = j["id"].as_str().unwrap_or("?");
        let name = j["name"].as_str().unwrap_or("-");
        let schedule = j["schedule"].as_str().unwrap_or("?");
        let enabled = if j["enabled"].as_i64().unwrap_or(0) != 0 { "enabled" } else { "disabled" };
        let next_at = j["next_at"].as_str().unwrap_or("-");
        let last = j["last_status"].as_str().unwrap_or("-");
        println!(
            "{:<34} {:<16} {:<16} {:<9} next={:<20} last={}",
            id, name, schedule, enabled, next_at, last
        );
    }
    Ok(())
}

fn cmd_show(opts: &Opts, port: u16) -> Result<(), String> {
    let id = opts.positional.first().ok_or_else(|| "show requires <id>".to_string())?;
    let data = call(port, "GET", &job_path("show", &[("job_id", id)]), None)?;
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
        return Ok(());
    }
    let j = &data["job"];
    println!("id:          {}", j["id"].as_str().unwrap_or("?"));
    println!("name:        {}", j["name"].as_str().unwrap_or("-"));
    println!("schedule:    {}", j["schedule"].as_str().unwrap_or("?"));
    println!(
        "exec:        {} {}",
        j["exec_type"].as_str().unwrap_or("?"),
        j["exec_value"].as_str().unwrap_or("?")
    );
    println!("cwd:         {}", j["cwd"].as_str().unwrap_or("-"));
    println!("timeout:     {}s", j["timeout_sec"].as_i64().unwrap_or(0));
    println!(
        "enabled:     {}",
        if j["enabled"].as_i64().unwrap_or(0) != 0 { "yes" } else { "no" }
    );
    println!(
        "once:        {}",
        if j["once"].as_i64().unwrap_or(0) != 0 { "yes" } else { "no" }
    );
    println!("next_at:     {}", j["next_at"].as_str().unwrap_or("-"));
    println!("created_at:  {}", j["created_at"].as_str().unwrap_or("-"));
    println!("recent executions:");
    let execs = data["executions"].as_array().cloned().unwrap_or_default();
    if execs.is_empty() {
        println!("  (none)");
    }
    for e in &execs {
        println!(
            "  {:<12} {:<9} trigger={:<7} started={:<20} finished={:<20} exit={}",
            e["status"].as_str().unwrap_or("?"),
            e["exec_id"].as_str().unwrap_or("?"),
            e["trigger"].as_str().unwrap_or("?"),
            e["started_at"].as_str().unwrap_or("-"),
            e["finished_at"].as_str().unwrap_or("-"),
            e["exit_code"].as_i64().map(|c| c.to_string()).unwrap_or_else(|| "-".into()),
        );
    }
    Ok(())
}

fn cmd_remove(opts: &Opts, port: u16) -> Result<(), String> {
    let id = opts.positional.first().ok_or_else(|| "remove requires <id>".to_string())?;
    call(port, "DELETE", &job_path("remove", &[("job_id", id)]), None)?;
    println!("Removed job {}", id);
    Ok(())
}

fn cmd_edit(opts: &Opts, port: u16) -> Result<(), String> {
    let id = opts.positional.first().ok_or_else(|| "edit requires <id>".to_string())?;
    let mut body = serde_json::json!({ "job_id": id });
    if let Some(cron) = &opts.cron {
        body["schedule"] = Value::String(cron.clone());
    }
    if let Some(exec) = &opts.exec {
        body["exec_type"] = Value::String("inline".into());
        body["exec_value"] = Value::String(exec.clone());
    } else if let Some(script) = &opts.script {
        body["exec_type"] = Value::String("script".into());
        body["exec_value"] = Value::String(script.clone());
    }
    if let Some(name) = &opts.name {
        body["name"] = Value::String(name.clone());
    }
    if let Some(cwd) = &opts.cwd {
        body["cwd"] = Value::String(cwd.clone());
    }
    if let Some(env) = &opts.env {
        body["env"] = parse_env_list(env)?;
    }
    if let Some(t) = opts.timeout {
        body["timeout_sec"] = Value::from(t as i64);
    }
    if let Some(once) = opts.once {
        body["once"] = Value::Bool(once);
    }
    call(port, "POST", &job_path("edit", &[]), Some(body))?;
    println!("Updated job {}", id);
    Ok(())
}

fn cmd_set_enabled(opts: &Opts, port: u16, enabled: bool) -> Result<(), String> {
    let cmd = if enabled { "enable" } else { "disable" };
    let id = opts
        .positional
        .first()
        .ok_or_else(|| format!("{} requires <id>", cmd))?;
    call(port, "POST", &job_path(cmd, &[("job_id", id)]), None)?;
    println!("{} job {}", if enabled { "Enabled" } else { "Disabled" }, id);
    Ok(())
}

fn cmd_run(opts: &Opts, port: u16) -> Result<(), String> {
    let id = opts.positional.first().ok_or_else(|| "run requires <id>".to_string())?;
    let data = call(port, "POST", &job_path("run", &[("job_id", id)]), None)?;
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
    } else {
        println!("Triggered job {} (execution {})", id, data["exec_id"].as_str().unwrap_or("?"));
    }
    Ok(())
}

fn cmd_logs(opts: &Opts, port: u16) -> Result<(), String> {
    let id = opts.positional.first().ok_or_else(|| "logs requires <id>".to_string())?;
    let mut query = vec![("job_id", id.as_str())];
    let limit_str;
    if let Some(limit) = opts.limit {
        limit_str = limit.to_string();
        query.push(("limit", limit_str.as_str()));
    }
    let data = call(port, "GET", &job_path("logs", &query), None)?;
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
        return Ok(());
    }
    let execs = data["executions"].as_array().cloned().unwrap_or_default();
    if execs.is_empty() {
        println!("No executions for job {}", id);
        return Ok(());
    }
    for e in &execs {
        println!(
            "{:<10} trigger={:<7} started={:<20} finished={:<20} exit={:<6} {}",
            e["status"].as_str().unwrap_or("?"),
            e["trigger"].as_str().unwrap_or("?"),
            e["started_at"].as_str().unwrap_or("-"),
            e["finished_at"].as_str().unwrap_or("-"),
            e["exit_code"].as_i64().map(|c| c.to_string()).unwrap_or_else(|| "-".into()),
            e["output_path"].as_str().unwrap_or(""),
        );
    }
    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn a(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_add_command() {
        let o = parse_args(&a(&[
            "add",
            "--cron",
            "0 0 9 * * *",
            "--script",
            "/home/u/backup.sh",
            "--name",
            "backup",
            "--timeout",
            "30",
            "--once",
        ]))
        .unwrap();
        assert_eq!(o.command, "add");
        assert_eq!(o.cron.as_deref(), Some("0 0 9 * * *"));
        assert_eq!(o.script.as_deref(), Some("/home/u/backup.sh"));
        assert_eq!(o.exec, None);
        assert_eq!(o.name.as_deref(), Some("backup"));
        assert_eq!(o.timeout, Some(30));
        assert_eq!(o.once, Some(true));
    }

    #[test]
    fn parse_flags_anywhere_and_positionals() {
        let o = parse_args(&a(&["--json", "show", "abc123", "--limit", "5"])).unwrap();
        assert!(o.json);
        assert_eq!(o.command, "show");
        assert_eq!(o.positional, vec!["abc123"]);
        assert_eq!(o.limit, Some(5));
    }

    #[test]
    fn parse_rejects_unknown_option_and_missing_value() {
        assert!(parse_args(&a(&["list", "--nope"])).is_err());
        assert!(parse_args(&a(&["add", "--cron"])).is_err());
        assert!(parse_args(&a(&["add", "--timeout", "abc"])).is_err());
    }

    #[test]
    fn parse_env_list_builds_object() {
        let v = parse_env_list("A=1, B=hello world ,C=").unwrap();
        assert_eq!(v["A"], "1");
        assert_eq!(v["B"], "hello world");
        assert_eq!(v["C"], "");
        // 非法条目（无 =）
        assert!(parse_env_list("A=1,BAD").is_err());
    }

    #[test]
    fn job_path_format() {
        let p = job_path("show", &[("job_id", "abc")]);
        assert_eq!(
            p,
            "/api/plugin/com.bedcode.scheduler/task-scheduler/show?job_id=abc"
        );
        assert_eq!(
            job_path("list", &[]),
            "/api/plugin/com.bedcode.scheduler/task-scheduler/list"
        );
    }
}
