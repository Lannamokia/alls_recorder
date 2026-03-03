-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Users table
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL DEFAULT 'user', -- 'admin', 'user'
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- System Config table (Key-Value store)
CREATE TABLE IF NOT EXISTS system_config (
    key VARCHAR(255) PRIMARY KEY,
    value JSONB NOT NULL
);

-- User Configs table
CREATE TABLE IF NOT EXISTS user_configs (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    max_bitrate INT,
    max_fps INT,
    resolution VARCHAR(50),
    monitor_id VARCHAR(50),
    desktop_audio VARCHAR(255),
    mic_audio VARCHAR(255),
    rtmp_url VARCHAR(255),
    rtmp_key VARCHAR(255)
);

-- Announcements table
CREATE TABLE IF NOT EXISTS announcements (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    content TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL
);

-- User Read Announcements table
CREATE TABLE IF NOT EXISTS user_read_announcements (
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    announcement_id UUID REFERENCES announcements(id) ON DELETE CASCADE,
    read_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, announcement_id)
);

-- Recordings table
CREATE TABLE IF NOT EXISTS recordings (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    filename VARCHAR(255) NOT NULL,
    filepath VARCHAR(1024) NOT NULL,
    status VARCHAR(50) NOT NULL, -- 'recording', 'stopped', 'saved'
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS max_bitrate INT;
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS max_fps INT;
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS resolution VARCHAR(50);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS monitor_id VARCHAR(50);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS desktop_audio VARCHAR(255);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS mic_audio VARCHAR(255);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS rtmp_url VARCHAR(255);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS rtmp_key VARCHAR(255);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS capture_mode VARCHAR(20);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS capture_method VARCHAR(20);
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS window_id VARCHAR(512);

ALTER TABLE users ADD COLUMN IF NOT EXISTS id UUID DEFAULT uuid_generate_v4();
ALTER TABLE users ADD COLUMN IF NOT EXISTS username VARCHAR(255);
ALTER TABLE users ADD COLUMN IF NOT EXISTS password_hash VARCHAR(255);
ALTER TABLE users ADD COLUMN IF NOT EXISTS role VARCHAR(50) DEFAULT 'user';
ALTER TABLE users ADD COLUMN IF NOT EXISTS created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP;

ALTER TABLE system_config ADD COLUMN IF NOT EXISTS key VARCHAR(255);
ALTER TABLE system_config ADD COLUMN IF NOT EXISTS value JSONB;

ALTER TABLE announcements ADD COLUMN IF NOT EXISTS id UUID DEFAULT uuid_generate_v4();
ALTER TABLE announcements ADD COLUMN IF NOT EXISTS content TEXT;
ALTER TABLE announcements ADD COLUMN IF NOT EXISTS created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP;
ALTER TABLE announcements ADD COLUMN IF NOT EXISTS created_by UUID;

ALTER TABLE user_read_announcements ADD COLUMN IF NOT EXISTS user_id UUID;
ALTER TABLE user_read_announcements ADD COLUMN IF NOT EXISTS announcement_id UUID;
ALTER TABLE user_read_announcements ADD COLUMN IF NOT EXISTS read_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP;

ALTER TABLE recordings ADD COLUMN IF NOT EXISTS id UUID DEFAULT uuid_generate_v4();
ALTER TABLE recordings ADD COLUMN IF NOT EXISTS user_id UUID;
ALTER TABLE recordings ADD COLUMN IF NOT EXISTS filename VARCHAR(255);
ALTER TABLE recordings ADD COLUMN IF NOT EXISTS filepath VARCHAR(1024);
ALTER TABLE recordings ADD COLUMN IF NOT EXISTS status VARCHAR(50);
ALTER TABLE recordings ADD COLUMN IF NOT EXISTS created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP;
