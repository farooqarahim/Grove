#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"

echo "=== Schema Verification ==="

# 1. Apply all migrations to a fresh in-memory DB
TEMP_DB=$(mktemp /tmp/grove-schema-verify.XXXXXX.db)
trap 'rm -f "$TEMP_DB"' EXIT

echo "[1/4] Applying migrations to fresh database..."
for sql_file in "$MIGRATIONS_DIR"/*.sql; do
    echo "  Applying $(basename "$sql_file")..."
    sqlite3 "$TEMP_DB" < "$sql_file"
done

# 2. Verify foreign keys are enabled and valid
echo "[2/4] Checking foreign key integrity..."
FK_RESULT=$(sqlite3 "$TEMP_DB" "PRAGMA foreign_key_check;")
if [ -n "$FK_RESULT" ]; then
    echo "FAIL: Foreign key violations found:"
    echo "$FK_RESULT"
    exit 1
fi
echo "  Foreign keys OK"

# 3. Verify all expected tables exist
echo "[3/4] Verifying expected tables..."
EXPECTED_TABLES=(
    meta workspaces users projects conversations runs sessions messages events
    checkpoints merge_queue tasks subtasks plan_steps audit_log
    performance_samples signals issues ownership_locks chatter_threads
    grove_graphs grove_phases grove_steps grove_graph_config
    pipeline_stages run_artifacts phase_checkpoints token_filter_stats
)
ACTUAL_TABLES=$(sqlite3 "$TEMP_DB" ".tables" | tr -s ' \n' '\n' | sort)

MISSING=0
for table in "${EXPECTED_TABLES[@]}"; do
    if ! echo "$ACTUAL_TABLES" | grep -qw "$table"; then
        echo "  MISSING: $table"
        MISSING=1
    fi
done

if [ "$MISSING" -eq 1 ]; then
    echo "FAIL: Missing tables detected"
    exit 1
fi
echo "  All ${#EXPECTED_TABLES[@]} tables present"

# 4. Verify schema version
echo "[4/4] Checking schema version..."
SCHEMA_VERSION=$(sqlite3 "$TEMP_DB" "SELECT value FROM meta WHERE key='schema_version';")
echo "  Schema version: $SCHEMA_VERSION"

echo ""
echo "=== Schema verification passed ==="
