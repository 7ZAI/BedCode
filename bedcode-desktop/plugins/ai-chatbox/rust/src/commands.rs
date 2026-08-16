//! Command Handlers
//!
//! 8 个命令处理器（同步调用，serde_json 类型化参数）：
//! chat-stream / chat-complete / fetch-models / list-conversations /
//! get-messages / save-conversation / save-message / delete-conversation

use crate::client;
use crate::store::{self, ChatMessageRecord, ConversationMeta};
use crate::DATA_DIR;
use bedcode_plugin_api::{CommandArgs, WasmHost};

fn host() -> WasmHost {
    WasmHost
}

/// 流式聊天：透传前端适配层构建的 raw 请求（立即返回，宿主后台推流）
pub fn chat_stream(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let args = CommandArgs::new(args);
    let stream_id = args
        .str("streamId")
        .ok_or_else(|| anyhow::anyhow!("chat_stream: missing streamId"))?;
    let request = args
        .value_owned("request")
        .ok_or_else(|| anyhow::anyhow!("chat_stream: missing request"))?;
    client::chat_stream(&stream_id, &request).map_err(|e| anyhow::anyhow!("chat_stream: {}", e))
}

/// 非流式聊天（测试连接用）：原样返回宿主响应（status/body/headers，前端解析）
pub fn chat_complete(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let args = CommandArgs::new(args);
    let request = args
        .value_owned("request")
        .ok_or_else(|| anyhow::anyhow!("chat_complete: missing request"))?;
    client::chat_complete(&request).map_err(|e| anyhow::anyhow!("chat_complete: {}", e))
}

/// 拉取模型列表：原样返回宿主响应（前端解析 data[].id）
pub fn fetch_models(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let args = CommandArgs::new(args);
    let request = args
        .value_owned("request")
        .ok_or_else(|| anyhow::anyhow!("fetch_models: missing request"))?;
    client::fetch_models(&request).map_err(|e| anyhow::anyhow!("fetch_models: {}", e))
}

/// 列出所有对话（index.jsonl，按 updatedAt DESC）
pub fn list_conversations(_args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let dir = DATA_DIR.read().ok().and_then(|g| g.clone()).unwrap_or_default();
    let conversations = store::list_conversations(&host(), &dir)?;
    Ok(serde_json::json!({ "conversations": conversations }))
}

/// 获取对话消息（跳过 meta 首行）
pub fn get_messages(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let args = CommandArgs::new(args);
    let conversation_id = args
        .str("conversationId")
        .ok_or_else(|| anyhow::anyhow!("get_messages: missing conversationId"))?;

    let messages = store::get_messages(&host(), &data_dir(), &conversation_id)?;
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

    store::save_conversation(&host(), &data_dir(), &conv)?;
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
        reasoning: args
            .value_owned("reasoning")
            .and_then(|v| v.as_str().map(|s| s.to_string())),
    };

    store::save_message(
        &host(),
        &data_dir(),
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

    store::delete_conversation(&host(), &data_dir(), &conversation_id)?;
    Ok(serde_json::json!({ "success": true }))
}

/// 数据目录（activate 时初始化）
fn data_dir() -> String {
    DATA_DIR
        .read()
        .expect("data_dir lock poisoned")
        .clone()
        .expect("data_dir must be initialized during plugin activate")
}
