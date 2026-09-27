//! 建表 + 幂等迁移（每条单独 execute：宿主 plugin_db_execute 为单语句语义，
//! 多语句会被静默截断；CREATE IF NOT EXISTS 可对旧库重跑）

use bedcode_plugin_api::host::HostPluginDatabase;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::Value;

// ==================== Schema（幂等建表，单语句逐条执行） ====================

/// 建表 + 索引（CREATE IF NOT EXISTS 幂等，可对旧库重跑；每条单独 execute，
/// 宿主 plugin_db_execute 为单语句语义，多语句会被静默截断）
pub(crate) fn ensure_schema(h: &WasmHost) -> anyhow::Result<()> {
    let stmts = [
        // 解析水位：mtime 预留（WIT 无 mtime 原语，当前恒 NULL，水位判定用 size）；
        // `signature` 票 07 新增——SQLite 源的变更指纹（`{db 字节}:{wal 字节}`），
        // 见 ensure_schema 的幂等迁移
        "CREATE TABLE IF NOT EXISTS parse_watermark (\
             id INTEGER PRIMARY KEY AUTOINCREMENT,\
             adapter TEXT NOT NULL,\
             source_path TEXT NOT NULL,\
             size INTEGER,\
             mtime INTEGER,\
             parsed_at INTEGER,\
             signature TEXT,\
             UNIQUE(adapter, source_path))",
        // 会话聚合（统计查询真源）；source_path / models_json 为 spec 草案
        // 之外的功能列：前者供日志视图定位源文件（§4.6 顶部展示），后者存
        // 每模型明细（按模型维度聚合在 Rust 侧展开，避免会话多模型时失真）
        "CREATE TABLE IF NOT EXISTS usage_session (\
             id INTEGER PRIMARY KEY AUTOINCREMENT,\
             adapter TEXT NOT NULL,\
             cli_session_id TEXT NOT NULL,\
             project TEXT,\
             title TEXT,\
             source_path TEXT,\
             started_at INTEGER,\
             ended_at INTEGER,\
             duration_ms INTEGER,\
             model TEXT,\
             models_json TEXT,\
             tokens_in INTEGER DEFAULT 0,\
             tokens_out INTEGER DEFAULT 0,\
             tokens_cache_read INTEGER DEFAULT 0,\
             tokens_cache_write INTEGER DEFAULT 0,\
             tokens_reasoning INTEGER DEFAULT 0,\
             cost_total REAL,\
             first_seen_at INTEGER,\
             updated_at INTEGER,\
             UNIQUE(adapter, cli_session_id))",
        "CREATE INDEX IF NOT EXISTS idx_usage_session_started ON usage_session(adapter, started_at)",
        "CREATE INDEX IF NOT EXISTS idx_usage_session_project ON usage_session(project)",
        // 供应商预设（票据 05 预留；api_key 中心凭据列由 providers::ensure_schema
        // 幂等迁移补齐，此处只保证表存在）
        "CREATE TABLE IF NOT EXISTS provider_preset (\
             id INTEGER PRIMARY KEY AUTOINCREMENT,\
             name TEXT NOT NULL,\
             base_url TEXT,\
             api_style TEXT,\
             models_json TEXT,\
             notes TEXT,\
             created_at INTEGER,\
             updated_at INTEGER)",
    ];
    for stmt in stmts {
        h.plugin_db_execute(stmt)
            .map_err(|e| anyhow::anyhow!("usage: schema execute failed: {e}"))?;
    }
    // 幂等迁移（票 07）：票 06 建的库无 `signature` 列，检测到缺列才补。
    // SQLite 无 `ADD COLUMN IF NOT EXISTS`，故先用 PRAGMA 判定（同 providers.rs
    // 的 api_key 迁移写法）。
    let cols = h
        .plugin_db_query("PRAGMA table_info(parse_watermark)")
        .map_err(|e| anyhow::anyhow!("usage: pragma failed: {e}"))?
        .unwrap_or(Value::Array(Vec::new()));
    let has_signature = cols
        .as_array()
        .map(|arr| {
            arr.iter()
                .any(|c| c.get("name").and_then(|v| v.as_str()) == Some("signature"))
        })
        .unwrap_or(false);
    if !has_signature {
        h.plugin_db_execute("ALTER TABLE parse_watermark ADD COLUMN signature TEXT")
            .map_err(|e| anyhow::anyhow!("usage: migrate signature failed: {e}"))?;
    }
    Ok(())
}
