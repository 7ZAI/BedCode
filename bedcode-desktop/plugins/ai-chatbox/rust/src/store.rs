//! JSONL 对话日志持久化
//!
//! 对话历史唯一存储：`conversations/{convId}.jsonl`（首行 meta + 逐行消息）、
//! `index.jsonl`（对话列表索引，按 updatedAt DESC）、`providers.json`（配置占位）。
//! 文件访问全部经 WASI（wasm32-wasip2 目标，std::fs 直连宿主预打开的 `/data`
//! 目录），不经宿主 fs_* 转发；日志仍经 HostLog 转发到宿主。

use bedcode_plugin_api::host::HostLog;

use serde::{Deserialize, Serialize};

/// 对话元数据（index.jsonl 每行一个；对话文件首行同构）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
}

/// token 用量（流结束时由宿主 usage 透传携带）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// 消息记录（对话文件 `type == "message"` 的行）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessageRecord {
    pub role: String,
    pub content: String,
    pub timestamp: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
    /// 思考过程全文（DeepSeek 思考模式；旧日志无此字段时反序列化为 None）
    #[serde(default)]
    pub reasoning: Option<String>,
}

/// 对话文件单行（meta 首行 + message 行统一格式）
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConversationLine {
    #[serde(rename = "type")]
    line_type: String,
    #[serde(flatten)]
    body: serde_json::Value,
}

/// 初始化数据目录：缺省子目录与文件（conversations / index.jsonl / providers.json）不存在时创建
pub fn init<H: HostLog>(_log: &H, data_dir: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(format!("{}/conversations", data_dir))
        .map_err(|e| anyhow::anyhow!("failed to create conversations dir under {}: {}", data_dir, e))?;

    let index_path = format!("{}/index.jsonl", data_dir);
    if !file_exists(&index_path) {
        write_file(&index_path, "")?;
    }

    let providers_path = format!("{}/providers.json", data_dir);
    if !file_exists(&providers_path) {
        let default = serde_json::json!({
            "providers": [],
            "activeProviderId": "",
            "activeModel": ""
        });
        write_file(
            &providers_path,
            &serde_json::to_string_pretty(&default)
                .map_err(|e| anyhow::anyhow!("failed to serialize providers.json: {}", e))?,
        )?;
    }
    Ok(())
}

/// 列出全部对话（index.jsonl），按 updatedAt 降序；索引文件缺失视为空列表
pub fn list_conversations<H: HostLog>(log: &H, data_dir: &str) -> anyhow::Result<Vec<ConversationMeta>> {
    let index_path = format!("{}/index.jsonl", data_dir);
    let Some(content) = read_file(&index_path)? else {
        return Ok(Vec::new());
    };

    let mut convs = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<ConversationMeta>(trimmed) {
            Ok(conv) => convs.push(conv),
            Err(e) => {
                // 解析告警经 HostLog 转发到宿主日志
                log.log_warn(&format!(
                    "list_conversations: skipping corrupted index line: {}",
                    e
                ));
            }
        }
    }
    convs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(convs)
}

/// 获取对话全部消息（跳过首行 meta；损坏行跳过）
pub fn get_messages<H: HostLog>(
    log: &H,
    data_dir: &str,
    conversation_id: &str,
) -> anyhow::Result<Vec<ChatMessageRecord>> {
    let path = conversation_path(data_dir, conversation_id);
    let Some(content) = read_file(&path)? else {
        return Ok(Vec::new());
    };

    let mut messages = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed = match serde_json::from_str::<ConversationLine>(trimmed) {
            Ok(p) => p,
            Err(e) => {
                log.log_warn(&format!(
                    "get_messages: skipping corrupted line: {} (conversation {})",
                    e, conversation_id
                ));
                continue;
            }
        };
        if parsed.line_type != "message" {
            continue;
        }
        match serde_json::from_value::<ChatMessageRecord>(parsed.body) {
            Ok(msg) => messages.push(msg),
            Err(e) => {
                log.log_warn(&format!(
                    "get_messages: skipping corrupted message line: {}",
                    e
                ));
            }
        }
    }
    Ok(messages)
}

