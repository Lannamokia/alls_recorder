-- Idempotent schema initialization
-- Executed on every startup to ensure all tables and columns exist

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ===== Tables =====

CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL DEFAULT 'user',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS system_config (
    key VARCHAR(255) PRIMARY KEY,
    value JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS user_configs (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    max_bitrate INT,
    max_fps INT,
    resolution VARCHAR(50),
    monitor_id VARCHAR(512),
    desktop_audio VARCHAR(255),
    mic_audio VARCHAR(255),
    rtmp_url VARCHAR(255),
    rtmp_key VARCHAR(255),
    capture_mode VARCHAR(20),
    capture_method VARCHAR(20),
    window_id VARCHAR(512)
);

CREATE TABLE IF NOT EXISTS announcements (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    content TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS user_read_announcements (
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    announcement_id UUID REFERENCES announcements(id) ON DELETE CASCADE,
    read_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, announcement_id)
);

CREATE TABLE IF NOT EXISTS recordings (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    filename VARCHAR(255) NOT NULL,
    filepath VARCHAR(1024) NOT NULL,
    status VARCHAR(50) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- ===== Ensure all columns exist (for databases created before schema updates) =====

-- user_configs columns
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS max_bitrate INT;
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS max_fps INT;
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS resolution VARCHAR(50);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS monitor_id VARCHAR(512);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS desktop_audio VARCHAR(255);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS mic_audio VARCHAR(255);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS rtmp_url VARCHAR(255);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS rtmp_key VARCHAR(255);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS capture_mode VARCHAR(20);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS capture_method VARCHAR(20);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS window_id VARCHAR(512);

-- Widen monitor_id for Windows device interface paths (e.g. \\?\DISPLAY#...#{guid})
-- This is safe to run repeatedly; ALTER COLUMN TYPE is a no-op if already the target type
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'user_configs' AND column_name = 'monitor_id'
        AND character_maximum_length < 512
    ) THEN
        ALTER TABLE user_configs ALTER COLUMN monitor_id TYPE VARCHAR(512);
    END IF;
END $$;
