#!/usr/bin/env bash
set -euo pipefail

# ── Grove Local Build ────────────────────────────────────────────────────────
# Builds a distributable DMG locally without an Apple Developer certificate.
# Uses ad-hoc signing (runs on this Mac only; Gatekeeper will warn on others).
#
# Usage: ./scripts/build-local.sh [--target aarch64|x86_64|universal]

# System tools (xattr, codesign, etc.) must come before anaconda/homebrew to avoid
# version conflicts — Tauri's bundler requires the macOS native xattr and codesign.
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$HOME/.cargo/bin:$HOME/.rustup/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

# ── Colors ───────────────────────────────────────────────────────────────────
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
BOLD='\033[1m'
DIM='\033[2m'
RESET='\033[0m'

info()  { printf "${CYAN}▸${RESET} %s\n" "$1"; }
ok()    { printf "${GREEN}✓${RESET} %s\n" "$1"; }
warn()  { printf "${YELLOW}⚠${RESET} %s\n" "$1"; }
err()   { printf "${RED}✗${RESET} %s\n" "$1"; exit 1; }

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GUI_DIR="$REPO_ROOT/crates/grove-gui"
DIST_DIR="$REPO_ROOT/dist"

# ── Parse args ───────────────────────────────────────────────────────────────
TARGET="${1:-}"
case "$TARGET" in
  --target) TARGET="${2:-}" ; shift 2 ;;
  "")       TARGET="native" ;;
  *)        TARGET="${TARGET#--target=}" ;;
esac

# Detect native arch if not specified
if [ "$TARGET" = "native" ]; then
  ARCH="$(uname -m)"
  case "$ARCH" in
    arm64)  TARGET="aarch64-apple-darwin" ;;
    x86_64) TARGET="x86_64-apple-darwin" ;;
    *)      err "Unsupported architecture: $ARCH" ;;
  esac
fi

printf "\n${BOLD}  Grove Local Build${RESET}\n"
printf "${DIM}  ──────────────────────────────────────${RESET}\n"
printf "  Target:  ${BOLD}%s${RESET}\n" "$TARGET"
printf "  Dist:    ${BOLD}%s${RESET}\n" "$DIST_DIR"
printf "\n"

# ── Check prerequisites ───────────────────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
  err "cargo not found. Install Rust: https://rustup.rs"
fi

cd "$GUI_DIR"
if ! command -v npx &>/dev/null; then
  err "npx not found. Install Node.js: https://nodejs.org"
fi

# ── Ad-hoc signing (no Apple Developer certificate required) ─────────────────
# APPLE_SIGNING_IDENTITY="-" tells Tauri to codesign with ad-hoc identity.
# The resulting app runs on THIS Mac but Gatekeeper will warn on other Macs.
# For a proper distribution build, use the release workflow with CI signing.
export APPLE_SIGNING_IDENTITY="-"

# Disable notarization for local builds
unset APPLE_ID
unset APPLE_PASSWORD
unset APPLE_TEAM_ID

# ── Build ────────────────────────────────────────────────────────────────────
info "Installing frontend dependencies..."
npm install --silent

info "Building frontend..."
npm run build

info "Compiling Rust + bundling DMG (target: $TARGET)..."
BUNDLE_DIR="$REPO_ROOT/target/$TARGET/release/bundle"

# Run tauri build; allow failure so we can still copy the DMG if it was created.
# The updater signing step (TAURI_SIGNING_PRIVATE_KEY) may fail for local builds —
# the DMG itself is still valid. We detect this case and warn the user.
if [ "$TARGET" = "universal-apple-darwin" ]; then
  npx tauri build --target universal-apple-darwin || TAURI_EXIT=$?
else
  npx tauri build --target "$TARGET" || TAURI_EXIT=$?
fi
TAURI_EXIT="${TAURI_EXIT:-0}"

# ── Copy to dist/ ────────────────────────────────────────────────────────────
mkdir -p "$DIST_DIR"

DMG_PATTERN="$BUNDLE_DIR/dmg/*.dmg"
# shellcheck disable=SC2206
DMG_FILES=( $DMG_PATTERN )

if [ ${#DMG_FILES[@]} -eq 0 ] || [ ! -f "${DMG_FILES[0]}" ]; then
  warn "No DMG found at $DMG_PATTERN"
  warn "Check $BUNDLE_DIR for build output."
  exit 1
fi

if [ "$TAURI_EXIT" -ne 0 ]; then
  warn "Tauri build exited with code $TAURI_EXIT (likely updater signing — DMG is still valid)"
fi

for DMG in "${DMG_FILES[@]}"; do
  BASENAME="$(basename "$DMG")"
  cp "$DMG" "$DIST_DIR/$BASENAME"
  ok "Copied → dist/$BASENAME"
done

# Also copy .app.tar.gz updater artifacts if present
UPDATER_PATTERN="$BUNDLE_DIR/macos/*.tar.gz"
# shellcheck disable=SC2206
UPDATER_FILES=( $UPDATER_PATTERN )
for UPD in "${UPDATER_FILES[@]}"; do
  [ -f "$UPD" ] || continue
  BASENAME="$(basename "$UPD")"
  cp "$UPD" "$DIST_DIR/$BASENAME"
  ok "Copied → dist/$BASENAME"
done

printf "\n${GREEN}${BOLD}  ✓ Build complete${RESET}\n"
printf "  Output: ${DIM}%s${RESET}\n\n" "$DIST_DIR"
