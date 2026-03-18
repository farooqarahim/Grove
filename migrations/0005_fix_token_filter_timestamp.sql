-- Standardize token_filter_stats.created_at to RFC3339 text (matches all other tables).
-- Existing DATETIME values are already ISO-8601 compatible; this just changes the default.
-- New rows will be inserted with explicit RFC3339 values from Rust code.

-- SQLite does not support ALTER COLUMN, so we recreate the table.
CREATE TABLE token_filter_stats_new (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id           TEXT    NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    session_id       TEXT,
    command          TEXT    NOT NULL,
    filter_type      TEXT    NOT NULL,
    raw_bytes        INTEGER NOT NULL,
    filtered_bytes   INTEGER NOT NULL,
    compression_level INTEGER NOT NULL DEFAULT 1,
    created_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

INSERT INTO token_filter_stats_new
SELECT id, run_id, session_id, command, filter_type, raw_bytes, filtered_bytes,
       compression_level,
       CASE
         WHEN created_at IS NULL THEN strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHEN created_at LIKE '%T%' THEN created_at
         ELSE replace(created_at, ' ', 'T') || '.000000Z'
       END
FROM token_filter_stats;

DROP TABLE token_filter_stats;
ALTER TABLE token_filter_stats_new RENAME TO token_filter_stats;

CREATE INDEX IF NOT EXISTS idx_token_filter_stats_run ON token_filter_stats(run_id);

UPDATE meta SET value = '58' WHERE key = 'schema_version';
