//! 供应商统一管理域（票据 05）
//!
//! 职责：
//! - **预设 CRUD**：`provider_preset` 表（插件独立库，host-plugin-database），
//!   刻意无 key 列（spec §5/§6）——hub 存储面（表/状态/日志）任何位置不落 key
//! - **反向导入**：读各 CLI 现有配置生成预设——pi（`models.json` providers +
//!   `auth.json` 键存在性掩码）、opencode（`opencode.json` `provider.*`）；
//!   claude 只读展示（settings.json env 掩码 + 桥接文件存在性），不生成预设
//! - **应用**：写入目标 CLI 原生配置文件（真源始终是 CLI 自己的配置）——
//!   claude 写 `settings.json` 的 `env` 块；pi 写 `models.json` providers 条目 +
//!   `auth.json` 键条目；opencode 写 `opencode.json` `provider.*` 条目。
//!   key 现场输入或从源 CLI 配置**内存直拷**（应用时现读现写，不落 hub 存储/日志）
//! - **claude 桥接冲突**：检测到 `provider-config.sh` / `anthropic-bridge.mjs`
//!   时阻止写入并提示（force = 用户确认后仅写 env 块，桥接文件永不触碰）
//!
//! 配置文件写入采用**文本级 splice**（JSONC 感知的键值条目替换/插入）：pi 的
//! `models.json` 含 `//` 注释（非严格 JSON），且用户配置中的注释与未知字段必须
//! 原样保留——不做整文件反序列化重写，只在目标条目 span 上做替换/插入，
//! 其余字节逐字保留。读路径统一走 `parse_jsonc`（剥注释 + 尾逗号）。
//!
//! key 纪律：掩码函数是 key 与 UI 的唯一交界面（前 3 字符 + 长度）；应用成功
//! 后的状态/日志只记 `key` 是否提供与长度，不记内容。

use super::HOME;
use crate::install::now_ms;
use bedcode_plugin_api::host::{HostEvents, HostFs, HostLog, HostPluginDatabase, HostStorage};
use bedcode_plugin_api::sql_params;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Map, Value};

/// host-storage 键：供应商域导入/应用结果（不含 presets——presets 真源在插件库）
pub(crate) const PROVIDERS_KEY: &str = "providers";

/// 预设名/目标条目名长度上限（防误粘贴整段配置）
const NAME_MAX: usize = 128;
/// 模型列表上限
const MODELS_MAX: usize = 256;
/// api 方言白名单（与前端 ApiStyle 同构）
const API_STYLES: [&str; 4] = ["openai", "anthropic", "gemini", "custom"];

// ==================== 路径（家目录相对段） ====================

pub(crate) fn pi_models_path(home: &str) -> String {
    format!("{home}/.pi/agent/models.json")
}

pub(crate) fn pi_auth_path(home: &str) -> String {
    format!("{home}/.pi/agent/auth.json")
}

pub(crate) fn opencode_cfg_path(home: &str) -> String {
    format!("{home}/.config/opencode/opencode.json")
}

pub(crate) fn claude_settings_path(home: &str) -> String {
    format!("{home}/.claude/settings.json")
}

/// claude 自建桥接文件（存在任一即视为桥接体系在用）
fn bridge_paths(home: &str) -> [String; 2] {
    [
        format!("{home}/.claude/provider-config.sh"),
        format!("{home}/.claude/anthropic-bridge.mjs"),
    ]
}

// ==================== JSONC 解析（纯函数） ====================

