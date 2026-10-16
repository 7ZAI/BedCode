//! codex 应用目标（`~/.codex/config.toml`，TOML 文本级 splice）
//!
//! # 为什么是这个形态（与 pi / opencode 的根本差异）
//!
//! - **格式**：TOML，不是 JSONC。写入仍是文本级 splice（不整文件反序列化重写），
//!   `#` 注释与用户手写内容逐字保留
//! - **没有模型清单**（实测 2026-10-04）：`[model_providers.<n>]` 里写
//!   `models = [...]` 会被 codex 直接拒（`unknown configuration field`，
//!   `--strict-config` 同样拦）。codex 只有**全局单值** `model` /
//!   `model_provider`，一次指向一个模型——所以「应用供应商」对 codex 的
//!   语义降级为「登记 provider + 设为当前模型（预设首个模型）」
//! - **凭据是间接的**：codex 配置里只写**环境变量名**（`env_key = "..."`），
//!   真值由用户的 shell 环境提供。本模块只写名字，绝不碰用户的 env 文件或
//!   shell 配置
//! - **只讲 Responses**：`wire_api` 的 `chat` 在 0.160.0 已被移除（实测
//!   `no longer supported`，且不发任何请求）。因此 anthropic / gemini 方言
//!   在 codex 侧无对应写法，显性拒绝而不是硬塞一个必然失败的配置
//!
//! # 结构性约束（TOML splice 的坑，写之前先读这段）
//!
//! ① 顶层键必须出现在**第一个 `[表头]` 之前**；出现之后再写 `model = ...`
//!    就变成了那个表的键——所以 `set_top_level` 只在首个表头之前的区域找/写，
//!    天然避开「误改同名键」（别的表里也可能有 `model =`）
//! ② 追加 `[model_providers.x]` 永远合法（表互相独立，追加到文件尾即可）；
//!    改已有表则必须在其 span 内（到下一个表头为止）替换，不能越过边界
//! ③ 替换整行会丢掉该行的行尾注释（本模块写的 4 个字段属受控字段，可接受；
//!    用户手写说明的注释通常独占整行，逐字保留）

use super::paths::codex_config_path;
use bedcode_plugin_api::host::{HostFs, HostLog};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

// ==================== TOML 纯函数（splice / 转义 / 校验） ====================

