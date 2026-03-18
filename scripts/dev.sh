#!/usr/bin/env bash
# ── Grove — Dev Launcher ───────────────────────────────────────────────────
#
# Single-command full dev stack: build + run the Grove GUI (Tauri + React).
#
# Usage:
#   ./scripts/dev.sh             # default: launch GUI in dev mode (hot reload)
#   ./scripts/dev.sh --build     # production build (native .app bundle)
#   ./scripts/dev.sh --check     # run full CI check suite (fmt, clippy, test)
#   ./scripts/dev.sh --admin     # launch grove-db-lookup (DB explorer)
#   ./scripts/dev.sh --kill      # kill any running grove-gui instances
#   ./scripts/dev.sh --help      # show help
# ───────────────────────────────────────────────────────────────────────────
set -euo pipefail

# ── Source Rust paths (non-interactive shells skip profiles) ───────────────
source "$HOME/.cargo/env" 2>/dev/null || true
export PATH="$HOME/.cargo/bin:$PATH"

# ── Project root ──────────────────────────────────────────────────────────
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
GUI_DIR="$PROJECT_DIR/crates/grove-gui"
ADMIN_DIR="$PROJECT_DIR/crates/grove-db-lookup"
cd "$PROJECT_DIR"

# ── Colours ───────────────────────────────────────────────────────────────
BOLD='\033[1m'
CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
DIM='\033[2m'
NC='\033[0m'

log()  { echo -e "${CYAN}◆${NC} $*"; }
ok()   { echo -e "${GREEN}✔${NC} $*"; }
warn() { echo -e "${YELLOW}⚠${NC} $*"; }
err()  { echo -e "${RED}✖${NC} $*" >&2; }
hdr()  { echo -e "\n${BOLD}$*${NC}"; }

# ── Kill running instances ────────────────────────────────────────────────
kill_running() {
    local killed=0
    for pat in "grove-gui" "grove_gui"; do
        if pgrep -f "$pat" >/dev/null 2>&1; then
            log "Stopping: $pat"
            pkill -f "$pat" 2>/dev/null || true
            killed=1
        fi
    done
    [ "$killed" -eq 1 ] && sleep 1 && ok "Stopped previous instances." || true
}

# ── Preflight ─────────────────────────────────────────────────────────────
preflight() {
    hdr "Preflight checks"

    if ! command -v cargo >/dev/null 2>&1; then
        err "cargo not found. Install Rust: https://rustup.rs"
        exit 1
    fi
    ok "Rust $(rustc --version | awk '{print $2}')"

    if ! command -v cargo-tauri >/dev/null 2>&1; then
        err "cargo-tauri not found. Install: cargo install tauri-cli --version '^2'"
        exit 1
    fi
    ok "Tauri CLI $(cargo tauri --version 2>/dev/null || echo 'installed')"

    if ! command -v node >/dev/null 2>&1; then
        err "node not found. Install Node.js: https://nodejs.org"
        exit 1
    fi
    ok "Node $(node --version)"

    # Ensure npm deps are installed
    if [ ! -d "$GUI_DIR/node_modules" ]; then
        log "Installing npm dependencies..."
        (cd "$GUI_DIR" && npm install)
    fi
    ok "npm dependencies ready"

    # Ensure centralized Grove data directory exists.
    # The GUI uses GroveApp::init() which creates ~/.grove/workspaces/<id>/.grove/grove.db
    # at startup, so we only need to ensure the base directory is present.
    GROVE_HOME="$HOME/.grove"
    if [ ! -d "$GROVE_HOME" ]; then
        log "Creating Grove app directory at $GROVE_HOME..."
        mkdir -p "$GROVE_HOME"
    fi
    ok "Grove app directory ready ($GROVE_HOME)"
}

# ── CI check suite ────────────────────────────────────────────────────────
run_checks() {
    hdr "Running full check suite"

    log "cargo clippy..."
    cargo clippy --workspace --all-targets -- -D warnings
    ok "Clippy clean"

    log "cargo test..."
    cargo test --workspace
    ok "All tests pass"

    log "TypeScript check..."
    (cd "$GUI_DIR" && npx tsc --noEmit)
    ok "TypeScript clean"
}

