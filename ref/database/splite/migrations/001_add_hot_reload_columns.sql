-- Migration 001: Add hot reload columns to project_status table
-- 为 project_status 表添加热重载相关字段

BEGIN TRANSACTION;

-- Check if the table exists and add hot reload columns
-- 检查表是否存在并添加热重载字段
ALTER TABLE project_status ADD COLUMN hot_reload_enabled BOOLEAN DEFAULT FALSE;
ALTER TABLE project_status ADD COLUMN hot_reload_config JSON;
ALTER TABLE project_status ADD COLUMN hot_reload_last_enabled DATETIME;
ALTER TABLE project_status ADD COLUMN hot_reload_last_disabled DATETIME;
ALTER TABLE project_status ADD COLUMN hot_reload_changes_detected INTEGER DEFAULT 0;
ALTER TABLE project_status ADD COLUMN hot_reload_errors_count INTEGER DEFAULT 0;

-- Create indexes for better query performance
-- 创建索引以优化查询性能
CREATE INDEX IF NOT EXISTS idx_project_status_hot_reload_enabled 
ON project_status(hot_reload_enabled);

CREATE INDEX IF NOT EXISTS idx_project_status_hot_reload_updated 
ON project_status(hot_reload_last_enabled, hot_reload_last_disabled);

COMMIT;

-- Migration completed successfully
-- 迁移完成