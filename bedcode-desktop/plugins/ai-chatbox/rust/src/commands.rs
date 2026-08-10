//! Command Handlers
//!
//! 8 个命令处理器（同步调用，serde_json 类型化参数）：
//! chat-stream / chat-complete / fetch-models / list-conversations /
//! get-messages / save-conversation / save-message / delete-conversation

use crate::client::{self, ApiProvider, ChatMessage};
use crate::store::{self, ChatMessageRecord, ConversationMeta};
use crate::DATA_DIR;
use bedcode_plugin_api::{CommandArgs, WasmHost};

fn host() -> WasmHost {
    WasmHost
}

/// 流式聊天：经宿主 http_fetch 后台推流，立即返回 streamId
pub fn chat_stream(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let args = CommandArgs::new(args);
    let stream_id = args
        .str("streamId")
        .ok_or_else(|| anyhow::anyhow!("chat_stream: missing streamId"))?;
    let provider: ApiProvider = serde_json::from_value(
        args.value_owned("provider")
            .ok_or_else(|| anyhow::anyhow!("chat_stream: missing provider"))?,
    )
    .map_err(|e| anyhow::anyhow!("chat_stream: invalid provider: {}", e))?;
    let messages: Vec<ChatMessage> = serde_json::from_value(
        args.value_owned("messages")
            .ok_or_else(|| anyhow::anyhow!("chat_stream: missing messages"))?,
    )
    .map_err(|e| anyhow::anyhow!("chat_stream: invalid messages: {}", e))?;

    if provider.api_key.is_empty() {
        return Err(anyhow::anyhow!("chat_stream: provider {} has no api key", provider.id));
    }

    client::chat_stream(&provider, &messages, &stream_id)?;
    Ok(serde_json::json!({ "streamId": stream_id }))
}

/// 非流式聊天（测试连接用），返回回复文本
pub fn chat_complete(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let args = CommandArgs::new(args);
    let provider: ApiProvider = serde_json::from_value(
        args.value_owned("provider")
            .ok_or_else(|| anyhow::anyhow!("chat_complete: missing provider"))?,
    )
    .map_err(|e| anyhow::anyhow!("chat_complete: invalid provider: {}", e))?;
    let messages: Vec<ChatMessage> = serde_json::from_value(
        args.value_owned("messages")
            .ok_or_else(|| anyhow::anyhow!("chat_complete: missing messages"))?,
    )
    .map_err(|e| anyhow::anyhow!("chat_complete: invalid messages: {}", e))?;

    let content = client::chat_complete(&provider, &messages)?;
    Ok(serde_json::json!({ "content": content }))
}

/// 拉取模型列表：真实 GET /models，解析 data[].id
pub fn fetch_models(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let args = CommandArgs::new(args);
    let provider: ApiProvider = serde_json::from_value(
        args.value_owned("provider")
            .ok_or_else(|| anyhow::anyhow!("fetch_models: missing provider"))?,
    )
    .map_err(|e| anyhow::anyhow!("fetch_models: invalid provider: {}", e))?;

    let models = client::fetch_models(&provider)?;
    Ok(serde_json::json!({ "models": models }))
}

/// 列出所有对话（index.jsonl，按 updatedAt DESC）
pub fn list_conversations(_args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let conversations = store::list_conversations(&host(), DATA_DIR.get().map(String::as_str).unwrap_or_default())?;
    Ok(serde_json::json!({ "conversations": conversations }))
}

/// 获取对话消息（跳过 meta 首行）
pub fn get_messages(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let args = CommandArgs::new(args);
    let conversation_id = args
        .str("conversationId")
        .ok_or_else(|| anyhow::anyhow!("get_messages: missing conversationId"))?;

    let messages = store::get_messages(&host(), data_dir(), &conversation_id)?;
    Ok(serde_json::json!({ "messages": messages }))
}

/// 保存/更新对话（meta 首行 + 索引重写）
pub fn save_conversation(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let args = CommandArgs::new(args);
    let conv: ConversationMeta = serde_json::from_value(
        args.value_owned("conversation")
            .ok_or_else(|| anyhow::anyhow!("save_conversation: missing conversation"))?,
    )
    .map_err(|e| anyhow::anyhow!("save_conversation: invalid conversation: {}", e))?;

    store::save_conversation(&host(), data_dir(), &conv)?;
    Ok(serde_json::json!({ "success": true }))
}

/// 保存消息（读-拼-写整文件；replaceLastAssistant 覆盖末尾 assistant 行）
pub fn save_message(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let args = CommandArgs::new(args);
    let conversation_id = args
        .str("conversationId")
        .ok_or_else(|| anyhow::anyhow!("save_message: missing conversationId"))?;
    let role = args
        .str("role")
        .ok_or_else(|| anyhow::anyhow!("save_message: missing role"))?;
    let msg = ChatMessageRecord {
        role: role.to_string(),
        content: args.str_or("content", "").to_string(),
        timestamp: args.str("timestamp").unwrap_or_default().to_string(),
        model: args.value_owned("model").and_then(|v| {
            v.as_str().map(|s| s.to_string())
        }),
        usage: args
            .value_owned("usage")
            .and_then(|v| serde_json::from_value(v).ok()),
    };

    store::save_message(
        &host(),
        data_dir(),
        &conversation_id,
        &msg,
        args.bool_or("replaceLastAssistant", false),
    )?;
    Ok(serde_json::json!({ "success": true }))
}

/// 删除对话（删文件 + 索引移除）
pub fn delete_conversation(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let args = CommandArgs::new(args);
    let conversation_id = args
        .str("conversationId")
        .ok_or_else(|| anyhow::anyhow!("delete_conversation: missing conversationId"))?;

    store::delete_conversation(&host(), data_dir(), &conversation_id)?;
    Ok(serde_json::json!({ "success": true }))
}

/// 数据目录（activate 时初始化）
fn data_dir() -> &'static str {
    DATA_DIR
        .get()
        .expect("data_dir must be initialized during plugin activate")
}
