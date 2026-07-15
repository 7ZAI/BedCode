//! Command Handlers (Mobile)
//!
//! ai-chatbox 插件的自定义 command 处理函数
//! WASM 模式同步调用

use crate::ai_client::{self, ApiProvider, ChatMessage};
use crate::db;
use bedcode_plugin_api_mobile::WasmHost;

/// 生成 RFC 3339 格式的时间戳（替代 chrono，避免 WASM 兼容问题）
fn now_rfc3339() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let days = secs / 86400;
    let (year, month, day) = days_to_ymd(days);
    let time_of_day = secs % 86400;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hour, minute, second)
}

/// 将自 epoch 以来的天数转换为 (year, month, day)
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let leap = is_leap(year);
    let month_days: [u64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// 获取 WasmHost 实例
fn host() -> WasmHost {
    WasmHost::new("com.bedcode.ai-chatbox")
}

/// 流式聊天：WASM 模式同步调用 http_fetch stream:true
pub fn chat_stream(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: serde_json::Value = serde_json::from_str(args_json)?;
    let stream_id = args["streamId"].as_str().unwrap_or("").to_string();
    let provider: ApiProvider = serde_json::from_value(args["provider"].clone())?;
    let messages: Vec<ChatMessage> = serde_json::from_value(args["messages"].clone())?;

    if stream_id.is_empty() {
        return Err(anyhow::anyhow!("Missing streamId"));
    }

    ai_client::chat_stream(&provider, &messages, &stream_id)?;
    Ok(serde_json::json!({ "streamId": stream_id }))
}

/// 非流式聊天
pub fn chat_complete(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: serde_json::Value = serde_json::from_str(args_json)?;
    let provider: ApiProvider = serde_json::from_value(args["provider"].clone())?;
    let messages: Vec<ChatMessage> = serde_json::from_value(args["messages"].clone())?;

    let result = ai_client::chat_complete(&provider, &messages)?;
    Ok(serde_json::json!({ "content": result }))
}

/// 提示词优化
pub fn optimize_prompt(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: serde_json::Value = serde_json::from_str(args_json)?;
    let provider: ApiProvider = serde_json::from_value(args["provider"].clone())?;
    let prompt = args["prompt"].as_str().unwrap_or("").to_string();

    let system_prompt = "你是一个专业的终端提示词优化器。用户会给你一段终端命令或提示词，\
        你需要将其优化为更精确、更有效的版本。保持原始意图，但改进表达方式。\
        只返回优化后的文本，不要添加解释。";

    let messages = vec![
        ChatMessage { role: "system".to_string(), content: system_prompt.to_string() },
        ChatMessage { role: "user".to_string(), content: prompt.clone() },
    ];

    let optimized = ai_client::chat_complete(&provider, &messages)?;
    Ok(serde_json::json!({ "original": prompt, "optimized": optimized }))
}

/// 列出所有对话
pub fn list_conversations(_args_json: &str) -> anyhow::Result<serde_json::Value> {
    let conversations = db::list_conversations(&host())?;
    Ok(serde_json::json!({ "conversations": conversations }))
}

/// 获取对话消息
pub fn get_messages(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: serde_json::Value = serde_json::from_str(args_json)?;
    let conversation_id = args["conversationId"].as_str().unwrap_or("").to_string();

    if conversation_id.is_empty() {
        return Err(anyhow::anyhow!("Missing conversationId"));
    }

    let messages = db::get_messages(&host(), &conversation_id)?;
    Ok(serde_json::json!({ "messages": messages }))
}

/// 保存对话
pub fn save_conversation(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: serde_json::Value = serde_json::from_str(args_json)?;
    let conv: db::ConversationMeta = serde_json::from_value(args["conversation"].clone())?;

    db::save_conversation(&host(), &conv)?;
    Ok(serde_json::json!({ "success": true }))
}

/// 保存消息
pub fn save_message(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: serde_json::Value = serde_json::from_str(args_json)?;
    let conversation_id = args["conversationId"].as_str().unwrap_or("").to_string();
    let role = args["role"].as_str().unwrap_or("").to_string();
    let content = args["content"].as_str().unwrap_or("").to_string();
    let timestamp = args["timestamp"].as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| now_rfc3339());

    if conversation_id.is_empty() || role.is_empty() {
        return Err(anyhow::anyhow!("Missing conversationId or role"));
    }

    db::save_message(&host(), &conversation_id, &role, &content, &timestamp)?;
    Ok(serde_json::json!({ "success": true }))
}

/// 删除对话
pub fn delete_conversation(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: serde_json::Value = serde_json::from_str(args_json)?;
    let conversation_id = args["conversationId"].as_str().unwrap_or("").to_string();

    if conversation_id.is_empty() {
        return Err(anyhow::anyhow!("Missing conversationId"));
    }

    db::delete_conversation(&host(), &conversation_id)?;
    Ok(serde_json::json!({ "success": true }))
}