/// 追加一条消息（读-拼-写整文件覆盖）
///
/// `replace_last_assistant` 为 true 时先删除文件末尾的 assistant 消息行再追加
/// （重新生成场景：旧回复被新回复覆盖，避免重启后旧回复复现）。
pub fn save_message<H: HostLog>(
    _log: &H,
    data_dir: &str,
    conversation_id: &str,
    msg: &ChatMessageRecord,
    replace_last_assistant: bool,
) -> anyhow::Result<()> {
    let path = conversation_path(data_dir, conversation_id);
    let mut content = read_file(&path)?
        .ok_or_else(|| anyhow::anyhow!("conversation file not found: {}", path))?;

    if replace_last_assistant {
        content = strip_last_assistant_line(&content);
    }

    let line = serde_json::to_string(&serde_json::json!({
        "type": "message",
        "role": msg.role,
        "content": msg.content,
        "timestamp": msg.timestamp,
        "model": msg.model,
        "usage": msg.usage,
        "reasoning": msg.reasoning,
    }))
    .map_err(|e| anyhow::anyhow!("failed to serialize message: {}", e))?;

    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&line);
    content.push('\n');

    write_file(&path, &content)?;
    Ok(())
}

/// 更新对话 meta（重写对话文件首行 + 重写 index.jsonl）
pub fn save_conversation<H: HostLog>(
    _log: &H,
    data_dir: &str,
    conv: &ConversationMeta,
) -> anyhow::Result<()> {
    // 重写对话文件首行（meta 与消息行保持同文件）；新对话文件尚不存在时直接创建
    let path = conversation_path(data_dir, &conv.id);
    let content = read_file(&path)?.unwrap_or_default();
    let mut lines: Vec<&str> = content.lines().collect();
    let meta_line = meta_json_line(conv)?;
    // 解析首行判断是否 meta（serde_json key 顺序不保证，不能用字符串前缀匹配）
    let is_meta_first = lines
        .first()
        .and_then(|l| serde_json::from_str::<ConversationLine>(l.trim()).ok())
        .map(|p| p.line_type == "meta")
        .unwrap_or(false);
    if is_meta_first {
        lines[0] = &meta_line;
    } else {
        lines.insert(0, &meta_line);
    }
    let mut rewritten = lines.join("\n");
    if !rewritten.ends_with('\n') {
        rewritten.push('\n');
    }
    write_file(&path, &rewritten)?;

    rewrite_index(_log, data_dir, &[conv.clone()], &[])
}

/// 删除对话（删对话文件 + 从 index.jsonl 移除）
pub fn delete_conversation<H: HostLog>(
    log: &H,
    data_dir: &str,
    conversation_id: &str,
) -> anyhow::Result<()> {
    let path = conversation_path(data_dir, conversation_id);
    delete_file(&path)?;
    rewrite_index(log, data_dir, &[], &[conversation_id.to_string()])
}

/// 对话文件路径
fn conversation_path(data_dir: &str, conversation_id: &str) -> String {
    format!("{}/conversations/{}.jsonl", data_dir, conversation_id)
}

// ==================== WASI std::fs 访问辅助 ====================

/// 读文件：不存在返回 None，其余 IO 错误上抛（错误带路径上下文）
fn read_file(path: &str) -> anyhow::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::anyhow!("failed to read {}: {}", path, e)),
    }
}

/// 写文件（整文件覆盖）：父目录不存在时自动创建
fn write_file(path: &str, content: &str) -> anyhow::Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("failed to create parent dir for {}: {}", path, e))?;
    }
    std::fs::write(path, content).map_err(|e| anyhow::anyhow!("failed to write {}: {}", path, e))
}

/// 文件是否存在
fn file_exists(path: &str) -> bool {
    std::path::Path::new(path).exists()
}

/// 删文件：不存在视为已删除（幂等）
fn delete_file(path: &str) -> anyhow::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::anyhow!("failed to delete {}: {}", path, e)),
    }
}

