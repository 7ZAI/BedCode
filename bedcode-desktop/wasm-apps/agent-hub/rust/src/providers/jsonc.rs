//! JSONC 解析与文本级 splice（纯函数）
//!
//! pi 的 `models.json` 含 `//` 注释（非严格 JSON），且用户配置中的注释与
//! 未知字段必须原样保留——写路径不做整文件反序列化重写，只在目标条目
//! span 上做替换/插入（`upsert_entry` / `ensure_container`），其余字节逐字
//! 保留；读路径统一走 `parse_jsonc`（剥注释 + 尾逗号）。

// ==================== JSONC 解析（纯函数） ====================

use serde_json::Value;

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

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
}
