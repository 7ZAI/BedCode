//! Custom SQLite Tables
//!
//! ai-chatbox 插件的自定义数据库表操作
//! 所有表名以 plugin_com_bedcode_ai_chatbox_ 为前缀，确保宿主校验通过

use crate::HOST_CONTEXT;
use serde::{Deserialize, Serialize};

/// 对话元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMeta {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub provider_name: String,
}

/// 消息记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageRecord {
    pub id: i64,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

const TABLE_CONVERSATIONS: &str = "plugin_com_bedcode_ai_chatbox_conversations";
const TABLE_MESSAGES: &str = "plugin_com_bedcode_ai_chatbox_messages";

/// 初始化自定义数据库表
pub fn init() -> anyhow::Result<()> {
    let host = HOST_CONTEXT.get()
        .ok_or_else(|| anyhow::anyhow!("Plugin not activated"))?;

    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (\
            id TEXT PRIMARY KEY, \
            title TEXT NOT NULL, \
            created_at TEXT NOT NULL, \
            updated_at TEXT NOT NULL, \
            provider_name TEXT NOT NULL\
        );\
        CREATE TABLE IF NOT EXISTS {} (\
            id INTEGER PRIMARY KEY AUTOINCREMENT, \
            conversation_id TEXT NOT NULL, \
            role TEXT NOT NULL, \
            content TEXT NOT NULL, \
            timestamp TEXT NOT NULL, \
            FOREIGN KEY (conversation_id) REFERENCES {}(id)\
        );",
        TABLE_CONVERSATIONS, TABLE_MESSAGES, TABLE_CONVERSATIONS
    );

    let result = host.db_execute_sql(&sql);
    if result < 0 {
        return Err(anyhow::anyhow!("Failed to create tables: error code {}", result));
    }

    tracing::info!("[AiChatbox] Custom DB tables initialized");
    Ok(())
}

/// 列出所有对话
pub fn list_conversations() -> anyhow::Result<Vec<ConversationMeta>> {
    let host = HOST_CONTEXT.get()
        .ok_or_else(|| anyhow::anyhow!("Plugin not activated"))?;

    let sql = format!(
        "SELECT id, title, created_at, updated_at, provider_name FROM {} ORDER BY updated_at DESC",
        TABLE_CONVERSATIONS
    );

    let rows = host.db_query_sql(&sql)
        .ok_or_else(|| anyhow::anyhow!("Failed to query conversations"))?;

    let conversations: Vec<ConversationMeta> = serde_json::from_value(rows)
        .unwrap_or_default();

    Ok(conversations)
}

/// 获取对话的所有消息
pub fn get_messages(conversation_id: &str) -> anyhow::Result<Vec<ChatMessageRecord>> {
    let host = HOST_CONTEXT.get()
        .ok_or_else(|| anyhow::anyhow!("Plugin not activated"))?;

    let sql = format!(
        "SELECT id, conversation_id, role, content, timestamp FROM {} WHERE conversation_id = '{}' ORDER BY timestamp ASC",
        TABLE_MESSAGES, conversation_id.replace('\'', "''")
    );

    let rows = host.db_query_sql(&sql)
        .ok_or_else(|| anyhow::anyhow!("Failed to query messages"))?;

    let messages: Vec<ChatMessageRecord> = serde_json::from_value(rows)
        .unwrap_or_default();

    Ok(messages)
}

/// 保存对话（INSERT OR REPLACE）
pub fn save_conversation(conv: &ConversationMeta) -> anyhow::Result<()> {
    let host = HOST_CONTEXT.get()
        .ok_or_else(|| anyhow::anyhow!("Plugin not activated"))?;

    let sql = format!(
        "INSERT OR REPLACE INTO {} (id, title, created_at, updated_at, provider_name) VALUES ('{}', '{}', '{}', '{}', '{}')",
        TABLE_CONVERSATIONS,
        conv.id.replace('\'', "''"),
        conv.title.replace('\'', "''"),
        conv.created_at.replace('\'', "''"),
        conv.updated_at.replace('\'', "''"),
        conv.provider_name.replace('\'', "''")
    );

    let result = host.db_execute_sql(&sql);
    if result < 0 {
        return Err(anyhow::anyhow!("Failed to save conversation: error code {}", result));
    }
    Ok(())
}

/// 保存消息
pub fn save_message(conversation_id: &str, role: &str, content: &str, timestamp: &str) -> anyhow::Result<()> {
    let host = HOST_CONTEXT.get()
        .ok_or_else(|| anyhow::anyhow!("Plugin not activated"))?;

    let sql = format!(
        "INSERT INTO {} (conversation_id, role, content, timestamp) VALUES ('{}', '{}', '{}', '{}')",
        TABLE_MESSAGES,
        conversation_id.replace('\'', "''"),
        role.replace('\'', "''"),
        content.replace('\'', "''"),
        timestamp.replace('\'', "''")
    );

    let result = host.db_execute_sql(&sql);
    if result < 0 {
        return Err(anyhow::anyhow!("Failed to save message: error code {}", result));
    }
    Ok(())
}

/// 删除对话及其所有消息
pub fn delete_conversation(conversation_id: &str) -> anyhow::Result<()> {
    let host = HOST_CONTEXT.get()
        .ok_or_else(|| anyhow::anyhow!("Plugin not activated"))?;

    // 先删除消息
    let sql_msgs = format!(
        "DELETE FROM {} WHERE conversation_id = '{}'",
        TABLE_MESSAGES,
        conversation_id.replace('\'', "''")
    );
    host.db_execute_sql(&sql_msgs);

    // 再删除对话
    let sql_conv = format!(
        "DELETE FROM {} WHERE id = '{}'",
        TABLE_CONVERSATIONS,
        conversation_id.replace('\'', "''")
    );
    let result = host.db_execute_sql(&sql_conv);
    if result < 0 {
        return Err(anyhow::anyhow!("Failed to delete conversation: error code {}", result));
    }
    Ok(())
}