/// 序列化 meta 行（对话文件首行 / index.jsonl 行）
fn meta_json_line(conv: &ConversationMeta) -> anyhow::Result<String> {
    serde_json::to_string(&serde_json::json!({
        "type": "meta",
        "id": conv.id,
        "title": conv.title,
        "createdAt": conv.created_at,
        "updatedAt": conv.updated_at,
        "providerId": conv.provider_id,
        "providerName": conv.provider_name,
        "model": conv.model,
    }))
    .map_err(|e| anyhow::anyhow!("failed to serialize conversation meta: {}", e))
}

/// 重写 index.jsonl（upsert 覆盖同 id，exclude 剔除 id，按 updatedAt DESC 全量落盘）
fn rewrite_index<H: HostLog>(
    log: &H,
    data_dir: &str,
    upsert: &[ConversationMeta],
    exclude: &[String],
) -> anyhow::Result<()> {
    let index_path = format!("{}/index.jsonl", data_dir);
    let existing = list_conversations(log, data_dir)?;

    // upsert 覆盖同 id，exclude 剔除，其余保留
    let mut merged: Vec<ConversationMeta> = existing
        .into_iter()
        .filter(|c| !upsert.iter().any(|u| u.id == c.id) && !exclude.contains(&c.id))
        .collect();
    merged.extend_from_slice(upsert);
    merged.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    let mut content = String::new();
    for conv in &merged {
        content.push_str(&meta_json_line(conv)?);
        content.push('\n');
    }
    write_file(&index_path, &content)
}

