-- Migration 0002: track which coding agent and model was chosen per run/task.

-- tasks: store the provider id chosen at queue time (e.g. "claude_code", "codex", "gemini").
ALTER TABLE tasks ADD COLUMN provider TEXT;

-- runs: store the resolved provider and model for observability / UI display.
ALTER TABLE runs ADD COLUMN provider TEXT;
ALTER TABLE runs ADD COLUMN model     TEXT;
