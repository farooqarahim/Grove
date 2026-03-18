-- Migration 0036: Performance indexes for hot query paths.
CREATE INDEX IF NOT EXISTS idx_projects_root_path ON projects(root_path);
CREATE INDEX IF NOT EXISTS idx_issues_project_updated ON issues(project_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_issues_provider_ext_project ON issues(provider, external_id, project_id);
CREATE INDEX IF NOT EXISTS idx_runs_created ON runs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_runs_conversation ON runs(conversation_id, created_at DESC);
