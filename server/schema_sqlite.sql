-- SQLite schema (单机零配置部署)
-- 与 schema.sql（PostgreSQL）保持同构，方言差异说明：
--   * UUID 列使用 BLOB 类型（BLOB affinity）：sqlx 将 Uuid 绑定为 16 字节 BLOB，
--     BLOB affinity 不做任何存储/比较转换，保证 WHERE 比较与 FK 引用一致；
--   * JSON/JSONB 列使用 TEXT（sqlx "json" feature 以 TEXT 编解码 serde_json::Value）；
--   * 时间戳使用 TEXT，CURRENT_TIMESTAMP 输出 "YYYY-MM-%d HH:MM:SS"，
--     sqlx chrono 解码支持该格式；应用侧绑定则写 RFC3339，两者混存均可解码；
--   * SQLite 不支持 ADD COLUMN IF NOT EXISTS / DO 块，本文件仅供全新库初始化，
--     已有库的列演进需另行处理。

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS users (
    id BLOB PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS system_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS user_configs (
    user_id BLOB PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    max_bitrate INTEGER,
    max_fps INTEGER,
    resolution TEXT,
    monitor_id TEXT,
    desktop_audio TEXT,
    mic_audio TEXT,
    rtmp_url TEXT,
    rtmp_key TEXT,
    capture_mode TEXT,
    capture_method TEXT,
    window_id TEXT
);

CREATE TABLE IF NOT EXISTS announcements (
    id BLOB PRIMARY KEY,
    content TEXT NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    created_by BLOB REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS user_read_announcements (
    user_id BLOB REFERENCES users(id) ON DELETE CASCADE,
    announcement_id BLOB REFERENCES announcements(id) ON DELETE CASCADE,
    read_at TEXT DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, announcement_id)
);

CREATE TABLE IF NOT EXISTS recordings (
    id BLOB PRIMARY KEY,
    user_id BLOB REFERENCES users(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    filepath TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_recordings_user ON recordings(user_id);
CREATE INDEX IF NOT EXISTS idx_announcements_created ON announcements(created_at);
