-- Schema for Claude Code Remote

-- Paired devices table
CREATE TABLE IF NOT EXISTS pairings (
    id TEXT PRIMARY KEY,
    device_name TEXT NOT NULL,
    device_fingerprint TEXT NOT NULL UNIQUE,
    public_key TEXT NOT NULL,
    paired_at TEXT NOT NULL,
    last_seen TEXT,
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
    tmux_session TEXT,
    auto_start INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Session history table
CREATE TABLE IF NOT EXISTS history (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    session_name TEXT NOT NULL,
    device_id TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    output_summary TEXT,
    FOREIGN KEY (session_id) REFERENCES session_configs(id),
    FOREIGN KEY (device_id) REFERENCES pairings(id)
);

-- Message history table (for input/output logs)
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    history_id TEXT,
    message_type TEXT NOT NULL CHECK(message_type IN ('input', 'output')),
    content TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    metadata TEXT,  -- JSON for additional metadata
    FOREIGN KEY (session_id) REFERENCES session_configs(id),
    FOREIGN KEY (history_id) REFERENCES history(id)
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
CREATE INDEX IF NOT EXISTS idx_history_session ON history(session_id);
CREATE INDEX IF NOT EXISTS idx_history_started ON history(started_at);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
CREATE INDEX IF NOT EXISTS idx_messages_type ON messages(message_type);
CREATE INDEX IF NOT EXISTS idx_quick_actions_order ON quick_actions(sort_order);
CREATE INDEX IF NOT EXISTS idx_quick_actions_category ON quick_actions(category);