/// TOML 基本字符串转义（`\` 与 `"`；控制字符走 `\uXXXX`）
pub(crate) fn toml_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// 表头键是否可裸写（TOML bare key：字母数字 / `_` / `-`）
fn is_bare_key(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// 表头渲染：bare key 直接写，含特殊字符则加引号（与 codex 自己写出的形态一致）
fn render_header(header: &str) -> String {
    header
        .split('.')
        .map(|seg| {
            if is_bare_key(seg) {
                seg.to_string()
            } else {
                toml_quote(seg)
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// 行是否表头（首个非空白字符是 `[`；`[[` 数组表也算）
fn is_header_line(line: &str) -> bool {
    line.trim_start().starts_with('[')
}

/// 行是否形如 `key = ...`（键在行首，允许前后空白；排除注释行）
fn key_of_line(line: &str) -> Option<&str> {
    let t = line.trim_start();
    if t.starts_with('#') || is_header_line(t) {
        return None;
    }
    let eq = t.find('=')?;
    let key = t[..eq].trim();
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

/// 定位表头行的两个合法写法（`[a.b]` 与 `[a."b"]`）之一
fn find_header(lines: &[String], header: &str) -> Option<usize> {
    let want_plain = format!("[{header}]");
    let want_quoted = format!("[{}]", render_header(header));
    lines
        .iter()
        .position(|l| is_header_line(l) && (l.trim() == want_plain || l.trim() == want_quoted))
}

/// 表 span：表头行之后、下一个表头行（或文件尾）之前
fn table_span_end(lines: &[String], header_idx: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(header_idx + 1)
        .find(|(_, l)| is_header_line(l))
        .map(|(i, _)| i)
        .unwrap_or(lines.len())
}

/// 追加位置：文件尾但保留结尾换行（`split('\n')` 的末位空串即结尾换行）
fn append_idx(lines: &[String]) -> usize {
    if lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.len() - 1
    } else {
        lines.len()
    }
}

/// upsert 一个表（`[header]` 下的若干 `key = value`）
///
/// 已存在 → 在其 span 内逐键替换/补写（不动表外的任何字节）；不存在 → 追加到
/// 文件尾（带一行来源标注注释，便于日后辨认是谁写的）。entries 的 value 已是
/// **渲染好的 TOML 值**（字符串请先过 [`toml_quote`]）。
pub(crate) fn upsert_table(text: &str, header: &str, entries: &[(&str, &str)]) -> String {
    let mut lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    match find_header(&lines, header) {
        Some(h_idx) => {
            // 逐键处理并**每次重算 span 终点**：插入会让行号漂移，
            // 预先算好再批量改会写错位置（且「替换」会被写成「再插一行」）
            for (key, value) in entries {
                let end = table_span_end(&lines, h_idx);
                let new_line = format!("{key} = {value}");
                let hit = (h_idx + 1..end).find(|&i| key_of_line(&lines[i]) == Some(*key));
                match hit {
                    Some(i) => lines[i] = new_line,
                    None => lines.insert(end, new_line),
                }
            }
        }
        None => {
            let at = append_idx(&lines);
            let mut block = vec!["# Agent Hub: 供应商条目（应用时写入）".to_string()];
            block.push(format!("[{}]", render_header(header)));
            for (key, value) in entries {
                block.push(format!("{key} = {value}"));
            }
            block.push(String::new());
            for (offset, l) in block.into_iter().enumerate() {
                lines.insert(at + offset, l);
            }
        }
    }
    lines.join("\n")
}

/// 写/改一个**顶层**键（只在首个表头之前的区域操作——见模块头约束①）
pub(crate) fn set_top_level(text: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    let top_end = lines
        .iter()
        .position(|l| is_header_line(l))
        .unwrap_or(lines.len());
    let hit = (0..top_end).find(|&i| key_of_line(&lines[i]) == Some(key));
    let new_line = format!("{key} = {value}");
    match hit {
        Some(i) => lines[i] = new_line,
        // 顶层区域末尾（= 首个表头之前；没有表头则文件尾，保留结尾换行）
        None => {
            let at = if top_end == lines.len() {
                append_idx(&lines)
            } else {
                top_end
            };
            lines.insert(at, new_line);
        }
    }
    lines.join("\n")
}

/// 环境变量名校验（`env_key` 的取值面：必须是合法 shell 变量名）
pub(crate) fn validate_env_key_name(name: &str) -> Result<&str, String> {
    let n = name.trim();
    let mut chars = n.chars();
    let ok_first = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_');
    let ok_rest = chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if n.is_empty() || !ok_first || !ok_rest {
        return Err("env var name must match [A-Za-z_][A-Za-z0-9_]*".to_string());
    }
    Ok(n)
}

/// codex 应用计划（纯函数：写入内容 + 三道守卫都在这里定）
///
/// 守卫顺序按「先说最可行动的」排：方言无对应写法 → env 变量名非法 → 无模型
#[derive(Debug)]
pub(crate) struct CodexPlan {
    /// 表头键（`model_providers.<条目名>`）
    pub table: String,
    /// 将被设为当前模型的 id（预设首个模型）
    pub model: String,
    /// provider 条目的四个受控字段（已过校验，未渲染）
    pub provider: String,
    pub base_url: String,
    pub env_key: String,
    pub target_name: String,
}

/// provider 条目渲染为 `(key, TOML 值)`（写入方直接拿去 upsert）
impl CodexPlan {
    pub(crate) fn entries(&self) -> [(&'static str, String); 4] {
        [
            ("name", toml_quote(&self.provider)),
            ("base_url", toml_quote(&self.base_url)),
            ("env_key", toml_quote(&self.env_key)),
            // codex 0.160.0 已移除 chat（实测 no longer supported 且不发请求）
            ("wire_api", toml_quote("responses")),
        ]
    }
}

/// 计划构建失败：`(reason, 诊断原文)`（reason 为前端查 i18n 的分类码）
pub(crate) fn plan_apply(
    target_name: &str,
    name: &str,
    base_url: &str,
    api_style: &str,
    models: &[String],
    env_key: &str,
) -> Result<CodexPlan, (&'static str, String)> {
    // 只讲 Responses：anthropic / gemini 方言在 codex 侧无对应写法（硬塞=必失败）
    if matches!(api_style, "anthropic" | "gemini") {
        return Err((
            "unsupportedDialect",
            format!("apiStyle {api_style} has no codex wire form"),
        ));
    }
    // env_key 只写变量名：必须是合法 shell 变量名
    let env_name = validate_env_key_name(env_key)
        .map_err(|e| ("invalidEnvKey", format!("codex env var name: {e}")))?
        .to_string();
    // 无模型 = 无法指向任何一个模型（codex 无模型清单可登记）
    let model = models
        .first()
        .cloned()
        .ok_or_else(|| ("noModels", format!("preset {target_name} has no models")))?;
    Ok(CodexPlan {
        table: format!("model_providers.{target_name}"),
        model,
        provider: name.to_string(),
        base_url: base_url.to_string(),
        env_key: env_name,
        target_name: target_name.to_string(),
    })
}

/// 只读解析顶层 `model` / `model_provider` 与已登记的 provider 名
///
/// 读路径不引 TOML 解析器（零依赖）：按行取顶层键 + 收集
/// `[model_providers.<n>]` 表头。解析不出来一律给 `null` / 空数组（不猜）。
pub(crate) fn parse_codex_config(text: &str) -> Value {
    let mut model = Value::Null;
    let mut model_provider = Value::Null;
    let mut providers: Vec<String> = Vec::new();
    let mut in_model_providers = false;
    for line in text.split('\n') {
        let t = line.trim();
        if is_header_line(t) {
            let header = t.trim_start_matches('[').trim_end_matches(']').trim();
            in_model_providers = header.starts_with("model_providers.");
            if in_model_providers {
                let raw = header.trim_start_matches("model_providers.").trim();
                if !raw.is_empty() {
                    let unq = raw.trim_matches('"');
                    if !providers.iter().any(|p| p == unq) {
                        providers.push(unq.to_string());
                    }
                }
            }
            continue;
        }
        // 顶层键只在首个表头之前有效（TOML 语义，见模块头约束①）
        if in_model_providers {
            continue;
        }
        match key_of_line(t) {
            Some("model") if model.is_null() => model = scalar_of(t),
            Some("model_provider") if model_provider.is_null() => model_provider = scalar_of(t),
            _ => {}
        }
    }
    json!({ "model": model, "modelProvider": model_provider, "providers": providers })
}

/// 行值 → JSON 标量（带引号去引号；其余原样字符串）
fn scalar_of(line: &str) -> Value {
    let t = line.trim();
    let Some(eq) = t.find('=') else {
        return Value::Null;
    };
    let v = t[eq + 1..].trim();
    json!(v.trim_matches('"').to_string())
}

/// codex 只读视图（读时现查；读失败降级为空视图，不阻断页签）
pub(super) fn codex_view(h: &WasmHost, home: &str) -> Value {
    match h.fs_read(&codex_config_path(home)) {
        Ok(Some(text)) => parse_codex_config(&text),
        Ok(None) => {
            json!({ "model": null, "modelProvider": null, "providers": [], "missing": true })
        }
        Err(e) => {
            h.log_warn(&format!("providers: read codex config failed: {e}"));
            json!({ "model": null, "modelProvider": null, "providers": [], "denied": true })
        }
    }
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实形态的 config.toml（手写中文注释必须逐字保留）
    const REAL: &str = r#"# TokenPlan — 供应商配置 2026-10-04
# 注意: 当前上游不支持 previous_response_id,故必须走 stateless 全量历史请求
model = "deepseek-v4-pro-0813"
model_provider = "tokenplan"
model_reasoning_effort = "medium"

[model_providers.tokenplan]
name = "TokenPlan"
base_url = "https://discovery-api.intern-ai.org.cn/v1"
env_key = "TOKENPLAN_API_KEY"
wire_api = "responses"

[features]
hooks = true

[projects."/home/binblink/project/tauriProject/BedCode"]
trust_level = "trusted"

[hooks.state]

[hooks.state."/home/binblink/.codex/hooks.json:session_start:0:0"]
trusted_hash = "sha256:f6a317fc90538a009fbd2137d70d945393bfaa4c37dcdf4601d82864cdd91d93"
"#;

    /// 新增 provider 表：追加到文件尾（表头之前的内容零改动）
    #[test]
    fn appends_new_provider_table() {
        let out = upsert_table(
            REAL,
            "model_providers.inkstone",
            &[
                ("name", &toml_quote("InkStone")),
                ("base_url", &toml_quote("https://b/v1")),
            ],
        );
        assert!(out.contains("[model_providers.inkstone]"));
        assert!(out.contains(r#"base_url = "https://b/v1""#));
        // 原有内容逐字保留
        assert!(out.contains("# 注意: 当前上游不支持 previous_response_id"));
        assert!(out.contains(r#"model_provider = "tokenplan""#));
        assert!(out.contains("[features]"));
        // 结尾换行保留（不是文件最后一行）
        assert!(out.ends_with('\n'), "trailing newline must survive");
        // 新表在 features 之后，不破坏既有表
        let ink = out.find("[model_providers.inkstone]").unwrap();
        assert!(ink > out.find("[features]").unwrap());
    }

    /// 改已有 provider 表：只动 span 内的受控字段，注释与相邻表不动
    #[test]
    fn replaces_keys_inside_existing_table_only() {
        let out = upsert_table(
            REAL,
            "model_providers.tokenplan",
            &[
                ("base_url", &toml_quote("https://new/v1")),
                ("env_key", &toml_quote("NEW_KEY")),
            ],
        );
        assert!(out.contains(r#"base_url = "https://new/v1""#));
        assert!(out.contains(r#"env_key = "NEW_KEY""#));
        assert!(!out.contains("https://discovery-api.intern-ai.org.cn"));
        // 未受控字段保留
        assert!(out.contains(r#"name = "TokenPlan""#));
        assert!(out.contains(r#"wire_api = "responses""#));
        // 相邻表没被挪动/吞掉
        assert!(out.contains("[features]"));
        assert!(out.contains("hooks = true"));
        assert_eq!(out.matches("[model_providers.tokenplan]").count(), 1);
    }

    /// 真实文件的难缠尾部：带引号的表头（路径里含 `/` `.` `:`）与空表。
    /// 追加的provider 表必须落在这堆表**之后**且不影响它们（TOML 里表头
    /// 不可重开，写错位置会把后续键吸进别的表——静默改坏用户配置）
    #[test]
    fn real_file_tail_with_quoted_headers_survives() {
        let out = upsert_table(
            REAL,
            "model_providers.inkstone",
            &[
                ("name", &toml_quote("InkStone")),
                ("base_url", &toml_quote("https://b/v1")),
            ],
        );
        let ink = out.find("[model_providers.inkstone]").unwrap();
        let hooks = out
            .find(r#"[hooks.state."/home/binblink/.codex/hooks.json:session_start:0:0"]"#)
            .unwrap();
        assert!(ink > hooks, "new table must come after existing tables");
        // hooks 段内容逐字保留（sha256 值不能被截断/搬家）
        assert!(out.contains(r#"trusted_hash = "sha256:f6a317fc90538a009fbd2137d70d945393bfaa4c37dcdf4601d82864cdd91d93""#));
        // 只读视图仍能正确识别 provider 清单（不被带引号的表头带偏）
        let v = parse_codex_config(&out);
        assert_eq!(v["modelProvider"], "tokenplan");
        assert_eq!(
            v["providers"],
            json!(["tokenplan", "inkstone"]),
            "quoted non-model_providers tables must not leak into the provider list"
        );
    }

    /// 顶层键：改已有键 + 缺键时补在首个表头之前（不能补到表后，那会变成别的表的键）
    #[test]
    fn sets_top_level_keys_above_first_header() {
        let swapped = set_top_level(
            &set_top_level(REAL, "model", &toml_quote("m-1")),
            "model_provider",
            &toml_quote("inkstone"),
        );
        assert!(swapped.contains(r#"model = "m-1""#));
        assert!(swapped.contains(r#"model_provider = "inkstone""#));
        assert!(swapped.find("model = ").unwrap() < swapped.find("[model_providers.").unwrap());

        // 表头开头的文件也要能补顶层键
        let bare = "[features]\nhooks = true\n";
        let out = set_top_level(bare, "model", &toml_quote("m-1"));
        assert!(out.starts_with("model = \"m-1\"\n"), "got: {out}");
        assert!(out.contains("[features]"));
    }

    /// 顶层写入不得误伤同名键：表内的 `model = ` 不是顶层键
    #[test]
    fn top_level_write_ignores_keys_inside_tables() {
        let text = "model = \"top\"\n\n[some.table]\nmodel = \"inner\"\n";
        let out = set_top_level(text, "model", &toml_quote("changed"));
        assert!(out.contains("model = \"changed\""));
        assert!(out.contains("model = \"inner\""), "table内同名键必须原样");
    }

    /// 表名含特殊字符时表头加引号（与 codex 自己的写法一致）
    #[test]
    fn header_quoting_for_non_bare_names() {
        let out = upsert_table("", "model_providers.my provider", &[("name", "\"x\"")]);
        assert!(
            out.contains(r#"[model_providers."my provider"]"#),
            "got: {out}"
        );
    }

    /// 值转义：引号 / 反斜杠 / 换行不破坏 TOML
    #[test]
    fn toml_quote_escapes() {
        assert_eq!(toml_quote("a\"b"), r#""a\"b""#);
        assert_eq!(toml_quote("a\\b"), r#""a\\b""#);
        assert_eq!(toml_quote("a\nb"), r#""a\nb""#);
        assert_eq!(toml_quote("plain"), r#""plain""#);
    }

    /// env_key 取值面：只收合法 shell 变量名
    #[test]
    fn env_key_name_validation() {
        assert!(validate_env_key_name("TOKENPLAN_API_KEY").is_ok());
        assert!(validate_env_key_name("_x1").is_ok());
        assert!(validate_env_key_name(" has space").is_err());
        assert!(validate_env_key_name("1ABC").is_err());
        assert!(validate_env_key_name("A-B").is_err());
        assert!(validate_env_key_name("").is_err());
    }

    /// 计划：正常路径 → 表头 / 当前模型 / 四个受控字段（wire_api 锁 responses）
    #[test]
    fn plan_targets_first_model_and_responses_wire() {
        let plan = plan_apply(
            "inkstone",
            "InkStone",
            "https://b/v1",
            "openai",
            &["glm-5.2".to_string(), "kimi-k3".to_string()],
            "INKSTONE_API_KEY",
        )
        .expect("plan");
        assert_eq!(plan.table, "model_providers.inkstone");
        assert_eq!(plan.model, "glm-5.2", "codex 只能指向一个模型 = 预设首个");
        let entries = plan.entries();
        assert_eq!(entries[0], ("name", r#""InkStone""#.to_string()));
        assert_eq!(entries[1], ("base_url", r#""https://b/v1""#.to_string()));
        assert_eq!(entries[2], ("env_key", r#""INKSTONE_API_KEY""#.to_string()));
        assert_eq!(entries[3], ("wire_api", r#""responses""#.to_string()));
    }

    /// 三道守卫：方言无对应写法 / env 变量名非法 / 无模型——各有分类码，
    /// 前端据此给不同文案（不混成一句「写入失败」）
    #[test]
    fn plan_guards_report_actionable_reasons() {
        let models = vec!["m1".to_string()];
        let err = plan_apply("p", "P", "https://b/v1", "anthropic", &models, "P_KEY").unwrap_err();
        assert_eq!(err.0, "unsupportedDialect");
        let err = plan_apply("p", "P", "https://b/v1", "gemini", &models, "P_KEY").unwrap_err();
        assert_eq!(err.0, "unsupportedDialect");
        let err = plan_apply("p", "P", "https://b/v1", "openai", &models, "1BAD").unwrap_err();
        assert_eq!(err.0, "invalidEnvKey");
        let err = plan_apply("p", "P", "https://b/v1", "openai", &[], "P_KEY").unwrap_err();
        assert_eq!(err.0, "noModels");
        // openai / custom 方言均放行（custom 也走 responses——codex 没别的选择）
        assert!(plan_apply("p", "P", "https://b/v1", "custom", &models, "P_KEY").is_ok());
        // env_key 名带首尾空白：trim 后可用
        assert!(plan_apply("p", "P", "https://b/v1", "openai", &models, "  P_KEY  ").is_ok());
    }

    /// 只读解析：顶层 model / model_provider + provider 清单；空文件不猜
    #[test]
    fn parses_current_model_and_providers() {
        let v = parse_codex_config(REAL);
        assert_eq!(v["model"], "deepseek-v4-pro-0813");
        assert_eq!(v["modelProvider"], "tokenplan");
        assert_eq!(v["providers"], json!(["tokenplan"]));

        let empty = parse_codex_config("");
        assert!(empty["model"].is_null());
        assert_eq!(empty["providers"], json!([]));

        // 两个 provider + 重复表头去重
        let two = parse_codex_config(
            "[model_providers.a]\nname=\"A\"\n[model_providers.b]\nname=\"B\"\n[model_providers.a]\n",
        );
        assert_eq!(two["providers"], json!(["a", "b"]));
    }

    /// 跨形状串联：应用供应商后的文本仍可被同一套纯函数读回（写读同源自洽）
    #[test]
    fn write_then_read_roundtrip() {
        let applied = set_top_level(
            &upsert_table(
                REAL,
                "model_providers.inkstone",
                &[
                    ("name", &toml_quote("InkStone")),
                    ("base_url", &toml_quote("https://b/v1")),
                    ("env_key", &toml_quote("INKSTONE_API_KEY")),
                    ("wire_api", "\"responses\""),
                ],
            ),
            "model",
            &toml_quote("glm-5.2"),
        );
        let applied = set_top_level(&applied, "model_provider", &toml_quote("inkstone"));
        let v = parse_codex_config(&applied);
        assert_eq!(v["model"], "glm-5.2");
        assert_eq!(v["modelProvider"], "inkstone");
        assert!(v["providers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "inkstone"));
        // 目标配置不含 models 列表字段（codex 会拒）
        assert!(!applied.contains("models = "));
    }
}