/// 删除文件末尾最后一条 assistant 消息行（按行倒序查找 role == assistant）
fn strip_last_assistant_line(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut idx = None;
    for (i, line) in lines.iter().enumerate().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(parsed) = serde_json::from_str::<ConversationLine>(trimmed) {
            if parsed.line_type == "message"
                && parsed.body.get("role").and_then(|r| r.as_str()) == Some("assistant")
            {
                idx = Some(i);
                break;
            }
        }
    }
    let Some(idx) = idx else {
        return content.to_string();
    };
    let kept: Vec<&str> = lines.into_iter().take(idx).collect();
    let mut out = kept.join("\n");
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试日志：静默（不落盘）
    struct TestLog;

    impl HostLog for TestLog {
        fn log_info(&self, _message: &str) {}
        fn log_debug(&self, _message: &str) {}
        fn log_warn(&self, _message: &str) {}
        fn log_error(&self, _message: &str) {}
        fn mark_plugin_error(&self, _error: &str) {}
    }

    /// 每测试独立临时目录作为数据根（生产中为 WASI 预打开的 /data，
    /// 函数签名保留 data_dir 参数使测试可用真实文件系统直连）
    fn data_root() -> String {
        tempfile::tempdir()
            .expect("tempdir")
            .path()
            .to_string_lossy()
            .trim_end_matches(['/', '\\'])
            .to_string()
    }

    fn meta(id: &str, updated_at: &str) -> ConversationMeta {
        ConversationMeta {
            id: id.to_string(),
            title: format!("title-{}", id),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: updated_at.to_string(),
            provider_id: "p1".to_string(),
            provider_name: "DeepSeek".to_string(),
            model: "deepseek-chat".to_string(),
        }
    }

    fn msg(role: &str, content: &str) -> ChatMessageRecord {
        ChatMessageRecord {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: "2026-01-01T00:00:01Z".to_string(),
            model: None,
            usage: None,
            reasoning: None,
        }
    }

    #[test]
    fn init_creates_default_files() {
        let dir = data_root();
        let log = TestLog;
        init(&log, &dir).unwrap();
        assert_eq!(std::fs::read_to_string(format!("{}/index.jsonl", dir)).unwrap(), "");
        assert!(std::fs::read_to_string(format!("{}/providers.json", dir))
            .unwrap()
            .contains("\"activeProviderId\""));
    }

    #[test]
    fn save_message_creates_file_with_meta_and_appends() {
        let dir = data_root();
        let log = TestLog;
        std::fs::create_dir_all(format!("{}/conversations", dir)).unwrap();
        // 先建对话文件（meta 首行）
        let conv = meta("c1", "2026-01-01T00:00:00Z");
        std::fs::write(
            format!("{}/conversations/c1.jsonl", dir),
            meta_json_line(&conv).unwrap(),
        )
        .unwrap();

        save_message(&log, &dir, "c1", &msg("user", "hello"), false).unwrap();
        save_message(&log, &dir, "c1", &msg("assistant", "hi"), false).unwrap();

        let content = std::fs::read_to_string(format!("{}/conversations/c1.jsonl", dir)).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("\"type\":\"meta\""));
        assert!(lines[1].contains("\"role\":\"user\""));
        assert!(lines[2].contains("\"role\":\"assistant\""));
    }

    #[test]
    fn get_messages_skips_meta_and_corrupted_lines() {
        let dir = data_root();
        let log = TestLog;
        std::fs::create_dir_all(format!("{}/conversations", dir)).unwrap();
        let conv = meta("c1", "2026-01-01T00:00:00Z");
        let mut content = meta_json_line(&conv).unwrap();
        content.push('\n');
        content.push_str("{\"type\":\"message\",\"role\":\"user\",\"content\":\"a\",\"timestamp\":\"t\"}\n");
        content.push_str("this is not json\n");
        content.push_str("{\"type\":\"message\",\"role\":\"assistant\",\"content\":\"b\",\"timestamp\":\"t\"}\n");
        std::fs::write(format!("{}/conversations/c1.jsonl", dir), content).unwrap();

        let messages = get_messages(&log, &dir, "c1").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].content, "b");
    }

    #[test]
    fn index_sorted_by_updated_at_desc() {
        let dir = data_root();
        let log = TestLog;
        init(&log, &dir).unwrap();
        let c1 = meta("c1", "2026-01-02T00:00:00Z");
        let c2 = meta("c2", "2026-01-03T00:00:00Z");
        save_conversation(&log, &dir, &c1).unwrap();
        save_conversation(&log, &dir, &c2).unwrap();

        let list = list_conversations(&log, &dir).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "c2");
        assert_eq!(list[1].id, "c1");
    }

    #[test]
    fn delete_conversation_removes_file_and_index() {
        let dir = data_root();
        let log = TestLog;
        init(&log, &dir).unwrap();
        let c1 = meta("c1", "2026-01-02T00:00:00Z");
        save_conversation(&log, &dir, &c1).unwrap();
        assert_eq!(list_conversations(&log, &dir).unwrap().len(), 1);

        delete_conversation(&log, &dir, "c1").unwrap();
        assert!(!std::path::Path::new(&format!("{}/conversations/c1.jsonl", dir)).exists());
        assert!(list_conversations(&log, &dir).unwrap().is_empty());
    }

    #[test]
    fn save_message_replace_last_assistant() {
        let dir = data_root();
        let log = TestLog;
        init(&log, &dir).unwrap();
        let conv = meta("c1", "2026-01-01T00:00:00Z");
        save_conversation(&log, &dir, &conv).unwrap();
        save_message(&log, &dir, "c1", &msg("user", "q"), false).unwrap();
        save_message(&log, &dir, "c1", &msg("assistant", "old answer"), false).unwrap();

        save_message(&log, &dir, "c1", &msg("assistant", "new answer"), true).unwrap();

        let messages = get_messages(&log, &dir, "c1").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].content, "new answer");
    }

    #[test]
    fn save_message_persists_reasoning_and_reads_back() {
        let dir = data_root();
        let log = TestLog;
        init(&log, &dir).unwrap();
        let conv = meta("c1", "2026-01-01T00:00:00Z");
        save_conversation(&log, &dir, &conv).unwrap();

        let mut assistant = msg("assistant", "正文");
        assistant.reasoning = Some("思考过程".to_string());
        save_message(&log, &dir, "c1", &assistant, false).unwrap();

        // JSONL 行含 reasoning 字段
        let content = std::fs::read_to_string(format!("{}/conversations/c1.jsonl", dir)).unwrap();
        assert!(content.contains("\"reasoning\":\"思考过程\""));

        // 读回：reasoning 与正文同消息还原
        let messages = get_messages(&log, &dir, "c1").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "正文");
        assert_eq!(messages[0].reasoning.as_deref(), Some("思考过程"));
    }

    #[test]
    fn get_messages_legacy_lines_without_reasoning_read_as_none() {
        // P3 前的历史日志无 reasoning 字段：必须读回 None 而非解析失败
        let dir = data_root();
        let log = TestLog;
        std::fs::create_dir_all(format!("{}/conversations", dir)).unwrap();
        let conv = meta("c1", "2026-01-01T00:00:00Z");
        let mut content = meta_json_line(&conv).unwrap();
        content.push_str("\n{\"type\":\"message\",\"role\":\"assistant\",\"content\":\"旧回复\",\"timestamp\":\"t\"}\n");
        std::fs::write(format!("{}/conversations/c1.jsonl", dir), content).unwrap();

        let messages = get_messages(&log, &dir, "c1").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].reasoning, None);
        assert_eq!(messages[0].content, "旧回复");
    }

    #[test]
    fn save_message_replace_last_assistant_overwrites_reasoning() {
        // 重新生成：正文与思考一并覆盖（旧 reasoning 不得残留）
        let dir = data_root();
        let log = TestLog;
        init(&log, &dir).unwrap();
        let conv = meta("c1", "2026-01-01T00:00:00Z");
        save_conversation(&log, &dir, &conv).unwrap();
        save_message(&log, &dir, "c1", &msg("user", "q"), false).unwrap();

        let mut old = msg("assistant", "旧正文");
        old.reasoning = Some("旧思考".to_string());
        save_message(&log, &dir, "c1", &old, false).unwrap();

        let mut fresh = msg("assistant", "新正文");
        fresh.reasoning = Some("新思考".to_string());
        save_message(&log, &dir, "c1", &fresh, true).unwrap();

        let messages = get_messages(&log, &dir, "c1").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].content, "新正文");
        assert_eq!(messages[1].reasoning.as_deref(), Some("新思考"));
        // 旧思考已随旧行一起被覆盖，不残留
        let raw = std::fs::read_to_string(format!("{}/conversations/c1.jsonl", dir)).unwrap();
        assert!(!raw.contains("旧思考"));
    }

    #[test]
    fn save_conversation_updates_meta_first_line() {
        let dir = data_root();
        let log = TestLog;
        init(&log, &dir).unwrap();
        let conv = meta("c1", "2026-01-01T00:00:00Z");
        save_conversation(&log, &dir, &conv).unwrap();
        save_message(&log, &dir, "c1", &msg("user", "hello"), false).unwrap();

        let mut renamed = conv.clone();
        renamed.title = "renamed".to_string();
        renamed.updated_at = "2026-01-04T00:00:00Z".to_string();
        save_conversation(&log, &dir, &renamed).unwrap();

        let content = std::fs::read_to_string(format!("{}/conversations/c1.jsonl", dir)).unwrap();
        let first_line = content.lines().next().unwrap();
        assert!(first_line.contains("\"title\":\"renamed\""));
        // 消息行保留
        assert!(content.contains("\"role\":\"user\""));
        // 索引已更新
        assert_eq!(list_conversations(&log, &dir).unwrap()[0].title, "renamed");
    }

    #[test]
    fn save_conversation_creates_file_when_missing() {
        let dir = data_root();
        let log = TestLog;
        init(&log, &dir).unwrap();
        let conv = meta("c-new", "2026-01-05T00:00:00Z");

        // 新建对话：对话文件不存在，save_conversation 应创建（meta 首行）而非报错
        save_conversation(&log, &dir, &conv).unwrap();

        let content = std::fs::read_to_string(format!("{}/conversations/c-new.jsonl", dir)).unwrap();
        assert!(content.lines().next().unwrap().contains("\"type\":\"meta\""));
        // 索引同步
        assert_eq!(list_conversations(&log, &dir).unwrap()[0].id, "c-new");

        // 之后 save_message 可正常追加（文件已存在）
        save_message(&log, &dir, "c-new", &msg("user", "first"), false).unwrap();
        let messages = get_messages(&log, &dir, "c-new").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "first");
    }
}