# ── Build companion binaries (MCP server, filter, CLI) ────────────────────
build_companions() {
    log "Building grove-mcp-server..."
    cargo build -p grove-mcp-server 2>&1
    ok "grove-mcp-server built"
}

# ── Launch GUI (dev mode with hot reload) ─────────────────────────────────
launch_gui() {
    hdr "Launching Grove GUI (dev mode)"
    echo -e "${DIM}  GUI dir: $GUI_DIR${NC}"
    echo -e "${DIM}  Project: $PROJECT_DIR${NC}"
    echo ""

    build_companions

    cd "$GUI_DIR"
    exec cargo tauri dev
}

# ── Launch grove-db-lookup (DB explorer) ──────────────────────────────────────
launch_admin() {
    hdr "Launching grove-db-lookup (DB explorer)"
    echo -e "${DIM}  Admin dir: $ADMIN_DIR${NC}"
    echo -e "${DIM}  API:       http://localhost:3741${NC}"
    echo -e "${DIM}  UI:        http://localhost:5173${NC}"
    echo ""

    # Kill any existing processes on our ports
    for port in 3741 5173; do
        local pid
        pid=$(lsof -ti :"$port" 2>/dev/null || true)
        if [ -n "$pid" ]; then
            log "Killing existing process on port $port (PID $pid)..."
            kill $pid 2>/dev/null || true
            sleep 0.5
        fi
    done

    # Install frontend deps if needed
    if [ ! -d "$ADMIN_DIR/web/node_modules" ]; then
        log "Installing grove-db-lookup frontend dependencies..."
        (cd "$ADMIN_DIR/web" && npm install)
    fi
    ok "grove-db-lookup npm dependencies ready"

    cleanup_admin() {
        echo ""
        log "Shutting down grove-db-lookup..."
        kill $ADMIN_API_PID $ADMIN_VITE_PID 2>/dev/null || true
        wait $ADMIN_API_PID $ADMIN_VITE_PID 2>/dev/null || true
        ok "grove-db-lookup stopped."
    }
    trap cleanup_admin EXIT INT TERM

    (cd "$ADMIN_DIR" && cargo run 2>&1 | sed 's/^/[api] /') &
    ADMIN_API_PID=$!

    (cd "$ADMIN_DIR/web" && npx vite 2>&1 | sed 's/^/[web] /') &
    ADMIN_VITE_PID=$!

    wait
}

# ── Production build ──────────────────────────────────────────────────────
build_gui() {
    hdr "Building Grove GUI (production)"
    build_companions
    cd "$GUI_DIR"
    cargo tauri build
    ok "Production build complete"
    echo ""
    echo -e "  ${DIM}Bundle at: $GUI_DIR/src-tauri/target/release/bundle/${NC}"
}

# ── Argument parsing ──────────────────────────────────────────────────────
BUILD_ONLY=false
KILL_ONLY=false
CHECK_ONLY=false
ADMIN_ONLY=false

for arg in "$@"; do
    case "$arg" in
        --build)    BUILD_ONLY=true ;;
        --admin)    ADMIN_ONLY=true ;;
        --kill)     KILL_ONLY=true ;;
        --check)    CHECK_ONLY=true ;;
        --help|-h)
            echo ""
            echo "  Usage: ./scripts/dev.sh [OPTIONS]"
            echo ""
            echo "  Options:"
            echo "    --admin     Launch grove-db-lookup DB explorer (API + UI)"
            echo "    --build     Production build (native .app bundle)"
            echo "    --check     Run full CI checks (clippy, test, tsc) then exit"
            echo "    --kill      Kill running grove-gui instances and exit"
            echo "    -h, --help  Show this help"
            echo ""
            exit 0
            ;;
        *)
            err "Unknown option: $arg  (try --help)"
            exit 1
            ;;
    esac
done

# ── Main ──────────────────────────────────────────────────────────────────
echo -e "${BOLD}${CYAN}  ▲ Grove — Dev${NC}  ${DIM}$(date '+%H:%M:%S')${NC}"
echo ""

kill_running

[ "$KILL_ONLY" = true ] && exit 0

if [ "$CHECK_ONLY" = true ]; then
    run_checks
    exit 0
fi

preflight

if [ "$ADMIN_ONLY" = true ]; then
    launch_admin
    exit 0
fi

if [ "$BUILD_ONLY" = true ]; then
    build_gui
    exit 0
fi

launch_gui
