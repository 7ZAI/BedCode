-- Schema for Claude Code Remote

-- 2026-09-22 认证记录下沉（v24）：`pairings` / `connection_history` / `session_configs`
-- 三表退役——认证记录（配对设备 / 连接历史）与会话配置真源已下沉
-- `com.bedcode.terminal-session` 插件私有库（旧库存量由宿主 handoff 迁移：
-- `plugin/auth_records_migration.rs` 推 pairings/connection_history，quick_actions
-- 同款的 config 迁移已随 v21 完成）。内核主库只保留配置与引擎原语。

-- App settings table
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Plugin key-value storage (per-plugin isolation)
CREATE TABLE IF NOT EXISTS plugin_storage (
    plugin_id TEXT NOT NULL,
    key       TEXT NOT NULL,
    value     TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (plugin_id, key)
);

-- 密钥托管（v15 host-auth secret-store，按插件属主隔离）
-- 明文不落日志（宿主只记长度）；本表是凭据的唯一指定存储位（AGENTS.md §8
-- 「日志与存储中凭据只记长度不落明文」的例外/指定位——secret-store 的用途即
-- 可读回凭据，其余任何存储/日志位置禁止出现值本身）。
-- v24 追加：生物凭证公钥亦托管于此（key = `biometric:<fingerprint>`，
-- 配对记录下沉后公钥随 §8 凭据红线留宿主）
CREATE TABLE IF NOT EXISTS plugin_secrets (
    plugin_id  TEXT NOT NULL,
    key        TEXT NOT NULL,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (plugin_id, key)
);