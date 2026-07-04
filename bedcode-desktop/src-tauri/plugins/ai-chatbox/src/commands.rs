//! Command Handlers
//!
//! ai-chatbox 插件的自定义 Tauri command 处理函数
//! 每个函数接收 JSON 参数字符串，返回 JSON Value

use crate::ai_client::{self, ApiProvider, ChatMessage};
use crate::db;
use chrono::Utc;

/// 流式聊天：启动异步任务，通过事件推送 chunks
pub fn chat_stream(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: serde_json::Value = serde_json::from_str(args_json)?;
    let stream_id = args["streamId"].as_str().unwrap_or("").to_string();
    let provider: ApiProvider = serde_json::from_value(args["provider"].clone())?;
    let messages: Vec<ChatMessage> = serde_json::from_value(args["messages"].clone())?;

    if stream_id.is_empty() {
        return Err(anyhow::anyhow!("Missing streamId"));
    }

    // spawn 异步任务，立即返回 stream_id
    // 错误由 ai_client::chat_stream 内部通过 emit 推送到前端
    let spawn_stream_id = stream_id.clone();
    tokio::spawn(async move {
        let _ = ai_client::chat_stream(&provider, &messages, &spawn_stream_id).await;
    });

    Ok(serde_json::json!({ "streamId": stream_id }))
}

/// 非流式聊天（用于短回复场景）
pub fn chat_complete(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: serde_json::Value = serde_json::from_str(args_json)?;
    let provider: ApiProvider = serde_json::from_value(args["provider"].clone())?;
    let messages: Vec<ChatMessage> = serde_json::from_value(args["messages"].clone())?;

    let rt = tokio::runtime::Handle::current();
    let result = rt.block_on(async {
        ai_client::chat_complete(&provider, &messages).await
    })?;

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

    let rt = tokio::runtime::Handle::current();
    let optimized = rt.block_on(async {
        ai_client::chat_complete(&provider, &messages).await
    })?;

    Ok(serde_json::json!({ "original": prompt, "optimized": optimized }))
}

/// 列出所有对话
pub fn list_conversations(_args_json: &str) -> anyhow::Result<serde_json::Value> {
    let conversations = db::list_conversations()?;
    Ok(serde_json::json!({ "conversations": conversations }))
}

/// 获取对话消息
pub fn get_messages(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: serde_json::Value = serde_json::from_str(args_json)?;
    let conversation_id = args["conversationId"].as_str().unwrap_or("").to_string();

    if conversation_id.is_empty() {
        return Err(anyhow::anyhow!("Missing conversationId"));
    }

    let messages = db::get_messages(&conversation_id)?;
    Ok(serde_json::json!({ "messages": messages }))
}

/// 保存对话
pub fn save_conversation(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: serde_json::Value = serde_json::from_str(args_json)?;
    let conv: db::ConversationMeta = serde_json::from_value(args["conversation"].clone())?;

    db::save_conversation(&conv)?;
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
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    if conversation_id.is_empty() || role.is_empty() {
        return Err(anyhow::anyhow!("Missing conversationId or role"));
    }

    db::save_message(&conversation_id, &role, &content, &timestamp)?;
    Ok(serde_json::json!({ "success": true }))
}

/// 删除对话
pub fn delete_conversation(args_json: &str) -> anyhow::Result<serde_json::Value> {
    let args: serde_json::Value = serde_json::from_str(args_json)?;
    let conversation_id = args["conversationId"].as_str().unwrap_or("").to_string();

    if conversation_id.is_empty() {
        return Err(anyhow::anyhow!("Missing conversationId"));
    }

    db::delete_conversation(&conversation_id)?;
    Ok(serde_json::json!({ "success": true }))
}
