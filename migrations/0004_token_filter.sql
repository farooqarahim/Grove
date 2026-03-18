-- Token filter statistics: tracks per-command compression savings for each run.
CREATE TABLE IF NOT EXISTS token_filter_stats (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id           TEXT    NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    session_id       TEXT,
    command          TEXT    NOT NULL,
    filter_type      TEXT    NOT NULL,
    raw_bytes        INTEGER NOT NULL,
    filtered_bytes   INTEGER NOT NULL,
    compression_level INTEGER NOT NULL DEFAULT 1,
    created_at       DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_token_filter_stats_run ON token_filter_stats(run_id);

UPDATE meta SET value = '57' WHERE key = 'schema_version';