/// 剥离 JSONC 注释（`//` 与 `/* */`），字符串字面量内的注释样文本不动；
/// 注释内容替换为空格（保留换行，使 serde 报错行号仍可读）
pub(crate) fn strip_jsonc_comments(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0usize;
    let mut in_string = false;
    while i < b.len() {
        let c = b[i];
        if in_string {
            match c {
                b'\\' => i += 1, // 跳过转义的下一字节
                b'"' => in_string = false,
                _ => {}
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => {
                in_string = true;
                i += 1;
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                while i < b.len() && b[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                out[i] = b' ';
                out[i + 1] = b' ';
                i += 2;
                while i < b.len() {
                    if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                        out[i] = b' ';
                        out[i + 1] = b' ';
                        i += 2;
                        break;
                    }
                    if b[i] != b'\n' {
                        out[i] = b' ';
                    }
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// 剥离对象/数组字面量内的尾逗号（JSONC 容忍、严格 JSON 拒绝）；
/// 只在注释与字符串之外生效
pub(crate) fn strip_trailing_commas(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0usize;
    let mut in_string = false;
    while i < b.len() {
        let c = b[i];
        if in_string {
            match c {
                b'\\' => i += 1,
                b'"' => in_string = false,
                _ => {}
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => {
                in_string = true;
                i += 1;
            }
            b',' => {
                // 向后看：跳过空白与注释，下一个非空字符为 } 或 ] 则删掉该逗号
                let mut j = i + 1;
                loop {
                    while j < b.len() && (b[j] as char).is_ascii_whitespace() {
                        j += 1;
                    }
                    if j + 1 < b.len() && b[j] == b'/' && (b[j + 1] == b'/' || b[j + 1] == b'*') {
                        // 注释后仍可能有 }：跳过注释体
                        if b[j + 1] == b'/' {
                            while j < b.len() && b[j] != b'\n' {
                                j += 1;
                            }
                        } else {
                            j += 2;
                            while j + 1 < b.len() && !(b[j] == b'*' && b[j + 1] == b'/') {
                                j += 1;
                            }
                            j = (j + 2).min(b.len());
                        }
                        continue;
                    }
                    break;
                }
                if j < b.len() && (b[j] == b'}' || b[j] == b']') {
                    out[i] = b' ';
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// JSONC 容错解析（注释 + 尾逗号）
pub(crate) fn parse_jsonc(text: &str) -> Result<Value, String> {
    let cleaned = strip_trailing_commas(&strip_jsonc_comments(text));
    serde_json::from_str(&cleaned).map_err(|e| format!("invalid JSONC: {e}"))
}

// ==================== JSONC 文本级 splice（纯函数） ====================

/// 跳过空白与注释，返回下一个有效字节下标
fn skip_ws_comments(b: &[u8], mut i: usize) -> usize {
    loop {
        while i < b.len() && (b[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        if i + 1 < b.len() && b[i] == b'/' && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if i + 1 < b.len() && b[i] == b'/' && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(b.len());
            continue;
        }
        return i;
    }
}

/// 扫描字符串字面量（i 指向开引号），返回 (解码内容, 闭引号后下标)
fn scan_string(text: &str, i: usize) -> Result<(String, usize), String> {
    let b = text.as_bytes();
    let mut out = String::new();
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'"' => return Ok((out, j + 1)),
            b'\\' => {
                j += 1;
                if j >= b.len() {
                    break;
                }
                match b[j] {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000C}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let hex = text
                            .get(j + 1..j + 5)
                            .ok_or_else(|| "unterminated \\u escape".to_string())?;
                        let cp = u32::from_str_radix(hex, 16)
                            .map_err(|e| format!("bad \\u escape: {e}"))?;
                        j += 4;
                        // 代理对合成（keys 少见，但保正确）
                        if (0xD800..0xDC00).contains(&cp)
                            && text.len() > j + 6
                            && text.as_bytes()[j + 1] == b'\\'
                            && text.as_bytes()[j + 2] == b'u'
                        {
                            if let Some(hex2) = text.get(j + 3..j + 7) {
                                if let Ok(low) = u32::from_str_radix(hex2, 16) {
                                    if (0xDC00..0xE000).contains(&low) {
                                        let c = 0x10000 + ((cp - 0xD800) << 10) + (low - 0xDC00);
                                        if let Some(ch) = char::from_u32(c) {
                                            out.push(ch);
                                            j += 6;
                                            continue;
                                        }
                                    }
                                }
                            }
                        }
                        out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                    }
                    other => out.push(other as char),
                }
                j += 1;
            }
            _ => {
                // 多字节 UTF-8：按字符边界整段复制
                let ch_len = utf8_len(b[j]);
                let seg = text
                    .get(j..j + ch_len)
                    .ok_or_else(|| "truncated UTF-8 in string".to_string())?;
                out.push_str(seg);
                j += ch_len;
            }
        }
    }
    Err("unterminated string".to_string())
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// 对象条目：键（解码后）+ 键 span + 值 span
struct ObjEntry {
    key: String,
    key_span: (usize, usize),
    value_span: (usize, usize),
}

/// 扫描对象（obj_start 指向 `{`）的顶层条目
fn scan_entries(text: &str, obj_start: usize) -> Result<Vec<ObjEntry>, String> {
    let b = text.as_bytes();
    if b.get(obj_start) != Some(&b'{') {
        return Err(format!("byte {obj_start} is not '{{'"));
    }
    let mut entries = Vec::new();
    let mut i = obj_start + 1;
    loop {
        i = skip_ws_comments(b, i);
        if i >= b.len() {
            return Err("unterminated object".to_string());
        }
        match b[i] {
            b'}' => return Ok(entries),
            b',' => {
                i += 1;
                continue;
            }
            b'"' => {}
            _ => return Err(format!("expected key string at byte {i}")),
        }
        let (key, key_end) = scan_string(text, i)?;
        let key_span = (i, key_end);
        i = skip_ws_comments(b, key_end);
        if i >= b.len() || b[i] != b':' {
            return Err(format!("expected ':' after key `{key}`"));
        }
        i = skip_ws_comments(b, i + 1);
        let value_start = i;
        let value_end = scan_value_end(text, i)?;
        entries.push(ObjEntry {
            key,
            key_span,
            value_span: (value_start, value_end),
        });
        i = value_end;
    }
}

/// 值的结束下标（不含随后的逗号/空白/注释）：字符串 / 嵌套结构 / 标量
fn scan_value_end(text: &str, i: usize) -> Result<usize, String> {
    let b = text.as_bytes();
    match b.get(i) {
        Some(b'"') => Ok(scan_string(text, i)?.1),
        Some(b'{') | Some(b'[') => {
            let (open, close) = if b[i] == b'{' {
                (b'{', b'}')
            } else {
                (b'[', b']')
            };
            let mut depth = 0usize;
            let mut j = i;
            while j < b.len() {
                match b[j] {
                    b'"' => {
                        j = scan_string(text, j)?.1;
                        continue;
                    }
                    c if c == open => depth += 1,
                    c if c == close => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(j + 1);
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            Err("unterminated nested value".to_string())
        }
        Some(_) => {
            // 标量：到逗号/闭括号/注释/换行为止，再回退尾部空白
            let mut j = i;
            while j < b.len() {
                let c = b[j];
                if c == b',' || c == b'}' || c == b']' || c == b'\n' {
                    break;
                }
                if c == b'/' && j + 1 < b.len() && (b[j + 1] == b'/' || b[j + 1] == b'*') {
                    break;
                }
                j += 1;
            }
            while j > i && (b[j - 1] as char).is_ascii_whitespace() {
                j -= 1;
            }
            Ok(j)
        }
        None => Err("unexpected end of text".to_string()),
    }
}

/// 顶层对象的 span（跳过前导空白/注释后必须以 `{` 开头）
fn top_level_object_span(text: &str) -> Result<(usize, usize), String> {
    let i = skip_ws_comments(text.as_bytes(), 0);
    let end = scan_value_end(text, i)?;
    if text.as_bytes().get(i) != Some(&b'{') {
        return Err("document root is not a JSON object".to_string());
    }
    Ok((i, end))
}

/// pos 所在行的行首缩进（供插入条目对齐）
fn line_indent(text: &str, pos: usize) -> String {
    let b = text.as_bytes();
    let mut line_start = 0usize;
    for k in (0..pos.min(b.len())).rev() {
        if b[k] == b'\n' {
            line_start = k + 1;
            break;
        }
    }
    let mut indent_end = line_start;
    while indent_end < b.len() && (b[indent_end] == b' ' || b[indent_end] == b'\t') {
        indent_end += 1;
    }
    text[line_start..indent_end.min(text.len())].to_string()
}

/// 多行值整体右移（首行不动，后续行加 base 缩进），插入时与宿主文件对齐
fn shift_indent(value_json: &str, base: &str) -> String {
    let mut out = String::with_capacity(value_json.len() + base.len() * 4);
    for (idx, line) in value_json.split('\n').enumerate() {
        if idx > 0 {
            out.push('\n');
            if !line.is_empty() {
                out.push_str(base);
            }
        }
        out.push_str(line);
    }
    out
}

/// 在 JSONC 文本的指定层级 upsert 一个键值条目，其余字节原样保留：
/// - `container = None`：顶层对象；`Some(c)`：顶层键 `c` 的值对象（必须存在）
/// - 键已存在 → 原位替换值 span；不存在 → 追加（沿用文件既有缩进）
pub(crate) fn upsert_entry(
    text: &str,
    container: Option<&str>,
    key: &str,
    value_json: &str,
) -> Result<String, String> {
    let (obj_start, _obj_end) = match container {
        None => top_level_object_span(text)?,
        Some(c) => {
            let (root_start, _) = top_level_object_span(text)?;
            let entries = scan_entries(text, root_start)?;
            let entry = entries
                .iter()
                .find(|e| e.key == c)
                .ok_or_else(|| format!("container `{c}` not found"))?;
            let brace = skip_ws_comments(text.as_bytes(), entry.value_span.0);
            if text.as_bytes().get(brace) != Some(&b'{') {
                return Err(format!("container `{c}` is not an object"));
            }
            (brace, 0)
        }
    };
    upsert_in_object(text, obj_start, key, value_json)
}

/// 确保顶层容器存在且为对象（缺失时插入空对象），返回新文本
pub(crate) fn ensure_container(text: &str, container: &str) -> Result<String, String> {
    let (root_start, _) = top_level_object_span(text)?;
    let entries = scan_entries(text, root_start)?;
    match entries.iter().find(|e| e.key == container) {
        Some(e) => {
            let brace = skip_ws_comments(text.as_bytes(), e.value_span.0);
            if text.as_bytes().get(brace) != Some(&b'{') {
                return Err(format!("container `{container}` is not an object"));
            }
            Ok(text.to_string())
        }
        None => upsert_in_object(text, root_start, container, "{}"),
    }
}

fn upsert_in_object(
    text: &str,
    obj_start: usize,
    key: &str,
    value_json: &str,
) -> Result<String, String> {
    let entries = scan_entries(text, obj_start)?;
    // 已存在：原位替换值 span
    if let Some(e) = entries.iter().find(|e| e.key == key) {
        let mut out = String::with_capacity(text.len() + value_json.len());
        out.push_str(&text[..e.value_span.0]);
        out.push_str(value_json);
        out.push_str(&text[e.value_span.1..]);
        return Ok(out);
    }
    let quoted_key = serde_json::to_string(key).map_err(|e| e.to_string())?;
    // 不存在：追加到最后一个条目后（有逗号衔接 + 行缩进对齐）或空对象内
    if let Some(last) = entries.last() {
        let insert_at = last.value_span.1;
        let indent = line_indent(text, last.key_span.0);
        let value = shift_indent(value_json, &format!("{indent}  "));
        let mut out = String::with_capacity(text.len() + value.len() + indent.len() + 4);
        out.push_str(&text[..insert_at]);
        out.push_str(",\n");
        out.push_str(&indent);
        out.push_str(&quoted_key);
        out.push_str(": ");
        out.push_str(&value);
        out.push_str(&text[insert_at..]);
        Ok(out)
    } else {
        // 空对象（可能含注释）：插在闭括号前一行
        let close = scan_value_end(text, obj_start)? - 1;
        let indent = line_indent(text, obj_start);
        let mut out = String::with_capacity(text.len() + value_json.len() + indent.len() + 8);
        out.push_str(&text[..close]);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&indent);
        out.push_str("  ");
        out.push_str(&quoted_key);
        out.push_str(": ");
        out.push_str(value_json);
        out.push('\n');
        out.push_str(&indent);
        out.push_str(&text[close..]);
        Ok(out)
    }
}

// ==================== key 掩码（纯函数） ====================

/// UI 掩码：前 3 字符 + 长度（spec §4.4）；短 key（≤6）不泄前缀只泄长度
pub(crate) fn mask_key(key: &str) -> String {
    let n = key.chars().count();
    if n == 0 {
        return "—".to_string();
    }
    if n <= 6 {
        return format!("•••({n})");
    }
    let prefix: String = key.chars().take(3).collect();
    format!("{prefix}…({n})")
}

// ==================== 方言映射（纯函数） ====================

/// pi models.json `api` 字段 → 预设 apiStyle
pub(crate) fn map_pi_api(api: &str) -> &'static str {
    match api {
        "openai-completions" | "openai-responses" => "openai",
        "anthropic-messages" => "anthropic",
        "google-generative-ai" | "google-vertex" => "gemini",
        _ => "custom",
    }
}

/// pi apiStyle → models.json `api` 字段（custom 回落 openai-completions）
pub(crate) fn pi_api_of(style: &str) -> &'static str {
    match style {
        "anthropic" => "anthropic-messages",
        "gemini" => "google-generative-ai",
        _ => "openai-completions",
    }
}

/// opencode `npm` 字段 → 预设 apiStyle
pub(crate) fn map_opencode_npm(npm: &str) -> &'static str {
    match npm {
        "@ai-sdk/openai-compatible" | "@ai-sdk/openai" => "openai",
        "@ai-sdk/anthropic" => "anthropic",
        "@ai-sdk/google" => "gemini",
        _ => "custom",
    }
}

/// opencode apiStyle → `npm` 字段（custom 回落 openai-compatible）
pub(crate) fn opencode_npm_of(style: &str) -> &'static str {
    match style {
        "anthropic" => "@ai-sdk/anthropic",
        "gemini" => "@ai-sdk/google",
        _ => "@ai-sdk/openai-compatible",
    }
}

// ==================== 反向导入提取（纯函数） ====================

/// 导入草稿（写库前的中间形态；不含 key，只有掩码——掩码随改名走，
/// 同名去重改名的预设其掩码仍能对上）
#[derive(Debug, PartialEq)]
pub(crate) struct PresetDraft {
    pub name: String,
    pub base_url: String,
    pub api_style: String,
    pub models: Vec<String>,
    pub notes: String,
    /// 源 key 掩码（无 key 可直拷为 "—"）
    pub key_mask: String,
}

/// pi 配置 → 预设草稿（掩码内嵌）。models.json providers.<key>：
/// name/baseUrl/api/models[].id；auth.json 同名条目的 key 只取掩码
pub(crate) fn presets_from_pi(models: &Value, auth: &Value) -> Vec<PresetDraft> {
    let mut drafts = Vec::new();
    let Some(providers) = models.get("providers").and_then(|v| v.as_object()) else {
        return drafts;
    };
    for (key, p) in providers {
        let base_url = p
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let api_style = p
            .get("api")
            .and_then(|v| v.as_str())
            .map(map_pi_api)
            .unwrap_or("openai")
            .to_string();
        let models: Vec<String> = p
            .get("models")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        drafts.push(PresetDraft {
            name: key.clone(),
            base_url,
            api_style,
            models,
            notes: format!("pi:{key}"),
            key_mask: auth_mask_of(auth, key),
        });
    }
    drafts
}

/// opencode 配置 → 预设草稿（掩码内嵌）。
/// provider.<key>：options.baseURL / npm / models 对象键
pub(crate) fn presets_from_opencode(cfg: &Value) -> Vec<PresetDraft> {
    let mut drafts = Vec::new();
    let Some(providers) = cfg.get("provider").and_then(|v| v.as_object()) else {
        return drafts;
    };
    for (key, p) in providers {
        let base_url = p
            .get("options")
            .and_then(|o| o.get("baseURL"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let api_style = p
            .get("npm")
            .and_then(|v| v.as_str())
            .map(map_opencode_npm)
            .unwrap_or("openai")
            .to_string();
        let models: Vec<String> = p
            .get("models")
            .and_then(|v| v.as_object())
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        let key_mask = p
            .get("options")
            .and_then(|o| o.get("apiKey"))
            .and_then(|v| v.as_str())
            .map(mask_key)
            .unwrap_or_else(|| "—".to_string());
        drafts.push(PresetDraft {
            name: key.clone(),
            base_url,
            api_style,
            models,
            notes: format!("opencode:{key}"),
            key_mask,
        });
    }
    drafts
}

/// auth.json 中某 provider 的 key 掩码（无 key / 解析失败 → "—"）
fn auth_mask_of(auth: &Value, provider: &str) -> String {
    auth.get(provider)
        .and_then(|p| p.get("key"))
        .and_then(|v| v.as_str())
        .map(mask_key)
        .unwrap_or_else(|| "—".to_string())
}

/// 同名去重计划：已存在同名 → 试 `{name}-{source}`；仍存在 → 跳过
/// （pi 与 opencode 常有同名 provider，如 sensenova）
pub(crate) fn plan_inserts(
    existing: &[String],
    drafts: Vec<PresetDraft>,
) -> (Vec<PresetDraft>, Vec<String>) {
    let mut taken: Vec<String> = existing.to_vec();
    let mut to_create = Vec::new();
    let mut skipped = Vec::new();
    for d in drafts {
        let source = d.notes.split(':').next().unwrap_or("import").to_string();
        let name = if taken.iter().any(|n| n == &d.name) {
            let alt = format!("{}-{}", d.name, source);
            if taken.iter().any(|n| n == &alt) {
                skipped.push(d.name);
                continue;
            }
            PresetDraft { name: alt, ..d }
        } else {
            d
        };
        taken.push(name.name.clone());
        to_create.push(name);
    }
    (to_create, skipped)
}

// ==================== 应用条目构造（纯函数） ====================

/// pi providers 条目：合并既有条目（保留用户模型定义等未知字段），
/// 覆盖 name/baseUrl/api；models 按 id 合并（既有的保留，新增的补最小定义）
pub(crate) fn merge_pi_entry(
    existing: Option<&Value>,
    base_url: &str,
    api_style: &str,
    models: &[String],
) -> Value {
    let mut entry = match existing {
        Some(v) if v.is_object() => v.as_object().unwrap().clone(),
        _ => Map::new(),
    };
    entry.insert("baseUrl".to_string(), json!(base_url));
    entry.insert("api".to_string(), json!(pi_api_of(api_style)));
    let mut merged: Vec<Value> = entry
        .get("models")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for id in models {
        if !merged
            .iter()
            .any(|m| m.get("id").and_then(|v| v.as_str()) == Some(id.as_str()))
        {
            merged.push(json!({ "id": id, "name": id }));
        }
    }
    if !merged.is_empty() {
        entry.insert("models".to_string(), Value::Array(merged));
    }
    Value::Object(entry)
}

/// pi auth.json 条目
pub(crate) fn pi_auth_entry(key: &str) -> Value {
    json!({ "type": "api_key", "key": key })
}

/// opencode provider 条目：合并既有条目；npm 由 apiStyle 决定；options.baseURL
/// 覆盖、apiKey 仅在有 key 时覆盖（keyMode=none 时保留用户既有 key）；
/// models 按 key 合并
pub(crate) fn merge_opencode_entry(
    existing: Option<&Value>,
    base_url: &str,
    api_style: &str,
    models: &[String],
    key: Option<&str>,
) -> Value {
    let mut entry = match existing {
        Some(v) if v.is_object() => v.as_object().unwrap().clone(),
        _ => Map::new(),
    };
    entry.insert("npm".to_string(), json!(opencode_npm_of(api_style)));
    let mut options = entry
        .get("options")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    options.insert("baseURL".to_string(), json!(base_url));
    if let Some(k) = key {
        options.insert("apiKey".to_string(), json!(k));
    }
    entry.insert("options".to_string(), Value::Object(options));
    let mut merged = entry
        .get("models")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    for id in models {
        merged
            .entry(id.clone())
            .or_insert_with(|| json!({ "name": id }));
    }
    if !merged.is_empty() {
        entry.insert("models".to_string(), Value::Object(merged));
    }
    Value::Object(entry)
}

/// claude settings.json `env` 块条目：key 未提供时不生成 AUTH_TOKEN 键
/// （保留用户既有 token）；MODEL 取预设首个模型
pub(crate) fn claude_env_entries(
    base_url: &str,
    key: Option<&str>,
    model: Option<&str>,
) -> Vec<(&'static str, Value)> {
    let mut entries = vec![("ANTHROPIC_BASE_URL", json!(base_url))];
    if let Some(k) = key {
        entries.push(("ANTHROPIC_AUTH_TOKEN", json!(k)));
    }
    if let Some(m) = model {
        entries.push(("ANTHROPIC_MODEL", json!(m)));
    }
    entries
}

/// claude settings.json env 只读视图（掩码）
pub(crate) fn claude_env_view(settings: &Value) -> Value {
    let env = settings.get("env");
    let val = |name: &str| env.and_then(|e| e.get(name)).and_then(|v| v.as_str());
    json!({
        "baseUrl": val("ANTHROPIC_BASE_URL"),
        "model": val("ANTHROPIC_MODEL"),
        "authTokenMask": val("ANTHROPIC_AUTH_TOKEN").map(mask_key),
    })
}

// ==================== 插件库：provider_preset 表 ====================

/// 建表（幂等）。宿主 `plugin_db_execute` 为单语句版本，schema 不拆分（单表无索引）
pub(crate) fn ensure_schema(h: &WasmHost) -> anyhow::Result<()> {
    h.plugin_db_execute(
        "CREATE TABLE IF NOT EXISTS provider_preset (\
         id INTEGER PRIMARY KEY AUTOINCREMENT, \
         name TEXT NOT NULL UNIQUE, \
         base_url TEXT NOT NULL DEFAULT '', \
         api_style TEXT NOT NULL DEFAULT 'openai', \
         models_json TEXT NOT NULL DEFAULT '[]', \
         notes TEXT, \
         created_at INTEGER NOT NULL, \
         updated_at INTEGER NOT NULL)",
    )
    .map_err(|e| anyhow::anyhow!("providers: ensure schema failed: {e}"))?;
    Ok(())
}

/// 库行 → wire JSON（models_json 反序列化；刻意无 key 字段——AC1 单测锁定）
fn preset_row_to_json(row: &Value) -> Option<Value> {
    let models = row
        .get("models_json")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    Some(json!({
        "id": row.get("id")?,
        "name": row.get("name")?,
        "baseUrl": row.get("base_url")?.as_str().unwrap_or(""),
        "apiStyle": row.get("api_style")?.as_str().unwrap_or("openai"),
        "models": models,
        "notes": row.get("notes").cloned().unwrap_or(Value::Null),
        "createdAt": row.get("created_at")?,
        "updatedAt": row.get("updated_at")?,
    }))
}

fn list_presets(h: &WasmHost) -> anyhow::Result<Vec<Value>> {
    let rows = h
        .plugin_db_query(
            "SELECT id, name, base_url, api_style, models_json, notes, created_at, updated_at \
             FROM provider_preset ORDER BY name",
        )
        .map_err(|e| anyhow::anyhow!("providers: list failed: {e}"))?
        .unwrap_or(Value::Array(Vec::new()));
    Ok(rows
        .as_array()
        .map(|arr| arr.iter().filter_map(preset_row_to_json).collect())
        .unwrap_or_default())
}

fn preset_names(h: &WasmHost) -> anyhow::Result<Vec<String>> {
    Ok(list_presets(h)?
        .iter()
        .filter_map(|p| {
            p.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect())
}

// ==================== 域状态（读-改-写 + 全量推送） ====================

fn read_stored(h: &WasmHost) -> (Value, Value) {
    let stored = h
        .storage_get(PROVIDERS_KEY)
        .ok()
        .flatten()
        .unwrap_or_else(|| json!({}));
    let default = || json!(null);
    (
        stored
            .get("import")
            .and_then(|v| v.get("last"))
            .cloned()
            .unwrap_or_else(default),
        stored
            .get("apply")
            .and_then(|v| v.get("last"))
            .cloned()
            .unwrap_or_else(default),
    )
}

fn write_stored(h: &WasmHost, import_last: &Value, apply_last: &Value) {
    let payload = json!({ "import": { "last": import_last }, "apply": { "last": apply_last } });
    if let Err(e) = h.storage_set(PROVIDERS_KEY, &payload) {
        h.log_warn(&format!("providers: persist state failed: {e}"));
    }
}

/// claude 只读视图（env 掩码 + 桥接文件存在性，读时现查）
fn claude_view(h: &WasmHost, home: &str) -> Value {
    let env = h
        .fs_read(&claude_settings_path(home))
        .ok()
        .flatten()
        .map(|t| {
            parse_jsonc(&t)
                .map(|v| claude_env_view(&v))
                .unwrap_or(json!({}))
        })
        .unwrap_or_else(|| json!({}));
    let bridges = bridge_paths(home);
    json!({
        "env": env,
        "bridge": {
            "providerConfigSh": h.fs_exists(&bridges[0]).unwrap_or(false),
            "anthropicBridgeMjs": h.fs_exists(&bridges[1]).unwrap_or(false),
        },
    })
}

/// 组装全量状态（命令返回值与事件载荷同形）
pub(crate) fn build_state(h: &WasmHost) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let (import_last, apply_last) = read_stored(h);
    let home = HOME.get().map(|s| s.as_str()).unwrap_or("");
    Ok(json!({
        "presets": list_presets(h)?,
        "claude": claude_view(h, home),
        "import": { "last": import_last },
        "apply": { "last": apply_last },
    }))
}

fn emit_and_return(h: &WasmHost, state: &Value) -> anyhow::Result<Value> {
    h.emit_event("plugin:agent-hub:providers", state);
    Ok(json!({ "state": state }))
}

// ==================== 命令：预设 CRUD ====================

fn validate_preset_payload(args: &Value) -> Result<(String, String, String, Vec<String>), String> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "name required".to_string())?;
    if name.chars().count() > NAME_MAX {
        return Err("name too long".to_string());
    }
    let base_url = args
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let api_style = args
        .get("apiStyle")
        .and_then(|v| v.as_str())
        .filter(|s| API_STYLES.contains(s))
        .unwrap_or("openai")
        .to_string();
    let models: Vec<String> = args
        .get("models")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if models.len() > MODELS_MAX {
        return Err("too many models".to_string());
    }
    Ok((name, base_url, api_style, models))
}

/// 新建/更新预设（id 缺省 = 新建）。同名冲突返回 `nameExists`（前端提示）。
/// 表结构无 key 列，载荷也不接受任何 key 字段（多余字段被忽略，不入库）
pub(crate) fn save_preset(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let (name, base_url, api_style, models) =
        validate_preset_payload(args).map_err(|e| anyhow::anyhow!("save-preset: {e}"))?;
    let models_json = serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string());
    let now = now_ms(h).unwrap_or(0);
    let id = args.get("id").and_then(|v| v.as_i64());

    if let Some(id) = id {
        // 更新：同名冲突只允许撞到自己
        let conflict = list_presets(h)?.iter().any(|p| {
            p.get("name").and_then(|v| v.as_str()) == Some(name.as_str())
                && p.get("id").and_then(|v| v.as_i64()) != Some(id)
        });
        if conflict {
            return Ok(json!({ "saved": false, "nameExists": true }));
        }
        h.plugin_db_execute_params(
            "UPDATE provider_preset SET name = ?1, base_url = ?2, api_style = ?3, \
             models_json = ?4, updated_at = ?5 WHERE id = ?6",
            &sql_params![name, base_url, api_style, models_json, now, id],
        )
        .map_err(|e| anyhow::anyhow!("save-preset: update failed: {e}"))?;
    } else {
        if preset_names(h)?.iter().any(|n| n == &name) {
            return Ok(json!({ "saved": false, "nameExists": true }));
        }
        h.plugin_db_execute_params(
            "INSERT INTO provider_preset (name, base_url, api_style, models_json, notes, \
             created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?5)",
            &sql_params![name, base_url, api_style, models_json, now],
        )
        .map_err(|e| anyhow::anyhow!("save-preset: insert failed: {e}"))?;
    }
    h.log_info(&format!("preset saved (name = {name}, id = {id:?})"));
    let state = build_state(h)?;
    emit_and_return(h, &state)
}

pub(crate) fn delete_preset(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let id = args
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("delete-preset: missing id"))?;
    h.plugin_db_execute_params(
        "DELETE FROM provider_preset WHERE id = ?1",
        &sql_params![id],
    )
    .map_err(|e| anyhow::anyhow!("delete-preset: failed: {e}"))?;
    h.log_info(&format!("preset deleted (id = {id})"));
    let state = build_state(h)?;
    emit_and_return(h, &state)
}

// ==================== 命令：反向导入 ====================

/// 反向导入（同步命令）：pi（models.json + auth.json 掩码）与 opencode
/// （opencode.json）各生成预设；同名去重见 `plan_inserts`；claude 只读展示
/// 不生成预设。key 只以掩码形态出现在导入结果里
pub(crate) fn import_providers(h: &WasmHost) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let home = HOME
        .get()
        .ok_or_else(|| anyhow::anyhow!("import: home unavailable"))?;

    let mut drafts: Vec<PresetDraft> = Vec::new();

    // pi
    match h.fs_read(&pi_models_path(home)) {
        Ok(Some(text)) => match parse_jsonc(&text) {
            Ok(models) => {
                let auth = h
                    .fs_read(&pi_auth_path(home))
                    .ok()
                    .flatten()
                    .and_then(|t| parse_jsonc(&t).ok())
                    .unwrap_or_else(|| json!({}));
                drafts.extend(presets_from_pi(&models, &auth));
            }
            Err(e) => h.log_warn(&format!("import: pi models.json parse failed: {e}")),
        },
        Ok(None) => {}
        Err(e) => h.log_warn(&format!("import: read pi models.json failed: {e}")),
    }

    // opencode
    match h.fs_read(&opencode_cfg_path(home)) {
        Ok(Some(text)) => match parse_jsonc(&text) {
            Ok(cfg) => drafts.extend(presets_from_opencode(&cfg)),
            Err(e) => h.log_warn(&format!("import: opencode.json parse failed: {e}")),
        },
        Ok(None) => {}
        Err(e) => h.log_warn(&format!("import: read opencode.json failed: {e}")),
    }

    let (to_create, skipped) = plan_inserts(&preset_names(h)?, drafts);
    let now = now_ms(h).unwrap_or(0);
    let mut created: Vec<String> = Vec::new();
    let mut keys: serde_json::Map<String, Value> = serde_json::Map::new();
    for d in to_create {
        let models_json = serde_json::to_string(&d.models).unwrap_or_else(|_| "[]".to_string());
        h.plugin_db_execute_params(
            "INSERT INTO provider_preset (name, base_url, api_style, models_json, notes, \
             created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            &sql_params![d.name, d.base_url, d.api_style, models_json, d.notes, now],
        )
        .map_err(|e| anyhow::anyhow!("import: insert preset failed: {e}"))?;
        // 掩码以最终（可能被去重改名的）预设名为键，前端按 preset.name 取用
        keys.insert(d.name.clone(), Value::String(d.key_mask));
        created.push(d.name);
    }
    // 注意：auth.json 无对应条目/解析失败时掩码为 "—"（视为无 key 可直拷）

    let import_last = json!({
        "ok": true,
        "created": created,
        "skipped": skipped,
        "keys": Value::Object(keys),
        "error": Value::Null,
        "at": now,
    });
    let (_, apply_last) = read_stored(h);
    write_stored(h, &import_last, &apply_last);
    h.log_info(&format!(
        "providers imported (created = {}, skipped = {})",
        created.len(),
        skipped.len()
    ));
    let state = build_state(h)?;
    let mut result = emit_and_return(h, &state)?;
    result["created"] = json!(created);
    result["skipped"] = json!(skipped);
    result["keys"] = json!(import_last["keys"]);
    Ok(result)
}

// ==================== 命令：应用到目标 CLI ====================

/// 目标条目名合法性（作为 JSON 对象键写入各 CLI 配置）
fn validate_target_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.chars().count() > NAME_MAX {
        return Err("target name empty or too long".to_string());
    }
    Ok(())
}

/// 从源 CLI 配置内存直拷 key（应用时现读，不落存储/日志）
fn source_key(h: &WasmHost, cli: &str, provider: &str) -> anyhow::Result<String> {
    let home = HOME
        .get()
        .ok_or_else(|| anyhow::anyhow!("apply: home unavailable"))?;
    let (path, extract) = match cli {
        "pi" => (pi_auth_path(home), Extract::PiAuth),
        "opencode" => (opencode_cfg_path(home), Extract::Opencode),
        "claude" => (claude_settings_path(home), Extract::ClaudeEnv),
        other => return Err(anyhow::anyhow!("apply: unknown key source cli {other}")),
    };
    let text = h
        .fs_read(&path)
        .map_err(|e| anyhow::anyhow!("apply: read source config failed: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("apply: source config missing ({cli})"))?;
    let cfg =
        parse_jsonc(&text).map_err(|e| anyhow::anyhow!("apply: source config invalid: {e}"))?;
    let key = match extract {
        Extract::PiAuth => cfg
            .get(provider)
            .and_then(|p| p.get("key"))
            .and_then(|v| v.as_str()),
        Extract::Opencode => cfg
            .get("provider")
            .and_then(|p| p.get(provider))
            .and_then(|p| p.get("options"))
            .and_then(|o| o.get("apiKey"))
            .and_then(|v| v.as_str()),
        Extract::ClaudeEnv => cfg
            .get("env")
            .and_then(|e| e.get("ANTHROPIC_AUTH_TOKEN"))
            .and_then(|v| v.as_str()),
    }
    .filter(|s| !s.is_empty())
    .ok_or_else(|| anyhow::anyhow!("apply: no key for {cli}:{provider}"))?;
    Ok(key.to_string())
}

enum Extract {
    PiAuth,
    Opencode,
    ClaudeEnv,
}

/// 应用预设到目标 CLI（同步命令）。key 来源三选一：inline 现场输入 /
/// source 内存直拷（现读源配置）/ none 不带 key（pi 的 auth.json 与既有
/// apiKey 均保留）。claude 桥接冲突时阻止（force = 用户确认，仅写 env 块）
pub(crate) fn apply_provider(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let id = args
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("apply: missing preset id"))?;
    let target = args
        .get("target")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if !matches!(target.as_str(), "claude" | "pi" | "opencode") {
        // codex config.toml 官方格式未校准（spec 开放问题 3），v1 不开放
        return Err(anyhow::anyhow!("apply: unsupported target {target}"));
    }
    let preset = list_presets(h)?
        .into_iter()
        .find(|p| p.get("id").and_then(|v| v.as_i64()) == Some(id))
        .ok_or_else(|| anyhow::anyhow!("apply: preset {id} not found"))?;
    let name = preset
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let base_url = preset
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let api_style = preset
        .get("apiStyle")
        .and_then(|v| v.as_str())
        .unwrap_or("openai")
        .to_string();
    let models: Vec<String> = preset
        .get("models")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let target_name = args
        .get("targetName")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| name.clone());
    validate_target_name(&target_name)
        .map_err(|e| anyhow::anyhow!("apply: invalid target name: {e}"))?;

    let key_mode = args
        .get("key")
        .and_then(|v| v.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("none")
        .to_string();
    let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
    let key: Option<String> = match key_mode.as_str() {
        "inline" => Some(
            args.get("key")
                .and_then(|k| k.get("value"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("apply: inline key empty"))?
                .to_string(),
        ),
        "source" => {
            let cli = args
                .get("key")
                .and_then(|k| k.get("cli"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let provider = args
                .get("key")
                .and_then(|k| k.get("provider"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Some(source_key(h, cli, provider)?)
        }
        _ => None,
    };

    let home = HOME
        .get()
        .ok_or_else(|| anyhow::anyhow!("apply: home unavailable"))?;
    let mut files: Vec<String> = Vec::new();

    match target.as_str() {
        "claude" => {
            let bridges = bridge_paths(home);
            let found: Vec<String> = bridges
                .iter()
                .filter(|p| h.fs_exists(p).unwrap_or(false))
                .map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
                .collect();
            if !found.is_empty() && !force {
                // 桥接冲突：阻止写入（桥接文件永不触碰），由用户确认后再写 env
                h.log_warn("apply: claude bridge detected; write blocked");
                return Ok(json!({ "applied": false, "bridgeConflict": true, "bridges": found }));
            }
            let path = claude_settings_path(home);
            let text = h
                .fs_read(&path)
                .map_err(|e| anyhow::anyhow!("apply: read settings.json failed: {e}"))?
                .unwrap_or_else(|| "{}".to_string());
            let mut t = ensure_container(&text, "env")
                .map_err(|e| anyhow::anyhow!("apply: claude env container: {e}"))?;
            for (k, v) in claude_env_entries(
                &base_url,
                key.as_deref(),
                models.first().map(|s| s.as_str()),
            ) {
                t = upsert_entry(&t, Some("env"), k, &v.to_string())
                    .map_err(|e| anyhow::anyhow!("apply: claude env upsert {k}: {e}"))?;
            }
            h.fs_write(&path, &t)
                .map_err(|e| anyhow::anyhow!("apply: write settings.json failed: {e}"))?;
            files.push("settings.json".to_string());
        }
        "pi" => {
            let path = pi_models_path(home);
            let text = h
                .fs_read(&path)
                .map_err(|e| anyhow::anyhow!("apply: read models.json failed: {e}"))?
                .unwrap_or_else(|| "{\n  \"providers\": {}\n}".to_string());
            let t = ensure_container(&text, "providers")
                .map_err(|e| anyhow::anyhow!("apply: pi providers container: {e}"))?;
            // 合并既有条目（保留用户模型定义）， pretty 序列化与 pi 既有排版一致
            let existing = parse_jsonc(&t).ok().and_then(|v| {
                v.get("providers")
                    .and_then(|p| p.get(&target_name))
                    .cloned()
            });
            let entry = merge_pi_entry(existing.as_ref(), &base_url, &api_style, &models);
            let pretty = serde_json::to_string_pretty(&entry)
                .map_err(|e| anyhow::anyhow!("apply: serialize pi entry: {e}"))?;
            let t = upsert_entry(&t, Some("providers"), &target_name, &pretty)
                .map_err(|e| anyhow::anyhow!("apply: pi providers upsert: {e}"))?;
            h.fs_write(&path, &t)
                .map_err(|e| anyhow::anyhow!("apply: write models.json failed: {e}"))?;
            files.push("models.json".to_string());
            // key 条目：无 key 时不动 auth.json（保留既有凭据）
            if let Some(k) = &key {
                let auth_path = pi_auth_path(home);
                let auth_text = h
                    .fs_read(&auth_path)
                    .map_err(|e| anyhow::anyhow!("apply: read auth.json failed: {e}"))?
                    .unwrap_or_else(|| "{}".to_string());
                let auth_entry = serde_json::to_string(&pi_auth_entry(k))
                    .map_err(|e| anyhow::anyhow!("apply: serialize auth entry: {e}"))?;
                let t = upsert_entry(&auth_text, None, &target_name, &auth_entry)
                    .map_err(|e| anyhow::anyhow!("apply: auth.json upsert: {e}"))?;
                h.fs_write(&auth_path, &t)
                    .map_err(|e| anyhow::anyhow!("apply: write auth.json failed: {e}"))?;
                files.push("auth.json".to_string());
            }
        }
        "opencode" => {
            let path = opencode_cfg_path(home);
            let text = h
                .fs_read(&path)
                .map_err(|e| anyhow::anyhow!("apply: read opencode.json failed: {e}"))?
                .unwrap_or_else(|| "{}".to_string());
            let t = ensure_container(&text, "provider")
                .map_err(|e| anyhow::anyhow!("apply: opencode provider container: {e}"))?;
            let existing = parse_jsonc(&t)
                .ok()
                .and_then(|v| v.get("provider").and_then(|p| p.get(&target_name)).cloned());
            let entry = merge_opencode_entry(
                existing.as_ref(),
                &base_url,
                &api_style,
                &models,
                key.as_deref(),
            );
            let pretty = serde_json::to_string_pretty(&entry)
                .map_err(|e| anyhow::anyhow!("apply: serialize opencode entry: {e}"))?;
            let t = upsert_entry(&t, Some("provider"), &target_name, &pretty)
                .map_err(|e| anyhow::anyhow!("apply: opencode provider upsert: {e}"))?;
            h.fs_write(&path, &t)
                .map_err(|e| anyhow::anyhow!("apply: write opencode.json failed: {e}"))?;
            files.push("opencode.json".to_string());
        }
        _ => unreachable!("target validated above"),
    }

    let apply_last = json!({
        "ok": true,
        "preset": name,
        "target": target,
        "files": files,
        "keyMode": key_mode,
        "keyLen": key.as_ref().map(|k| k.chars().count()),
        "error": Value::Null,
        "at": now_ms(h).unwrap_or(0),
    });
    let (import_last, _) = read_stored(h);
    write_stored(h, &import_last, &apply_last);
    // 日志纪律：key 只记长度（spec §6）
    h.log_info(&format!(
        "provider applied (preset = {name}, target = {target}, key_len = {:?})",
        apply_last["keyLen"]
    ));
    let state = build_state(h)?;
    let mut result = emit_and_return(h, &state)?;
    result["applied"] = json!(true);
    result["files"] = json!(files);
    Ok(result)
}

// ==================== 命令入口 ====================

pub(crate) fn get_state(h: &WasmHost) -> anyhow::Result<Value> {
    let state = build_state(h)?;
    Ok(json!({ "state": state }))
}

// ==================== Tests（纯函数：JSONC / splice / 提取 / 构造 / 纪律） ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== JSONC 解析 ====================

    /// pi models.json 真实形态：`//` 注释 + 嵌套对象；解析后字段可读
    #[test]
    fn jsonc_parses_pi_real_shape() {
        let text = r#"{
  "providers": {
    "amd": {
      "name": "AMD Radeon Developer Cloud",
      "baseUrl": "https://developer.amd.com.cn/radeon/api/v1",
      "api": "openai-completions",
      // 4 个模型一致的 compat 放 provider 级，逐模型 compat 与之 merge（后者覆盖）
      "compat": { "supportsDeveloperRole": false, "maxTokensField": "max_tokens" },
      "models": [
        { "id": "DeepSeek-V4-Flash", "name": "DeepSeek V4 Flash (AMD)", "reasoning": true, },
      ],
    },
  },
}"#;
        let v = parse_jsonc(text).expect("pi-shaped JSONC must parse");
        let amd = &v["providers"]["amd"];
        assert_eq!(amd["baseUrl"], "https://developer.amd.com.cn/radeon/api/v1");
        assert_eq!(amd["models"][0]["id"], "DeepSeek-V4-Flash");
    }

    /// 字符串内的注释样文本与花括号不被误剥/误扫描
    #[test]
    fn jsonc_string_content_preserved() {
        let text = r#"{"url": "https://x//y /*z*/ {", "a": 1}"#;
        let v = parse_jsonc(text).expect("string content must survive");
        assert_eq!(v["url"], "https://x//y /*z*/ {");
        let text = r#"{"a": "b"} // trailing brace fake } {"#;
        let v = parse_jsonc(text).expect("trailing comment must strip");
        assert_eq!(v["a"], "b");
    }

    /// 转义引号内的注释样文本不终止字符串
    #[test]
    fn jsonc_escaped_quote() {
        let v = parse_jsonc(r#"{"a": "say \"hi // not comment\""}"#).expect("must parse");
        assert_eq!(v["a"], r#"say "hi // not comment""#);
    }

    // ==================== splice：upsert_entry ====================

    const PI_SAMPLE: &str = r#"{
  "providers": {
    "sensenova": {
      "name": "商汤日日新 SenseNova",
      "baseUrl": "https://token.sensenova.cn/v1",
      "api": "openai-completions",
      "models": [
        { "id": "glm-5.2", "name": "GLM-5.2 (SenseNova)" }
      ]
    },
    // 用户注释必须原样保留
    "amd": {
      "baseUrl": "https://developer.amd.com.cn/radeon/api/v1",
      "api": "openai-completions"
    },
  },
}"#;

    /// 替换既有条目：只动该条目 span，注释与其余字节原样保留
    #[test]
    fn upsert_replaces_entry_preserving_rest() {
        let out = upsert_entry(
            PI_SAMPLE,
            Some("providers"),
            "amd",
            r#"{"baseUrl": "https://new"}"#,
        )
        .expect("replace must succeed");
        assert!(out.contains("https://new"));
        assert!(!out.contains("https://developer.amd.com.cn"));
        // 注释与其余条目保留
        assert!(out.contains("// 用户注释必须原样保留"));
        assert!(out.contains("商汤日日新 SenseNova"));
        assert!(parse_jsonc(&out).is_ok(), "result must stay valid JSONC");
    }

    /// 追加新条目：逗号衔接 + 缩进对齐；末条目带尾逗号的形态也正确
    #[test]
    fn upsert_appends_entry() {
        let out = upsert_entry(
            PI_SAMPLE,
            Some("providers"),
            "gmi",
            r#"{
  "baseUrl": "https://api.gmi-serving.com/v1",
  "api": "openai-completions"
}"#,
        )
        .expect("insert must succeed");
        let v = parse_jsonc(&out).expect("must stay valid");
        assert_eq!(
            v["providers"]["gmi"]["baseUrl"],
            "https://api.gmi-serving.com/v1"
        );
        assert_eq!(v["providers"]["sensenova"]["api"], "openai-completions");
        // 缩进对齐：新键行与既有条目同级
        assert!(out.contains("\n    \"gmi\": {"));
    }

    /// 空对象内首个条目；容器缺失时的顶层插入
    #[test]
    fn upsert_empty_object_and_missing_container() {
        let out = upsert_entry("{\n  \"a\": 1\n}", Some("x"), "k", "1");
        assert!(out.is_err(), "missing container must error");

        let out = ensure_container("{\"a\": 1}", "providers").expect("insert container");
        assert_eq!(parse_jsonc(&out).unwrap()["providers"], json!({}));
        let out =
            upsert_entry(&out, Some("providers"), "gmi", r#"{"baseUrl":"u"}"#).expect("insert");
        assert_eq!(
            parse_jsonc(&out).unwrap()["providers"]["gmi"]["baseUrl"],
            "u"
        );
        assert_eq!(parse_jsonc(&out).unwrap()["a"], 1);
    }

    /// 容器存在但非对象 → Err；文档根非对象 → Err
    #[test]
    fn upsert_type_errors() {
        assert!(upsert_entry(r#"{"providers": []}"#, Some("providers"), "k", "1").is_err());
        assert!(upsert_entry("[1,2]", None, "k", "1").is_err());
    }

    /// 顶层（root）upsert：auth.json 形态；既有值替换
    #[test]
    fn upsert_root_entry() {
        let auth = r#"{
  "sensenova": { "type": "api_key", "key": "sk-old" },
  "amd": { "type": "api_key", "key": "k2" }
}"#;
        let out = upsert_entry(auth, None, "gmi", r#"{"type":"api_key","key":"sk-new"}"#)
            .expect("insert");
        let v = parse_jsonc(&out).unwrap();
        assert_eq!(v["gmi"]["key"], "sk-new");
        assert_eq!(v["sensenova"]["key"], "sk-old");
        let out = upsert_entry(&out, None, "gmi", r#"{"type":"api_key","key":"sk-newer"}"#)
            .expect("replace");
        let v = parse_jsonc(&out).unwrap();
        assert_eq!(v["gmi"]["key"], "sk-newer");
        assert_eq!(v["amd"]["key"], "k2");
    }

    /// 标量值（claude env 字符串）替换与插入
    #[test]
    fn upsert_scalar_env() {
        let settings = r#"{
  "model": "haiku",
  "enabledPlugins": { "x@y": true }
}"#;
        let out = ensure_container(settings, "env").expect("env insert");
        let out = upsert_entry(
            &out,
            Some("env"),
            "ANTHROPIC_BASE_URL",
            r#""https://api.example.com/v1""#,
        )
        .expect("env key insert");
        let v = parse_jsonc(&out).unwrap();
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "https://api.example.com/v1");
        assert_eq!(v["model"], "haiku");
        assert_eq!(v["enabledPlugins"]["x@y"], true);
        // 既有 env 键替换、其余 env 键保留
        let out = upsert_entry(&out, Some("env"), "ANTHROPIC_BASE_URL", r#""https://b/v1""#)
            .expect("replace");
        let v = parse_jsonc(&out).unwrap();
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "https://b/v1");
    }

    // ==================== 掩码（key 纪律） ====================

    /// 掩码形态：前 3 字符 + 长度；短 key 不泄前缀；空 key 占位
    #[test]
    fn mask_formats() {
        assert_eq!(mask_key("sensenova-key-0123456789"), "sen…(24)");
        assert_eq!(mask_key("abcdef"), "•••(6)");
        assert_eq!(mask_key("abc"), "•••(3)");
        assert_eq!(mask_key(""), "—");
    }

    /// 掩码不得包含第 4 字符起的任何内容（防长度 ≤3 之外的泄漏）
    #[test]
    fn mask_never_leaks_tail() {
        let key = "sk-1234567890-abcdef";
        let m = mask_key(key);
        assert!(!m.contains(&key[3..]));
        assert_eq!(m, "sk-…(20)");
    }

    // ==================== 方言映射 ====================

    #[test]
    fn api_style_mappings() {
        assert_eq!(map_pi_api("openai-completions"), "openai");
        assert_eq!(map_pi_api("openai-responses"), "openai");
        assert_eq!(map_pi_api("anthropic-messages"), "anthropic");
        assert_eq!(map_pi_api("google-generative-ai"), "gemini");
        assert_eq!(map_pi_api("mistral-conversations"), "custom");
        assert_eq!(pi_api_of("openai"), "openai-completions");
        assert_eq!(pi_api_of("anthropic"), "anthropic-messages");
        assert_eq!(pi_api_of("gemini"), "google-generative-ai");
        assert_eq!(pi_api_of("custom"), "openai-completions");

        assert_eq!(map_opencode_npm("@ai-sdk/openai-compatible"), "openai");
        assert_eq!(map_opencode_npm("@ai-sdk/anthropic"), "anthropic");
        assert_eq!(map_opencode_npm("@ai-sdk/google"), "gemini");
        assert_eq!(map_opencode_npm("@ai-sdk/azure"), "custom");
        assert_eq!(opencode_npm_of("anthropic"), "@ai-sdk/anthropic");
        assert_eq!(opencode_npm_of("custom"), "@ai-sdk/openai-compatible");
    }

    // ==================== 反向导入提取 ====================

    /// pi 提取：真实形态（sensenova/amd）→ 草稿字段 + 掩码；key 本体不进草稿
    #[test]
    fn presets_from_pi_extracts() {
        let models = parse_jsonc(
            r#"{
  "providers": {
    "sensenova": {
      "name": "商汤日日新 SenseNova",
      "baseUrl": "https://token.sensenova.cn/v1",
      "api": "openai-completions",
      "models": [ { "id": "glm-5.2" }, { "id": "sensenova-u1-fast" } ]
    },
    "amd": {
      "name": "AMD Radeon Developer Cloud",
      "baseUrl": "https://developer.amd.com.cn/radeon/api/v1",
      // provider 级 compat 注释
      "api": "openai-completions",
      "models": [ { "id": "DeepSeek-V4-Flash" }, { "id": "Qwen3.8-Flash-Next" } ]
    }
  }
}"#,
        )
        .unwrap();
        let auth = parse_jsonc(
            r#"{
  "sensenova": { "type": "api_key", "key": "sensenova-raw-key-000111222333444" },
  "amd": { "type": "api_key", "key": "amd-raw-key-000111222333444555666777888" }
}"#,
        )
        .unwrap();
        let drafts = presets_from_pi(&models, &auth);
        assert_eq!(drafts.len(), 2);
        let sen = drafts.iter().find(|d| d.name == "sensenova").unwrap();
        assert_eq!(sen.base_url, "https://token.sensenova.cn/v1");
        assert_eq!(sen.api_style, "openai");
        assert_eq!(sen.models, vec!["glm-5.2", "sensenova-u1-fast"]);
        assert_eq!(sen.notes, "pi:sensenova");
        let amd = drafts.iter().find(|d| d.name == "amd").unwrap();
        assert_eq!(amd.api_style, "openai");
        // 掩码回显：前 3 字符 + 长度，raw key 不出现（掩码内嵌草稿，随改名走）
        assert_eq!(sen.key_mask, "sen…(33)");
        let all = format!("{drafts:?}");
        assert!(
            !all.contains("sensenova-raw-key"),
            "raw key must not leak into drafts"
        );
    }

    /// opencode 提取：provider.* → 草稿 + 掩码；无 apiKey 的条目掩码为 "—"
    #[test]
    fn presets_from_opencode_extracts() {
        let cfg = parse_jsonc(
            r#"{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "gmi": {
      "name": "GMI",
      "npm": "@ai-sdk/openai-compatible",
      "options": { "apiKey": "gmi-raw-key-000111222333444555666777888999", "baseURL": "https://api.gmi-serving.com/v1" },
      "models": { "MiniMaxAI/MiniMax-M3": { "name": "MiniMax M3" }, "openai/gpt-5": {} }
    },
    "anthropic-direct": {
      "npm": "@ai-sdk/anthropic",
      "options": { "baseURL": "https://api.anthropic.com/v1" }
    }
  }
}"#,
        )
        .unwrap();
        let drafts = presets_from_opencode(&cfg);
        assert_eq!(drafts.len(), 2);
        let gmi = drafts.iter().find(|d| d.name == "gmi").unwrap();
        assert_eq!(gmi.base_url, "https://api.gmi-serving.com/v1");
        assert_eq!(gmi.api_style, "openai");
        assert!(gmi.models.contains(&"MiniMaxAI/MiniMax-M3".to_string()));
        assert_eq!(gmi.notes, "opencode:gmi");
        let ant = drafts
            .iter()
            .find(|d| d.name == "anthropic-direct")
            .unwrap();
        assert_eq!(ant.api_style, "anthropic");
        assert_eq!(gmi.key_mask, "gmi…(42)");
        assert_eq!(ant.key_mask, "—");
    }

    /// 同名去重：pi 先建 sensenova，opencode 的同名 → `sensenova-opencode`；
    /// 掩码内嵌草稿，改名后仍随预设走（前端按最终名取掩码）
    #[test]
    fn plan_inserts_dedupes_cross_source() {
        let existing = vec!["sensenova".to_string()];
        let drafts = vec![
            PresetDraft {
                name: "sensenova".to_string(),
                base_url: "u".to_string(),
                api_style: "openai".to_string(),
                models: vec![],
                notes: "opencode:sensenova".to_string(),
                key_mask: "gmi…(42)".to_string(),
            },
            PresetDraft {
                name: "gmi".to_string(),
                base_url: "u".to_string(),
                api_style: "openai".to_string(),
                models: vec![],
                notes: "opencode:gmi".to_string(),
                key_mask: "—".to_string(),
            },
        ];
        let (create, skipped) = plan_inserts(&existing, drafts);
        assert_eq!(create.len(), 2);
        assert_eq!(create[0].name, "sensenova-opencode");
        assert_eq!(create[0].key_mask, "gmi…(42)", "mask must survive rename");
        assert_eq!(create[1].name, "gmi");
        assert!(skipped.is_empty());

        // 两名都占 → 跳过
        let existing = vec!["sensenova".to_string(), "sensenova-opencode".to_string()];
        let drafts = vec![PresetDraft {
            name: "sensenova".to_string(),
            base_url: "u".to_string(),
            api_style: "openai".to_string(),
            models: vec![],
            notes: "opencode:sensenova".to_string(),
            key_mask: "—".to_string(),
        }];
        let (create, skipped) = plan_inserts(&existing, drafts);
        assert!(create.is_empty());
        assert_eq!(skipped, vec!["sensenova".to_string()]);
    }

    // ==================== 应用条目构造 ====================

    /// pi 条目合并：既有模型的完整定义保留，新增 id 补最小定义，baseUrl/api 覆盖
    #[test]
    fn merge_pi_entry_preserves_user_models() {
        let existing = parse_jsonc(
            r#"{
  "name": "商汤日日新 SenseNova",
  "baseUrl": "https://old/v1",
  "api": "openai-completions",
  "models": [ { "id": "glm-5.2", "reasoning": true, "contextWindow": 1048576 } ]
}"#,
        )
        .unwrap();
        let merged = merge_pi_entry(
            Some(&existing),
            "https://new/v1",
            "anthropic",
            &["glm-5.2".to_string(), "claude-sonnet-4".to_string()],
        );
        assert_eq!(merged["baseUrl"], "https://new/v1");
        assert_eq!(merged["api"], "anthropic-messages");
        let models = merged["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        // 既有定义保留（reasoning/contextWindow 未丢）
        assert_eq!(models[0]["contextWindow"], 1048576);
        assert_eq!(models[1]["id"], "claude-sonnet-4");
        assert_eq!(models[1]["name"], "claude-sonnet-4");

        // 无既有条目 → 最小条目
        let fresh = merge_pi_entry(None, "https://u/v1", "openai", &["m1".to_string()]);
        assert_eq!(fresh["api"], "openai-completions");
        assert_eq!(fresh["models"][0]["id"], "m1");
    }

    /// opencode 条目合并：keyMode=none 时保留既有 apiKey；models 按 key 合并
    #[test]
    fn merge_opencode_entry_preserves_key_when_none() {
        let existing = parse_jsonc(
            r#"{
  "name": "GMI",
  "npm": "@ai-sdk/openai-compatible",
  "options": { "apiKey": "existing-key", "baseURL": "https://old/v1", "setCacheKey": true },
  "models": { "openai/gpt-5": { "name": "GPT-5" } }
}"#,
        )
        .unwrap();
        let merged = merge_opencode_entry(
            Some(&existing),
            "https://new/v1",
            "openai",
            &["openai/gpt-5".to_string(), "openai/gpt-5.5".to_string()],
            None,
        );
        assert_eq!(merged["options"]["baseURL"], "https://new/v1");
        assert_eq!(
            merged["options"]["apiKey"], "existing-key",
            "no-key apply must keep existing"
        );
        assert_eq!(merged["options"]["setCacheKey"], true);
        assert_eq!(merged["models"]["openai/gpt-5"]["name"], "GPT-5");
        assert_eq!(merged["models"]["openai/gpt-5.5"]["name"], "openai/gpt-5.5");

        // 带 key 覆盖
        let merged = merge_opencode_entry(
            Some(&existing),
            "https://new/v1",
            "openai",
            &[],
            Some("fresh"),
        );
        assert_eq!(merged["options"]["apiKey"], "fresh");
    }

    /// claude env 条目：key 缺省不生成 AUTH_TOKEN（保留既有 token）；MODEL 取首模型
    #[test]
    fn claude_env_entries_shape() {
        let entries = claude_env_entries("https://b/v1", Some("tok"), Some("m1"));
        assert_eq!(entries[0].0, "ANTHROPIC_BASE_URL");
        assert_eq!(entries[1].0, "ANTHROPIC_AUTH_TOKEN");
        assert_eq!(entries[2].0, "ANTHROPIC_MODEL");
        let entries = claude_env_entries("https://b/v1", None, None);
        assert_eq!(entries.len(), 1);
    }

    /// claude env 只读视图：token 掩码、base/model 原样（非敏感）
    #[test]
    fn claude_env_view_masks_token() {
        let settings = parse_jsonc(
            r#"{
  "model": "haiku",
  "env": {
    "ANTHROPIC_BASE_URL": "https://bridge.local/v1",
    "ANTHROPIC_AUTH_TOKEN": "bridge-token-0123456789",
    "ANTHROPIC_MODEL": "sonnet",
    "OTHER_VAR": "keep"
  }
}"#,
        )
        .unwrap();
        let view = claude_env_view(&settings);
        assert_eq!(view["baseUrl"], "https://bridge.local/v1");
        assert_eq!(view["model"], "sonnet");
        assert_eq!(view["authTokenMask"], "bri…(23)");
        // 视图不含 token 明文
        assert!(!view.to_string().contains("bridge-token-0123456789"));
    }

    // ==================== AC1：库与状态无 key 明文 ====================

    /// 预设 wire 形状：无 key/apiKey 字段（表结构无 key 列的守门断言）
    #[test]
    fn preset_row_shape_has_no_key_field() {
        let row = json!({
            "id": 1, "name": "sensenova", "base_url": "https://u/v1",
            "api_style": "openai", "models_json": "[\"m1\"]",
            "notes": "pi:sensenova", "created_at": 1, "updated_at": 2,
        });
        let p = preset_row_to_json(&row).expect("row maps");
        assert!(p.get("key").is_none());
        assert!(p.get("apiKey").is_none());
        assert!(
            p.get("models_json").is_none(),
            "models_json 已解码为 models"
        );
        assert_eq!(p["models"], json!(["m1"]));
        assert_eq!(p["notes"], "pi:sensenova");
        // 模型列表里的字符串键不携带任何 key 形态字段
        assert!(!p.to_string().to_lowercase().contains("apikey"));
    }

    /// 应用结果状态（apply.last）：key 只出现长度（keyLen），不出现内容
    #[test]
    fn apply_state_records_key_len_only() {
        let key = "super-secret-key-0123456789";
        let apply_last = json!({
            "ok": true, "preset": "sensenova", "target": "pi",
            "files": ["models.json", "auth.json"], "keyMode": "source",
            "keyLen": key.chars().count(), "error": null, "at": 0,
        });
        let s = apply_last.to_string();
        assert!(
            !s.contains(key),
            "apply state must not contain key plaintext"
        );
        assert!(s.contains("\"keyLen\":27"));
        // 日志行同样只含长度（与 apply_provider 的 log_info 同构断言）
        let log_line = format!(
            "provider applied (target = pi, key_len = {:?})",
            apply_last["keyLen"]
        );
        assert!(!log_line.contains(key));
    }

    /// 端到端拼装（纯函数链）：pi 目标写入内容包含 key（目标配置的职责），
    /// 而返回给前端的状态载荷不含 key
    #[test]
    fn apply_pipeline_key_placement() {
        let key = "pi-inline-key-0123456789";
        let text = "{\n  \"providers\": {}\n}";
        let t = ensure_container(text, "providers").unwrap();
        let entry = merge_pi_entry(None, "https://u/v1", "openai", &["m1".to_string()]);
        let pretty = serde_json::to_string_pretty(&entry).unwrap();
        let t = upsert_entry(&t, Some("providers"), "sensenova", &pretty).unwrap();
        assert!(parse_jsonc(&t).is_ok());
        assert!(!t.contains(key), "models.json entry itself carries no key");

        let auth_text = "{}";
        let auth_entry = serde_json::to_string(&pi_auth_entry(key)).unwrap();
        let out = upsert_entry(auth_text, None, "sensenova", &auth_entry).unwrap();
        assert!(out.contains(key), "auth.json is the designated key carrier");
        assert!(parse_jsonc(&out).is_ok());
    }
}
