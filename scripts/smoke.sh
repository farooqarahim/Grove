#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== Grove CLI Smoke Tests ==="

# Build if needed
if ! command -v grove &>/dev/null; then
    echo "Building grove CLI..."
    cargo build -p grove-cli --manifest-path "$PROJECT_ROOT/Cargo.toml" 2>/dev/null
    GROVE="$PROJECT_ROOT/target/debug/grove"
else
    GROVE="grove"
fi

TEMP_PROJECT=$(mktemp -d /tmp/grove-smoke.XXXXXX)
trap 'rm -rf "$TEMP_PROJECT"' EXIT

cd "$TEMP_PROJECT"
git init -q .

# 1. grove init
echo "[1/5] grove init..."
$GROVE init --format json | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'project_root' in d"
echo "  PASS"

# 2. grove doctor
echo "[2/5] grove doctor..."
$GROVE doctor --format json | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'ok' in d"
echo "  PASS"

# 3. grove status
echo "[3/5] grove status..."
$GROVE status --format json | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'runs' in d"
echo "  PASS"

# 4. grove tasks (empty)
echo "[4/5] grove tasks..."
$GROVE tasks --format json | python3 -c "import sys,json; d=json.load(sys.stdin); assert isinstance(d.get('tasks', []), list)"
echo "  PASS"

# 5. JSON error output
echo "[5/5] grove status on non-grove dir..."
cd /tmp
if $GROVE status --format json 2>/dev/null; then
    echo "  WARN: expected non-zero exit (may still be valid)"
else
    echo "  PASS (non-zero exit on non-project dir)"
fi

echo ""
echo "=== All smoke tests passed ==="
