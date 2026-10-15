//! Command Handlers
//!
//! 8 个命令处理器（同步调用，serde_json 类型化参数）：
//! chat-stream / chat-complete / fetch-models / list-conversations /
//! get-messages / save-conversation / save-message / delete-conversation

use crate::client;
use crate::store::{self, ChatMessageRecord, ConversationMeta};
use crate::DATA_DIR;
use bedcode_plugin_api_mobile::{CommandArgs, WasmHost};

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
    let conversations = store::list_conversations(&host(), &data_dir()?)?;
    Ok(serde_json::json!({ "conversations": conversations }))
}

/// 获取对话消息（跳过 meta 首行）
pub fn get_messages(args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let args = CommandArgs::new(args);
    let conversation_id = args
        .str("conversationId")
        .ok_or_else(|| anyhow::anyhow!("get_messages: missing conversationId"))?;

    let messages = store::get_messages(&host(), &data_dir()?, &conversation_id)?;
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

    store::save_conversation(&host(), &data_dir()?, &conv)?;
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
        &data_dir()?,
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

    store::delete_conversation(&host(), &data_dir()?, &conversation_id)?;
    Ok(serde_json::json!({ "success": true }))
}

/// 数据目录（activate 时初始化）
///
/// 未初始化时返回**错误而不是 panic**：宿主可能因目录授权被拒 / 激活失败而未
/// 调 activate，而插件前端仍会发命令（实测真机：activate 失败 + 前端
/// `list-conversations` → 旧实现的 `expect` panic → wasm `unreachable` trap →
/// 真机 trap 未被处理 → 进程 SIGILL 崩溃）。错误文案带操作上下文，便于在宿主
/// 日志里一眼看出是「插件未激活」而不是业务失败。
fn data_dir() -> anyhow::Result<String> {
    DATA_DIR
        .read()
        .map_err(|e| anyhow::anyhow!("data_dir: lock poisoned: {}", e))?
        .clone()
        .ok_or_else(|| anyhow::anyhow!("data_dir: plugin not activated (activate must succeed first)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 未激活（DATA_DIR 未初始化）时命令面返回 Err，**不得 panic**
    ///
    /// 真机链路（2026-10-15 SIGILL）：activate 失败后插件前端仍发命令，旧实现的
    /// `expect("data_dir must be initialized during plugin activate")` 即 panic →
    /// wasm `unreachable` → 真机 trap 未被处理 → 进程退出。
    #[test]
    fn commands_before_activate_return_error_not_panic() {
        *crate::DATA_DIR.write().expect("data_dir write lock") = None;

        let err = list_conversations(serde_json::json!({})).expect_err("未激活时 list 应 Err");
        assert!(
            err.to_string().contains("not activated"),
            "错误文案需点名未激活，便于宿主排障: {err}"
        );
        let err = get_messages(serde_json::json!({ "conversationId": "c1" })).expect_err("未激活时 get 应 Err");
        assert!(err.to_string().contains("not activated"), "got: {err}");
        // save/delete：参数先过校验、再取数据目录（字段名 camelCase 同 ConversationMeta）
        let conv = serde_json::json!({
            "id": "c1",
            "title": "t",
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z",
            "providerId": "p1",
            "providerName": "P",
            "model": "m",
        });
        let err = save_conversation(serde_json::json!({ "conversation": conv }))
            .expect_err("未激活时 save 应 Err");
        assert!(err.to_string().contains("not activated"), "got: {err}");
        let err = save_message(serde_json::json!({ "conversationId": "c1", "role": "user" }))
            .expect_err("未激活时 save_message 应 Err");
        assert!(err.to_string().contains("not activated"), "got: {err}");
        let err = delete_conversation(serde_json::json!({ "conversationId": "c1" }))
            .expect_err("未激活时 delete 应 Err");
        assert!(err.to_string().contains("not activated"), "got: {err}");
    }
}
