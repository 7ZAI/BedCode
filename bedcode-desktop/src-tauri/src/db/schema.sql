-- Schema for Claude Code Remote

-- Paired devices table
CREATE TABLE IF NOT EXISTS pairings (
    id TEXT PRIMARY KEY,
    device_name TEXT NOT NULL,
    device_fingerprint TEXT NOT NULL UNIQUE,
    public_key TEXT NOT NULL,
    address TEXT,
    session_token TEXT,
    paired_at TEXT NOT NULL,
    last_seen TEXT,
    connect_count INTEGER DEFAULT 1,
    is_active INTEGER DEFAULT 1
);

-- Session configurations table
CREATE TABLE IF NOT EXISTS session_configs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    environment TEXT NOT NULL CHECK(environment IN ('windows', 'wsl2')),
    wsl_distro TEXT,
    working_dir TEXT NOT NULL,
    command TEXT NOT NULL,
    auto_start INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Quick actions table
CREATE TABLE IF NOT EXISTS quick_actions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    content TEXT NOT NULL,
    icon TEXT,
    color TEXT,
    category TEXT,
    sort_order INTEGER DEFAULT 0,
    created_at TEXT NOT NULL
);

-- App settings table
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_pairings_fingerprint ON pairings(device_fingerprint);
CREATE INDEX IF NOT EXISTS idx_pairings_active ON pairings(is_active);
CREATE INDEX IF NOT EXISTS idx_session_configs_name ON session_configs(name);
CREATE INDEX IF NOT EXISTS idx_quick_actions_order ON quick_actions(sort_order);
CREATE INDEX IF NOT EXISTS idx_quick_actions_category ON quick_actions(category);

-- Plugin key-value storage (per-plugin isolation)
CREATE TABLE IF NOT EXISTS plugin_storage (
    plugin_id TEXT NOT NULL,
    key       TEXT NOT NULL,
    value     TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (plugin_id, key)
);
